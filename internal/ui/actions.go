package ui

import (
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"

	"git.hemmalab.se/scuttle/pichouse/internal/scan"
)

// AddLibraryFolder prompts for a folder, records it, and scans it in the
// background with progress in the status bar.
func (a *App) AddLibraryFolder() {
	dialog.ShowFolderOpen(func(uri fyne.ListableURI, err error) {
		if err != nil || uri == nil {
			return
		}
		path := uri.Path()
		if _, err := a.lib.AddLibraryFolder(path); err != nil {
			dialog.ShowError(err, a.win)
			return
		}
		a.scanPath(path)
	}, a.win)
}

// RescanAll rescans every previously added library folder in the background.
func (a *App) RescanAll() {
	folders, err := a.lib.LibraryFolders()
	if err != nil {
		dialog.ShowError(err, a.win)
		return
	}
	if len(folders) == 0 {
		dialog.ShowInformation("Rescan", "No library folders to rescan.", a.win)
		return
	}
	go func() {
		s := scan.New(a.lib)
		for _, lf := range folders {
			a.runScan(s, lf.Path)
		}
		fyne.Do(func() {
			a.sidebar.Reload()
			a.status.SetProgress(-1)
			a.status.SetMessage("Rescan complete")
		})
	}()
}

// scanPath scans a single path in the background.
func (a *App) scanPath(path string) {
	go func() {
		s := scan.New(a.lib)
		a.runScan(s, path)
		fyne.Do(func() {
			a.sidebar.Reload()
			a.status.SetProgress(-1)
			a.status.SetMessage("Scan complete")
		})
	}()
}

// runScan performs one folder scan, updating the status bar. It must be called
// off the UI goroutine.
func (a *App) runScan(s *scan.Scanner, path string) {
	fyne.Do(func() {
		a.status.SetMessage("Scanning " + path)
		a.status.SetProgress(0)
	})
	_, err := s.ScanFolder(path, func(p scan.Progress) {
		frac := 0.0
		if p.Total > 0 {
			frac = float64(p.Done) / float64(p.Total)
		}
		msg := fmt.Sprintf("Scanning %s (%d/%d)", p.Folder, p.Done, p.Total)
		fyne.Do(func() {
			a.status.SetProgress(frac)
			a.status.SetMessage(msg)
		})
	})
	if err != nil {
		fyne.Do(func() { dialog.ShowError(err, a.win) })
	}
}
