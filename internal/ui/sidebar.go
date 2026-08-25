package ui

import (
	"sort"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Node id scheme for the Library tree.
const (
	newFoldersID = "newfolders"
	albumPrefix  = "album:"
	folderPrefix = "folder:"
)

// Sidebar is the left-hand Library view: a virtual organisation of scanned
// folders into albums (with sub-albums). Folders not in any album appear under
// "New folders". Ordering here is virtual and independent of disk order.
type Sidebar struct {
	app  *App
	box  *gtk.Box
	root *gtk.StringList

	selection *gtk.MultiSelection
	listView  *gtk.ListView
	treeModel *gtk.TreeListModel

	// context menu (native GMenu/GtkPopoverMenu + GSimpleActions)
	actions  *gio.SimpleActionGroup
	menuPop  *gtk.PopoverMenu
	menuNode string // node id the currently-open context menu targets

	// expandedByDefault records album node-ids the user has expanded, so the
	// tree stays open across Reload (e.g. after moving a folder into an album).
	expanded map[string]bool

	// state rebuilt on Reload
	folders       map[int64]model.Folder
	counts        map[int64]int
	albums        map[int64]model.Album
	albumChildren map[int64][]int64
	albumFolders  map[int64][]int64
	folderAlbum   map[int64]int64
	unassigned    []int64
}

func newSidebar(a *App) *Sidebar {
	s := &Sidebar{
		app:           a,
		folders:       map[int64]model.Folder{},
		counts:        map[int64]int{},
		albums:        map[int64]model.Album{},
		albumChildren: map[int64][]int64{},
		albumFolders:  map[int64][]int64{},
		folderAlbum:   map[int64]int64{},
		expanded:      map[string]bool{},
	}

	s.root = gtk.NewStringList(nil)

	treeModel := gtk.NewTreeListModel(s.root, false, false, func(item *glib.Object) *gio.ListModel {
		so, ok := item.Cast().(*gtk.StringObject)
		if !ok {
			return nil
		}
		kids := s.childIDs(so.String())
		if len(kids) == 0 {
			return nil
		}
		cl := gtk.NewStringList(kids)
		lm := cl.ListModel
		return &lm
	})
	s.treeModel = treeModel

	s.selection = gtk.NewMultiSelection(&treeModel.ListModel)

	factory := gtk.NewSignalListItemFactory()
	factory.ConnectSetup(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		expander := gtk.NewTreeExpander()
		// Reserve space for the expander arrow on every row so leaf rows
		// (folders) line up with branch rows (albums) that show an arrow.
		expander.SetIndentForIcon(true)
		expander.SetIndentForDepth(true)
		row := gtk.NewBox(gtk.OrientationHorizontal, 4)
		icon := gtk.NewImageFromIconName("folder-symbolic")
		label := gtk.NewLabel("")
		label.SetXAlign(0)
		row.Append(icon)
		row.Append(label)
		expander.SetChild(row)
		item.SetChild(expander)
		s.attachRowMenu(expander)
		s.attachRowDrag(expander)
	})
	factory.ConnectBind(func(obj *glib.Object) {
		item := obj.Cast().(*gtk.ListItem)
		s.bindRow(item)
	})

	s.listView = gtk.NewListView(s.selection, &factory.ListItemFactory)
	s.selection.ConnectSelectionChanged(func(uint, uint) { s.onSelectionChanged() })

	// Native right-click menu: an action group backs a reusable GtkPopoverMenu
	// that is re-modelled per row. This themes correctly (unlike hand-built
	// button popovers).
	s.installContextMenu()
	s.installDragDrop()

	newAlbumBtn := gtk.NewButtonWithLabel("New Album")
	newAlbumBtn.SetHAlign(gtk.AlignStart)
	newAlbumBtn.SetMarginTop(4)
	newAlbumBtn.SetMarginStart(4)
	newAlbumBtn.SetMarginBottom(4)
	newAlbumBtn.ConnectClicked(func() { s.promptCreateAlbum(0) })

	scroll := gtk.NewScrolledWindow()
	scroll.SetVExpand(true)
	scroll.SetChild(s.listView)

	// The reusable popover is parented to the list view so it participates in
	// the normal widget/style hierarchy.
	s.menuPop.SetParent(s.listView)

	s.box = gtk.NewBox(gtk.OrientationVertical, 0)
	s.box.Append(newAlbumBtn)
	s.box.Append(scroll)
	return s
}

// Widget returns the sidebar root widget.
func (s *Sidebar) Widget() gtk.Widgetter { return s.box }

// childIDs returns the child node-id strings for a node id.
func (s *Sidebar) childIDs(id string) []string {
	switch {
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		var out []string
		for _, child := range s.albumChildren[aid] {
			out = append(out, albumPrefix+strconv.FormatInt(child, 10))
		}
		for _, fid := range s.albumFolders[aid] {
			out = append(out, folderPrefix+strconv.FormatInt(fid, 10))
		}
		return out
	case id == newFoldersID:
		var out []string
		for _, fid := range s.unassigned {
			out = append(out, folderPrefix+strconv.FormatInt(fid, 10))
		}
		return out
	}
	return nil
}

// bindRow fills a list row for the tree item at the given list position.
func (s *Sidebar) bindRow(item *gtk.ListItem) {
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
	expander.SetName(so.String())
	box, _ := expander.Child().(*gtk.Box)
	if box == nil {
		return
	}
	icon, _ := box.FirstChild().(*gtk.Image)
	label, _ := box.LastChild().(*gtk.Label)
	id := so.String()
	name, iconName := s.nodeLabel(id)
	if icon != nil {
		icon.SetFromIconName(iconName)
	}
	if label != nil {
		label.SetText(name)
	}
}

