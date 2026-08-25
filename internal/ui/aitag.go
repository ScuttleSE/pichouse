package ui

import (
	"context"
	"errors"
	"fmt"
	"log"
	"sync"

	"git.hemmalab.se/scuttle/pichouse/internal/ai"
	"git.hemmalab.se/scuttle/pichouse/internal/model"
	"git.hemmalab.se/scuttle/pichouse/internal/thumb"
)

// aiController tracks the currently running background AI tagging session so it
// can be cancelled from the UI. It mirrors scanController.
type aiController struct {
	mu     sync.Mutex
	cancel context.CancelFunc
}

func (ac *aiController) begin() context.Context {
	ac.mu.Lock()
	defer ac.mu.Unlock()
	if ac.cancel != nil {
		ac.cancel()
	}
	ctx, cancel := context.WithCancel(context.Background())
	ac.cancel = cancel
	return ctx
}

func (ac *aiController) finish() {
	ac.mu.Lock()
	defer ac.mu.Unlock()
	ac.cancel = nil
}

func (ac *aiController) running() bool {
	ac.mu.Lock()
	defer ac.mu.Unlock()
	return ac.cancel != nil
}

// Stop cancels a running AI tagging session, if any.
func (ac *aiController) Stop() {
	ac.mu.Lock()
	defer ac.mu.Unlock()
	if ac.cancel != nil {
		ac.cancel()
	}
}

// AITagLibrary tags every untagged photo across all folders in the background.
func (a *App) AITagLibrary() { a.startAITagging(0) }

// AITagFolder tags untagged photos in the currently shown folder.
func (a *App) AITagFolder() {
	if a.grid.folder == nil {
		a.showInfo("AI Tagging", "Open a library folder first, or use \"Tag Library\".")
		return
	}
	a.startAITagging(a.grid.folder.ID)
}

// startAITagging runs a cancellable background tagging session over the photos
// needing tags. folderID <= 0 means the whole library.
func (a *App) startAITagging(folderID int64) {
	if !a.aiConfig.Enabled {
		a.showInfo("AI Tagging", "AI tagging is disabled. Enable it in Settings → AI Tagging.")
		return
	}
	if a.aiJob.running() {
		a.showInfo("AI Tagging", "AI tagging is already running.")
		return
	}

	ids, err := a.lib.PhotosNeedingTags(folderID, false)
	if err != nil {
		a.showError(err)
		return
	}
	if len(ids) == 0 {
		a.showInfo("AI Tagging", "No untagged photos found.")
		return
	}

	cfg := a.aiConfig
	client := ai.NewClient(cfg.Host, cfg.Port)
	ctx := a.aiJob.begin()

	a.status.SetScanning(true)
	a.status.SetMessage("Contacting AI server…")
	a.status.SetProgress(0)
	log.Printf("[ai] starting tagging: %d photo(s), model=%q host=%s:%d concurrency=%d",
		len(ids), cfg.Model, cfg.Host, cfg.Port, cfg.Concurrency)

	go func() {
		defer a.aiJob.finish()

		// Ensure a server is reachable (optionally launching one).
		log.Printf("[ai] ensuring server is reachable…")
		if err := a.aiManager.EnsureRunning(ctx, cfg, client); err != nil {
			log.Printf("[ai] server unavailable: %v", err)
			onUI(func() {
				a.status.SetScanning(false)
				a.status.SetProgress(-1)
				a.status.SetMessage("AI tagging unavailable")
				a.showError(err)
			})
			return
		}
		log.Printf("[ai] server reachable; first request may be slow while the model loads")
		onUI(func() {
			a.status.SetMessage(fmt.Sprintf("Loading model %q… (0/%d)", cfg.Model, len(ids)))
		})

		total := len(ids)
		var done, errs int
		var mu sync.Mutex

		// started tracks the highest photo index handed to a worker, so the
		// status bar shows activity even before the first inference finishes
		// (the initial model load can take a while).
		var started int

		jobs := make(chan int64)
		var wg sync.WaitGroup
		for i := 0; i < cfg.Concurrency; i++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				for id := range jobs {
					if ctx.Err() != nil {
						return
					}
					mu.Lock()
					started++
					s := started
					mu.Unlock()
					onUI(func() {
						a.status.SetMessage(fmt.Sprintf("AI tagging %d/%d…", s, total))
					})

					ok := a.tagOnePhoto(ctx, client, cfg, id)

					mu.Lock()
					done++
					if !ok {
						errs++
					}
					d, e := done, errs
					mu.Unlock()
					frac := float64(d) / float64(total)
					onUI(func() {
						a.status.SetProgress(frac)
						if e > 0 {
							a.status.SetMessage(fmt.Sprintf("AI tagging %d/%d (%d failed)", d, total, e))
						} else {
							a.status.SetMessage(fmt.Sprintf("AI tagging %d/%d", d, total))
						}
					})
				}
			}()
		}

		for _, id := range ids {
			if ctx.Err() != nil {
				break
			}
			jobs <- id
		}
		close(jobs)
		wg.Wait()
		log.Printf("[ai] tagging finished: %d done, %d failed", done, errs)

		onUI(func() {
			a.status.SetScanning(false)
			a.status.SetProgress(-1)
			switch {
			case errors.Is(ctx.Err(), context.Canceled):
				a.status.SetMessage(fmt.Sprintf("AI tagging stopped (%d/%d done)", done, total))
			case errs > 0:
				a.status.SetMessage(fmt.Sprintf("AI tagging complete: %d tagged, %d failed", done-errs, errs))
			default:
				a.status.SetMessage(fmt.Sprintf("AI tagging complete: %d tagged", done))
			}
			// Refresh the properties panel for the selected photo.
			a.refreshSelectedTags()
		})
	}()
}

