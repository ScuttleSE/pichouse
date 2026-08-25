package ui

import (
	"fyne.io/fyne/v2/dialog"
)

// AddLibraryFolder is wired fully in a later step; for the scaffold it shows a
// placeholder dialog.
func (a *App) AddLibraryFolder() {
	dialog.ShowInformation("Add Folder", "Folder picker and scan are coming next.", a.win)
}

// RescanAll is wired fully in a later step.
func (a *App) RescanAll() {
	dialog.ShowInformation("Rescan", "Rescan is coming next.", a.win)
}
