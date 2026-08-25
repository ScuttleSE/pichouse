package ui

import (
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Action names in the "sidebar" group. Each takes a single string target: the
// node id it operates on (e.g. "album:3" or "folder:12"). Folder operations act
// on the current multi-selection, falling back to the target folder.
const (
	actNewAlbum    = "new-album"     // target ignored
	actNewSubAlbum = "new-subalbum"  // target: album id
	actRenameAlbum = "rename-album"  // target: album id
	actDeleteAlbum = "delete-album"  // target: album id
	actMoveToAlbum = "move-to-album" // target: album id (dest); moves selection
	actRemoveAlbum = "remove-folder" // target: folder id
)

// installContextMenu creates the action group backing the right-click menu and
// a single reusable GtkPopoverMenu. Using GMenu + GActions means the menu is
// styled by GTK's own menu machinery, which themes correctly.
func (s *Sidebar) installContextMenu() {
	group := gio.NewSimpleActionGroup()

	addAction := func(name string, fn func(target string)) {
		act := gio.NewSimpleAction(name, glib.NewVariantType("s"))
		act.ConnectActivate(func(param *glib.Variant) {
			target := ""
			if param != nil {
				target = param.String()
			}
			fn(target)
		})
		group.AddAction(act)
	}

	addAction(actNewAlbum, func(string) { s.promptCreateAlbum(0) })
	addAction(actNewSubAlbum, func(t string) { s.promptCreateAlbum(albumIDOf(t)) })
	addAction(actRenameAlbum, func(t string) { s.promptRenameAlbum(albumIDOf(t)) })
	addAction(actDeleteAlbum, func(t string) { s.deleteAlbum(albumIDOf(t)) })
	addAction(actMoveToAlbum, func(t string) { s.moveSelectedOrOne(albumIDOf(t)) })
	addAction(actRemoveAlbum, func(t string) { s.removeSelectedOrOne(folderIDOf(t)) })

	s.actions = group

	// The list view hosts the action group so the menu's actions resolve under
	// the "sidebar" prefix.
	s.listView.InsertActionGroup("sidebar", group)
	s.menuPop = gtk.NewPopoverMenuFromModel(nil)
	s.menuPop.SetHasArrow(false)
}

// attachRowMenu attaches a right-click gesture to a row's expander. On press it
// re-models the shared popover for the clicked node and pops it up there.
func (s *Sidebar) attachRowMenu(expander *gtk.TreeExpander) {
	click := gtk.NewGestureClick()
	click.SetButton(gdk.BUTTON_SECONDARY)
	click.ConnectPressed(func(_ int, x, y float64) {
		id := expander.Name()
		if id == "" {
			return
		}
		s.showRowMenu(id, expander, x, y)
	})
	expander.AddController(click)
}

// showRowMenu builds the GMenu for the node id, points the shared popover at the
// click location within the expander (translated to list-view coordinates), and
// pops it up.
func (s *Sidebar) showRowMenu(id string, expander *gtk.TreeExpander, x, y float64) {
	menu := s.buildRowMenu(id)
	if menu == nil {
		return
	}
	s.menuNode = id
	s.menuPop.SetMenuModel(menu)

	// Translate the click point from the expander to the list view's coords.
	if px, py, ok := expander.TranslateCoordinates(s.listView, x, y); ok {
		rect := gdk.NewRectangle(int(px), int(py), 1, 1)
		s.menuPop.SetPointingTo(&rect)
	}
	s.menuPop.Popup()
}

// buildRowMenu returns the GMenu model for a node id, or nil if none applies.
func (s *Sidebar) buildRowMenu(id string) *gio.Menu {
	menu := gio.NewMenu()
	switch {
	case strings.HasPrefix(id, albumPrefix):
		menu.Append("New Sub-Album…", detailed(actNewSubAlbum, id))
		menu.Append("Rename Album…", detailed(actRenameAlbum, id))
		menu.Append("Delete Album", detailed(actDeleteAlbum, id))
		if len(s.selectedFolderIDs()) > 0 {
			menu.Append("Move selected here", detailed(actMoveToAlbum, id))
		}
	case strings.HasPrefix(id, folderPrefix):
		albums := s.albumsSorted()
		if len(albums) == 0 {
			// A disabled hint is awkward with GMenu; offer album creation.
			menu.Append("New Album…", detailed(actNewAlbum, id))
		} else {
			moveSection := gio.NewMenu()
			for _, al := range albums {
				aid := albumPrefix + strconv.FormatInt(al.ID, 10)
				moveSection.Append("Move to: "+s.albumPathName(al.ID), detailed(actMoveToAlbum, aid))
			}
			menu.AppendSection("", moveSection)
		}
		menu.Append("Remove from Album", detailed(actRemoveAlbum, id))
	case id == newFoldersID:
		menu.Append("New Album…", detailed(actNewAlbum, id))
	default:
		return nil
	}
	return menu
}

// detailed builds a "sidebar.<action>::<target>" detailed action string with the
// target passed as a string parameter.
func detailed(action, target string) string {
	// Use printf-style detailed action with a quoted string target.
	return "sidebar." + action + "::" + target
}

// moveSelectedOrOne moves the current folder selection (or, if empty, the folder
// whose menu was opened) into album target.
func (s *Sidebar) moveSelectedOrOne(target int64) {
	fids := s.selectedFolderIDs()
	if len(fids) == 0 {
		if fid := folderIDOf(s.menuNode); fid != 0 {
			fids = []int64{fid}
		}
	}
	s.moveFoldersToAlbum(fids, target)
}

// removeSelectedOrOne removes the current folder selection (or the menu's folder)
// from any album.
func (s *Sidebar) removeSelectedOrOne(one int64) {
	fids := s.selectedFolderIDs()
	if len(fids) == 0 && one != 0 {
		fids = []int64{one}
	}
	for _, fid := range fids {
		if err := s.app.lib.RemoveFolderFromAlbum(fid); err != nil {
			s.app.showError(err)
			return
		}
	}
	s.Reload()
}

// albumIDOf parses an "album:<n>" node id, returning 0 if it is not one.
func albumIDOf(id string) int64 {
	if !strings.HasPrefix(id, albumPrefix) {
		return 0
	}
	n, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
	return n
}

// folderIDOf parses a "folder:<n>" node id, returning 0 if it is not one.
func folderIDOf(id string) int64 {
	if !strings.HasPrefix(id, folderPrefix) {
		return 0
	}
	n, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
	return n
}
