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

	_ "golang.org/x/image/bmp"
	"golang.org/x/image/draw"
	_ "golang.org/x/image/tiff"
	_ "golang.org/x/image/webp"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
)

// DefaultSize is the default maximum thumbnail dimension in pixels.
const DefaultSize = 320

// Generator produces thumbnails and caches them in per-size thumbnail
// databases. Each thumbnail size uses its own database file, so switching
// quality never overwrites another size's cache. Set AllSizes to pre-generate
// every size on a cache miss.
type Generator struct {
	mu       sync.Mutex
	size     int
	allSizes []int
	stores   map[int]*db.Thumbs
}

// New returns a Generator caching into per-size databases. size selects the
// active thumbnail size (size <= 0 uses DefaultSize).
func New(size int) *Generator {
	if size <= 0 {
		size = DefaultSize
	}
	return &Generator{size: size, stores: map[int]*db.Thumbs{}}
}

// SetSize selects the active thumbnail size used by Get.
func (g *Generator) SetSize(size int) {
	if size <= 0 {
		size = DefaultSize
	}
	g.mu.Lock()
	g.size = size
	g.mu.Unlock()
}

// Size returns the active thumbnail size.
func (g *Generator) Size() int {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.size
}

// SetAllSizes sets the sizes to pre-generate on a cache miss. Pass nil to
// disable pre-generation.
func (g *Generator) SetAllSizes(sizes []int) {
	g.mu.Lock()
	g.allSizes = sizes
	g.mu.Unlock()
}

// store returns (opening if necessary) the per-size thumbnail database.
func (g *Generator) store(size int) (*db.Thumbs, error) {
	g.mu.Lock()
	defer g.mu.Unlock()
	if s, ok := g.stores[size]; ok {
		return s, nil
	}
	s, err := db.OpenThumbsForSize(size)
	if err != nil {
		return nil, err
	}
	g.stores[size] = s
	return s, nil
}

// Close closes all open per-size stores.
func (g *Generator) Close() {
	g.mu.Lock()
	defer g.mu.Unlock()
	for _, s := range g.stores {
		s.Close()
	}
	g.stores = map[int]*db.Thumbs{}
}

// ClearAll closes all stores and deletes every thumbnail database file.
func (g *Generator) ClearAll() error {
	g.Close()
	return db.RemoveAllThumbDatabases()
}

// Get returns a cached JPEG thumbnail for the photo identified by hash at the
// active size, applying the given rotation (degrees clockwise). On a cache miss
// it renders from srcPath and caches the result. When AllSizes is set, all
// configured sizes are pre-generated.
func (g *Generator) Get(hash, srcPath string, rotation int) ([]byte, error) {
	g.mu.Lock()
	active := g.size
	all := g.allSizes
	g.mu.Unlock()

	if hash != "" {
		store, err := g.store(active)
		if err != nil {
			return nil, err
		}
		if blob, ok, err := store.Get(hash); err != nil {
			return nil, err
		} else if ok {
			return blob, nil
		}
	}

	// Decode once, then produce every requested size.
	src, err := decode(srcPath)
	if err != nil {
		return nil, err
	}
	src = rotate(src, rotation)

	sizes := all
	if len(sizes) == 0 {
		sizes = []int{active}
	}
	var activeBlob []byte
	for _, sz := range sizes {
		blob, err := encode(src, sz)
		if err != nil {
			return nil, err
		}
		if sz == active {
			activeBlob = blob
		}
		if hash != "" {
			store, err := g.store(sz)
			if err != nil {
				return nil, err
			}
			if err := store.Put(hash, sz, blob); err != nil {
				return nil, err
			}
		}
	}
	if activeBlob == nil {
		// Active size not among all-sizes: produce it directly.
		activeBlob, err = encode(src, active)
		if err != nil {
			return nil, err
		}
		if hash != "" {
			if store, err := g.store(active); err == nil {
				_ = store.Put(hash, active, activeBlob)
			}
		}
	}
	return activeBlob, nil
}

// Invalidate removes cached thumbnails for a hash across all open sizes. Used
// when a photo's rotation changes.
func (g *Generator) Invalidate(hash string) error {
	if hash == "" {
		return nil
	}
	g.mu.Lock()
	stores := make([]*db.Thumbs, 0, len(g.stores))
	for _, s := range g.stores {
		stores = append(stores, s)
	}
	g.mu.Unlock()
	for _, s := range stores {
		if err := s.Delete(hash); err != nil {
			return err
		}
	}
	return nil
}

// decode reads and decodes an image file.
func decode(srcPath string) (image.Image, error) {
	f, err := os.Open(srcPath)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	src, _, err := image.Decode(f)
	if err != nil {
		return nil, fmt.Errorf("decode %s: %w", srcPath, err)
	}
	return src, nil
}

// encode resizes src to maxSide and JPEG-encodes it.
func encode(src image.Image, maxSide int) ([]byte, error) {
	dst := resize(src, maxSide)
	var buf bytes.Buffer
	if err := jpeg.Encode(&buf, dst, &jpeg.Options{Quality: 85}); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// rotate returns img rotated clockwise by the given degrees (0/90/180/270).
func rotate(img image.Image, degrees int) image.Image {
	degrees = ((degrees % 360) + 360) % 360
	if degrees == 0 {
		return img
	}
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	switch degrees {
	case 90:
		dst := image.NewRGBA(image.Rect(0, 0, h, w))
		for y := 0; y < h; y++ {
			for x := 0; x < w; x++ {
				dst.Set(h-1-y, x, img.At(b.Min.X+x, b.Min.Y+y))
			}
		}
		return dst
	case 180:
		dst := image.NewRGBA(image.Rect(0, 0, w, h))
		for y := 0; y < h; y++ {
			for x := 0; x < w; x++ {
				dst.Set(w-1-x, h-1-y, img.At(b.Min.X+x, b.Min.Y+y))
			}
		}
		return dst
	case 270:
		dst := image.NewRGBA(image.Rect(0, 0, h, w))
		for y := 0; y < h; y++ {
			for x := 0; x < w; x++ {
				dst.Set(y, w-1-x, img.At(b.Min.X+x, b.Min.Y+y))
			}
		}
		return dst
	}
	return img
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
