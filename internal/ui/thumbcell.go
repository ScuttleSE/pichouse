package ui

import (
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// thumbCell is a thumbnail image with a filename caption, used as the recycled
// child widget of the grid's list-item factory.
type thumbCell struct {
	*gtk.Box
	picture *gtk.Picture
	caption *gtk.Label
}

func newThumbCell(edge int) *thumbCell {
	c := &thumbCell{}
	c.picture = gtk.NewPicture()
	c.picture.SetCanShrink(true)
	c.picture.SetContentFit(gtk.ContentFitContain)
	c.picture.SetSizeRequest(edge, edge)

	c.caption = gtk.NewLabel("")
	c.caption.SetEllipsize(pango.EllipsizeEnd)
	c.caption.SetMaxWidthChars(1)
	c.caption.SetXAlign(0.5)

	box := gtk.NewBox(gtk.OrientationVertical, 2)
	box.SetSizeRequest(edge, -1)
	box.SetMarginTop(4)
	box.SetMarginBottom(4)
	box.SetMarginStart(4)
	box.SetMarginEnd(4)
	box.Append(c.picture)
	box.Append(c.caption)
	c.Box = box
	return c
}

func (c *thumbCell) setCaption(s string) { c.caption.SetText(s) }

func (c *thumbCell) setPixbuf(pb *gdkpixbuf.Pixbuf) {
	c.picture.SetPixbuf(pb)
}

// setPlaceholder clears the image while a thumbnail loads.
func (c *thumbCell) setPlaceholder() {
	c.picture.SetPaintable(nil)
}
