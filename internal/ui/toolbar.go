package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// Toolbar is the top toolbar: add-folder, view toggles, search, and a
// thumbnail-size slider.
type Toolbar struct {
	app       *App
	container *fyne.Container
}

func newToolbar(a *App) *Toolbar {
	t := &Toolbar{app: a}

	addFolder := widget.NewButtonWithIcon("Add Folder", theme.FolderNewIcon(), func() {
		a.AddLibraryFolder()
	})
	rescan := widget.NewButtonWithIcon("Rescan", theme.ViewRefreshIcon(), func() {
		a.RescanAll()
	})
	folderView := widget.NewButtonWithIcon("Folder View", theme.FolderOpenIcon(), func() {
		a.OpenFolderView()
	})

	search := widget.NewEntry()
	search.SetPlaceHolder("Search")
	search.OnChanged = func(q string) {
		a.grid.SetFilter(q)
	}

	sizeSlider := widget.NewSlider(96, 320)
	sizeSlider.Value = float64(thumbGridSize)
	sizeSlider.Step = 16
	sizeSlider.OnChanged = func(v float64) {
		a.grid.SetThumbSize(int(v))
	}
	sizeSlider.Resize(fyne.NewSize(160, sizeSlider.MinSize().Height))

	left := container.NewHBox(addFolder, rescan, folderView)
	right := container.NewHBox(
		widget.NewIcon(theme.ZoomInIcon()),
		container.NewGridWrap(fyne.NewSize(160, sizeSlider.MinSize().Height), sizeSlider),
	)

	bar := container.NewBorder(nil, nil, left, right, search)
	t.container = container.NewVBox(container.NewPadded(bar), widget.NewSeparator())
	return t
}

// Container returns the toolbar's root widget.
func (t *Toolbar) Container() *fyne.Container { return t.container }
