package ui

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// thumbPresets are the snap positions (in px) for the thumbnail-size slider.
var thumbPresets = []int{96, 160, 240, 320}

// Toolbar is the top toolbar: settings, rescan, search, and a thumbnail-size
// slider with preset snap positions.
type Toolbar struct {
	app *App
	box *gtk.Box
}

func newToolbar(a *App) *Toolbar {
	t := &Toolbar{app: a}

	settings := gtk.NewButtonFromIconName("emblem-system-symbolic")
	settings.SetTooltipText("Settings")
	settings.ConnectClicked(func() { a.ShowSettings() })

	rescan := gtk.NewButtonFromIconName("view-refresh-symbolic")
	rescan.SetTooltipText("Rescan all library folders")
	rescan.ConnectClicked(func() { a.RescanAll() })

	search := gtk.NewSearchEntry()
	search.SetHExpand(true)
	search.ConnectSearchChanged(func() {
		a.grid.SetFilter(search.Text())
	})

	// Slider snaps to preset indices; value maps into thumbPresets.
	slider := gtk.NewScaleWithRange(gtk.OrientationHorizontal, 0, float64(len(thumbPresets)-1), 1)
	slider.SetDrawValue(false)
	slider.SetDigits(0)
	slider.SetSizeRequest(160, -1)
	slider.SetValue(float64(presetIndex(thumbGridSize)))
	for i := range thumbPresets {
		slider.AddMark(float64(i), gtk.PosBottom, "")
	}
	slider.ConnectValueChanged(func() {
		i := int(slider.Value() + 0.5)
		if i < 0 {
			i = 0
		}
		if i >= len(thumbPresets) {
			i = len(thumbPresets) - 1
		}
		a.grid.SetThumbSize(thumbPresets[i])
	})

	zoom := gtk.NewImageFromIconName("zoom-in-symbolic")

	box := gtk.NewBox(gtk.OrientationHorizontal, 6)
	box.SetMarginTop(6)
	box.SetMarginBottom(6)
	box.SetMarginStart(6)
	box.SetMarginEnd(6)
	box.Append(settings)
	box.Append(rescan)
	box.Append(search)
	box.Append(zoom)
	box.Append(slider)

	t.box = box
	return t
}

// Widget returns the toolbar's root widget.
func (t *Toolbar) Widget() gtk.Widgetter { return t.box }

// presetIndex returns the index of the closest preset to px.
func presetIndex(px int) int {
	best, bestDiff := 0, 1<<30
	for i, p := range thumbPresets {
		d := p - px
		if d < 0 {
			d = -d
		}
		if d < bestDiff {
			best, bestDiff = i, d
		}
	}
	return best
}
