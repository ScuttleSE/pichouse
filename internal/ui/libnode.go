package ui

import (
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// libNode is a Library tree row that supports left-click selection (delegated
// to the tree) and right-click for a context menu of album operations.
type libNode struct {
	widget.BaseWidget
	sidebar *Sidebar
	id      widget.TreeNodeID

	icon    *widget.Icon
	label   *widget.Label
	box     *fyne.Container
}

func newLibNode(s *Sidebar) *libNode {
	n := &libNode{sidebar: s}
	n.icon = widget.NewIcon(theme.FolderIcon())
	n.label = widget.NewLabel("template")
	n.box = container.NewHBox(n.icon, n.label)
	n.ExtendBaseWidget(n)
	return n
}

func (n *libNode) set(res fyne.Resource, text string, selected bool) {
	n.icon.SetResource(res)
	if selected {
		n.label.TextStyle = fyne.TextStyle{Bold: true}
	} else {
		n.label.TextStyle = fyne.TextStyle{}
	}
	n.label.SetText(text)
}

func (n *libNode) CreateRenderer() fyne.WidgetRenderer {
	return widget.NewSimpleRenderer(n.box)
}

// Tapped selects the node in the tree (normal left-click).
func (n *libNode) Tapped(_ *fyne.PointEvent) {
	n.sidebar.tree.Select(n.id)
}

// TappedSecondary shows the context menu for album operations.
func (n *libNode) TappedSecondary(ev *fyne.PointEvent) {
	menu := n.contextMenu()
	if menu == nil {
		return
	}
	widget.ShowPopUpMenuAtPosition(menu, fyne.CurrentApp().Driver().CanvasForObject(n), ev.AbsolutePosition)
}

// MouseIn/Out/Moved satisfy desktop.Hoverable so we can track the drop target
// for drag-and-drop of folders onto albums.
func (n *libNode) MouseIn(*desktop.MouseEvent)    { n.sidebar.hoverNode = n.id }
func (n *libNode) MouseMoved(*desktop.MouseEvent) { n.sidebar.hoverNode = n.id }
func (n *libNode) MouseOut() {
	if n.sidebar.hoverNode == n.id {
		n.sidebar.hoverNode = ""
	}
}

// Dragged begins/continues dragging a folder node. Only folder rows are
// draggable; dragging one that is not selected makes it the sole selection.
func (n *libNode) Dragged(*fyne.DragEvent) {
	if !strings.HasPrefix(n.id, folderPrefix) {
		return
	}
	if !n.sidebar.dragging {
		n.sidebar.dragging = true
		n.sidebar.dragFrom = n.id
		if !n.sidebar.selected[n.id] {
			n.sidebar.selected = map[string]bool{n.id: true}
			n.sidebar.tree.Refresh()
		}
	}
}

// DragEnd completes a drag: if the cursor is over an album (or a folder inside
// one), the selected folders are moved into that album.
func (n *libNode) DragEnd() {
	s := n.sidebar
	if !s.dragging {
		return
	}
	s.dragging = false
	target := s.dropTargetAlbum()
	if target != 0 {
		s.moveSelectedToAlbum(target)
	}
	s.dragFrom = ""
}

// contextMenu builds the right-click menu appropriate to the node type.
func (n *libNode) contextMenu() *fyne.Menu {
	s := n.sidebar
	switch {
	case strings.HasPrefix(n.id, albumPrefix):
		aid, _ := strconv.ParseInt(n.id[len(albumPrefix):], 10, 64)
		return fyne.NewMenu("",
			fyne.NewMenuItem("New Sub-Album…", func() { s.promptCreateAlbum(aid) }),
			fyne.NewMenuItem("Rename Album…", func() { s.promptRenameAlbum(aid) }),
			fyne.NewMenuItem("Delete Album", func() { s.deleteAlbum(aid) }),
			addToAlbumSubmenu(s, aid),
		)
	case strings.HasPrefix(n.id, folderPrefix):
		// Ensure this folder is part of the selection so the action applies.
		if !s.selected[n.id] {
			s.selected = map[string]bool{n.id: true}
		}
		items := []*fyne.MenuItem{
			{Label: "Add to selection", Action: func() { s.toggleSelect(n.id) }},
			moveToAlbumSubmenu(s),
			{Label: "Remove from Album", Action: func() { s.removeSelectedFromAlbum() }},
		}
		return fyne.NewMenu("", items...)
	case n.id == newFoldersID:
		return fyne.NewMenu("",
			fyne.NewMenuItem("New Album…", func() { s.promptCreateAlbum(0) }),
		)
	}
	return nil
}

// moveToAlbumSubmenu builds a "Move to Album ▸" submenu listing all albums.
func moveToAlbumSubmenu(s *Sidebar) *fyne.MenuItem {
	item := fyne.NewMenuItem("Move to Album", nil)
	var children []*fyne.MenuItem
	for _, a := range s.albumsSorted() {
		aid := a.ID
		children = append(children, fyne.NewMenuItem(s.albumPathName(aid), func() {
			s.moveSelectedToAlbum(aid)
		}))
	}
	if len(children) == 0 {
		children = []*fyne.MenuItem{{Label: "(no albums yet)", Disabled: true}}
	}
	item.ChildMenu = fyne.NewMenu("", children...)
	return item
}

// addToAlbumSubmenu builds a submenu to move the current selection into the
// album with id target (used from an album's own context menu).
func addToAlbumSubmenu(s *Sidebar, target int64) *fyne.MenuItem {
	return fyne.NewMenuItem("Move selected here", func() {
		s.moveSelectedToAlbum(target)
	})
}
