package ui

import (
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

// onUI schedules fn to run on the GTK main thread. Safe to call from any
// goroutine. This is the GTK equivalent of Fyne's fyne.Do.
func onUI(fn func()) {
	glib.IdleAdd(fn)
}

// escapeMarkup escapes text for safe use in Pango markup.
func escapeMarkup(s string) string {
	r := strings.NewReplacer(
		"&", "&amp;",
		"<", "&lt;",
		">", "&gt;",
		"'", "&#39;",
		"\"", "&quot;",
	)
	return r.Replace(s)
}
