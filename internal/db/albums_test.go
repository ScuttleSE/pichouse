package db

import (
	"path/filepath"
	"testing"
	"time"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

func mkFolder(t *testing.T, lib *Library, path, name string) int64 {
	t.Helper()
	fid, err := lib.UpsertFolder(model.Folder{Path: path, Name: name, MTime: time.Now(), Year: 2020})
	if err != nil {
		t.Fatalf("upsert folder: %v", err)
	}
	return fid
}

func TestAlbumsAndMembership(t *testing.T) {
	dir := t.TempDir()
	lib, err := OpenLibraryAt(filepath.Join(dir, "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer lib.Close()

	f1 := mkFolder(t, lib, "/p/a", "a")
	f2 := mkFolder(t, lib, "/p/b", "b")

	trips, err := lib.CreateAlbum("Trips", 0)
	if err != nil {
		t.Fatalf("create album: %v", err)
	}
	sub, err := lib.CreateAlbum("2019", trips)
	if err != nil {
		t.Fatalf("create sub-album: %v", err)
	}

	albums, err := lib.Albums()
	if err != nil || len(albums) != 2 {
		t.Fatalf("albums: %v len=%d", err, len(albums))
	}
	// Verify parent relationship.
	var subParent int64
	for _, a := range albums {
		if a.ID == sub {
			subParent = a.ParentID
		}
	}
	if subParent != trips {
		t.Fatalf("sub-album parent = %d want %d", subParent, trips)
	}

	// Move both folders into the sub-album.
	if err := lib.AddFolderToAlbum(f1, sub); err != nil {
		t.Fatal(err)
	}
	if err := lib.AddFolderToAlbum(f2, sub); err != nil {
		t.Fatal(err)
	}
	fa, err := lib.FolderAlbums()
	if err != nil {
		t.Fatal(err)
	}
	if fa[f1] != sub || fa[f2] != sub {
		t.Fatalf("membership wrong: %+v", fa)
	}

	// Moving f1 to Trips removes it from sub (one album max).
	if err := lib.AddFolderToAlbum(f1, trips); err != nil {
		t.Fatal(err)
	}
	fa, _ = lib.FolderAlbums()
	if fa[f1] != trips {
		t.Fatalf("f1 album = %d want %d", fa[f1], trips)
	}

	// Remove f2 -> back to unassigned.
	if err := lib.RemoveFolderFromAlbum(f2); err != nil {
		t.Fatal(err)
	}
	fa, _ = lib.FolderAlbums()
	if _, ok := fa[f2]; ok {
		t.Fatalf("f2 should be unassigned, got album %d", fa[f2])
	}

	// Deleting Trips cascades sub and reverts f1 to unassigned.
	if err := lib.DeleteAlbum(trips); err != nil {
		t.Fatal(err)
	}
	albums, _ = lib.Albums()
	if len(albums) != 0 {
		t.Fatalf("want 0 albums after cascade delete, got %d", len(albums))
	}
	fa, _ = lib.FolderAlbums()
	if len(fa) != 0 {
		t.Fatalf("want no memberships after delete, got %+v", fa)
	}
}

func TestHashesByDir(t *testing.T) {
	dir := t.TempDir()
	lib, err := OpenLibraryAt(filepath.Join(dir, "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer lib.Close()

	fid := mkFolder(t, lib, "/photos/trip", "trip")
	if _, err := lib.UpsertPhoto(model.Photo{
		FolderID: fid, Path: "/photos/trip/a.jpg", Filename: "a.jpg",
		Size: 1, ModTime: time.Now(), Hash: "hashA",
	}); err != nil {
		t.Fatal(err)
	}
	m, err := lib.HashesByDir("/photos/trip")
	if err != nil {
		t.Fatal(err)
	}
	if m["/photos/trip/a.jpg"] != "hashA" {
		t.Fatalf("HashesByDir wrong: %+v", m)
	}
}

func TestSetAlbumParent(t *testing.T) {
	dir := t.TempDir()
	lib, err := OpenLibraryAt(filepath.Join(dir, "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer lib.Close()

	a, _ := lib.CreateAlbum("A", 0)
	b, _ := lib.CreateAlbum("B", 0)

	// Make B a sub-album of A.
	if err := lib.SetAlbumParent(b, a); err != nil {
		t.Fatal(err)
	}
	albums, _ := lib.Albums()
	for _, al := range albums {
		if al.ID == b && al.ParentID != a {
			t.Fatalf("B parent = %d want %d", al.ParentID, a)
		}
	}

	// Attempting to make A a child of B (its descendant) must be a no-op.
	if err := lib.SetAlbumParent(a, b); err != nil {
		t.Fatal(err)
	}
	albums, _ = lib.Albums()
	for _, al := range albums {
		if al.ID == a && al.ParentID != 0 {
			t.Fatalf("cycle created: A parent = %d, want 0", al.ParentID)
		}
	}
}
