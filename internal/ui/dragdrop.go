package ui

import (
	"strconv"
	"strings"

	coreglib "github.com/diamondburned/gotk4/pkg/core/glib"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// installDragDrop is a no-op placeholder; per-row sources/targets are attached
// in the factory setup.
func (s *Sidebar) installDragDrop() {}

// attachRowDrag makes folder and album rows draggable, and makes album rows drop
// targets. Dropping folders onto an album moves them into it; dropping an album
// onto an album makes it a sub-album. The dragged node id travels as a string.
func (s *Sidebar) attachRowDrag(expander *gtk.TreeExpander) {
	// Drag source: folders and albums can be dragged.
	src := gtk.NewDragSource()
	src.SetActions(gdk.ActionMove)
	src.ConnectPrepare(func(_, _ float64) *gdk.ContentProvider {
		id := expander.Name()
		if !strings.HasPrefix(id, folderPrefix) && !strings.HasPrefix(id, albumPrefix) {
			return nil
		}
		val := coreglib.NewValue(id)
		return gdk.NewContentProviderForValue(val)
	})
	expander.AddController(src)

	// Drop target: album rows accept folders (move into album) and other albums
	// (make sub-album).
	tgt := gtk.NewDropTarget(glib.TypeString, gdk.ActionMove)
	tgt.ConnectDrop(func(value *coreglib.Value, _, _ float64) bool {
		targetID := expander.Name()
		if !strings.HasPrefix(targetID, albumPrefix) {
			return false
		}
		targetAlbum, _ := strconv.ParseInt(targetID[len(albumPrefix):], 10, 64)
		dragged := value.String()

		switch {
		case strings.HasPrefix(dragged, albumPrefix):
			// Re-parent the dragged album under the target album.
			srcAlbum, _ := strconv.ParseInt(dragged[len(albumPrefix):], 10, 64)
			if srcAlbum == targetAlbum {
				return false
			}
			if err := s.app.lib.SetAlbumParent(srcAlbum, targetAlbum); err != nil {
				s.app.showError(err)
				return false
			}
			s.markExpanded(targetID)
			s.Reload()
			return true

		case strings.HasPrefix(dragged, folderPrefix):
			fids := s.selectedFolderIDs()
			if len(fids) == 0 {
				fid, _ := strconv.ParseInt(dragged[len(folderPrefix):], 10, 64)
				fids = []int64{fid}
			}
			if len(fids) == 0 {
				return false
			}
			s.moveFoldersToAlbum(fids, targetAlbum)
			return true
		}
		return false
	})
	expander.AddController(tgt)
}
