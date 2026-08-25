# HANDOFF

Running handoff notes for pichouse. Update this when the working context gets large.

## Current state

GTK4 (gotk4) UI. Builds, vets, and tests clean; pushed to `main`
(latest: `35a50b3`). Version `0.1.0` (`internal/version/version.go`).

Recent work (commits `f3cea2b`, `9e1b027`, `35a50b3`) added four feature areas:
configurable thumbnail quality/storage, a closable tabbed info panel, a
full-image viewer with rotation, and configurable keyboard shortcuts. See
"Recent features" below. **All of this is user-facing GUI and is unverified on
this machine** — this dev/build box is headless, so the GUI compiles/links but
cannot launch. The user tests on a real desktop and pastes results back. Never
claim a UI change works until the user confirms; state that caveat when handing
changes over.

## CRITICAL: build/toolchain pins (do not break these)

- **gotk4 pinned to `v0.3.1`** in `go.mod`. Deliberate and load-bearing:
  - gotk4 `v0.4.x` requires **GLib >= 2.88**; Debian 13 ships **GLib 2.84** and
    fails to compile v0.4.x (`could not determine what
    C.g_get_monotonic_time_ns refers to`, etc.).
  - v0.3.1 targets GLib 2.84 and builds under **Go 1.27**. Older gotk4 pulls a
    `go4.org/unsafe/assume-no-moving-gc` that breaks on Go 1.27.
  - **Do not `go get -u` gotk4 or bump it** unless GLib is also upgraded. GLib
    symbol errors mean someone bumped it.
- The gotk4 cgo binding is **slow to compile the first time** (several minutes),
  cached after. It is not hung.
- Use `GOTOOLCHAIN=local` to force installed Go 1.27 and avoid download churn:

      GOTOOLCHAIN=local go build ./...
      GOTOOLCHAIN=local go vet ./...
      GOTOOLCHAIN=local go test ./...

## System deps (host + Gitea runner, Debian 13)

    sudo apt-get install -y gcc pkg-config libgtk-4-dev libgirepository1.0-dev

`libgirepository1.0-dev` provides `gobject-introspection-1.0.pc`. Runtime needs
GTK4 (Debian 13 ships 4.18).

## Data locations & config

- **Config file: `~/.config/pichouse/config`** (honors `$XDG_CONFIG_HOME`).
  Contains ONLY the path to the data directory (one line). Lets the user
  relocate the DB files. Managed by `internal/db/config.go`
  (`readConfiguredDataDir`, `WriteConfiguredDataDir`). `DataDir()` reads it and
  falls back to `~/.local/share/pichouse` (honors `$XDG_DATA_HOME`).
- **Data dir** holds `library.db` and the per-size thumbnail DBs
  `thumbs-<N>.db` (e.g. `thumbs-96.db`, `thumbs-160.db`, ...). `.db*` files are
  gitignored — never commit them even if the user copies them into the repo.
- **All other settings live in `library.db`** in a `settings(key,value)` table
  (never in the config file). See `GetSetting`/`SetSetting`.

## Architecture

Core packages stay UI-agnostic; only `internal/ui` and `cmd/pichouse` touch GTK.

- `internal/db` (SQLite via `modernc.org/sqlite`):
  - `library.db`: `library_folders`, `folders`,
    `photos(... , orientation)`, `scan_state`, `albums`, `album_folders`,
    and **`settings(key TEXT PK, value TEXT)`**.
  - **`photos.orientation`**: user-applied rotation in degrees clockwise
    (0/90/180/270). Added via `migrate()` in `OpenLibraryAt` (ALTER TABLE if the
    column is missing) AND in `schema.sql` for fresh DBs. **Stored only in the
    DB, never written to image files on disk.** `SetOrientation(photoID,deg)`
    normalizes and persists; `UpsertPhoto` deliberately does NOT overwrite
    orientation on rescan (insert sets 0, the DO UPDATE clause omits it).
  - Settings: `GetSetting(key,def)`, `SetSetting(key,value)` (`library.go`).
  - Thumbs (`thumbs.go`): `thumbnails(photo_hash PK, size, jpeg, created_at)`,
    `busy_timeout=5000` + `SetMaxOpenConns(1)`. **One DB file per thumbnail
    size** — `ThumbsPathForSize(size)` → `thumbs-<size>.db`,
    `OpenThumbsForSize(size)`. `Thumbs` also has `Delete(hash)` and `Clear()`.
    `RemoveAllThumbDatabases()` deletes every `thumbs*.db*` file (used by
    "Clear thumbnail cache"; callers must close handles first).
  - `config.go`: config-file + `DataDir()` logic (see above).
  - `albums.go`: album CRUD, membership, `SetAlbumParent` (cycle-protected).
  - Tests: `db_test.go`, `albums_test.go`.
