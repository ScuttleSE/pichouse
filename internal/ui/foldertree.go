package ui

import (
	"os"
	"path/filepath"
	"sort"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// FolderTree is a raw filesystem tree of the user's added library roots. Nodes
// are directory paths; children are read live from disk on demand. Selecting a
// directory shows its images in the grid via raw folder view.
type FolderTree struct {
	app       *App
	container *fyne.Container
	tree      *widget.Tree

	roots []string
}

func newFolderTree(a *App) *FolderTree {
	ft := &FolderTree{app: a}
	ft.tree = widget.NewTree(ft.childUIDs, ft.isBranch, ft.createNode, ft.updateNode)
	ft.tree.OnSelected = ft.onSelected

	header := widget.NewLabelWithStyle("Folders", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	ft.container = container.NewBorder(header, nil, nil, nil, ft.tree)
	return ft
}

// Container returns the folder-tree root widget.
func (ft *FolderTree) Container() *fyne.Container { return ft.container }

// Reload refreshes the set of root folders from the library and rebuilds the
// tree.
func (ft *FolderTree) Reload() {
	lfs, err := ft.app.lib.LibraryFolders()
	if err != nil {
		return
	}
	ft.roots = ft.roots[:0]
	for _, lf := range lfs {
		ft.roots = append(ft.roots, lf.Path)
	}
	ft.tree.Refresh()
}

// childUIDs returns child directory paths for a node. The empty id is the
// virtual root whose children are the added library roots.
func (ft *FolderTree) childUIDs(id widget.TreeNodeID) []widget.TreeNodeID {
	if id == "" {
		return ft.roots
	}
	return subdirs(id)
}

// isBranch reports whether a directory has at least one subdirectory.
func (ft *FolderTree) isBranch(id widget.TreeNodeID) bool {
	if id == "" {
		return true
	}
	return len(subdirs(id)) > 0
}

func (ft *FolderTree) createNode(branch bool) fyne.CanvasObject {
	return container.NewHBox(widget.NewIcon(theme.FolderIcon()), widget.NewLabel("template"))
}

func (ft *FolderTree) updateNode(id widget.TreeNodeID, branch bool, obj fyne.CanvasObject) {
	box := obj.(*fyne.Container)
	label := box.Objects[1].(*widget.Label)
	name := filepath.Base(id)
	if name == "" || name == string(os.PathSeparator) {
		name = id
	}
	label.SetText(name)
}

// onSelected shows the selected directory's images in the grid.
func (ft *FolderTree) onSelected(id widget.TreeNodeID) {
	if id == "" {
		return
	}
	ft.app.grid.ShowRawFolder(id)
}

// subdirs returns the immediate subdirectory paths of dir, sorted by name.
func subdirs(dir string) []widget.TreeNodeID {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []widget.TreeNodeID
	for _, e := range entries {
		if e.IsDir() && e.Name()[0] != '.' {
			out = append(out, filepath.Join(dir, e.Name()))
		}
	}
	sort.Strings(out)
	return out
}
