package db

import (
	"path/filepath"
	"testing"
	"time"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

func TestLibraryRoundTrip(t *testing.T) {
	dir := t.TempDir()
	lib, err := OpenLibraryAt(filepath.Join(dir, "library.db"))
	if err != nil {
		t.Fatalf("open library: %v", err)
	}
	defer lib.Close()

	if _, err := lib.AddLibraryFolder("/photos"); err != nil {
		t.Fatalf("add library folder: %v", err)
	}
	// Idempotent.
	if _, err := lib.AddLibraryFolder("/photos"); err != nil {
		t.Fatalf("re-add library folder: %v", err)
	}
	lfs, err := lib.LibraryFolders()
	if err != nil {
		t.Fatalf("list library folders: %v", err)
	}
	if len(lfs) != 1 {
		t.Fatalf("want 1 library folder, got %d", len(lfs))
	}

	fid, err := lib.UpsertFolder(model.Folder{
		Path: "/photos/2019", Name: "2019", MTime: time.Now(), Year: 2019,
	})
	if err != nil {
		t.Fatalf("upsert folder: %v", err)
	}

	pid, err := lib.UpsertPhoto(model.Photo{
		FolderID: fid, Path: "/photos/2019/a.jpg", Filename: "a.jpg",
		Size: 100, ModTime: time.Now(), Width: 640, Height: 480, Hash: "abc",
	})
	if err != nil {
		t.Fatalf("upsert photo: %v", err)
	}
	if err := lib.SetThumbReady(pid, true); err != nil {
		t.Fatalf("set thumb ready: %v", err)
	}

	photos, err := lib.PhotosInFolder(fid)
	if err != nil {
		t.Fatalf("photos in folder: %v", err)
	}
	if len(photos) != 1 || !photos[0].ThumbReady || photos[0].Hash != "abc" {
		t.Fatalf("unexpected photos: %+v", photos)
	}
}

func TestRemoveLibraryFolder(t *testing.T) {
	dir := t.TempDir()
	lib, err := OpenLibraryAt(filepath.Join(dir, "library.db"))
	if err != nil {
		t.Fatalf("open library: %v", err)
	}
	defer lib.Close()

	if _, err := lib.AddLibraryFolder("/photos"); err != nil {
		t.Fatalf("add library folder: %v", err)
	}
	fid, err := lib.UpsertFolder(model.Folder{
		Path: "/photos/2019", Name: "2019", MTime: time.Now(), Year: 2019,
	})
	if err != nil {
		t.Fatalf("upsert folder: %v", err)
	}
	if _, err := lib.UpsertPhoto(model.Photo{
		FolderID: fid, Path: "/photos/2019/a.jpg", Filename: "a.jpg",
		Size: 1, ModTime: time.Now(), Hash: "h",
	}); err != nil {
		t.Fatalf("upsert photo: %v", err)
	}

	if err := lib.RemoveLibraryFolder("/photos"); err != nil {
		t.Fatalf("remove library folder: %v", err)
	}

	lfs, err := lib.LibraryFolders()
	if err != nil {
		t.Fatalf("list library folders: %v", err)
	}
	if len(lfs) != 0 {
		t.Fatalf("want 0 library folders after remove, got %d", len(lfs))
	}
	folders, err := lib.Folders()
	if err != nil {
		t.Fatalf("list folders: %v", err)
	}
	if len(folders) != 0 {
		t.Fatalf("want 0 scanned folders after remove, got %d", len(folders))
	}
}

func TestThumbsRoundTrip(t *testing.T) {
	dir := t.TempDir()
	th, err := OpenThumbsAt(filepath.Join(dir, "thumbs.db"))
	if err != nil {
		t.Fatalf("open thumbs: %v", err)
	}
	defer th.Close()

	if _, ok, _ := th.Get("missing"); ok {
		t.Fatal("expected miss for absent hash")
	}
	if err := th.Put("h1", 256, []byte("jpegbytes")); err != nil {
		t.Fatalf("put: %v", err)
	}
	blob, ok, err := th.Get("h1")
	if err != nil || !ok || string(blob) != "jpegbytes" {
		t.Fatalf("get mismatch: blob=%q ok=%v err=%v", blob, ok, err)
	}
}
