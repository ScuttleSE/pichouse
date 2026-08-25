package ui

import (
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// installContextMenu is a no-op placeholder; per-row right-click gestures are
// attached in the factory setup (see attachRowMenu).
func (s *Sidebar) installContextMenu() {}

// attachRowMenu attaches a right-click gesture to a row's expander. The node id
// is read at click time from the expander's widget name (set during bind).
func (s *Sidebar) attachRowMenu(expander *gtk.TreeExpander) {
	click := gtk.NewGestureClick()
	click.SetButton(gdk.BUTTON_SECONDARY)
	click.ConnectPressed(func(_ int, x, y float64) {
		id := expander.Name()
		if id == "" {
			return
		}
		menu := s.buildRowPopover(id)
		if menu == nil {
			return
		}
		menu.SetParent(expander)
		rect := gdk.NewRectangle(int(x), int(y), 1, 1)
		menu.SetPointingTo(&rect)
		menu.Popup()
	})
	expander.AddController(click)
}

// buildRowPopover builds a context-menu popover for the given node id.
func (s *Sidebar) buildRowPopover(id string) *gtk.Popover {
	box := gtk.NewBox(gtk.OrientationVertical, 2)
	box.SetMarginTop(4)
	box.SetMarginBottom(4)
	box.SetMarginStart(4)
	box.SetMarginEnd(4)

	pop := gtk.NewPopover()
	pop.SetAutohide(true)
	pop.SetChild(box)

	add := func(label string, fn func()) {
		b := gtk.NewButton()
		lbl := gtk.NewLabel(label)
		lbl.SetXAlign(0)
		lbl.SetHExpand(true)
		b.SetChild(lbl)
		b.AddCSSClass("flat")
		b.SetHAlign(gtk.AlignFill)
		b.ConnectClicked(func() {
			pop.Popdown()
			fn()
		})
		box.Append(b)
	}

	switch {
	case strings.HasPrefix(id, albumPrefix):
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)
		add("New Sub-Album…", func() { s.promptCreateAlbum(aid) })
		add("Rename Album…", func() { s.promptRenameAlbum(aid) })
		add("Delete Album", func() { s.deleteAlbum(aid) })
		if len(s.selectedFolderIDs()) > 0 {
			add("Move selected here", func() { s.moveSelectedToAlbum(aid) })
		}
	case strings.HasPrefix(id, folderPrefix):
		fid, _ := strconv.ParseInt(id[len(folderPrefix):], 10, 64)
		// Ensure this folder is part of the operation set.
		targets := s.selectedFolderIDs()
		if len(targets) == 0 {
			targets = []int64{fid}
		}
		albums := s.albumsSorted()
		if len(albums) == 0 {
			add("(no albums — create one first)", func() {})
		} else {
			for _, al := range albums {
				aid := al.ID
				name := s.albumPathName(aid)
				add("Move to: "+name, func() { s.moveFoldersToAlbum(targets, aid) })
			}
		}
		add("Remove from Album", func() {
			for _, t := range targets {
				if err := s.app.lib.RemoveFolderFromAlbum(t); err != nil {
					s.app.showError(err)
					return
				}
			}
			s.Reload()
		})
	case id == newFoldersID:
		add("New Album…", func() { s.promptCreateAlbum(0) })
	default:
		return nil
	}
	return pop
}
