// Package ui contains the Fyne-based user interface for pichouse.
package ui

import (
	"log"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
	"git.hemmalab.se/scuttle/pichouse/internal/model"
	"git.hemmalab.se/scuttle/pichouse/internal/thumb"
	"git.hemmalab.se/scuttle/pichouse/internal/version"
)

// App holds the top-level application state and widgets.
type App struct {
	fyneApp fyne.App
	win     fyne.Window

	lib    *db.Library
	thumbs *db.Thumbs
	gen    *thumb.Generator

	sidebar    *Sidebar
	folderTree *FolderTree
	properties *Properties
	grid       *Grid
	status     *StatusBar

	scan scanController
}

// Run starts the pichouse GUI application. It is the single entry point called
// from main.
func Run() {
	a := app.NewWithID("se.hemmalab.pichouse")
	w := a.NewWindow("pichouse " + version.Version)

	lib, err := db.OpenLibrary()
	if err != nil {
		log.Fatalf("open library database: %v", err)
	}
	thumbs, err := db.OpenThumbs()
	if err != nil {
		log.Fatalf("open thumbnail database: %v", err)
	}

	ap := &App{
		fyneApp: a,
		win:     w,
		lib:     lib,
		thumbs:  thumbs,
		gen:     thumb.New(thumbs, thumb.DefaultSize),
	}
	ap.build()

	w.SetOnClosed(func() {
		lib.Close()
		thumbs.Close()
	})
	w.Resize(fyne.NewSize(1280, 820))
	w.ShowAndRun()
}

// build assembles the three-pane Picasa-style layout with toolbar and status
// bar.
func (a *App) build() {
	a.status = newStatusBar()
	a.status.SetOnStop(func() { a.scan.Stop() })
	a.properties = newProperties()
	a.grid = newGrid(a)
	a.sidebar = newSidebar(a)
	a.folderTree = newFolderTree(a)

	toolbar := newToolbar(a)

	// Center: folder header + grid handled inside Grid.
	center := a.grid.Container()

	// Left sidebar: tabs for the year-grouped Library and the raw Folders tree.
	leftTabs := container.NewAppTabs(
		container.NewTabItem("Library", a.sidebar.Container()),
		container.NewTabItem("Folders", a.folderTree.Container()),
	)

	// Left sidebar and right properties are collapsible split containers.
	leftSplit := container.NewHSplit(leftTabs, center)
	leftSplit.SetOffset(0.2)

	mainSplit := container.NewHSplit(leftSplit, a.properties.Container())
	mainSplit.SetOffset(0.8)

	content := container.NewBorder(
		toolbar.Container(), // top
		a.status.Container(), // bottom
		nil, nil,
		mainSplit,
	)
	a.win.SetContent(content)

	// Populate the trees from the current DB state once the UI is running.
	// widget.Tree only builds its visible content after the window is shown and
	// laid out, so doing this during build() leaves the tree blank. Defer it to
	// the lifecycle "started" hook, which fires on the UI goroutine after show.
	a.fyneApp.Lifecycle().SetOnStarted(func() {
		fyne.Do(func() {
			a.sidebar.Reload()
			a.folderTree.Reload()
		})
	})
}

// Window returns the main application window.
func (a *App) Window() fyne.Window { return a.win }

// selectPhoto shows a photo's details in the properties panel.
func (a *App) selectPhoto(p model.Photo) {
	a.properties.Show(p)
}
