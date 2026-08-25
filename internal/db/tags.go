package db

import (
	"database/sql"
	"strings"
	"time"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// AddPhotoTags upserts the given tag names into the global vocabulary and links
// them to the photo with the given source (model.TagSourceAI / TagSourceUser).
// Existing links are preserved; a user tag never downgrades an existing link's
// source. The photo's FTS row is rebuilt. Writes are serialized.
func (l *Library) AddPhotoTags(photoID int64, tags []string, source int) error {
	if len(tags) == 0 {
		return nil
	}
	l.writeMu.Lock()
	defer l.writeMu.Unlock()

	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	now := time.Now().Unix()
	for _, raw := range tags {
		name := strings.TrimSpace(raw)
		if name == "" {
			continue
		}
		if _, err := tx.Exec(
			`INSERT INTO tags(name) VALUES(?) ON CONFLICT(name) DO NOTHING`, name); err != nil {
			return err
		}
		var tagID int64
		if err := tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, name).Scan(&tagID); err != nil {
			return err
		}
		// Insert link if absent. If a user tag arrives, upgrade source to user.
		if _, err := tx.Exec(
			`INSERT INTO photo_tags(photo_id, tag_id, source, confirmed, created_at)
			 VALUES(?, ?, ?, ?, ?)
			 ON CONFLICT(photo_id, tag_id) DO UPDATE SET
			   source = CASE WHEN ? = ? THEN ? ELSE photo_tags.source END`,
			photoID, tagID, source, 0, now,
			source, model.TagSourceUser, model.TagSourceUser); err != nil {
			return err
		}
	}
	if err := rebuildPhotoFTS(tx, photoID); err != nil {
		return err
	}
	return tx.Commit()
}

// RemovePhotoTag unlinks a tag (by name) from a photo and rebuilds the FTS row.
func (l *Library) RemovePhotoTag(photoID int64, name string) error {
	l.writeMu.Lock()
	defer l.writeMu.Unlock()
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(
		`DELETE FROM photo_tags WHERE photo_id = ?
		 AND tag_id = (SELECT id FROM tags WHERE name = ?)`, photoID, name); err != nil {
		return err
	}
	if err := rebuildPhotoFTS(tx, photoID); err != nil {
		return err
	}
	return tx.Commit()
}

// ConfirmPhotoTag marks an AI tag on a photo as confirmed by the user.
func (l *Library) ConfirmPhotoTag(photoID int64, name string) error {
	l.writeMu.Lock()
	defer l.writeMu.Unlock()
	_, err := l.db.Exec(
		`UPDATE photo_tags SET confirmed = 1
		 WHERE photo_id = ? AND tag_id = (SELECT id FROM tags WHERE name = ?)`,
		photoID, name)
	return err
}

// PhotoTags returns the tags on a photo ordered by source then name.
func (l *Library) PhotoTags(photoID int64) ([]model.Tag, error) {
	rows, err := l.db.Query(
		`SELECT t.name, pt.source, pt.confirmed
		 FROM photo_tags pt JOIN tags t ON t.id = pt.tag_id
		 WHERE pt.photo_id = ? ORDER BY pt.source ASC, t.name ASC`, photoID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []model.Tag
	for rows.Next() {
		var t model.Tag
		var confirmed int
		if err := rows.Scan(&t.Name, &t.Source, &confirmed); err != nil {
			return nil, err
		}
		t.Confirmed = confirmed != 0
		out = append(out, t)
	}
	return out, rows.Err()
}

// SetAIStatus updates a photo's AI tagging status.
func (l *Library) SetAIStatus(photoID int64, status int) error {
	_, err := l.db.Exec(`UPDATE photos SET ai_status = ? WHERE id = ?`, status, photoID)
	return err
}

// PhotosNeedingTags returns photo ids that have not yet been AI-tagged. If
// folderID > 0 the search is limited to that folder. Photos marked done or
// skipped are excluded unless includeDone is true.
func (l *Library) PhotosNeedingTags(folderID int64, includeDone bool) ([]int64, error) {
	q := `SELECT id FROM photos WHERE 1=1`
	var args []any
	if folderID > 0 {
		q += ` AND folder_id = ?`
		args = append(args, folderID)
	}
	if !includeDone {
		q += ` AND ai_status NOT IN (?, ?)`
		args = append(args, model.AIDone, model.AISkipped)
	}
	q += ` ORDER BY id ASC`
	rows, err := l.db.Query(q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []int64
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out = append(out, id)
	}
	return out, rows.Err()
}

// PhotoByID loads a single photo by id.
func (l *Library) PhotoByID(id int64) (model.Photo, error) {
	rows, err := l.db.Query(
		`SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status
		 FROM photos WHERE id = ?`, id)
	if err != nil {
		return model.Photo{}, err
	}
	defer rows.Close()
	ps, err := scanPhotos(rows)
	if err != nil {
		return model.Photo{}, err
	}
	if len(ps) == 0 {
		return model.Photo{}, sql.ErrNoRows
	}
	return ps[0], nil
}

// SearchPhotoIDsByTag returns the set of photo ids whose tags match the query
// using FTS5. A trailing '*' is appended to the last token for prefix matching.
func (l *Library) SearchPhotoIDsByTag(query string) (map[int64]bool, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return map[int64]bool{}, nil
	}
	match := ftsQuery(query)
	rows, err := l.db.Query(
		`SELECT rowid FROM photo_tags_fts WHERE photo_tags_fts MATCH ?`, match)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := map[int64]bool{}
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out[id] = true
	}
	return out, rows.Err()
}