// nodeLabel returns the display text and icon name for a node id.
func (s *Sidebar) nodeLabel(id string) (string, string) {
	switch {
	case id == newFoldersID:
		return "New folders (" + itoa(len(s.unassigned)) + ")", "folder-symbolic"
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		return s.albums[aid].Name, "folder-new-symbolic"
	case strings.HasPrefix(id, folderPrefix):
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		f := s.folders[fid]
		return f.Name + " (" + itoa(s.counts[fid]) + ")", "image-x-generic-symbolic"
	}
	return id, "folder-symbolic"
}

// onSelectionChanged loads the (first) selected folder into the grid.
func (s *Sidebar) onSelectionChanged() {
	for _, id := range s.selectedIDs() {
		if strings.HasPrefix(id, folderPrefix) {
			fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
			if f, ok := s.folders[fid]; ok {
				s.app.grid.ShowFolder(f)
				return
			}
		}
	}
}

// selectedIDs returns the node-id strings of all selected rows.
func (s *Sidebar) selectedIDs() []string {
	var out []string
	bs := s.selection.Selection()
	n := bs.Size()
	for i := uint64(0); i < n; i++ {
		pos := bs.Nth(uint(i))
		obj := s.selection.Item(pos)
		if obj == nil {
			continue
		}
		row, ok := obj.Cast().(*gtk.TreeListRow)
		if !ok {
			continue
		}
		if so, ok := row.Item().Cast().(*gtk.StringObject); ok {
			out = append(out, so.String())
		}
	}
	return out
}

// selectedFolderIDs returns folder ids among the current selection.
func (s *Sidebar) selectedFolderIDs() []int64 {
	var out []int64
	for _, id := range s.selectedIDs() {
		if strings.HasPrefix(id, folderPrefix) {
			fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
			out = append(out, fid)
		}
	}
	return out
}

// Reload rebuilds the tree from the current database state.
func (s *Sidebar) Reload() {
	folders, err := s.app.lib.Folders()
	if err != nil {
		return
	}
	counts, err := s.app.lib.FolderPhotoCounts()
	if err != nil {
		counts = map[int64]int{}
	}
	albums, err := s.app.lib.Albums()
	if err != nil {
		albums = nil
	}
	folderAlbum, err := s.app.lib.FolderAlbums()
	if err != nil {
		folderAlbum = map[int64]int64{}
	}

	s.folders = map[int64]model.Folder{}
	s.counts = counts
	s.albums = map[int64]model.Album{}
	s.albumChildren = map[int64][]int64{}
	s.albumFolders = map[int64][]int64{}
	s.folderAlbum = folderAlbum
	s.unassigned = nil

	for _, a := range albums {
		s.albums[a.ID] = a
		s.albumChildren[a.ParentID] = append(s.albumChildren[a.ParentID], a.ID)
	}
	sort.Slice(folders, func(i, j int) bool { return folders[i].Name < folders[j].Name })
	for _, f := range folders {
		s.folders[f.ID] = f
		if aid, ok := folderAlbum[f.ID]; ok {
			s.albumFolders[aid] = append(s.albumFolders[aid], f.ID)
		} else {
			s.unassigned = append(s.unassigned, f.ID)
		}
	}

	// Capture which branches are currently expanded so we can restore them
	// after the model is rebuilt (moving a folder must not collapse the tree).
	s.saveExpansion()

	// Rebuild the root node list: top-level albums, then New folders.
	var roots []string
	for _, aid := range s.albumChildren[0] {
		roots = append(roots, albumPrefix+strconv.FormatInt(aid, 10))
	}
	if len(s.unassigned) > 0 {
		roots = append(roots, newFoldersID)
	}
	n := s.root.NItems()
	s.root.Splice(0, n, roots)

	s.restoreExpansion()
}

// saveExpansion records the node-ids of all currently expanded branch rows.
func (s *Sidebar) saveExpansion() {
	if s.treeModel == nil {
		return
	}
	n := s.treeModel.NItems()
	for i := uint(0); i < n; i++ {
		row := s.treeModel.Row(i)
		if row == nil {
			continue
		}
		if !row.Expanded() {
			continue
		}
		if so, ok := row.Item().Cast().(*gtk.StringObject); ok {
			s.expanded[so.String()] = true
		}
	}
}

// restoreExpansion re-expands branches previously recorded as expanded. Because
// child models are built lazily, expanding a parent materialises its children,
// so we iterate until no further rows can be expanded.
func (s *Sidebar) restoreExpansion() {
	if s.treeModel == nil {
		return
	}
	for pass := 0; pass < 32; pass++ {
		changed := false
		n := s.treeModel.NItems()
		for i := uint(0); i < n; i++ {
			row := s.treeModel.Row(i)
			if row == nil {
				continue
			}
			so, ok := row.Item().Cast().(*gtk.StringObject)
			if !ok {
				continue
			}
			if s.expanded[so.String()] && !row.Expanded() {
				row.SetExpanded(true)
				changed = true
			}
		}
		if !changed {
			break
		}
	}
}

// markExpanded records that a node should be shown expanded across reloads.
func (s *Sidebar) markExpanded(id string) { s.expanded[id] = true }