- `internal/model`: `LibraryFolder`, `Folder`, `Photo` (now has
  `Orientation int`), `Album`, `ScanStatus`.
- `internal/scan`: recursive walk, EXIF taken-date, dimensions, sha256 hash,
  upsert; `ScanFolderContext(ctx,...)` cancellable. Does NOT touch orientation.
- `internal/thumb` (**rewritten**): `Generator` now owns per-size
  `db.Thumbs` stores (`New(size)`, no longer takes a `*db.Thumbs`). Key methods:
  `SetSize`, `Size`, `SetAllSizes([]int)` (pre-generate every configured size on
  a miss), `Get(hash, srcPath, rotation)`, `Invalidate(hash)` (drop cached
  thumbs across open sizes — used after a rotation), `Close`, `ClearAll`.
  `render` decodes once, applies `rotate(img,deg)` (pure-Go 90/180/270), resizes
  (CatmullRom) → JPEG q85. The old `Pool` API was removed. Tests:
  `generator_test.go` (uses `t.Setenv("XDG_DATA_HOME", ...)` to sandbox the
  per-size DBs; covers resize+cache, rotation, save-all-sizes).

### GTK4 UI (`internal/ui`)

- `app.go`: `gtk.Application`, window, `lib *db.Library`, `gen *thumb.Generator`,
  `prefs`, `shortcuts`, `thumbCache`, `centerStack`, `viewer`, `properties`.
  On startup: `loadPrefs`, `loadShortcuts`, `applyThumbPrefs` (pushes active
  size + save-all into the generator). `activate()` installs a **window-level
  key controller in the capture phase** that routes keys to `viewer.HandleKey`
  while the viewer is the visible center page (this is why viewer shortcuts work
  without focus — do not move it back onto a widget). `ToggleProperties`
  shows/hides the info panel and persists visibility. On exit: `gen.Close()`,
  `lib.Close()`.
- `prefs.go`: `prefs` struct + `loadPrefs`. Setting keys: `thumb.sizes`
  (comma-separated 4 px presets), `thumb.active` (0..3), `thumb.regen`,
  `thumb.save_all`, `ui.props_visible`. Defaults 96/160/240/320, active=1.
- `shortcuts.go`: configurable viewer keybindings. `shortcutDefs` lists actions
  (`prev`/`next`/`rotate`/`close`) with defaults (Left/Right/r/Escape). Stored
  under `keybind.<action>` in `library.db` as GDK keyval names. `shortcuts`
  maps keyval↔action; `action(keyval)` is case-insensitive for letters via
  `KeyvalToLower`.
- `layout.go`: toolbar / left `GtkStack` (Library, Folders) / **center
  `GtkStack`** swapping `grid` ↔ `viewer` / right properties `Notebook` /
  bottom status, in nested `GtkPaned`. `a.centerStack` + `a.propsPaned` stored.
- `viewer.go`: full-image view that replaces the grid on double-click.
  `Open(photos,index)`, Left/Right nav, `rotate()` (+90°, persists via
  `SetOrientation`, `gen.Invalidate(hash)`, refreshes grid), Esc closes.
  `HandleKey` resolves keyvals through `app.shortcuts`. Rendering loads the
  full pixbuf on a goroutine and applies `RotateSimple` for display.
  `RefreshTooltips` updates button tooltips to the active bindings.
- `properties.go`: right panel is now a **`GtkNotebook`** with a "Pic Info" tab
  (room for more tabs). `SetVisible` toggles it.
