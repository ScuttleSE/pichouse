# HANDOFF

Running handoff notes for pichouse. Update this when the working context gets large.

## Current state

The UI has been **fully rewritten from Fyne to GTK4 (gotk4)**. The app builds,
vets, and tests clean, and is pushed to `main` (latest: `ee3777d`). Version is
`0.1.0` (`internal/version/version.go`).

The user tests on a real desktop; **this dev/build machine is headless** — the
GUI compiles and links here but cannot be launched. All GUI behavior is verified
by the user, who pastes back results. Do NOT assume a UI change works until the
user confirms; state that caveat when handing changes over.

## CRITICAL: build/toolchain pins (do not break these)

- **gotk4 pinned to `v0.3.1`** in `go.mod`. This is deliberate and load-bearing:
  - gotk4 `v0.4.x` requires **GLib >= 2.88**; Debian 13 ships **GLib 2.84**.
    v0.4.x fails to compile here with `could not determine what
    C.g_get_monotonic_time_ns refers to` (and similar 2.88 symbols).
  - v0.3.1 targets GLib 2.84 and builds under **Go 1.27**. Older gotk4 (<v0.3.1)
    pulls a `go4.org/unsafe/assume-no-moving-gc` version that breaks on Go 1.27;
    v0.3.1 + the pinned `go.sum` deps (`KarpelesLab/weak`, newer
    `assume-no-moving-gc`) work.
  - **Do not `go get -u` gotk4 or bump it** unless GLib is also upgraded. If a
    GLib symbol error appears, this is why.
- The whole gotk4 cgo binding is **slow to compile the first time** (several
  minutes). Subsequent builds are cached. Be patient; it is not hung.
- Build/vet/test commands here use `GOTOOLCHAIN=local` to force the installed
  Go 1.27 and avoid toolchain-download churn:

      GOTOOLCHAIN=local go build ./...
      GOTOOLCHAIN=local go vet ./...
      GOTOOLCHAIN=local go test ./...

## System deps (host + Gitea runner, Debian 13)

    sudo apt-get install -y gcc pkg-config libgtk-4-dev libgirepository1.0-dev

`libgirepository1.0-dev` is required (provides `gobject-introspection-1.0.pc`)
in addition to `libgtk-4-dev`. Runtime needs GTK4 (Debian 13 ships 4.18).

## Architecture

Core packages are **UI-agnostic and unchanged** across the Fyne→GTK4 rewrite —
only `internal/ui` and `cmd/pichouse` touch the GUI toolkit:

- `internal/db`: SQLite via `modernc.org/sqlite`.
  - `library.db`: `library_folders`, `folders`, `photos`, `scan_state`,
    `albums(id,name,parent_id,position)`,
    `album_folders(album_id,folder_id,position)`.
  - `thumbs.db`: `thumbnails(photo_hash PK, size, jpeg, created_at)`.
    Opened with `busy_timeout=5000` and `SetMaxOpenConns(1)` so concurrent
    thumbnail workers don't hit "database is locked".
  - `albums.go`: `CreateAlbum`, `RenameAlbum`, `DeleteAlbum`, `Albums`,
    `AddFolderToAlbum`, `RemoveFolderFromAlbum`, `FolderAlbums`,
    `SetAlbumParent` (re-parent, with cycle protection).
  - `library.go`: folder/photo upserts, `PhotosInFolder`, `HashesByDir`
    (reuse scanned hashes for raw folder view), `RemoveLibraryFolder`.
  - Tests: `db_test.go`, `albums_test.go`.
- `internal/model`: `LibraryFolder`, `Folder`, `Photo`, `Album`, `ScanStatus`.
- `internal/scan`: recursive walk, EXIF taken-date, dimensions, sha256 hash,
  upsert. `ScanFolderContext(ctx,...)` supports cancellation. `scanner_test.go`.
- `internal/thumb`: decode/resize (CatmullRom) → JPEG q85, cache by hash.
  **`DefaultSize = 320`** (raised from 256 so thumbnails stay crisp at the
  largest grid zoom preset; existing 256px cached thumbs are not rebuilt).

### GTK4 UI (`internal/ui`)

- `app.go`: `gtk.Application`, window, DB open/close, `thumbCache`,
  `scanController`. Views populated in `activate()` after `SetVisible(true)`.
- `layout.go`: toolbar / left `GtkStack` (Library, Folders via `GtkStackSwitcher`)
  / center grid / right properties / bottom status, in nested `GtkPaned`.
  Left panel min width 300.
- `sidebar.go`: **Library** view. `GtkListView` + `GtkTreeListModel`; nodes are
  string ids (`album:<id>`, `folder:<id>`, `newfolders`). Top level = top-level
  albums + a "New folders" branch of unassigned folders. `GtkMultiSelection`.
  Expansion state is saved/restored across `Reload()` (`saveExpansion`/
  `restoreExpansion`/`markExpanded`) so moving a folder / creating a sub-album
  doesn't collapse the tree. `TreeExpander.SetIndentForIcon/Depth(true)` aligns
  leaf and branch rows.
