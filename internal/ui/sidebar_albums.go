package ui

import (
	"sort"

	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// promptCreateAlbum asks for a name and creates an album under parentID (0 for
// top-level).
func (s *Sidebar) promptCreateAlbum(parentID int64) {
	entry := widget.NewEntry()
	entry.SetPlaceHolder("Album name")
	title := "New Album"
	if parentID != 0 {
		title = "New Sub-Album"
	}
	dialog.ShowForm(title, "Create", "Cancel",
		[]*widget.FormItem{{Text: "Name", Widget: entry}},
		func(ok bool) {
			if !ok || entry.Text == "" {
				return
			}
			if _, err := s.app.lib.CreateAlbum(entry.Text, parentID); err != nil {
				dialog.ShowError(err, s.app.win)
				return
			}
			s.Reload()
		}, s.app.win)
}

// promptRenameAlbum asks for a new name for an album.
func (s *Sidebar) promptRenameAlbum(id int64) {
	entry := widget.NewEntry()
	entry.SetText(s.albums[id].Name)
	dialog.ShowForm("Rename Album", "Rename", "Cancel",
		[]*widget.FormItem{{Text: "Name", Widget: entry}},
		func(ok bool) {
			if !ok || entry.Text == "" {
				return
			}
			if err := s.app.lib.RenameAlbum(id, entry.Text); err != nil {
				dialog.ShowError(err, s.app.win)
				return
			}
			s.Reload()
		}, s.app.win)
}

// deleteAlbum confirms and deletes an album (folders revert to New folders).
func (s *Sidebar) deleteAlbum(id int64) {
	dialog.ShowConfirm("Delete Album",
		"Delete album \""+s.albums[id].Name+"\"?\nIts folders return to New folders; sub-albums are also deleted.",
		func(ok bool) {
			if !ok {
				return
			}
			if err := s.app.lib.DeleteAlbum(id); err != nil {
				dialog.ShowError(err, s.app.win)
				return
			}
			s.Reload()
		}, s.app.win)
}

// moveSelectedToAlbum moves all currently selected folders into album target.
func (s *Sidebar) moveSelectedToAlbum(target int64) {
	fids := s.selectedFolderIDs()
	if len(fids) == 0 {
		return
	}
	for _, fid := range fids {
		if err := s.app.lib.AddFolderToAlbum(fid, target); err != nil {
			dialog.ShowError(err, s.app.win)
			return
		}
	}
	s.selected = map[string]bool{}
	s.Reload()
}

// removeSelectedFromAlbum detaches all selected folders back to New folders.
func (s *Sidebar) removeSelectedFromAlbum() {
	fids := s.selectedFolderIDs()
	for _, fid := range fids {
		if err := s.app.lib.RemoveFolderFromAlbum(fid); err != nil {
			dialog.ShowError(err, s.app.win)
			return
		}
	}
	s.selected = map[string]bool{}
	s.Reload()
}

// albumsSorted returns all albums in a stable order for menus.
func (s *Sidebar) albumsSorted() []model.Album {
	out := make([]model.Album, 0, len(s.albums))
	for _, a := range s.albums {
		out = append(out, a)
	}
	sort.Slice(out, func(i, j int) bool {
		return s.albumPathName(out[i].ID) < s.albumPathName(out[j].ID)
	})
	return out
}

// albumPathName returns an album's name qualified by its ancestor path, e.g.
// "Trips / 2019".
func (s *Sidebar) albumPathName(id int64) string {
	a, ok := s.albums[id]
	if !ok {
		return ""
	}
	name := a.Name
	for a.ParentID != 0 {
		p, ok := s.albums[a.ParentID]
		if !ok {
			break
		}
		name = p.Name + " / " + name
		a = p
	}
	return name
}
