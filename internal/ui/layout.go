package ui

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// build assembles the Picasa-style layout: toolbar on top, a left sidebar with
// Library/Folders stack, a center thumbnail grid, a right properties panel, and
// a bottom status bar.
func (a *App) build() {
	a.status = newStatusBar(a)
	a.properties = newProperties()
	a.grid = newGrid(a)
	a.sidebar = newSidebar(a)
	a.folderTree = newFolderTree(a)

	toolbar := newToolbar(a)

	// Left: a stack switcher toggles between the Library and Folders views.
	stack := gtk.NewStack()
	stack.SetVExpand(true)
	stack.AddTitled(a.sidebar.Widget(), "library", "Library")
	stack.AddTitled(a.folderTree.Widget(), "folders", "Folders")

	switcher := gtk.NewStackSwitcher()
	switcher.SetStack(stack)

	leftBox := gtk.NewBox(gtk.OrientationVertical, 0)
	leftBox.Append(switcher)
	leftBox.Append(stack)
	leftBox.SetSizeRequest(240, -1)

	// Center-left split: sidebar | grid.
	leftPaned := gtk.NewPaned(gtk.OrientationHorizontal)
	leftPaned.SetStartChild(leftBox)
	leftPaned.SetEndChild(a.grid.Widget())
	leftPaned.SetResizeStartChild(false)
	leftPaned.SetPosition(240)

	// Main split: (sidebar|grid) | properties.
	mainPaned := gtk.NewPaned(gtk.OrientationHorizontal)
	mainPaned.SetStartChild(leftPaned)
	mainPaned.SetEndChild(a.properties.Widget())
	mainPaned.SetResizeEndChild(false)
	mainPaned.SetVExpand(true)

	root := gtk.NewBox(gtk.OrientationVertical, 0)
	root.Append(toolbar.Widget())
	root.Append(mainPaned)
	root.Append(a.status.Widget())

	a.win.SetChild(root)
}
