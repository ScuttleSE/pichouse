# HANDOFF

Running handoff notes for pichouse. Update this when the working context gets large.

## Current state (step 1 — scaffold)

- Module: `git.hemmalab.se/scuttle/pichouse` (Go).
- Scaffold created:
  - `cmd/pichouse/main.go` — entry point, calls `ui.Run()`.
  - `internal/version/version.go` — `Version = "0.1.0"` (read by CI).
  - `internal/ui/app.go` — minimal Fyne window (placeholder content).
  - `.gitea/workflows/build.yaml` — adapted from template (pichouse binary).
  - `.gitignore` — ignores dist/, *.db, and the never-commit files
    `build.yaml` (template) and `picasa.png`.
  - `AGENTS.md`, `README.md` — populated.

## Decisions locked

- GUI: Fyne v2, default X11/GLFW driver (XWayland on Wayland).
- DB: two SQLite files (`library.db`, `thumbs.db`) via `modernc.org/sqlite`.
- EXIF: `github.com/rwcarlsen/goexif`.
- CI: build-only on `debian-go` runner (this machine), rolling pre-release.
- System deps installed on host: gcc, libgl1-mesa-dev, xorg-dev, libxxf86vm-dev.

## Next steps

2. DB layer + schema (library.db + thumbs.db).
3. Model + scanner (walk, upsert, EXIF).
4. Thumbnail generator + cache (worker pool).
5. UI scaffold: 3-pane layout + toolbar + status bar.
6. Sidebar tree from DB.
7. Grid + lazy thumbnails + size slider.
8. Properties panel.
9. Add Library Folder + background scan with progress.
10. Raw Folder View mode.
11. Cleanup: delete build.yaml + picasa.png.

## Open questions

- None currently.
