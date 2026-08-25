package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// Sidebar is the left-hand collapsible folder tree.
type Sidebar struct {
	app       *App
	container *fyne.Container
	label     *widget.Label
}

func newSidebar(a *App) *Sidebar {
	s := &Sidebar{
		app:   a,
		label: widget.NewLabel("Folders"),
	}
	header := widget.NewLabelWithStyle("Library", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	s.container = container.NewBorder(header, nil, nil, nil, s.label)
	return s
}

// Container returns the sidebar root widget.
func (s *Sidebar) Container() *fyne.Container { return s.container }

// Reload rebuilds the tree from the current database state.
func (s *Sidebar) Reload() {
	folders, err := s.app.lib.Folders()
	if err != nil {
		s.label.SetText("Error loading folders")
		return
	}
	if len(folders) == 0 {
		s.label.SetText("No folders yet.\nUse Add Folder.")
		return
	}
	s.label.SetText("Folders: " + itoa(len(folders)))
}
