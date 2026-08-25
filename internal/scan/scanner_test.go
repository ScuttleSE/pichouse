package scan

import (
	"context"
	"errors"
	"image"
	"image/color"
	"image/jpeg"
	"os"
	"path/filepath"
	"testing"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

func writeJPEG(t *testing.T, path string, w, h int) {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for x := 0; x < w; x++ {
		for y := 0; y < h; y++ {
			img.Set(x, y, color.RGBA{uint8(x), uint8(y), 0, 255})
		}
	}
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if err := jpeg.Encode(f, img, nil); err != nil {
		t.Fatal(err)
	}
}

func TestScanFolder(t *testing.T) {
	root := t.TempDir()
	sub := filepath.Join(root, "trip")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	writeJPEG(t, filepath.Join(sub, "a.jpg"), 32, 16)
	writeJPEG(t, filepath.Join(sub, "b.jpg"), 20, 20)
	// A non-image file must be ignored.
	if err := os.WriteFile(filepath.Join(sub, "notes.txt"), []byte("hi"), 0o644); err != nil {
		t.Fatal(err)
	}

	lib, err := db.OpenLibraryAt(filepath.Join(root, "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer lib.Close()

	s := New(lib)
	n, err := s.ScanFolder(root, nil)
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	if n != 2 {
		t.Fatalf("want 2 photos, got %d", n)
	}

	folders, err := lib.Folders()
	if err != nil {
		t.Fatal(err)
	}
	if len(folders) != 1 || folders[0].Name != "trip" {
		t.Fatalf("unexpected folders: %+v", folders)
	}
	photos, err := lib.PhotosInFolder(folders[0].ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(photos) != 2 {
		t.Fatalf("want 2 photos in folder, got %d", len(photos))
	}
	for _, p := range photos {
		if p.Width == 0 || p.Height == 0 {
			t.Errorf("photo %s missing dimensions", p.Filename)
		}
		if p.Hash == "" {
			t.Errorf("photo %s missing hash", p.Filename)
		}
	}
}

func TestScanFolderContextCancel(t *testing.T) {
	root := t.TempDir()
	for i := 0; i < 5; i++ {
		writeJPEG(t, filepath.Join(root, image.Pt(i, 0).String()+".jpg"), 16, 16)
	}
	lib, err := db.OpenLibraryAt(filepath.Join(root, "library.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer lib.Close()

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel before scanning starts
	s := New(lib)
	_, err = s.ScanFolderContext(ctx, root, nil)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("want context.Canceled, got %v", err)
	}
}

func TestIsImage(t *testing.T) {
	cases := map[string]bool{
		"a.jpg": true, "b.JPEG": true, "c.png": true,
		"d.txt": false, "e": false, "f.mp4": false,
	}
	for name, want := range cases {
		if got := IsImage(name); got != want {
			t.Errorf("IsImage(%q)=%v want %v", name, got, want)
		}
	}
}
