package ui

import (
	"os"
	"path/filepath"
	"sort"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// FolderTree is a raw filesystem tree of the user's added library roots. Nodes
// are directory paths; children are read live from disk on demand. Selecting a
// directory shows its images in the grid.
type FolderTree struct {
	app  *App
	box  *gtk.Box
	root *gtk.StringList

	selection *gtk.SingleSelection
	listView  *gtk.ListView
}

func newFolderTree(a *App) *FolderTree {
	ft := &FolderTree{app: a}

	ft.root = gtk.NewStringList(nil)

	treeModel := gtk.NewTreeListModel(ft.root, false, false, func(item *glib.Object) *gio.ListModel {
		so, ok := item.Cast().(*gtk.StringObject)
		if !ok {
			return nil
		}
		children := subdirs(so.String())
		if len(children) == 0 {
			return nil
		}
		cl := gtk.NewStringList(children)
		lm := cl.ListModel
		return &lm
	})

	ft.selection = gtk.NewSingleSelection(&treeModel.ListModel)
	ft.selection.SetAutoselect(false)
	ft.selection.SetCanUnselect(true)

	factory := gtk.NewSignalListItemFactory()
	factory.ConnectSetup(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		expander := gtk.NewTreeExpander()
		label := gtk.NewLabel("")
		label.SetXAlign(0)
		expander.SetChild(label)
		item.SetChild(expander)
	})
	factory.ConnectBind(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		row, ok := item.Item().Cast().(*gtk.TreeListRow)
		if !ok {
			return
		}
		expander, _ := item.Child().(*gtk.TreeExpander)
		if expander == nil {
			return
		}
		expander.SetListRow(row)
		so, _ := row.Item().Cast().(*gtk.StringObject)
		if so == nil {
			return
		}
		label, _ := expander.Child().(*gtk.Label)
		if label != nil {
			label.SetText(baseName(so.String()))
		}
	})

	ft.listView = gtk.NewListView(ft.selection, &factory.ListItemFactory)
	ft.selection.ConnectSelectionChanged(func(uint, uint) {
		sel := ft.selection.SelectedItem()
		if sel == nil {
			return
		}
		row, ok := sel.Cast().(*gtk.TreeListRow)
		if !ok {
			return
		}
		so, _ := row.Item().Cast().(*gtk.StringObject)
		if so != nil {
			ft.app.grid.ShowRawFolder(so.String())
		}
	})

	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetChild(ft.listView)

	ft.box = gtk.NewBox(gtk.OrientationVertical, 0)
	ft.box.Append(scroll)
	return ft
}

// Widget returns the folder-tree root widget.
func (ft *FolderTree) Widget() gtk.Widgetter { return ft.box }

// Reload refreshes the set of root folders from the library.
func (ft *FolderTree) Reload() {
	lfs, err := ft.app.lib.LibraryFolders()
	if err != nil {
		return
	}
	n := ft.root.NItems()
	paths := make([]string, 0, len(lfs))
	for _, lf := range lfs {
		paths = append(paths, lf.Path)
	}
	ft.root.Splice(0, n, paths)
}

// subdirs returns the immediate subdirectory paths of dir, sorted by name,
// skipping hidden directories.
func subdirs(dir string) []string {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range entries {
		if e.IsDir() && len(e.Name()) > 0 && e.Name()[0] != '.' {
			out = append(out, filepath.Join(dir, e.Name()))
		}
	}
	sort.Strings(out)
	return out
}

// baseName returns the last path element, falling back to the full path for
// roots like "/".
func baseName(p string) string {
	b := filepath.Base(p)
	if b == "" || b == string(os.PathSeparator) {
		return p
	}
	return b
}
