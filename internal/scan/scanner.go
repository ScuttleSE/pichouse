// Package scan walks library folders and records photos into the library
// database.
package scan

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/rwcarlsen/goexif/exif"
	_ "golang.org/x/image/bmp"
	_ "golang.org/x/image/tiff"
	_ "golang.org/x/image/webp"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// imageExts are the file extensions treated as photos (lowercase, with dot).
var imageExts = map[string]bool{
	".jpg": true, ".jpeg": true, ".png": true, ".gif": true,
	".webp": true, ".bmp": true, ".tif": true, ".tiff": true,
}

// IsImage reports whether a filename has a supported image extension.
func IsImage(name string) bool {
	return imageExts[strings.ToLower(filepath.Ext(name))]
}

// Progress reports scan progress. Done is the number of photos processed so
// far; Total is the number discovered (may grow as walking continues).
type Progress struct {
	Folder string
	Done   int
	Total  int
}

// Scanner records photos from library folders into the library database.
type Scanner struct {
	lib *db.Library
}

// New returns a Scanner backed by the given library.
func New(lib *db.Library) *Scanner {
	return &Scanner{lib: lib}
}

// ScanFolder walks root recursively, recording folders and photos. progress may
// be nil. It returns the number of photos recorded.
func (s *Scanner) ScanFolder(root string, progress func(Progress)) (int, error) {
	return s.ScanFolderContext(context.Background(), root, progress)
}

// ScanFolderContext is ScanFolder with cancellation. When ctx is cancelled the
// walk stops promptly and the count recorded so far is returned along with
// ctx.Err().
func (s *Scanner) ScanFolderContext(ctx context.Context, root string, progress func(Progress)) (int, error) {
	// First pass: collect image files grouped by directory.
	byDir := map[string][]string{}
	total := 0
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil // skip unreadable entries
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if d.IsDir() {
			return nil
		}
		if IsImage(d.Name()) {
			dir := filepath.Dir(path)
			byDir[dir] = append(byDir[dir], path)
			total++
		}
		return nil
	})
	if err != nil {
		return 0, err
	}

	done := 0
	for dir, files := range byDir {
		if ctx.Err() != nil {
			return done, ctx.Err()
		}
		fid, err := s.upsertFolderFor(dir, files)
		if err != nil {
			return done, err
		}
		if err := s.lib.SetScanState(fid, model.ScanRunning); err != nil {
			return done, err
		}
		for _, path := range files {
			if ctx.Err() != nil {
				return done, ctx.Err()
			}
			if err := s.recordPhoto(fid, path); err != nil {
				return done, err
			}
			done++
			if progress != nil {
				progress(Progress{Folder: dir, Done: done, Total: total})
			}
		}
		if err := s.lib.SetScanState(fid, model.ScanDone); err != nil {
			return done, err
		}
	}
	return done, nil
}

// upsertFolderFor records the folder for a directory, deriving its year from
// the earliest photo taken date, falling back to the folder mtime.
func (s *Scanner) upsertFolderFor(dir string, files []string) (int64, error) {
	info, err := os.Stat(dir)
	if err != nil {
		return 0, err
	}
	year := info.ModTime().Year()
	// Try to derive year from the earliest EXIF taken date.
	earliest := time.Time{}
	for _, f := range files {
		if t, ok := takenAt(f); ok {
			if earliest.IsZero() || t.Before(earliest) {
				earliest = t
			}
		}
	}
	if !earliest.IsZero() {
		year = earliest.Year()
	}
	return s.lib.UpsertFolder(model.Folder{
		Path:  dir,
		Name:  filepath.Base(dir),
		MTime: info.ModTime(),
		Year:  year,
	})
}

func (s *Scanner) recordPhoto(folderID int64, path string) error {
	info, err := os.Stat(path)
	if err != nil {
		return nil // file vanished; skip
	}
	p := model.Photo{
		FolderID: folderID,
		Path:     path,
		Filename: filepath.Base(path),
		Size:     info.Size(),
		ModTime:  info.ModTime(),
	}
	if t, ok := takenAt(path); ok {
		p.TakenAt = t
	}
	if w, h, ok := dimensions(path); ok {
		p.Width, p.Height = w, h
	}
	if h, err := hashFile(path); err == nil {
		p.Hash = h
	}
	_, err = s.lib.UpsertPhoto(p)
	return err
}

// takenAt returns the EXIF DateTimeOriginal for a file, if present.
func takenAt(path string) (time.Time, bool) {
	f, err := os.Open(path)
	if err != nil {
		return time.Time{}, false
	}
	defer f.Close()
	x, err := exif.Decode(f)
	if err != nil {
		return time.Time{}, false
	}
	t, err := x.DateTime()
	if err != nil {
		return time.Time{}, false
	}
	return t, true
}

// dimensions returns the pixel width and height of an image file.
func dimensions(path string) (int, int, bool) {
	f, err := os.Open(path)
	if err != nil {
		return 0, 0, false
	}
	defer f.Close()
	cfg, _, err := image.DecodeConfig(f)
	if err != nil {
		return 0, 0, false
	}
	return cfg.Width, cfg.Height, true
}

// hashFile computes a sha256 hex digest of a file's contents.
func hashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}
