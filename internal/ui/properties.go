package ui

import (
	"fmt"
	"path/filepath"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Properties is the right-hand properties panel for the selected photo. It is
// a tabbed panel; the "Pic Info" tab shows file/EXIF details, and more tabs can
// be added later.
type Properties struct {
	root     *gtk.Notebook
	title    *gtk.Label
	location *gtk.Label
	size     *gtk.Label
	date     *gtk.Label
	dims     *gtk.Label
}

func newProperties() *Properties {
	p := &Properties{}
	p.title = boldLabel("Properties")
	p.location = valueLabel()
	p.size = valueLabel()
	p.date = valueLabel()
	p.dims = valueLabel()

	box := gtk.NewBox(gtk.OrientationVertical, 4)
	box.SetMarginTop(8)
	box.SetMarginBottom(8)
	box.SetMarginStart(8)
	box.SetMarginEnd(8)
	box.Append(p.title)
	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	box.Append(field("Location", p.location))
	box.Append(field("File Size", p.size))
	box.Append(field("File Date", p.date))
	box.Append(field("Dimensions", p.dims))

	notebook := gtk.NewNotebook()
	notebook.SetSizeRequest(240, -1)
	notebook.AppendPage(box, gtk.NewLabel("Pic Info"))
	p.root = notebook

	p.Clear()
	return p
}

func boldLabel(text string) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.SetMarkup("<b>" + text + "</b>")
	return l
}

func valueLabel() *gtk.Label {
	l := gtk.NewLabel("")
	l.SetXAlign(0)
	l.SetWrap(true)
	l.SetSelectable(true)
	return l
}

func field(caption string, value *gtk.Label) *gtk.Box {
	b := gtk.NewBox(gtk.OrientationVertical, 0)
	b.SetMarginTop(6)
	b.Append(boldLabel(caption))
	b.Append(value)
	return b
}

// Widget returns the properties panel root widget.
func (p *Properties) Widget() gtk.Widgetter { return p.root }

// SetVisible shows or hides the panel.
func (p *Properties) SetVisible(v bool) { p.root.SetVisible(v) }

// Clear resets the panel to an empty state.
func (p *Properties) Clear() {
	p.title.SetMarkup("<b>Properties</b>")
	p.location.SetText("—")
	p.size.SetText("—")
	p.date.SetText("—")
	p.dims.SetText("—")
}

// Show populates the panel from a photo.
func (p *Properties) Show(photo model.Photo) {
	p.title.SetMarkup("<b>Properties of " + escapeMarkup(photo.Filename) + "</b>")
	p.location.SetText(filepath.Dir(photo.Path))
	p.size.SetText(humanSize(photo.Size))
	if photo.TakenAt.IsZero() {
		p.date.SetText(photo.ModTime.Format("2006-01-02 15:04:05"))
	} else {
		p.date.SetText(photo.TakenAt.Format("2006-01-02 15:04:05"))
	}
	if photo.Width > 0 && photo.Height > 0 {
		p.dims.SetText(fmt.Sprintf("%d x %d", photo.Width, photo.Height))
	} else {
		p.dims.SetText("—")
	}
}

// humanSize formats a byte count as a human-readable string.
func humanSize(n int64) string {
	const unit = 1024
	if n < unit {
		return fmt.Sprintf("%d B", n)
	}
	div, exp := int64(unit), 0
	for m := n / unit; m >= unit; m /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %cB", float64(n)/float64(div), "KMGTPE"[exp])
}
