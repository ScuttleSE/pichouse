package ui

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Toolbar is the top toolbar: settings, rescan, search, a thumbnail-size slider
// with preset snap positions, and a button to toggle the properties panel.
type Toolbar struct {
	app    *App
	box    *gtk.Box
	slider *gtk.Scale
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

	zoom := gtk.NewImageFromIconName("zoom-in-symbolic")

	presets := a.prefs.sizes
	// Slider snaps to preset indices; value maps into the preset sizes.
	slider := gtk.NewScaleWithRange(gtk.OrientationHorizontal, 0, float64(len(presets)-1), 1)
	slider.SetDrawValue(false)
	slider.SetDigits(0)
	slider.SetSizeRequest(160, -1)
	slider.SetValue(float64(a.prefs.active))
	for i := range presets {
		slider.AddMark(float64(i), gtk.PosBottom, "")
	}
	slider.ConnectValueChanged(func() {
		i := int(slider.Value() + 0.5)
		if i < 0 {
			i = 0
		}
		if i >= len(a.prefs.sizes) {
			i = len(a.prefs.sizes) - 1
		}
		a.prefs.active = i
		a.lib.SetSetting(keyThumbActive, itoa(i))
		a.grid.SetThumbSize(a.prefs.sizes[i])
	})
	t.slider = slider

	// Toggle the properties panel. Placed next to the slider per the design
	// ("an icon just under the size-slider").
	propsToggle := gtk.NewButtonFromIconName("sidebar-show-right-symbolic")
	propsToggle.SetTooltipText("Toggle info panel")
	propsToggle.ConnectClicked(func() { a.ToggleProperties() })

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
	box.Append(propsToggle)

	t.box = box
	return t
}

// Widget returns the toolbar's root widget.
func (t *Toolbar) Widget() gtk.Widgetter { return t.box }