// tagOnePhoto runs inference for a single photo id and stores the resulting
// tags. Errors are recorded as AIError; success as AIDone. It returns true on
// success (including "ran but produced no tags") and false on error.
func (a *App) tagOnePhoto(ctx context.Context, client *ai.Client, cfg ai.Config, id int64) bool {
	p, err := a.lib.PhotoByID(id)
	if err != nil {
		log.Printf("[ai] photo %d: load failed: %v", id, err)
		return false
	}
	_ = a.lib.SetAIStatus(id, model.AIQueued)

	img, err := thumb.EncodeForAI(p.Path, p.Orientation, cfg.MaxSide)
	if err != nil {
		log.Printf("[ai] %s: encode failed: %v", p.Filename, err)
		_ = a.lib.SetAIStatus(id, model.AIError)
		return false
	}
	resp, err := client.Generate(ctx, cfg.Model, cfg.Prompt, img, cfg.NumThread, cfg.KeepAlive)
	if err != nil {
		if ctx.Err() == nil {
			log.Printf("[ai] %s: inference failed: %v", p.Filename, err)
			_ = a.lib.SetAIStatus(id, model.AIError)
		}
		return false
	}
	tags := ai.ParseTags(resp, cfg.MaxTags)
	if len(tags) == 0 {
		log.Printf("[ai] %s: no tags parsed from response", p.Filename)
		_ = a.lib.SetAIStatus(id, model.AIDone)
		return true
	}
	if err := a.lib.AddPhotoTags(id, tags, model.TagSourceAI); err != nil {
		log.Printf("[ai] %s: store tags failed: %v", p.Filename, err)
		_ = a.lib.SetAIStatus(id, model.AIError)
		return false
	}
	log.Printf("[ai] %s: %d tags: %v", p.Filename, len(tags), tags)
	_ = a.lib.SetAIStatus(id, model.AIDone)
	return true
}

// tagOnePhotoNow runs inference for a single photo synchronously in the
// background and refreshes the properties panel. Used by the per-photo
// "Tag now" button.
func (a *App) tagOnePhotoNow(p model.Photo) {
	if !a.aiConfig.Enabled {
		a.showInfo("AI Tagging", "AI tagging is disabled. Enable it in Settings → AI Tagging.")
		return
	}
	cfg := a.aiConfig
	client := ai.NewClient(cfg.Host, cfg.Port)
	ctx := context.Background()
	a.status.SetMessage("Tagging " + p.Filename + "…")
	go func() {
		if err := a.aiManager.EnsureRunning(ctx, cfg, client); err != nil {
			onUI(func() {
				a.status.SetMessage("AI tagging unavailable")
				a.showError(err)
			})
			return
		}
		a.tagOnePhoto(ctx, client, cfg, p.ID)
		onUI(func() {
			a.status.SetMessage("Tagged " + p.Filename)
			a.refreshSelectedTags()
		})
	}()
}
