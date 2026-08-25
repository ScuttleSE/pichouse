package db

import (
	"database/sql"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// CreateAlbum inserts a new album. parentID of 0 creates a top-level album.
func (l *Library) CreateAlbum(name string, parentID int64) (int64, error) {
	var parent any
	if parentID != 0 {
		parent = parentID
	}
	res, err := l.db.Exec(
		`INSERT INTO albums(name, parent_id, position) VALUES(?, ?, ?)`,
		name, parent, nextAlbumPosition(l.db, parentID))
	if err != nil {
		return 0, err
	}
	return res.LastInsertId()
}

// nextAlbumPosition returns the next ordering position for a new album under
// the given parent (0 for top-level).
func nextAlbumPosition(db *sql.DB, parentID int64) int {
	var n sql.NullInt64
	if parentID == 0 {
		db.QueryRow(`SELECT MAX(position) FROM albums WHERE parent_id IS NULL`).Scan(&n)
	} else {
		db.QueryRow(`SELECT MAX(position) FROM albums WHERE parent_id = ?`, parentID).Scan(&n)
	}
	return int(n.Int64) + 1
}

// RenameAlbum changes an album's display name.
func (l *Library) RenameAlbum(id int64, name string) error {
	_, err := l.db.Exec(`UPDATE albums SET name = ? WHERE id = ?`, name, id)
	return err
}

// DeleteAlbum removes an album. Sub-albums cascade; member folders revert to
// the Library root ("New folders").
func (l *Library) DeleteAlbum(id int64) error {
	_, err := l.db.Exec(`DELETE FROM albums WHERE id = ?`, id)
	return err
}

// SetAlbumParent re-parents an album. parentID of 0 makes it top-level. It
// refuses to create a cycle (making an album a descendant of itself).
func (l *Library) SetAlbumParent(id, parentID int64) error {
	if id == parentID {
		return nil
	}
	// Walk up from the proposed parent; if we reach id, this would create a
	// cycle, so refuse.
	cur := parentID
	for cur != 0 {
		if cur == id {
			return nil // would create a cycle; ignore
		}
		var next sql.NullInt64
		if err := l.db.QueryRow(`SELECT parent_id FROM albums WHERE id = ?`, cur).Scan(&next); err != nil {
			break
		}
		cur = next.Int64
	}
	var parent any
	if parentID != 0 {
		parent = parentID
	}
	_, err := l.db.Exec(`UPDATE albums SET parent_id = ? WHERE id = ?`, parent, id)
	return err
}

// Albums returns all albums ordered by parent then position then name.
func (l *Library) Albums() ([]model.Album, error) {
	rows, err := l.db.Query(
		`SELECT id, name, COALESCE(parent_id, 0), position
		 FROM albums ORDER BY position ASC, name ASC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []model.Album
	for rows.Next() {
		var a model.Album
		if err := rows.Scan(&a.ID, &a.Name, &a.ParentID, &a.Position); err != nil {
			return nil, err
		}
		out = append(out, a)
	}
	return out, rows.Err()
}

// AddFolderToAlbum places a folder into an album, removing it from any other
// album first (a folder belongs to at most one album in the tree).
func (l *Library) AddFolderToAlbum(folderID, albumID int64) error {
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`DELETE FROM album_folders WHERE folder_id = ?`, folderID); err != nil {
		return err
	}
	var pos sql.NullInt64
	tx.QueryRow(`SELECT MAX(position) FROM album_folders WHERE album_id = ?`, albumID).Scan(&pos)
	if _, err := tx.Exec(
		`INSERT INTO album_folders(album_id, folder_id, position) VALUES(?, ?, ?)`,
		albumID, folderID, int(pos.Int64)+1); err != nil {
		return err
	}
	return tx.Commit()
}

// RemoveFolderFromAlbum detaches a folder from any album, returning it to the
// Library root.
func (l *Library) RemoveFolderFromAlbum(folderID int64) error {
	_, err := l.db.Exec(`DELETE FROM album_folders WHERE folder_id = ?`, folderID)
	return err
}

// FolderAlbums returns a map of folder id to the album id it belongs to. Folders
// not in any album are absent from the map.
func (l *Library) FolderAlbums() (map[int64]int64, error) {
	rows, err := l.db.Query(`SELECT folder_id, album_id FROM album_folders`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[int64]int64{}
	for rows.Next() {
		var fid, aid int64
		if err := rows.Scan(&fid, &aid); err != nil {
			return nil, err
		}
		out[fid] = aid
	}
	return out, rows.Err()
}
