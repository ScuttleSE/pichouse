package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// StatusBar is the bottom status bar showing scan progress and selection info.
type StatusBar struct {
	container *fyne.Container
	message   *widget.Label
	progress  *widget.ProgressBar
}

func newStatusBar() *StatusBar {
	s := &StatusBar{
		message:  widget.NewLabel("Ready"),
		progress: widget.NewProgressBar(),
	}
	s.progress.Hide()
	bar := container.NewBorder(nil, nil, s.message, nil,
		container.NewGridWrap(fyne.NewSize(220, s.progress.MinSize().Height), s.progress))
	s.container = container.NewVBox(widget.NewSeparator(), container.NewPadded(bar))
	return s
}

// Container returns the status bar's root widget.
func (s *StatusBar) Container() *fyne.Container { return s.container }

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
