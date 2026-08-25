// Package db provides SQLite-backed storage for pichouse. It manages two
// database files: library.db (metadata) and thumbs.db (thumbnail blobs).
package db

import (
	"database/sql"
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

//go:embed schema.sql
var librarySchema string

// Library is a handle to the library.db metadata database.
type Library struct {
	db *sql.DB
}

// DataDir returns the pichouse data directory (~/.local/share/pichouse),
// creating it if necessary. It honors XDG_DATA_HOME when set.
func DataDir() (string, error) {
	var base string
	if dir, ok := os.LookupEnv("XDG_DATA_HOME"); ok && dir != "" {
		base = dir
	} else {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		base = filepath.Join(home, ".local", "share")
	}
	dir := filepath.Join(base, "pichouse")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}

// OpenLibrary opens (and migrates) library.db in the pichouse data directory.
func OpenLibrary() (*Library, error) {
	dir, err := DataDir()
	if err != nil {
		return nil, err
	}
	return OpenLibraryAt(filepath.Join(dir, "library.db"))
}

// OpenLibraryAt opens (and migrates) a library database at the given path.
func OpenLibraryAt(path string) (*Library, error) {
	sqldb, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	if _, err := sqldb.Exec("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;"); err != nil {
		sqldb.Close()
		return nil, err
	}
	if _, err := sqldb.Exec(librarySchema); err != nil {
		sqldb.Close()
		return nil, fmt.Errorf("apply schema: %w", err)
	}
	return &Library{db: sqldb}, nil
}

// Close closes the underlying database.
func (l *Library) Close() error { return l.db.Close() }

// AddLibraryFolder records a user-added root folder. It is idempotent.
func (l *Library) AddLibraryFolder(path string) (model.LibraryFolder, error) {
	now := time.Now()
	_, err := l.db.Exec(
		`INSERT INTO library_folders(path, added_at) VALUES(?, ?)
		 ON CONFLICT(path) DO NOTHING`,
		path, now.Unix(),
	)
	if err != nil {
		return model.LibraryFolder{}, err
	}
	var lf model.LibraryFolder
	var added int64
	err = l.db.QueryRow(
		`SELECT id, path, added_at FROM library_folders WHERE path = ?`, path,
	).Scan(&lf.ID, &lf.Path, &added)
	if err != nil {
		return model.LibraryFolder{}, err
	}
	lf.AddedAt = time.Unix(added, 0)
	return lf, nil
}

// LibraryFolders returns all user-added root folders.
func (l *Library) LibraryFolders() ([]model.LibraryFolder, error) {
	rows, err := l.db.Query(`SELECT id, path, added_at FROM library_folders ORDER BY path`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []model.LibraryFolder
	for rows.Next() {
		var lf model.LibraryFolder
		var added int64
		if err := rows.Scan(&lf.ID, &lf.Path, &added); err != nil {
			return nil, err
		}
		lf.AddedAt = time.Unix(added, 0)
		out = append(out, lf)
	}
	return out, rows.Err()
}

// UpsertFolder inserts or updates a scanned folder and returns its id.
func (l *Library) UpsertFolder(f model.Folder) (int64, error) {
	_, err := l.db.Exec(
		`INSERT INTO folders(path, name, mtime, year) VALUES(?, ?, ?, ?)
		 ON CONFLICT(path) DO UPDATE SET name=excluded.name, mtime=excluded.mtime, year=excluded.year`,
		f.Path, f.Name, f.MTime.Unix(), f.Year,
	)
	if err != nil {
		return 0, err
	}
	var id int64
	err = l.db.QueryRow(`SELECT id FROM folders WHERE path = ?`, f.Path).Scan(&id)
	return id, err
}

// Folders returns all scanned folders ordered by year (desc) then name.
func (l *Library) Folders() ([]model.Folder, error) {
	rows, err := l.db.Query(
		`SELECT id, path, name, mtime, year FROM folders ORDER BY year DESC, name ASC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []model.Folder
	for rows.Next() {
		var f model.Folder
		var mtime int64
		if err := rows.Scan(&f.ID, &f.Path, &f.Name, &mtime, &f.Year); err != nil {
			return nil, err
		}
		f.MTime = time.Unix(mtime, 0)
		out = append(out, f)
	}
	return out, rows.Err()
}

// UpsertPhoto inserts or updates a photo by path and returns its id.
func (l *Library) UpsertPhoto(p model.Photo) (int64, error) {
	thumb := 0
	if p.ThumbReady {
		thumb = 1
	}
	_, err := l.db.Exec(
		`INSERT INTO photos(folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready)
		 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(path) DO UPDATE SET
		   folder_id=excluded.folder_id, filename=excluded.filename, size=excluded.size,
		   mod_time=excluded.mod_time, taken_at=excluded.taken_at, width=excluded.width,
		   height=excluded.height, hash=excluded.hash`,
		p.FolderID, p.Path, p.Filename, p.Size, p.ModTime.Unix(),
		p.TakenAt.Unix(), p.Width, p.Height, p.Hash, thumb,
	)
	if err != nil {
		return 0, err
	}
	var id int64
	err = l.db.QueryRow(`SELECT id FROM photos WHERE path = ?`, p.Path).Scan(&id)
	return id, err
}

// PhotosInFolder returns all photos for a folder ordered by taken date then name.
func (l *Library) PhotosInFolder(folderID int64) ([]model.Photo, error) {
	rows, err := l.db.Query(
		`SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready
		 FROM photos WHERE folder_id = ? ORDER BY taken_at ASC, filename ASC`, folderID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanPhotos(rows)
}

// SetThumbReady marks whether a photo's thumbnail has been generated.
func (l *Library) SetThumbReady(photoID int64, ready bool) error {
	v := 0
	if ready {
		v = 1
	}
	_, err := l.db.Exec(`UPDATE photos SET thumb_ready = ? WHERE id = ?`, v, photoID)
	return err
}

// SetScanState records the scan status for a folder.
func (l *Library) SetScanState(folderID int64, status model.ScanStatus) error {
	_, err := l.db.Exec(
		`INSERT INTO scan_state(folder_id, last_scanned, status) VALUES(?, ?, ?)
		 ON CONFLICT(folder_id) DO UPDATE SET last_scanned=excluded.last_scanned, status=excluded.status`,
		folderID, time.Now().Unix(), string(status))
	return err
}

func scanPhotos(rows *sql.Rows) ([]model.Photo, error) {
	var out []model.Photo
	for rows.Next() {
		var p model.Photo
		var modTime, takenAt int64
		var thumb int
		if err := rows.Scan(&p.ID, &p.FolderID, &p.Path, &p.Filename, &p.Size,
			&modTime, &takenAt, &p.Width, &p.Height, &p.Hash, &thumb); err != nil {
			return nil, err
		}
		p.ModTime = time.Unix(modTime, 0)
		if takenAt > 0 {
			p.TakenAt = time.Unix(takenAt, 0)
		}
		p.ThumbReady = thumb != 0
		out = append(out, p)
	}
	return out, rows.Err()
}
