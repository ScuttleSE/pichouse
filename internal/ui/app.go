// Package ui contains the GTK4-based user interface for pichouse.
package ui

import (
	"log"
	"os"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/db"
	"git.hemmalab.se/scuttle/pichouse/internal/thumb"
	"git.hemmalab.se/scuttle/pichouse/internal/version"
)

// appID is the GTK application identifier.
const appID = "se.hemmalab.pichouse"

// App holds the top-level application state and widgets.
type App struct {
	gtkApp *gtk.Application
	win    *gtk.ApplicationWindow

	lib    *db.Library
	thumbs *db.Thumbs
	gen    *thumb.Generator

	sidebar    *Sidebar
	folderTree *FolderTree
	grid       *Grid
	properties *Properties
	status     *StatusBar

	thumbCache *thumbCache
	scan       scanController
}

// Run starts the pichouse GUI application. It is the single entry point called
// from main.
func Run() {
	lib, err := db.OpenLibrary()
	if err != nil {
		log.Fatalf("open library database: %v", err)
	}
	thumbs, err := db.OpenThumbs()
	if err != nil {
		log.Fatalf("open thumbnail database: %v", err)
	}

	a := &App{
		lib:        lib,
		thumbs:     thumbs,
		gen:        thumb.New(thumbs, thumb.DefaultSize),
		thumbCache: newThumbCache(512),
	}

	a.gtkApp = gtk.NewApplication(appID, gio.ApplicationFlagsNone)
	a.gtkApp.ConnectActivate(func() { a.activate() })

	code := a.gtkApp.Run(os.Args)
	lib.Close()
	thumbs.Close()
	os.Exit(code)
}

// activate builds the window and widgets. Called on the GTK main thread when
// the application is activated.
func (a *App) activate() {
	a.win = gtk.NewApplicationWindow(a.gtkApp)
	a.win.SetTitle("pichouse " + version.Version)
	a.win.SetDefaultSize(1280, 820)

	a.build()

	a.win.SetVisible(true)

	// Populate views now that widgets exist and the window is shown.
	a.sidebar.Reload()
	a.folderTree.Reload()
}
