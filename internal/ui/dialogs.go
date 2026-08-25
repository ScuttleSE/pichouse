package ui

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

// selectPhoto shows a photo's details in the properties panel.
func (a *App) selectPhoto(p model.Photo) {
	a.properties.Show(p)
}

// showError displays an error in a modal dialog.
func (a *App) showError(err error) {
	a.showMessage(gtk.MessageError, "Error", err.Error())
}

// showInfo displays an informational message in a modal dialog.
func (a *App) showInfo(title, msg string) {
	a.showMessage(gtk.MessageInfo, title, msg)
}

func (a *App) showMessage(typ gtk.MessageType, title, detail string) {
	d := gtk.NewMessageDialog(&a.win.Window, gtk.DialogModal, typ, gtk.ButtonsClose)
	d.SetTitle(title)
	d.SetMarkup("<b>" + escapeMarkup(title) + "</b>\n" + escapeMarkup(detail))
	d.ConnectResponse(func(int) { d.Destroy() })
	d.SetVisible(true)
}