- `grid.go` + `thumbcell.go`: center `GtkGridView`. Double-click (`ConnectActivate`)
  → `app.OpenViewer`; single click → `selectPhoto` (info panel). Bounded worker
  pool (`thumbWorkers = 4`) calls `gen.Get(hash,path,orientation)`.
  **In-memory cache key now includes active size and orientation**
  (`hash|size|orientation`) so slider/rotation changes don't serve stale blobs.
  `SetThumbSize(px)` also calls `gen.SetSize(px)` so each slider position uses
  its own `thumbs-<px>.db`. `debugThumbs = true` still logs `[thumb] ...`.
- `toolbar.go`: Settings, Rescan, search, size slider (presets from
  `prefs.sizes`, persists `thumb.active`), and a **properties-toggle button**
  next to the slider.
- `settings.go`: `GtkStackSidebar` with panes:
  - **Library Folders** — add/remove roots.
  - **Thumbnails** — four size spin buttons (Apply Sizes → persists
    `thumb.sizes`, re-applies prefs), regenerate-on-move checkbox, save-all-sizes
    checkbox, **Clear Thumbnail Cache** (`gen.ClearAll()` + reset LRU).
  - **Data Location** — choose the data dir (writes the config file; takes
    effect on restart).
  - **Shortcuts** — per-action current key + "Change…" capture dialog
    (`captureShortcut`), plus Reset to Defaults. Escape cannot be *assigned*
    (it cancels the capture dialog).
- `dialogs.go`: `selectPhoto`, `OpenViewer`/`CloseViewer` (flip `centerStack`),
  error/info/confirm dialogs.
- `sidebar*.go`, `foldertree.go`, `dragdrop.go`, `thumbcache.go`, `status.go`,
  `util.go`: unchanged from prior handoff (albums tree, native GMenu context
  menu, folder view, in-memory LRU, `onUI`).

## Feature status (built; user-verified where noted)

- Thumbnail quality settings, per-size DBs, save-all, clear-cache. Built,
  **awaiting user confirmation**.
- Closable tabbed info panel + toolbar toggle. Built, **awaiting confirmation**.
- Full-image viewer, double-click open, Left/Right nav, R rotate (persisted),
  Esc close. Built. User reported Left/Right were flaky (needed a click first);
  fixed by the window-level capture-phase controller (`9e1b027`) —
  **awaiting confirmation that this resolved it**.
- Configurable shortcuts tab. Built, **awaiting confirmation**.
- Albums, sub-albums, drag/drop, context menu, folder view, scan/stop. WORKS
  (from prior handoff).

## Known caveats / gotchas

- **Headless dev machine**: never claim a UI fix works. Prefer changes grounded
  in gotk4 source under
  `$(go env GOMODCACHE)/github.com/diamondburned/gotk4/pkg@v0.3.1/`.
- **Orientation is DB-only**, never written to disk. Raw folder-view photos have
  no DB row (`p.ID == 0`), so their rotation cannot persist — expected.
- **Per-size thumbnail DBs**: switching the slider changes both display edge and
  the generator's active size; each size has its own file. "Save all sizes"
  pre-generates every preset on first render (more storage, faster switching).
- The `thumb.regen` (regenerate-on-move) pref is persisted but currently a
  no-op beyond normal per-size fetch/generate. If the user wants it to
  force-delete-and-re-render the active size on each slider move, wire that.
- gotk4 recycled list rows: `item.Child()` returns the bare GTK widget, not a Go
  wrapper — retrieve sub-widgets by walking the box (`cellParts`, `bindRow`).
- `debugThumbs = true` in `grid.go`; flip to `false` once thumbnails are stable.

## CI

`.gitea/workflows/build.yaml` on `debian-go` runner (this host). Builds
`CGO_ENABLED=1 ./cmd/pichouse`, reads version from `internal/version`, publishes
a rolling pre-release. Build-only (no GUI launch).

## Not started (future)

- Crop/edit, faces, export/upload.
- Filesystem watching for live updates (currently manual Rescan).
- More info-panel tabs (the Notebook is ready for them).
- No `internal/ui` unit tests (GTK needs a display; core packages are tested).
