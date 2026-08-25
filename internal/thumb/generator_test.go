package thumb

import (
	"bytes"
	"image"
	"image/color"
	"image/jpeg"
	"os"
	"path/filepath"
	"testing"
)

func writeJPEG(t *testing.T, path string, w, h int) {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for x := 0; x < w; x++ {
		for y := 0; y < h; y++ {
			img.Set(x, y, color.RGBA{uint8(x), uint8(y), 128, 255})
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

func TestGeneratorResizesAndCaches(t *testing.T) {
	dir := t.TempDir()
	// Redirect the data directory (where per-size thumb DBs live) to a temp dir.
	t.Setenv("XDG_DATA_HOME", filepath.Join(dir, "data"))

	src := filepath.Join(dir, "big.jpg")
	writeJPEG(t, src, 800, 400)

	g := New(100)
	defer g.Close()

	blob, err := g.Get("hash1", src, 0)
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	cfg, err := jpeg.DecodeConfig(bytes.NewReader(blob))
	if err != nil {
		t.Fatalf("decode thumb: %v", err)
	}
	if cfg.Width != 100 || cfg.Height != 50 {
		t.Fatalf("want 100x50 thumbnail, got %dx%d", cfg.Width, cfg.Height)
	}

	// Cached copy must come back even if the source is removed.
	if err := os.Remove(src); err != nil {
		t.Fatal(err)
	}
	blob2, err := g.Get("hash1", src, 0)
	if err != nil {
		t.Fatalf("cached get: %v", err)
	}
	if !bytes.Equal(blob, blob2) {
		t.Fatal("cached thumbnail differs from original")
	}
}

func TestGeneratorRotation(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_DATA_HOME", filepath.Join(dir, "data"))

	src := filepath.Join(dir, "wide.jpg")
	writeJPEG(t, src, 800, 400)

	g := New(100)
	defer g.Close()

	// A 90° rotation swaps orientation: a wide image becomes tall.
	blob, err := g.Get("hashrot", src, 90)
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	cfg, err := jpeg.DecodeConfig(bytes.NewReader(blob))
	if err != nil {
		t.Fatalf("decode thumb: %v", err)
	}
	if cfg.Height <= cfg.Width {
		t.Fatalf("want a tall thumbnail after 90° rotation, got %dx%d", cfg.Width, cfg.Height)
	}
}

func TestGeneratorSaveAllSizes(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_DATA_HOME", filepath.Join(dir, "data"))

	src := filepath.Join(dir, "big.jpg")
	writeJPEG(t, src, 800, 400)

	g := New(160)
	g.SetAllSizes([]int{96, 160, 240})
	defer g.Close()

	if _, err := g.Get("hashall", src, 0); err != nil {
		t.Fatalf("get: %v", err)
	}
	// All three per-size databases should now exist.
	for _, sz := range []int{96, 160, 240} {
		g.SetSize(sz)
		if _, err := g.Get("hashall", src, 0); err != nil {
			t.Fatalf("get size %d: %v", sz, err)
		}
	}
}
