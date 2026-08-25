// Package thumb generates and caches image thumbnails.
package thumb

import (
	"bytes"
	"fmt"
	"image"
	_ "image/gif"
	"image/jpeg"
	_ "image/png"
	"os"
	"sync"

	"golang.org/x/image/draw"
	_ "golang.org/x/image/bmp"
	_ "golang.org/x/image/tiff"
	_ "golang.org/x/image/webp"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// DefaultSize is the default maximum thumbnail dimension in pixels.
const DefaultSize = 256

// Generator produces thumbnails and caches them in the thumbs database.
type Generator struct {
	thumbs *db.Thumbs
	size   int
}

// New returns a Generator that caches into thumbs, producing thumbnails whose
// longest side is at most size pixels (size <= 0 uses DefaultSize).
func New(thumbs *db.Thumbs, size int) *Generator {
	if size <= 0 {
		size = DefaultSize
	}
	return &Generator{thumbs: thumbs, size: size}
}

// Get returns a cached JPEG thumbnail for the photo identified by hash,
// generating and caching it from srcPath on a cache miss.
func (g *Generator) Get(hash, srcPath string) ([]byte, error) {
	if hash != "" {
		if blob, ok, err := g.thumbs.Get(hash); err != nil {
			return nil, err
		} else if ok {
			return blob, nil
		}
	}
	blob, err := g.render(srcPath)
	if err != nil {
		return nil, err
	}
	if hash != "" {
		if err := g.thumbs.Put(hash, g.size, blob); err != nil {
			return nil, err
		}
	}
	return blob, nil
}

// render decodes, resizes, and JPEG-encodes an image file.
func (g *Generator) render(srcPath string) ([]byte, error) {
	f, err := os.Open(srcPath)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	src, _, err := image.Decode(f)
	if err != nil {
		return nil, fmt.Errorf("decode %s: %w", srcPath, err)
	}
	dst := resize(src, g.size)
	var buf bytes.Buffer
	if err := jpeg.Encode(&buf, dst, &jpeg.Options{Quality: 85}); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// resize scales img so its longest side is at most maxSide, preserving aspect
// ratio. Images already within bounds are returned scaled to themselves.
func resize(img image.Image, maxSide int) image.Image {
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	if w == 0 || h == 0 {
		return img
	}
	nw, nh := w, h
	if w >= h && w > maxSide {
		nw = maxSide
		nh = h * maxSide / w
	} else if h > w && h > maxSide {
		nh = maxSide
		nw = w * maxSide / h
	} else if w == h && w > maxSide {
		nw, nh = maxSide, maxSide
	}
	if nw < 1 {
		nw = 1
	}
	if nh < 1 {
		nh = 1
	}
	dst := image.NewRGBA(image.Rect(0, 0, nw, nh))
	draw.CatmullRom.Scale(dst, dst.Bounds(), img, b, draw.Over, nil)
	return dst
}

// Job requests a thumbnail for one photo.
type Job struct {
	Hash string
	Path string
}

// Result is the outcome of a thumbnail Job.
type Result struct {
	Job  Job
	JPEG []byte
	Err  error
}

// Pool runs thumbnail jobs concurrently across n workers. Results are delivered
// on the returned channel; it is closed when jobs is drained and closed.
func (g *Generator) Pool(n int, jobs <-chan Job) <-chan Result {
	if n < 1 {
		n = 1
	}
	results := make(chan Result)
	var wg sync.WaitGroup
	wg.Add(n)
	for i := 0; i < n; i++ {
		go func() {
			defer wg.Done()
			for j := range jobs {
				blob, err := g.Get(j.Hash, j.Path)
				results <- Result{Job: j, JPEG: blob, Err: err}
			}
		}()
	}
	go func() {
		wg.Wait()
		close(results)
	}()
	return results
}
