package ui

import (
	"strconv"

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
	body      *widget.Label

	thumbSize int
	filter    string
	folder    *model.Folder
}

func newGrid(a *App) *Grid {
	g := &Grid{
		app:       a,
		thumbSize: thumbGridSize,
		header:    widget.NewLabelWithStyle("", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		body:      widget.NewLabel("Select a folder to view photos."),
	}
	g.container = container.NewBorder(
		container.NewVBox(g.header, widget.NewSeparator()),
		nil, nil, nil,
		container.NewScroll(g.body),
	)
	return g
}

// Container returns the grid root widget.
func (g *Grid) Container() *fyne.Container { return g.container }

// SetThumbSize updates the thumbnail edge length and refreshes the grid.
func (g *Grid) SetThumbSize(px int) {
	g.thumbSize = px
	g.reload()
}

// SetFilter applies a text filter to the current folder's photos.
func (g *Grid) SetFilter(q string) {
	g.filter = q
	g.reload()
}

// ShowFolder loads and displays the photos of a folder.
func (g *Grid) ShowFolder(f model.Folder) {
	g.folder = &f
	g.header.SetText(f.Name)
	g.reload()
}

// reload refreshes the grid body from the current folder/filter/size.
func (g *Grid) reload() {
	if g.folder == nil {
		g.body.SetText("Select a folder to view photos.")
		return
	}
	photos, err := g.app.lib.PhotosInFolder(g.folder.ID)
	if err != nil {
		g.body.SetText("Error loading photos.")
		return
	}
	g.body.SetText(g.folder.Name + ": " + strconv.Itoa(len(photos)) + " photos")
}
