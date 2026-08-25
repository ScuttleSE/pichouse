package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// thumbPresets are the snap positions (in px) for the thumbnail-size slider.
var thumbPresets = []int{96, 160, 240, 320}

// Toolbar is the top toolbar: settings, rescan, search, and a thumbnail-size
// slider with preset snap positions.
type Toolbar struct {
	app       *App
	container *fyne.Container
}

func newToolbar(a *App) *Toolbar {
	t := &Toolbar{app: a}

	settings := widget.NewButtonWithIcon("Settings", theme.SettingsIcon(), func() {
		a.ShowSettings()
	})
	rescan := widget.NewButtonWithIcon("Rescan", theme.ViewRefreshIcon(), func() {
		a.RescanAll()
	})

	search := widget.NewEntry()
	search.SetPlaceHolder("Search")
	search.OnChanged = func(q string) {
		a.grid.SetFilter(q)
	}

	// Slider indexes into thumbPresets so it snaps to preset positions only.
	sizeSlider := widget.NewSlider(0, float64(len(thumbPresets)-1))
	sizeSlider.Step = 1
	sizeSlider.Value = float64(presetIndex(thumbGridSize))
	sizeSlider.OnChanged = func(v float64) {
		i := int(v)
		if i < 0 {
			i = 0
		}
		if i >= len(thumbPresets) {
			i = len(thumbPresets) - 1
		}
		a.grid.SetThumbSize(thumbPresets[i])
	}

	left := container.NewHBox(settings, rescan)
	right := container.NewHBox(
		widget.NewIcon(theme.ZoomInIcon()),
		container.NewGridWrap(fyne.NewSize(160, sizeSlider.MinSize().Height), sizeSlider),
	)

	bar := container.NewBorder(nil, nil, left, right, search)
	t.container = container.NewVBox(container.NewPadded(bar), widget.NewSeparator())
	return t
}

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

// Container returns the toolbar's root widget.
func (t *Toolbar) Container() *fyne.Container { return t.container }