// AllTags returns every tag with the number of photos carrying it.
func (l *Library) AllTags() ([]model.TagCount, error) {
	rows, err := l.db.Query(
		`SELECT t.name, COUNT(pt.photo_id)
		 FROM tags t LEFT JOIN photo_tags pt ON pt.tag_id = t.id
		 GROUP BY t.id ORDER BY t.name ASC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []model.TagCount
	for rows.Next() {
		var tc model.TagCount
		if err := rows.Scan(&tc.Name, &tc.Count); err != nil {
			return nil, err
		}
		out = append(out, tc)
	}
	return out, rows.Err()
}

// RenameTag renames a tag globally. If the new name already exists the two are
// merged. FTS rows for all affected photos are rebuilt.
func (l *Library) RenameTag(oldName, newName string) error {
	newName = strings.TrimSpace(newName)
	if newName == "" || strings.EqualFold(oldName, newName) {
		return nil
	}
	l.writeMu.Lock()
	defer l.writeMu.Unlock()
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	var oldID int64
	if err := tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, oldName).Scan(&oldID); err != nil {
		if err == sql.ErrNoRows {
			return nil
		}
		return err
	}
	var newID int64
	err = tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, newName).Scan(&newID)
	affected, aerr := affectedPhotoIDs(tx, oldID)
	if aerr != nil {
		return aerr
	}
	if err == sql.ErrNoRows {
		// Simple rename.
		if _, err := tx.Exec(`UPDATE tags SET name = ? WHERE id = ?`, newName, oldID); err != nil {
			return err
		}
	} else if err != nil {
		return err
	} else {
		// Merge oldID into newID.
		if err := mergeTagInto(tx, oldID, newID); err != nil {
			return err
		}
		more, merr := affectedPhotoIDs(tx, newID)
		if merr != nil {
			return merr
		}
		affected = append(affected, more...)
	}
	for _, pid := range affected {
		if err := rebuildPhotoFTS(tx, pid); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// MergeTags merges src into dst (both by name) and rebuilds affected FTS rows.
func (l *Library) MergeTags(srcName, dstName string) error {
	if strings.EqualFold(srcName, dstName) {
		return nil
	}
	l.writeMu.Lock()
	defer l.writeMu.Unlock()
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	var srcID, dstID int64
	if err := tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, srcName).Scan(&srcID); err != nil {
		return err
	}
	if err := tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, dstName).Scan(&dstID); err != nil {
		return err
	}
	affected, err := affectedPhotoIDs(tx, srcID)
	if err != nil {
		return err
	}
	if err := mergeTagInto(tx, srcID, dstID); err != nil {
		return err
	}
	for _, pid := range affected {
		if err := rebuildPhotoFTS(tx, pid); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// DeleteTag removes a tag globally and rebuilds affected FTS rows.
func (l *Library) DeleteTag(name string) error {
	l.writeMu.Lock()
	defer l.writeMu.Unlock()
	tx, err := l.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	var id int64
	if err := tx.QueryRow(`SELECT id FROM tags WHERE name = ?`, name).Scan(&id); err != nil {
		if err == sql.ErrNoRows {
			return nil
		}
		return err
	}
	affected, err := affectedPhotoIDs(tx, id)
	if err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM tags WHERE id = ?`, id); err != nil {
		return err
	}
	for _, pid := range affected {
		if err := rebuildPhotoFTS(tx, pid); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// --- helpers (operate within a transaction) ---

// affectedPhotoIDs returns the photo ids linked to a tag id.
func affectedPhotoIDs(tx *sql.Tx, tagID int64) ([]int64, error) {
	rows, err := tx.Query(`SELECT photo_id FROM photo_tags WHERE tag_id = ?`, tagID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []int64
	for rows.Next() {
		var id int64
		if err := rows.Scan(&id); err != nil {
			return nil, err
		}
		out = append(out, id)
	}
	return out, rows.Err()
}

// mergeTagInto repoints all links from srcID to dstID (avoiding duplicates) and
// deletes srcID.
func mergeTagInto(tx *sql.Tx, srcID, dstID int64) error {
	if _, err := tx.Exec(
		`UPDATE OR IGNORE photo_tags SET tag_id = ? WHERE tag_id = ?`, dstID, srcID); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM photo_tags WHERE tag_id = ?`, srcID); err != nil {
		return err
	}
	_, err := tx.Exec(`DELETE FROM tags WHERE id = ?`, srcID)
	return err
}

// rebuildPhotoFTS replaces the FTS row for a photo with its current tag text.
func rebuildPhotoFTS(tx *sql.Tx, photoID int64) error {
	if _, err := tx.Exec(
		`DELETE FROM photo_tags_fts WHERE rowid = ?`, photoID); err != nil {
		return err
	}
	var text sql.NullString
	if err := tx.QueryRow(
		`SELECT group_concat(t.name, ' ')
		 FROM photo_tags pt JOIN tags t ON t.id = pt.tag_id
		 WHERE pt.photo_id = ?`, photoID).Scan(&text); err != nil {
		return err
	}
	if !text.Valid || text.String == "" {
		return nil
	}
	_, err := tx.Exec(
		`INSERT INTO photo_tags_fts(rowid, tags) VALUES(?, ?)`, photoID, text.String)
	return err
}

// ftsQuery turns a free-text query into an FTS5 MATCH expression: each token is
// quoted; the final token gets a prefix wildcard so typing is incremental.
func ftsQuery(q string) string {
	fields := strings.Fields(q)
	parts := make([]string, 0, len(fields))
	for i, f := range fields {
		f = strings.ReplaceAll(f, `"`, "")
		if f == "" {
			continue
		}
		if i == len(fields)-1 {
			parts = append(parts, `"`+f+`"*`)
		} else {
			parts = append(parts, `"`+f+`"`)
		}
	}
	return strings.Join(parts, " ")
}
