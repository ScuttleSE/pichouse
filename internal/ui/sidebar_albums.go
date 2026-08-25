package ui

import (
	"strconv"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// promptText shows a modal dialog with a single text entry and calls onOK with
// the entered text when confirmed.
func (a *App) promptText(title, label, initial string, onOK func(string)) {
	win := gtk.NewWindow()
	win.SetTitle(title)
	win.SetModal(true)
	win.SetTransientFor(&a.win.Window)
	win.SetDefaultSize(360, -1)

	entry := gtk.NewEntry()
	entry.SetText(initial)
	entry.SetHExpand(true)
	entry.SetActivatesDefault(true)

	cancel := gtk.NewButtonWithLabel("Cancel")
	ok := gtk.NewButtonWithLabel("OK")
	ok.AddCSSClass("suggested-action")

	cancel.ConnectClicked(func() { win.Destroy() })
	ok.ConnectClicked(func() {
		text := entry.Text()
		win.Destroy()
		if text != "" {
			onOK(text)
		}
	})
	entry.ConnectActivate(func() {
		text := entry.Text()
		win.Destroy()
		if text != "" {
			onOK(text)
		}
	})

	buttons := gtk.NewBox(gtk.OrientationHorizontal, 6)
	buttons.SetHAlign(gtk.AlignEnd)
	buttons.Append(cancel)
	buttons.Append(ok)

	form := gtk.NewBox(gtk.OrientationVertical, 8)
	form.SetMarginTop(12)
	form.SetMarginBottom(12)
	form.SetMarginStart(12)
	form.SetMarginEnd(12)
	lbl := gtk.NewLabel(label)
	lbl.SetXAlign(0)
	form.Append(lbl)
	form.Append(entry)
	form.Append(buttons)

	win.SetChild(form)
	win.SetVisible(true)
}

// confirm shows a yes/no modal and calls onYes if confirmed.
func (a *App) confirm(title, detail string, onYes func()) {
	d := gtk.NewMessageDialog(&a.win.Window, gtk.DialogModal, gtk.MessageQuestion, gtk.ButtonsYesNo)
	d.SetTitle(title)
	d.SetMarkup("<b>" + escapeMarkup(title) + "</b>\n" + escapeMarkup(detail))
	d.ConnectResponse(func(resp int) {
		if resp == int(gtk.ResponseYes) {
			onYes()
		}
		d.Destroy()
	})
	d.SetVisible(true)
}

func (s *Sidebar) promptCreateAlbum(parentID int64) {
	title := "New Album"
	if parentID != 0 {
		title = "New Sub-Album"
	}
	s.app.promptText(title, "Album name:", "", func(name string) {
		if _, err := s.app.lib.CreateAlbum(name, parentID); err != nil {
			s.app.showError(err)
			return
		}
		// Keep the parent album open so the new sub-album is visible.
		if parentID != 0 {
			s.markExpanded(albumPrefix + strconv.FormatInt(parentID, 10))
		}
		s.Reload()
	})
}

func (s *Sidebar) promptRenameAlbum(id int64) {
	s.app.promptText("Rename Album", "Album name:", s.albums[id].Name, func(name string) {
		if err := s.app.lib.RenameAlbum(id, name); err != nil {
			s.app.showError(err)
			return
		}
		s.Reload()
	})
}

func (s *Sidebar) deleteAlbum(id int64) {
	s.app.confirm("Delete Album",
		"Delete album \""+s.albums[id].Name+"\"? Its folders return to New folders; sub-albums are also deleted.",
		func() {
			if err := s.app.lib.DeleteAlbum(id); err != nil {
				s.app.showError(err)
				return
			}
			s.Reload()
		})
}

// moveFoldersToAlbum moves the given folders into album target.
func (s *Sidebar) moveFoldersToAlbum(fids []int64, target int64) {
	if len(fids) == 0 {
		return
	}
	for _, fid := range fids {
		if err := s.app.lib.AddFolderToAlbum(fid, target); err != nil {
			s.app.showError(err)
			return
		}
	}
	// Keep the destination album open so the moved folders remain visible.
	s.markExpanded(albumPrefix + strconv.FormatInt(target, 10))
	s.Reload()
}

// moveSelectedToAlbum moves all selected folders into album target.
func (s *Sidebar) moveSelectedToAlbum(target int64) {
	s.moveFoldersToAlbum(s.selectedFolderIDs(), target)
}

// removeSelectedFromAlbum detaches all selected folders back to New folders.
func (s *Sidebar) removeSelectedFromAlbum() {
	fids := s.selectedFolderIDs()
	for _, fid := range fids {
		if err := s.app.lib.RemoveFolderFromAlbum(fid); err != nil {
			s.app.showError(err)
			return
		}
	}
	s.Reload()
}
