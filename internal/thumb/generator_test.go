package thumb

import (
	"bytes"
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
	src := filepath.Join(dir, "big.jpg")
	writeJPEG(t, src, 800, 400)

	th, err := db.OpenThumbsAt(filepath.Join(dir, "thumbs.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer th.Close()

	g := New(th, 100)
	blob, err := g.Get("hash1", src)
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
	blob2, err := g.Get("hash1", src)
	if err != nil {
		t.Fatalf("cached get: %v", err)
	}
	if !bytes.Equal(blob, blob2) {
		t.Fatal("cached thumbnail differs from original")
	}
}

func TestPool(t *testing.T) {
	dir := t.TempDir()
	th, err := db.OpenThumbsAt(filepath.Join(dir, "thumbs.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer th.Close()
	g := New(th, 64)

	jobs := make(chan Job)
	results := g.Pool(3, jobs)

	go func() {
		for i := 0; i < 5; i++ {
			p := filepath.Join(dir, image.Pt(i, 0).String()+".jpg")
			writeJPEG(t, p, 128, 128)
			jobs <- Job{Hash: p, Path: p}
		}
		close(jobs)
	}()

	count := 0
	for r := range results {
		if r.Err != nil {
			t.Errorf("job %s: %v", r.Job.Path, r.Err)
		}
		count++
	}
	if count != 5 {
		t.Fatalf("want 5 results, got %d", count)
	}
}
