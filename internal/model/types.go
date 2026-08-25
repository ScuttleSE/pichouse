// Package model holds shared types used across pichouse.
package model

import "time"

// LibraryFolder is a user-added root folder that gets scanned.
type LibraryFolder struct {
	ID      int64
	Path    string
	AddedAt time.Time
}

// Folder is a scanned directory (a library root or any subfolder) that
// contains photos.
type Folder struct {
	ID    int64
	Path  string
	Name  string
	MTime time.Time
	Year  int // derived from the earliest photo taken date, else folder mtime
}

// Photo is a single image file recorded in the library.
type Photo struct {
	ID         int64
	FolderID   int64
	Path       string
	Filename   string
	Size       int64
	ModTime    time.Time
	TakenAt    time.Time // from EXIF; zero if unknown
	Width      int
	Height     int
	Hash       string // content hash, used as thumbnail key
	ThumbReady bool
}

// ScanStatus describes the scan state of a folder.
type ScanStatus string

const (
	// ScanPending means the folder is queued but not yet scanned.
	ScanPending ScanStatus = "pending"
	// ScanRunning means the folder is currently being scanned.
	ScanRunning ScanStatus = "running"
	// ScanDone means the folder scan completed.
	ScanDone ScanStatus = "done"
	// ScanError means the folder scan failed.
	ScanError ScanStatus = "error"
)
