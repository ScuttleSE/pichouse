package ui

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// Node id scheme for the Library tree.
const (
	newFoldersID = "newfolders"  // virtual branch holding unassigned folders
	albumPrefix  = "album:"      // album branch node ids ("album:3")
	folderPrefix = "folder:"     // folder leaf node ids ("folder:12")
)

// Sidebar is the left-hand Library view: a virtual organisation of scanned
// folders into albums (with sub-albums). Folders not in any album appear under
// "New folders". Ordering here is virtual and independent of disk order.
type Sidebar struct {
	app       *App
	container *fyne.Container
	tree      *widget.Tree

	// state rebuilt on Reload
	folders      map[int64]model.Folder
	counts       map[int64]int
	albums       map[int64]model.Album
	albumChildren map[int64][]int64 // parent album id (0=top) -> child album ids
	albumFolders map[int64][]int64  // album id -> folder ids
	folderAlbum  map[int64]int64    // folder id -> album id (if any)
	unassigned   []int64            // folder ids in no album

	// multi-selection of folder nodes (by node id)
	selected map[string]bool

	// drag-and-drop state
	dragging  bool           // a folder drag is in progress
	dragFrom  string         // node id where the drag started
	hoverNode string         // node id currently under the cursor (drop target)
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
		selected:      map[string]bool{},
	}

	s.tree = widget.NewTree(s.childUIDs, s.isBranch, s.createNode, s.updateNode)
	s.tree.OnSelected = s.onSelected

	header := widget.NewLabelWithStyle("Library", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	newAlbumBtn := widget.NewButtonWithIcon("New Album", theme.ContentAddIcon(), func() {
		s.promptCreateAlbum(0)
	})
	top := container.NewBorder(nil, nil, header, newAlbumBtn)
	s.container = container.NewBorder(top, nil, nil, nil, s.tree)
	return s
}

// Container returns the sidebar root widget.
func (s *Sidebar) Container() *fyne.Container { return s.container }

// childUIDs returns child node ids for a given node.
func (s *Sidebar) childUIDs(id widget.TreeNodeID) []widget.TreeNodeID {
	switch {
	case id == "":
		var ids []widget.TreeNodeID
		for _, aid := range s.albumChildren[0] {
			ids = append(ids, albumPrefix+strconv.FormatInt(aid, 10))
		}
		if len(s.unassigned) > 0 {
			ids = append(ids, newFoldersID)
		}
		return ids
	case id == newFoldersID:
		return folderNodeIDs(s.unassigned)
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		var ids []widget.TreeNodeID
		for _, child := range s.albumChildren[aid] {
			ids = append(ids, albumPrefix+strconv.FormatInt(child, 10))
		}
		ids = append(ids, folderNodeIDs(s.albumFolders[aid])...)
		return ids
	}
	return nil
}

func folderNodeIDs(fids []int64) []widget.TreeNodeID {
	ids := make([]widget.TreeNodeID, 0, len(fids))
	for _, fid := range fids {
		ids = append(ids, folderPrefix+strconv.FormatInt(fid, 10))
	}
	return ids
}

// isBranch reports whether a node can have children (albums and New folders).
func (s *Sidebar) isBranch(id widget.TreeNodeID) bool {
	return id == newFoldersID || strings.HasPrefix(id, albumPrefix)
}

// createNode builds a reusable node template.
func (s *Sidebar) createNode(branch bool) fyne.CanvasObject {
	return newLibNode(s)
}

// updateNode fills a node template for a specific id.
func (s *Sidebar) updateNode(id widget.TreeNodeID, branch bool, obj fyne.CanvasObject) {
	node := obj.(*libNode)
	node.id = id
	switch {
	case id == newFoldersID:
		node.set(theme.FolderIcon(), fmt.Sprintf("New folders (%d)", len(s.unassigned)), false)
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		a := s.albums[aid]
		node.set(theme.FolderNewIcon(), a.Name, s.selected[id])
	case strings.HasPrefix(id, folderPrefix):
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		f := s.folders[fid]
		node.set(theme.FileImageIcon(), fmt.Sprintf("%s (%d)", f.Name, s.counts[fid]), s.selected[id])
	}
}

// onSelected loads a folder into the grid when a folder leaf is selected. It
// also maintains the multi-selection set for batch operations.
func (s *Sidebar) onSelected(id widget.TreeNodeID) {
	if strings.HasPrefix(id, folderPrefix) {
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		if f, ok := s.folders[fid]; ok {
			s.app.grid.ShowFolder(f)
		}
	}
	// Single-click selection replaces the multi-selection unless extended via
	// the context menu (handled there).
	s.selected = map[string]bool{id: true}
	s.tree.UnselectAll()
	s.tree.Refresh()
}

// selectedFolderIDs returns folder ids currently in the multi-selection.
func (s *Sidebar) selectedFolderIDs() []int64 {
	var out []int64
	for id := range s.selected {
		if strings.HasPrefix(id, folderPrefix) {
			fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
			out = append(out, fid)
		}
	}
	return out
}

// dropTargetAlbum resolves the node currently under the cursor to an album id
// for use as a drag-and-drop target. An album node resolves to itself; a folder
// node resolves to the album containing it (if any). Returns 0 if there is no
// valid album target.
func (s *Sidebar) dropTargetAlbum() int64 {
	id := s.hoverNode
	switch {
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		return aid
	case strings.HasPrefix(id, folderPrefix):
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		if aid, ok := s.folderAlbum[fid]; ok {
			return aid
		}
	}
	return 0
}

// toggleSelect adds/removes a node from the multi-selection (Ctrl-style).
func (s *Sidebar) toggleSelect(id widget.TreeNodeID) {
	if s.selected[id] {
		delete(s.selected, id)
	} else {
		s.selected[id] = true
	}
	s.tree.Refresh()
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
	// Folders sorted by name for a stable virtual order.
	sort.Slice(folders, func(i, j int) bool { return folders[i].Name < folders[j].Name })
	for _, f := range folders {
		s.folders[f.ID] = f
		if aid, ok := folderAlbum[f.ID]; ok {
			s.albumFolders[aid] = append(s.albumFolders[aid], f.ID)
		} else {
			s.unassigned = append(s.unassigned, f.ID)
		}
	}

	s.tree.Refresh()
	for _, aid := range s.albumChildren[0] {
		s.tree.OpenBranch(albumPrefix + strconv.FormatInt(aid, 10))
	}
	if len(s.unassigned) > 0 {
		s.tree.OpenBranch(newFoldersID)
		s.tree.Select(folderPrefix + strconv.FormatInt(s.unassigned[0], 10))
	}
}
