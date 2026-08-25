package ui

import (
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// StatusBar is the bottom status bar showing scan progress and a Stop button.
type StatusBar struct {
	app      *App
	box      *gtk.Box
	message  *gtk.Label
	progress *gtk.ProgressBar
	stop     *gtk.Button
}

func newStatusBar(a *App) *StatusBar {
	s := &StatusBar{app: a}
	s.message = gtk.NewLabel("Ready")
	s.message.SetXAlign(0)
	s.message.SetHExpand(true)

	s.progress = gtk.NewProgressBar()
	s.progress.SetSizeRequest(220, -1)
	s.progress.SetVisible(false)

	s.stop = gtk.NewButtonWithLabel("Stop")
	s.stop.SetTooltipText("Stop scanning")
	s.stop.AddCSSClass("destructive-action")
	s.stop.SetVisible(false)
	s.stop.ConnectClicked(func() {
		a.scan.Stop()
		a.aiJob.Stop()
	})

	box := gtk.NewBox(gtk.OrientationHorizontal, 6)
	box.SetMarginTop(4)
	box.SetMarginBottom(4)
	box.SetMarginStart(6)
	box.SetMarginEnd(6)
	box.Append(s.message)
	box.Append(s.progress)
	box.Append(s.stop)
	s.box = box
	return s
}

// Widget returns the status bar's root widget.
func (s *StatusBar) Widget() gtk.Widgetter { return s.box }

// SetMessage updates the status message. Must be called on the GTK thread.
func (s *StatusBar) SetMessage(msg string) { s.message.SetText(msg) }

// SetScanning toggles the Stop button.
func (s *StatusBar) SetScanning(scanning bool) { s.stop.SetVisible(scanning) }

// SetProgress shows determinate progress in [0,1]. A value < 0 hides the bar.
func (s *StatusBar) SetProgress(v float64) {
	if v < 0 {
		s.progress.SetVisible(false)
		return
	}
	s.progress.SetVisible(true)
	s.progress.SetFraction(v)
}
