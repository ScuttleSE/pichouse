package ui

import (
	"fmt"
	"sort"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// rootFoldersID is the tree's top-level "Folders" node id.
const rootFoldersID = "folders"

// yearPrefix marks year branch node ids ("year:2019").
const yearPrefix = "year:"

// folderPrefix marks folder leaf node ids ("folder:12").
const folderPrefix = "folder:"

// Sidebar is the left-hand collapsible folder tree, grouped by year.
type Sidebar struct {
	app       *App
	container *fyne.Container
	tree      *widget.Tree

	// state rebuilt on Reload
	years     []int              // sorted descending
	byYear    map[int][]model.Folder
	folders   map[int64]model.Folder
	counts    map[int64]int
}

func newSidebar(a *App) *Sidebar {
	s := &Sidebar{
		app:     a,
		byYear:  map[int][]model.Folder{},
		folders: map[int64]model.Folder{},
		counts:  map[int64]int{},
	}

	s.tree = widget.NewTree(
		s.childUIDs,
		s.isBranch,
		s.createNode,
		s.updateNode,
	)
	s.tree.OnSelected = s.onSelected

	header := widget.NewLabelWithStyle("Library", fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
	s.container = container.NewBorder(header, nil, nil, nil, s.tree)
	return s
}

// Container returns the sidebar root widget.
func (s *Sidebar) Container() *fyne.Container { return s.container }

// childUIDs returns child node ids for a given node.
func (s *Sidebar) childUIDs(id widget.TreeNodeID) []widget.TreeNodeID {
	switch {
	case id == "":
		return []widget.TreeNodeID{rootFoldersID}
	case id == rootFoldersID:
		ids := make([]widget.TreeNodeID, 0, len(s.years))
		for _, y := range s.years {
			ids = append(ids, fmt.Sprintf("%s%d", yearPrefix, y))
		}
		return ids
	case len(id) > len(yearPrefix) && id[:len(yearPrefix)] == yearPrefix:
		year, _ := strconv.Atoi(id[len(yearPrefix):])
		folders := s.byYear[year]
		ids := make([]widget.TreeNodeID, 0, len(folders))
		for _, f := range folders {
			ids = append(ids, fmt.Sprintf("%s%d", folderPrefix, f.ID))
		}
		return ids
	}
	return nil
}

// isBranch reports whether a node can have children.
func (s *Sidebar) isBranch(id widget.TreeNodeID) bool {
	if id == rootFoldersID {
		return true
	}
	return len(id) > len(yearPrefix) && id[:len(yearPrefix)] == yearPrefix
}

// createNode builds a reusable node template.
func (s *Sidebar) createNode(branch bool) fyne.CanvasObject {
	return container.NewHBox(widget.NewIcon(theme.FolderIcon()), widget.NewLabel("template"))
}

// updateNode fills a node template for a specific id.
func (s *Sidebar) updateNode(id widget.TreeNodeID, branch bool, obj fyne.CanvasObject) {
	box := obj.(*fyne.Container)
	icon := box.Objects[0].(*widget.Icon)
	label := box.Objects[1].(*widget.Label)

	switch {
	case id == rootFoldersID:
		icon.SetResource(theme.StorageIcon())
		label.SetText(fmt.Sprintf("Folders (%d)", len(s.folders)))
	case len(id) > len(yearPrefix) && id[:len(yearPrefix)] == yearPrefix:
		year := id[len(yearPrefix):]
		icon.SetResource(theme.FolderIcon())
		label.SetText(year)
	case len(id) > len(folderPrefix) && id[:len(folderPrefix)] == folderPrefix:
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		f := s.folders[fid]
		icon.SetResource(theme.FolderIcon())
		label.SetText(fmt.Sprintf("%s (%d)", f.Name, s.counts[fid]))
	}
}

// onSelected loads a folder into the grid when a folder leaf is selected.
func (s *Sidebar) onSelected(id widget.TreeNodeID) {
	if len(id) > len(folderPrefix) && id[:len(folderPrefix)] == folderPrefix {
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		if f, ok := s.folders[fid]; ok {
			s.app.grid.ShowFolder(f)
		}
	}
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

	s.byYear = map[int][]model.Folder{}
	s.folders = map[int64]model.Folder{}
	s.counts = counts
	yearsSet := map[int]bool{}
	for _, f := range folders {
		s.folders[f.ID] = f
		s.byYear[f.Year] = append(s.byYear[f.Year], f)
		yearsSet[f.Year] = true
	}
	s.years = s.years[:0]
	for y := range yearsSet {
		s.years = append(s.years, y)
	}
	sort.Sort(sort.Reverse(sort.IntSlice(s.years)))

	s.tree.Refresh()
	s.tree.OpenBranch(rootFoldersID)

	// Auto-expand the most recent year and select its first folder so the grid
	// is populated instead of appearing empty after a scan.
	if len(s.years) > 0 {
		yearID := fmt.Sprintf("%s%d", yearPrefix, s.years[0])
		s.tree.OpenBranch(yearID)
		if fs := s.byYear[s.years[0]]; len(fs) > 0 {
			folderID := fmt.Sprintf("%s%d", folderPrefix, fs[0].ID)
			s.tree.Select(folderID)
		}
	}
}
