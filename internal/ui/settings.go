package ui

import (
	"context"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// ShowSettings opens the settings window. It currently manages Library folders;
// it is structured with a stack so more setting groups can be added later.
func (a *App) ShowSettings() {
	win := gtk.NewWindow()
	win.SetTitle("Settings")
	win.SetModal(true)
	win.SetTransientFor(&a.win.Window)
	win.SetDefaultSize(640, 420)

	stack := gtk.NewStack()
	stack.SetVExpand(true)
	stack.AddTitled(a.buildFolderSettings(win), "folders", "Library Folders")

	switcher := gtk.NewStackSidebar()
	switcher.SetStack(stack)

	body := gtk.NewBox(gtk.OrientationHorizontal, 0)
	body.Append(switcher)
	body.Append(stack)

	win.SetChild(body)
	win.SetVisible(true)
}

// buildFolderSettings builds the Library-folders management pane.
func (a *App) buildFolderSettings(parent *gtk.Window) gtk.Widgetter {
	model := gtk.NewStringList(nil)

	reload := func() {
		fs, err := a.lib.LibraryFolders()
		if err != nil {
			a.showError(err)
			return
		}
		paths := make([]string, 0, len(fs))
		for _, lf := range fs {
			paths = append(paths, lf.Path)
		}
		model.Splice(0, model.NItems(), paths)
	}
	reload()

	selection := gtk.NewSingleSelection(&model.ListModel)
	factory := gtk.NewSignalListItemFactory()
	factory.ConnectSetup(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		label := gtk.NewLabel("")
		label.SetXAlign(0)
		item.SetChild(label)
	})
	factory.ConnectBind(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		label, _ := item.Child().(*gtk.Label)
		so, _ := item.Item().Cast().(*gtk.StringObject)
		if label != nil && so != nil {
			label.SetText(so.String())
		}
	})
	list := gtk.NewListView(selection, &factory.ListItemFactory)

	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetChild(list)

	addBtn := gtk.NewButtonWithLabel("Add Folder…")
	addBtn.ConnectClicked(func() {
		dialog := gtk.NewFileDialog()
		dialog.SetTitle("Add Library Folder")
		dialog.SelectFolder(context.Background(), parent, func(res gio.AsyncResulter) {
			file, err := dialog.SelectFolderFinish(res)
			if err != nil || file == nil {
				return
			}
			a.AddLibraryFolder(file.Path())
			reload()
		})
	})

	removeBtn := gtk.NewButtonWithLabel("Remove")
	removeBtn.AddCSSClass("destructive-action")
	removeBtn.ConnectClicked(func() {
		pos := selection.Selected()
		obj := model.Item(pos)
		if obj == nil {
			return
		}
		so, ok := obj.Cast().(*gtk.StringObject)
		if !ok {
			return
		}
		path := so.String()
		a.confirm("Remove folder",
			"Remove \""+path+"\" from the library? Scanned entries for this folder will be deleted.",
			func() {
				if err := a.lib.RemoveLibraryFolder(path); err != nil {
					a.showError(err)
					return
				}
				a.sidebar.Reload()
				a.folderTree.Reload()
				reload()
			})
	})

	buttons := gtk.NewBox(gtk.OrientationHorizontal, 6)
	buttons.Append(addBtn)
	buttons.Append(removeBtn)

	help := gtk.NewLabel("Folders added here are scanned into your library.")
	help.SetXAlign(0)
	help.SetWrap(true)

	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.SetMarginTop(12)
	box.SetMarginBottom(12)
	box.SetMarginStart(12)
	box.SetMarginEnd(12)
	box.Append(help)
	box.Append(buttons)
	box.Append(scroll)
	return box
}
