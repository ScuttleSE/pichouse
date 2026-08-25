package ui

import (
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
	"git.hemmalab.se/scuttle/pichouse/internal/scan"
)

// thumbWorkers bounds how many thumbnails are generated concurrently. The
// thumbnail cache is SQLite-backed; too many concurrent writers cause
// "database is locked" errors, so keep this modest.
const thumbWorkers = 4

// debugThumbs enables verbose thumbnail load logging when true.
var debugThumbs = true

// Grid is the center thumbnail grid plus its folder header.
type Grid struct {
	app    *App
	box    *gtk.Box
	header *gtk.Label

	gridView  *gtk.GridView
	model     *gtk.StringList
	selection *gtk.SingleSelection

	thumbSize int
	filter    string
	folder    *model.Folder
	rawMode   bool
	rawDir    string

	// tagMatches holds photo ids whose tags match the current filter. It is
	// nil when the filter is empty (meaning "no tag constraint").
	tagMatches map[int64]bool

	photos []model.Photo // filtered photos currently shown

	// generation counter; async thumbnail results from an older load are
	// discarded when the grid content changes.
	generation uint64

	// thumbnail worker pool
	jobs     chan thumbJob
	poolOnce sync.Once
}

// thumbJob is a request to generate/fetch a thumbnail for one photo and apply
// it to a specific cell.
type thumbJob struct {
	parts thumbCellParts
	photo model.Photo
	key   string
	gen   uint64
}

func newGrid(a *App) *Grid {
	g := &Grid{app: a, thumbSize: a.prefs.sizes[a.prefs.active]}

	g.header = gtk.NewLabel("")
	g.header.SetXAlign(0)
	g.header.SetMarginStart(8)
	g.header.SetMarginTop(6)
	g.header.SetMarginBottom(6)

	g.model = gtk.NewStringList(nil)
	g.selection = gtk.NewSingleSelection(g.model)
	g.selection.SetAutoselect(false)
	g.selection.SetCanUnselect(true)

	factory := gtk.NewSignalListItemFactory()
	factory.ConnectSetup(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		item.SetChild(newThumbCellWidget(g.thumbSize))
	})
	factory.ConnectBind(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		box, ok := item.Child().(*gtk.Box)
		if !ok {
			return
		}
		parts, ok := cellParts(box)
		if !ok {
			return
		}
		pos := int(item.Position())
		if pos < 0 || pos >= len(g.photos) {
			return
		}
		g.bindCell(parts, g.photos[pos])
	})

	g.gridView = gtk.NewGridView(g.selection, &factory.ListItemFactory)
	g.gridView.SetMaxColumns(20)
	g.gridView.SetMinColumns(1)
	g.gridView.ConnectActivate(func(pos uint) {
		if int(pos) < len(g.photos) {
			g.app.OpenViewer(g.photos, int(pos))
		}
	})
	g.selection.ConnectSelectionChanged(func(uint, uint) {
		pos := int(g.selection.Selected())
		if pos >= 0 && pos < len(g.photos) {
			g.app.selectPhoto(g.photos[pos])
		}
	})

	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetHExpand(true)
	scroll.SetChild(g.gridView)

	g.box = gtk.NewBox(gtk.OrientationVertical, 0)
	g.box.Append(g.header)
	g.box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	g.box.Append(scroll)
	return g
}

// Widget returns the grid root widget.
func (g *Grid) Widget() gtk.Widgetter { return g.box }

// SetThumbSize updates the thumbnail edge length and rebuilds the grid. The
// generator's active size is aligned so each size uses its own cache database.
func (g *Grid) SetThumbSize(px int) {
	g.thumbSize = px
	g.app.gen.SetSize(px)
	g.reload()
}

// SetFilter applies a case-insensitive filter matching filename or tags.
func (g *Grid) SetFilter(q string) {
	g.filter = strings.ToLower(strings.TrimSpace(q))
	if g.filter == "" {
		g.tagMatches = nil
	} else if ids, err := g.app.lib.SearchPhotoIDsByTag(g.filter); err == nil {
		g.tagMatches = ids
	} else {
		g.tagMatches = nil
	}
	g.reload()
}

// RefreshVisible reloads the current source so newly scanned photos appear.
func (g *Grid) RefreshVisible() {
	if g.folder != nil || g.rawMode {
		g.reload()
	}
}

// ShowFolder loads and displays the photos of a library folder.
func (g *Grid) ShowFolder(f model.Folder) {
	g.rawMode = false
	g.folder = &f
	dateStr := ""
	if !f.MTime.IsZero() {
		dateStr = "   " + f.MTime.Format("Jan 2, 2006")
	}
	g.header.SetText(f.Name + dateStr)
	g.reload()
}

// ShowRawFolder displays images read live from a filesystem directory.
func (g *Grid) ShowRawFolder(dir string) {
	g.rawMode = true
	g.rawDir = dir
	g.folder = nil
	g.header.SetText("Folder view: " + dir)
	g.reload()
}

// reload rebuilds photos for the current source, applies the filter, and
// refreshes the grid model.
func (g *Grid) reload() {
	g.generation++
	g.photos = nil
	switch {
	case g.rawMode:
		g.loadRaw()
	case g.folder != nil:
		all, err := g.app.lib.PhotosInFolder(g.folder.ID)
		if err == nil {
			for _, p := range all {
				if g.matches(p) {
					g.photos = append(g.photos, p)
				}
			}
		}
	}
	g.resetModel()
	g.updateHeaderCount()
}