- `sidebar_menu.go`: right-click context menu via **native GMenu + GtkPopoverMenu
  + GSimpleActionGroup** (inserted on the list view under the `sidebar` prefix).
  Actions carry the target node id as a string variant. A **fresh popover is
  built per right-click** with **`PopoverMenuNested`** flag and parented to the
  clicked expander — this fixed both the "invisible until hover" text and the
  "tiny/scrolled" menu. "Move to Album" is a nested submenu mirroring the album
  hierarchy (`buildMoveSubmenu`, recursive; parent albums get a "Move here"
  entry).
- `sidebar_albums.go`: album op handlers + `promptText`/`confirm` dialogs.
- `dragdrop.go`: `GtkDragSource` on folder/album rows, `GtkDropTarget` on album
  rows. Drop folder(s) onto album → move; drop album onto album → sub-album
  (via `SetAlbumParent`). Multi-select supported.
- `foldertree.go`: **Folders** view. `GtkListView` + `GtkTreeListModel` reading
  subdirectories live from disk; selecting a dir shows its images (raw mode),
  reusing scanned hashes for cached thumbnails.
- `grid.go` + `thumbcell.go`: center `GtkGridView`. Cells are plain `*gtk.Box`
  (retrieved via `cellParts`, NOT a Go wrapper — `item.Child()` returns the bare
  box). **Bounded worker pool** (`thumbWorkers = 4`) generates thumbnails to
  avoid SQLite write contention; results marshaled to UI via `onUI`
  (`glib.IdleAdd`). Stale results guarded by a `generation` counter and the
  picture's stored cache key. `debugThumbs = true` logs `[thumb] ...` lines
  (ask the user to paste these when thumbnails misbehave).
- `thumbcache.go`: in-memory LRU (512) of encoded JPEG bytes, keyed by hash
  (fallback path).
- `properties.go`, `status.go` (Stop-scan button), `toolbar.go` (Settings,
  Rescan, search, size slider snapping to presets 96/160/240/320),
  `settings.go` (add/remove library folders via `GtkFileDialog`),
  `dialogs.go` (error/info via `GtkMessageDialog`), `util.go` (`onUI`,
  `escapeMarkup`).

## Feature status (all built; user-verified where noted)

- Library albums with sub-albums; New folders bucket. WORKS.
- Create album / sub-album (right-click). WORKS.
- Drag folders into albums, drag album into album (sub-album). WORKS.
- Right-click context menu, nested "Move to Album" submenu. Menu sizing/nesting
  just fixed (`ee3777d`) — **awaiting user confirmation**.
- Thumbnails (worker pool, 320px, LRU cache). Last reported inconsistency was
  addressed with the pool + SQLite serialization; **awaiting user confirmation**
  and any `[thumb]` logs.
- Stop-scan, size-slider presets, properties, settings, folder view. WORKS.

## Known caveats / gotchas for the next context

- **Headless dev machine**: never claim a UI fix works; the user must verify.
  Prefer changes grounded in gotk4 source (in the module cache under
  `$(go env GOMODCACHE)/github.com/diamondburned/gotk4/pkg@v0.3.1/`) over guesses.
- **gotk4 recycled list rows**: `GtkListView`/`GtkGridView` reuse row widgets;
  `item.Child()` returns the bare GTK widget, not a Go wrapper type. Always
  retrieve child sub-widgets by walking the box (see `cellParts`, `bindRow`).
- Node identity in both trees is a **string id** carried in `gtk.StringObject`
  inside `gtk.StringList`; child models built lazily in the `TreeListModel`
  create-func.
- **Slider does not regenerate thumbnails** — it only changes display size; the
  single 320px cached thumbnail is scaled by `GtkPicture`. A "rebuild
  thumbnails" action (clear + regenerate `thumbs.db`) is not implemented; the
  user may request it (old scans still have 256px thumbs).
- `debugThumbs` in `grid.go` is currently `true`; flip to `false` to silence
  `[thumb]` logging once thumbnails are confirmed stable.

## CI

`.gitea/workflows/build.yaml` on `debian-go` runner (this host). Installs GTK4
dev deps best-effort, reads version from `internal/version/version.go`, builds
`CGO_ENABLED=1 ./cmd/pichouse`, publishes a rolling pre-release. Build-only.

## Data locations

`~/.local/share/pichouse/{library.db,thumbs.db}` (honors `$XDG_DATA_HOME`).
`.db*` files are gitignored; the user has sometimes copied them into the repo
root for inspection — never commit those.

## Not started (future)

- Editing (rotate/crop), faces, export/upload.
- Filesystem watching for live updates (currently manual Rescan).
- "Rebuild thumbnails" / clear-cache action.
- No `internal/ui` unit tests (GTK needs a display; core packages are tested).
