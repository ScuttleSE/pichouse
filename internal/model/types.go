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
	// Orientation is the user-applied rotation in degrees clockwise (0, 90,
	// 180, 270). It is stored only in the database, never written to disk.
	Orientation int
	// AIStatus tracks the AI tagging state of this photo.
	AIStatus int
}

// AI tagging status values stored in Photo.AIStatus.
const (
	AIUntagged = 0
	AIQueued   = 1
	AIDone     = 2
	AIError    = 3
	AISkipped  = 4
)

// TagSource identifies who created a photo-tag link.
const (
	TagSourceAI   = 0
	TagSourceUser = 1
)

// Tag is a keyword associated with a photo.
type Tag struct {
	Name      string
	Source    int // TagSourceAI or TagSourceUser
	Confirmed bool
}

// TagCount is a tag together with how many photos carry it.
type TagCount struct {
	Name  string
	Count int
}

// Album is a virtual organisation of folders. Albums do not affect files on
// disk and may nest under a parent album.
type Album struct {
	ID       int64
	Name     string
	ParentID int64 // 0 means top-level
	Position int
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
