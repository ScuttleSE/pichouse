package db

import (
	"database/sql"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

const thumbsSchema = `
CREATE TABLE IF NOT EXISTS thumbnails (
    photo_hash TEXT PRIMARY KEY,
    size       INTEGER NOT NULL,
    jpeg       BLOB NOT NULL,
    created_at INTEGER NOT NULL
);`

// Thumbs is a handle to the thumbs.db thumbnail-blob database.
type Thumbs struct {
	db *sql.DB
}

// OpenThumbs opens (and migrates) thumbs.db in the pichouse data directory.
func OpenThumbs() (*Thumbs, error) {
	dir, err := DataDir()
	if err != nil {
		return nil, err
	}
	return OpenThumbsAt(filepath.Join(dir, "thumbs.db"))
}

// OpenThumbsAt opens (and migrates) a thumbnail database at the given path.
func OpenThumbsAt(path string) (*Thumbs, error) {
	sqldb, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	if _, err := sqldb.Exec("PRAGMA journal_mode=WAL;"); err != nil {
		sqldb.Close()
		return nil, err
	}
	if _, err := sqldb.Exec(thumbsSchema); err != nil {
		sqldb.Close()
		return nil, err
	}
	return &Thumbs{db: sqldb}, nil
}

// Close closes the underlying database.
func (t *Thumbs) Close() error { return t.db.Close() }

// Get returns the cached JPEG thumbnail for a photo hash, or (nil, false) if
// none is cached.
func (t *Thumbs) Get(hash string) ([]byte, bool, error) {
	var blob []byte
	err := t.db.QueryRow(`SELECT jpeg FROM thumbnails WHERE photo_hash = ?`, hash).Scan(&blob)
	if err == sql.ErrNoRows {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return blob, true, nil
}

// Put stores (or replaces) a JPEG thumbnail for a photo hash.
func (t *Thumbs) Put(hash string, size int, jpeg []byte) error {
	_, err := t.db.Exec(
		`INSERT INTO thumbnails(photo_hash, size, jpeg, created_at) VALUES(?, ?, ?, ?)
		 ON CONFLICT(photo_hash) DO UPDATE SET size=excluded.size, jpeg=excluded.jpeg, created_at=excluded.created_at`,
		hash, size, jpeg, time.Now().Unix())
	return err
}
