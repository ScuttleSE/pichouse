package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// StatusBar is the bottom status bar showing scan progress and selection info.
type StatusBar struct {
	container *fyne.Container
	message   *widget.Label
	progress  *widget.ProgressBar
	stop      *widget.Button
	onStop    func()
}

func newStatusBar() *StatusBar {
	s := &StatusBar{
		message:  widget.NewLabel("Ready"),
		progress: widget.NewProgressBar(),
	}
	s.stop = widget.NewButtonWithIcon("Stop", theme.CancelIcon(), func() {
		if s.onStop != nil {
			s.onStop()
		}
	})
	s.stop.Importance = widget.DangerImportance
	s.progress.Hide()
	s.stop.Hide()

	right := container.NewHBox(
		container.NewGridWrap(fyne.NewSize(220, s.progress.MinSize().Height), s.progress),
		s.stop,
	)
	bar := container.NewBorder(nil, nil, s.message, right, nil)
	s.container = container.NewVBox(widget.NewSeparator(), container.NewPadded(bar))
	return s
}

// Container returns the status bar's root widget.
func (s *StatusBar) Container() *fyne.Container { return s.container }

// SetOnStop sets the handler invoked when the Stop button is pressed.
func (s *StatusBar) SetOnStop(fn func()) { s.onStop = fn }

// SetScanning toggles the visibility of the Stop button.
func (s *StatusBar) SetScanning(scanning bool) {
	if scanning {
		s.stop.Show()
	} else {
		s.stop.Hide()
	}
}

// SetMessage updates the status message. Safe to call from any goroutine via
// fyne.Do at the call site.
func (s *StatusBar) SetMessage(msg string) {
	s.message.SetText(msg)
}

// SetProgress shows determinate progress in [0,1]. A value < 0 hides the bar.
func (s *StatusBar) SetProgress(v float64) {
	if v < 0 {
		s.progress.Hide()
		return
	}
	s.progress.Show()
	s.progress.SetValue(v)
}
