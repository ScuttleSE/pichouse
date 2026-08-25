package ui

import (
	"fmt"
	"path/filepath"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Properties is the right-hand properties panel for the selected photo.
type Properties struct {
	container *fyne.Container
	title     *widget.Label
	location  *widget.Label
	size      *widget.Label
	date      *widget.Label
	dims      *widget.Label
}

func newProperties() *Properties {
	p := &Properties{
		title:    widget.NewLabelWithStyle("Properties", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		location: widget.NewLabel(""),
		size:     widget.NewLabel(""),
		date:     widget.NewLabel(""),
		dims:     widget.NewLabel(""),
	}
	p.location.Wrapping = fyne.TextWrapWord

	form := container.NewVBox(
		p.title,
		widget.NewSeparator(),
		field("Location", p.location),
		field("File Size", p.size),
		field("File Date", p.date),
		field("Dimensions", p.dims),
	)
	p.container = container.NewPadded(form)
	p.Clear()
	return p
}

// field renders a bold caption above a value label.
func field(caption string, value *widget.Label) *fyne.Container {
	return container.NewVBox(
		widget.NewLabelWithStyle(caption, fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		value,
	)
}

// Container returns the properties panel root widget.
func (p *Properties) Container() *fyne.Container { return p.container }

// Clear resets the panel to an empty state.
func (p *Properties) Clear() {
	p.title.SetText("Properties")
	p.location.SetText("—")
	p.size.SetText("—")
	p.date.SetText("—")
	p.dims.SetText("—")
}

// Show populates the panel from a photo.
func (p *Properties) Show(photo model.Photo) {
	p.title.SetText("Properties of " + photo.Filename)
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
