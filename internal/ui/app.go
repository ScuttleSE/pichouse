// Package ui contains the GTK4-based user interface for pichouse.
package ui

import (
	"log"
	"os"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
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

	lib *db.Library
	gen *thumb.Generator

	prefs prefs

	sidebar    *Sidebar
	folderTree *FolderTree
	grid       *Grid
	properties *Properties
	status     *StatusBar

	// propsPaned is the outer paned holding the properties panel; used to
	// show/hide it.
	propsPaned  *gtk.Paned
	centerStack *gtk.Stack
	viewer      *Viewer

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

	a := &App{
		lib:        lib,
		gen:        thumb.New(0),
		thumbCache: newThumbCache(512),
	}
	a.prefs = loadPrefs(lib)
	a.applyThumbPrefs()

	a.gtkApp = gtk.NewApplication(appID, gio.ApplicationFlagsNone)
	a.gtkApp.ConnectActivate(func() { a.activate() })

	code := a.gtkApp.Run(os.Args)
	a.gen.Close()
	lib.Close()
	os.Exit(code)
}

// applyThumbPrefs pushes the current preferences into the generator.
func (a *App) applyThumbPrefs() {
	a.gen.SetSize(a.prefs.sizes[a.prefs.active])
	if a.prefs.saveAllSizes {
		a.gen.SetAllSizes(a.prefs.sizes)
	} else {
		a.gen.SetAllSizes(nil)
	}
}

// ToggleProperties shows or hides the right-hand properties panel and persists
// the choice.
func (a *App) ToggleProperties() {
	a.prefs.propsVisible = !a.prefs.propsVisible
	a.properties.SetVisible(a.prefs.propsVisible)
	v := "0"
	if a.prefs.propsVisible {
		v = "1"
	}
	a.lib.SetSetting(keyPropsVisible, v)
}

// activate builds the window and widgets. Called on the GTK main thread when
// the application is activated.
func (a *App) activate() {
	a.win = gtk.NewApplicationWindow(a.gtkApp)
	a.win.SetTitle("pichouse " + version.Version)
	a.win.SetDefaultSize(1280, 820)

	a.build()

	// Window-level key handling in the capture phase: while the full-image
	// viewer is active, arrow keys and R drive navigation/rotation regardless
	// of which widget currently holds focus.
	keys := gtk.NewEventControllerKey()
	keys.SetPropagationPhase(gtk.PhaseCapture)
	keys.ConnectKeyPressed(func(keyval, keycode uint, state gdk.ModifierType) bool {
		if a.centerStack != nil && a.centerStack.VisibleChildName() == "viewer" {
			return a.viewer.HandleKey(keyval)
		}
		return false
	})
	a.win.AddController(keys)

	a.win.SetVisible(true)

	// Populate views now that widgets exist and the window is shown.
	a.sidebar.Reload()
	a.folderTree.Reload()
}