// resetModel resizes the StringList model to match len(photos). The strings
// themselves are unused; binding indexes g.photos by ListItem position.
func (g *Grid) resetModel() {
	n := g.model.NItems()
	adds := make([]string, len(g.photos))
	g.model.Splice(0, n, adds)
}

// loadRaw reads image files directly from the raw directory, reusing scanned
// content hashes so cached thumbnails are served instead of re-rendering.
func (g *Grid) loadRaw() {
	entries, err := os.ReadDir(g.rawDir)
	if err != nil {
		return
	}
	hashes, _ := g.app.lib.HashesByDir(g.rawDir)
	for _, e := range entries {
		if e.IsDir() || !scan.IsImage(e.Name()) {
			continue
		}
		full := filepath.Join(g.rawDir, e.Name())
		p := model.Photo{Path: full, Filename: e.Name()}
		if info, err := e.Info(); err == nil {
			p.Size = info.Size()
			p.ModTime = info.ModTime()
		}
		if h, ok := hashes[full]; ok {
			p.Hash = h
		}
		if !g.matches(p) {
			continue
		}
		g.photos = append(g.photos, p)
	}
}

// matches reports whether a photo passes the current filter. It matches when
// the filter is empty, the filename contains the query, or the photo's tags
// match (via the FTS-backed tagMatches set built in SetFilter).
func (g *Grid) matches(p model.Photo) bool {
	if g.filter == "" {
		return true
	}
	if strings.Contains(strings.ToLower(p.Filename), g.filter) {
		return true
	}
	return p.ID != 0 && g.tagMatches[p.ID]
}

func (g *Grid) updateHeaderCount() {
	count := " (" + itoa(len(g.photos)) + ")"
	base := g.header.Text()
	// Strip any previous count suffix.
	if i := strings.LastIndex(base, " ("); i >= 0 {
		base = base[:i]
	}
	g.header.SetText(base + count)
}

// bindCell shows a photo in a cell and enqueues async thumbnail loading. The
// cache key is the content hash, falling back to the file path. The key is
// stored on the picture widget's name so a late async result is discarded if
// the cell has since been recycled for a different photo.
func (g *Grid) bindCell(parts thumbCellParts, p model.Photo) {
	parts.setCaption(p.Filename)
	parts.setPlaceholder()

	key := p.Hash
	if key == "" {
		key = p.Path
	}
	// Include the active size and orientation so a slider change or a rotation
	// does not serve a stale in-memory blob.
	key = key + "|" + itoa(g.thumbSize) + "|" + itoa(p.Orientation)
	parts.picture.SetName(key)
	gen := g.generation

	if blob, ok := g.app.thumbCache.Get(key); ok {
		g.applyThumb(parts, key, blob, gen)
		return
	}

	g.ensurePool()
	g.jobs <- thumbJob{parts: parts, photo: p, key: key, gen: gen}
}

// ensurePool lazily starts the thumbnail worker pool.
func (g *Grid) ensurePool() {
	g.poolOnce.Do(func() {
		g.jobs = make(chan thumbJob, 256)
		for i := 0; i < thumbWorkers; i++ {
			go g.thumbWorker()
		}
	})
}

// thumbWorker processes thumbnail jobs, bounding concurrent SQLite writes.
func (g *Grid) thumbWorker() {
	for job := range g.jobs {
		// Skip stale jobs cheaply before doing any work.
		if job.gen != g.generation {
			continue
		}
		blob, err := g.app.gen.Get(job.photo.Hash, job.photo.Path, job.photo.Orientation)
		if err != nil {
			if debugThumbs {
				log.Printf("[thumb] generate FAILED %s (hash=%q): %v", job.photo.Path, job.photo.Hash, err)
			}
			continue
		}
		if len(blob) == 0 {
			if debugThumbs {
				log.Printf("[thumb] empty blob for %s (hash=%q)", job.photo.Path, job.photo.Hash)
			}
			continue
		}
		g.app.thumbCache.Put(job.key, blob)
		j := job
		onUI(func() { g.applyThumb(j.parts, j.key, blob, j.gen) })
	}
}

// applyThumb decodes JPEG bytes into a pixbuf and sets it on the cell, unless
// the grid content changed or the cell was recycled for a different photo.
func (g *Grid) applyThumb(parts thumbCellParts, key string, blob []byte, gen uint64) {
	if gen != g.generation {
		return
	}
	if parts.picture.Name() != key {
		return // cell recycled for a different photo
	}
	loader := gdkpixbuf.NewPixbufLoader()
	if err := loader.Write(blob); err != nil {
		if debugThumbs {
			log.Printf("[thumb] loader.Write failed for key=%q: %v", key, err)
		}
		loader.Close()
		return
	}
	if err := loader.Close(); err != nil {
		if debugThumbs {
			log.Printf("[thumb] loader.Close failed for key=%q: %v", key, err)
		}
		return
	}
	pb := loader.Pixbuf()
	if pb == nil {
		if debugThumbs {
			log.Printf("[thumb] nil pixbuf after decode for key=%q", key)
		}
		return
	}
	parts.setPixbuf(pb)
	if debugThumbs {
		log.Printf("[thumb] OK key=%q size=%dx%d", key, pb.Width(), pb.Height())
	}
}

// itoa is a tiny int-to-string helper avoiding a strconv import churn.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	var b [20]byte
	i := len(b)
	for n > 0 {
		i--
		b[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		b[i] = '-'
	}
	return string(b[i:])
}
