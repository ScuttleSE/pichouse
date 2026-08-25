# HANDOFF

Running handoff notes for pichouse. Update this when the working context gets large.

## Current state — Milestone 1 complete

All milestone-1 features are built, tested, committed, and pushed to `main`.

### Implemented

- **DB layer** (`internal/db`): two SQLite files via `modernc.org/sqlite`.
  - `library.db`: `library_folders`, `folders`, `photos`, `scan_state`.
  - `thumbs.db`: `thumbnails(photo_hash PK, size, jpeg, created_at)`.
  - Access helpers + `db_test.go` (round-trip tests).
- **Model** (`internal/model`): `LibraryFolder`, `Folder`, `Photo`, scan status.
- **Scanner** (`internal/scan`): recursive walk, image-extension filter, EXIF
  taken-date, dimensions, sha256 content hash, upsert; `scanner_test.go`.
- **Thumbnails** (`internal/thumb`): decode/resize (CatmullRom) to JPEG, cache by
  hash, worker pool; `generator_test.go`.
- **UI** (`internal/ui`): Fyne three-pane Picasa-style layout.
  - `app.go`: window, DB open/close, layout wiring, `selectPhoto`.
  - `toolbar.go`: Add Folder, Rescan, Folder View, search, thumb-size slider.
  - `sidebar.go`: `widget.Tree` — Folders grouped by year with photo counts.
  - `grid.go` + `thumbcell.go`: `GridWrap` thumbnail grid, lazy async thumbnail
    loading, size slider, filename filter, raw folder-view mode.
  - `properties.go`: Location / File Size / File Date / Dimensions.
  - `status.go`: message + progress bar.
  - `actions.go`: folder picker + background scan with progress, rescan all,
    open raw folder view.

### Build / test

    go build ./...        # OK
    go vet ./...          # OK
    go test ./...         # OK (db, scan, thumb)

Release binary matches CI:

    CGO_ENABLED=1 go build -trimpath -o dist/pichouse-linux-amd64 ./cmd/pichouse

### System deps (host + runner, Debian 13)

    gcc libgl1-mesa-dev xorg-dev libxxf86vm-dev libwayland-dev libxkbcommon-dev

## Decisions locked

- GUI: Fyne v2, default X11/GLFW driver (XWayland on Wayland).
- Two SQLite files; EXIF via `github.com/rwcarlsen/goexif`.
- CI: build-only on `debian-go` runner, rolling pre-release.
- Template `build.yaml` and `picasa.png` deleted (were gitignored, never committed).

## Not started (future milestones)

- Editing (rotate/crop), faces, albums/projects, upload/export.
- Filesystem watching for live library updates (currently manual Rescan).
- The app has not been run interactively in this environment (headless); the
  GUI compiles and links but needs a display to launch. Verify on a desktop.

## Open questions

- None currently.
