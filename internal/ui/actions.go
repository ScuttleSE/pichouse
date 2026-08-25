package ui

import (
	"context"
	"errors"
	"fmt"
	"sync"

	"git.hemmalab.se/scuttle/pichouse/internal/scan"
)

// scanController tracks the currently running background scan so it can be
// cancelled from the UI.
type scanController struct {
	mu     sync.Mutex
	cancel context.CancelFunc
}

func (sc *scanController) begin() context.Context {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	if sc.cancel != nil {
		sc.cancel()
	}
	ctx, cancel := context.WithCancel(context.Background())
	sc.cancel = cancel
	return ctx
}

func (sc *scanController) finish() {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	sc.cancel = nil
}

// Stop cancels a running scan, if any.
func (sc *scanController) Stop() {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	if sc.cancel != nil {
		sc.cancel()
	}
}

// AddLibraryFolder records a folder and scans it in the background.
func (a *App) AddLibraryFolder(path string) {
	if _, err := a.lib.AddLibraryFolder(path); err != nil {
		a.showError(err)
		return
	}
	a.sidebar.Reload()
	a.folderTree.Reload()
	a.scanPaths([]string{path})
}

// RescanAll rescans every previously added library folder in the background.
func (a *App) RescanAll() {
	folders, err := a.lib.LibraryFolders()
	if err != nil {
		a.showError(err)
		return
	}
	if len(folders) == 0 {
		a.showInfo("Rescan", "No library folders to rescan.")
		return
	}
	paths := make([]string, 0, len(folders))
	for _, lf := range folders {
		paths = append(paths, lf.Path)
	}
	a.scanPaths(paths)
}

// scanPaths scans one or more paths in the background under a single cancellable
// scan session, updating the status bar and refreshing the UI when done.
func (a *App) scanPaths(paths []string) {
	ctx := a.scan.begin()
	a.status.SetScanning(true)
	go func() {
		defer a.scan.finish()
		s := scan.New(a.lib)
		var scanErr error
		for _, path := range paths {
			if err := a.runScan(ctx, s, path); err != nil {
				scanErr = err
				break
			}
		}
		onUI(func() {
			a.status.SetScanning(false)
			a.status.SetProgress(-1)
			a.sidebar.Reload()
			a.grid.RefreshVisible()
			switch {
			case errors.Is(scanErr, context.Canceled):
				a.status.SetMessage("Scan stopped")
			case scanErr != nil:
				a.status.SetMessage("Scan failed")
				a.showError(scanErr)
			default:
				a.status.SetMessage("Scan complete")
			}
		})
	}()
}

// runScan performs one folder scan, updating the status bar. It must be called
// off the UI goroutine. It returns ctx.Err() if cancelled.
func (a *App) runScan(ctx context.Context, s *scan.Scanner, path string) error {
	onUI(func() {
		a.status.SetMessage("Scanning " + path)
		a.status.SetProgress(0)
	})
	_, err := s.ScanFolderContext(ctx, path, func(p scan.Progress) {
		frac := 0.0
		if p.Total > 0 {
			frac = float64(p.Done) / float64(p.Total)
		}
		msg := fmt.Sprintf("Scanning %s (%d/%d)", p.Folder, p.Done, p.Total)
		onUI(func() {
			a.status.SetProgress(frac)
			a.status.SetMessage(msg)
		})
	})
	return err
}
