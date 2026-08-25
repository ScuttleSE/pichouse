package ui

import (
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// thumbGridSize is the default thumbnail edge in the grid, in pixels.
const thumbGridSize = 160

// Grid is the center thumbnail grid plus its folder header.
type Grid struct {
	app       *App
	container *fyne.Container
	header    *widget.Label
	scroll    *container.Scroll

	thumbSize int
	filter    string
	folder    *model.Folder

	photos   []model.Photo // filtered photos currently shown
	gridWrap *widget.GridWrap
}

func newGrid(a *App) *Grid {
	g := &Grid{
		app:       a,
		thumbSize: thumbGridSize,
		header:    widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
	}
	g.gridWrap = widget.NewGridWrap(
		func() int { return len(g.photos) },
		g.createItem,
		g.updateItem,
	)
	g.scroll = container.NewScroll(g.gridWrap)
	g.container = container.NewBorder(
		container.NewVBox(g.header, widget.NewSeparator()),
		nil, nil, nil,
		g.scroll,
	)
	return g
}

// Container returns the grid root widget.
func (g *Grid) Container() *fyne.Container { return g.container }

// SetThumbSize updates the thumbnail edge length and refreshes the grid.
func (g *Grid) SetThumbSize(px int) {
	g.thumbSize = px
	g.rebuild()
}

// SetFilter applies a case-insensitive filename filter.
func (g *Grid) SetFilter(q string) {
	g.filter = strings.ToLower(strings.TrimSpace(q))
	g.reload()
}

// ShowFolder loads and displays the photos of a folder.
func (g *Grid) ShowFolder(f model.Folder) {
	g.folder = &f
	dateStr := ""
	if !f.MTime.IsZero() {
		dateStr = "   " + f.MTime.Format("Jan 2, 2006")
	}
	g.header.SetText(f.Name + dateStr)
	g.reload()
}

// reload re-queries photos for the current folder, applies the filter, and
// refreshes the grid.
func (g *Grid) reload() {
	g.photos = nil
	if g.folder != nil {
		all, err := g.app.lib.PhotosInFolder(g.folder.ID)
		if err == nil {
			for _, p := range all {
				if g.filter == "" || strings.Contains(strings.ToLower(p.Filename), g.filter) {
					g.photos = append(g.photos, p)
				}
			}
		}
	}
	g.gridWrap.Refresh()
	g.updateHeaderCount()
}

// rebuild recreates the grid template when the thumbnail size changes.
func (g *Grid) rebuild() {
	// GridWrap sizes items from the template's MinSize; recreate to resize.
	g.gridWrap = widget.NewGridWrap(
		func() int { return len(g.photos) },
		g.createItem,
		g.updateItem,
	)
	g.scroll.Content = g.gridWrap
	g.scroll.Refresh()
}

func (g *Grid) updateHeaderCount() {
	if g.folder == nil {
		return
	}
	base := g.folder.Name
	if !g.folder.MTime.IsZero() {
		base += "   " + g.folder.MTime.Format("Jan 2, 2006")
	}
	g.header.SetText(base + "   (" + strconv.Itoa(len(g.photos)) + ")")
}

// createItem builds a reusable thumbnail cell.
func (g *Grid) createItem() fyne.CanvasObject {
	return newThumbCell(float32(g.thumbSize))
}

// updateItem binds a photo to a cell and kicks off async thumbnail loading.
func (g *Grid) updateItem(id widget.GridWrapItemID, obj fyne.CanvasObject) {
	cell := obj.(*thumbCell)
	if id < 0 || id >= len(g.photos) {
		return
	}
	p := g.photos[id]
	cell.setCaption(p.Filename)
	cell.setPlaceholder()
	cell.onTapped = func() { g.app.selectPhoto(p) }

	// Load the thumbnail asynchronously; swap it in on the UI thread.
	go func(photo model.Photo, c *thumbCell) {
		blob, err := g.app.gen.Get(photo.Hash, photo.Path)
		if err != nil || len(blob) == 0 {
			return
		}
		res := fyne.NewStaticResource(photo.Hash+".jpg", blob)
		fyne.Do(func() {
			// Only apply if the cell still shows this photo.
			if c.caption.Text == photo.Filename {
				c.setImage(res)
			}
		})
	}(p, cell)
}
