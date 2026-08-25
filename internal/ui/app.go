// Package ui contains the Fyne-based user interface for pichouse.
package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/version"
)

// Run starts the pichouse GUI application.
func Run() {
	a := app.NewWithID("se.hemmalab.pichouse")
	w := a.NewWindow("pichouse " + version.Version)

	placeholder := widget.NewLabel("pichouse — photo library (scaffold)")
	w.SetContent(container.NewCenter(placeholder))
	w.Resize(fyne.NewSize(1200, 800))
	w.ShowAndRun()
}
