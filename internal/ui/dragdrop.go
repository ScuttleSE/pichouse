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

// attachRowDrag makes a row both a drag source (when it is a folder) and a drop
// target (when it is an album). The node id travels as a string GValue; the
// drop resolves it to the folders to move.
func (s *Sidebar) attachRowDrag(expander *gtk.TreeExpander) {
	// Drag source: dragging a folder row carries the current selection.
	src := gtk.NewDragSource()
	src.SetActions(gdk.ActionMove)
	src.ConnectPrepare(func(_, _ float64) *gdk.ContentProvider {
		id := expander.Name()
		if !strings.HasPrefix(id, folderPrefix) {
			return nil
		}
		// If the dragged folder is not part of the selection, drag it alone.
		payload := id
		val := coreglib.NewValue(payload)
		return gdk.NewContentProviderForValue(val)
	})
	expander.AddController(src)

	// Drop target: album rows accept dropped folders.
	tgt := gtk.NewDropTarget(glib.TypeString, gdk.ActionMove)
	tgt.ConnectDrop(func(value *coreglib.Value, _, _ float64) bool {
		id := expander.Name()
		if !strings.HasPrefix(id, albumPrefix) {
			return false
		}
		aid, _ := strconv.ParseInt(id[len(albumPrefix):], 10, 64)

		dragged := value.String() // the source node id string
		fids := s.selectedFolderIDs()
		if len(fids) == 0 && strings.HasPrefix(dragged, folderPrefix) {
			fid, _ := strconv.ParseInt(dragged[len(folderPrefix):], 10, 64)
			fids = []int64{fid}
		}
		if len(fids) == 0 {
			return false
		}
		s.moveFoldersToAlbum(fids, aid)
		return true
	})
	expander.AddController(tgt)
}
