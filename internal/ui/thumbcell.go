package ui

import (
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// newThumbCellWidget builds the recycled child widget for a grid item: a
// vertical box containing a Picture above an ellipsized caption label. The
// widgets are retrieved later by walking the box's children (GTK hands back the
// bare *gtk.Box, not a Go wrapper type).
func newThumbCellWidget(edge int) *gtk.Box {
	picture := gtk.NewPicture()
	picture.SetCanShrink(true)
	picture.SetContentFit(gtk.ContentFitContain)
	picture.SetSizeRequest(edge, edge)

	caption := gtk.NewLabel("")
	caption.SetEllipsize(pango.EllipsizeEnd)
	caption.SetMaxWidthChars(1)
	caption.SetXAlign(0.5)

	box := gtk.NewBox(gtk.OrientationVertical, 2)
	box.SetSizeRequest(edge, -1)
	box.SetMarginTop(4)
	box.SetMarginBottom(4)
	box.SetMarginStart(4)
	box.SetMarginEnd(4)
	box.Append(picture)
	box.Append(caption)
	return box
}

// thumbCellParts holds the Picture and Label extracted from a cell box.
type thumbCellParts struct {
	picture *gtk.Picture
	caption *gtk.Label
}

// cellParts extracts the Picture and Label from a cell box built by
// newThumbCellWidget. Returns ok=false if the structure is unexpected.
func cellParts(box *gtk.Box) (thumbCellParts, bool) {
	var p thumbCellParts
	first := box.FirstChild()
	last := box.LastChild()
	if pic, ok := first.(*gtk.Picture); ok {
		p.picture = pic
	}
	if lbl, ok := last.(*gtk.Label); ok {
		p.caption = lbl
	}
	if p.picture == nil || p.caption == nil {
		return p, false
	}
	return p, true
}

func (p thumbCellParts) setCaption(s string)            { p.caption.SetText(s) }
func (p thumbCellParts) setPixbuf(pb *gdkpixbuf.Pixbuf) { p.picture.SetPixbuf(pb) }
func (p thumbCellParts) setPlaceholder()                { p.picture.SetPaintable(nil) }
