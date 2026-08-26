# HANDOFF

This document is for an agent with no memory of the last session. It uses
Simplified Technical English (ASD-STE100, Strict). Read AGENTS.md first. Read
ROADMAP.md for planned features.

## 1. State

The Go to Rust port is complete. The Rust application replaces the Go
application. The Go code is deleted. All work is on `main`. The temporary
`rust-port` branch is deleted.

The application builds, tests pass, and the UI runs on the target machine
(Debian 13, GTK 4.18).

Two roadmap features are now implemented: fast two-phase import and library
freshness. Section 10 describes them.

## 10. Fast two-phase import and library freshness

### Two-phase import
- Phase 1 (`scan::Scanner::scan_folder`) records photo structure only: path,
  filename, folder id, size, mod time. It does no EXIF decode, no dimension
  decode, and no hashing. The folder tree and grid populate almost at once.
- Phase 2 (`ui::enrich`) is a background worker pool. It drains a shared
  worklist of photo ids (`AppState::enrich_queue`), computes EXIF `taken_at`,
  dimensions, and the SHA-256 hash per photo (`scan::enrich_file`), writes them
  (`Library::enrich_photo`), then generates the thumbnail. The grid re-queries
  periodically, so placeholders become thumbnails as data lands.
- A photo carries a `scan_state` column: 0=structured, 1=enriching, 2=done.
  `photos_needing_enrichment` selects `scan_state <> 2 AND missing = 0`.
- Opening a folder calls `enrich::prioritize_folder`, which moves that folder's
  un-enriched ids to the front of the worklist. The queue follows the view.
- On startup, `enrich::ensure_running` reseeds the worklist from the database,
  so an interrupted import resumes without a manual rescan.
- The folder `year` starts from the folder mtime and is refined from the
  earliest known `taken_at` after a folder finishes enriching.
- Thumbnails need the hash (the cache key), so an un-enriched cell shows the
  filename-label placeholder until Phase 2 hashes it. No temporary key is used.

### Library freshness
- `reconcile.rs` diffs disk against the database per folder. New files are
  inserted (Phase 1) and queued for enrichment. Removed files are soft-marked
  `missing` (a `missing` column); the row and its tags/edits are kept. A
  reappeared file clears the flag. A new file whose size matches a missing row
  in the same root is treated as a move and re-points the existing row
  (`move_photo_path`), then is re-hashed to confirm identity.
- Reconciliation is the reliable path. It runs on startup, on demand (the
  Refresh Library toolbar button, `emblem-synchronizing-symbolic`), and on a
  periodic timer (`ui::freshness`, `PERIODIC` = 180 s). It works on network
  drives (NFS/SMB) and very large trees.
- `ui::watcher` adds an inotify fast path for local folders. It debounces event
  bursts (`DEBOUNCE` = 1500 ms) and reconciles only the affected directories.
  It degrades gracefully: if a watch cannot be added (e.g. the inotify watch
  limit), it logs and relies on the periodic reconcile. It is never required
  for correctness.
- IMPORTANT network-drive caveat: inotify does NOT see changes made by other
  machines on NFS/SMB mounts. The watcher may be silent there. The periodic
  reconcile is what catches remote changes. Do not remove the periodic
  reconcile in favor of inotify.
- Missing photos are shown dimmed in the grid (a `missing` property on
  `PhotoObject`).

### Schema migration
- `library.db` gained `photos.scan_state` and `photos.missing`. `Library::open_at`
  runs an additive `migrate` that adds the columns to an older database and marks
  already-hashed rows `scan_state = 2`, so no rebuild is forced.

### Open follow-ups (not done; noted in code)
- Full hash-based move detection beyond the size heuristic.
- A "new" badge for freshly added photos, and a "clean up missing" action to
  hard-delete missing rows on user confirmation.
- Interaction with future RAW+JPEG pairing (pair during Phase 1 or Phase 2).

## 2. What is ported

All Go modules are ported to Rust:

- `src/model.rs` — domain types.
- `src/db/` — rusqlite over `library.db` and per-size `thumbs-<N>.db`. Albums,
  tags, FTS5 search. Each connection is behind a `Mutex`.
- `src/scan.rs` — recursive image walk, EXIF date, dimensions, SHA-256 hash,
  cancellation.
- `src/thumb.rs` — decode, rotate, resize (fast_image_resize Catmull-Rom),
  JPEG encode, per-size cache.
- `src/ai/` — Ollama blocking client, config, tagger, subprocess manager.
- `src/ui/` — the full GTK4 UI. Notable files: `app.rs` (window/layout/wiring),
  `state.rs` (shared `Rc<AppState>`), `grid.rs` (thumbnail grid + worker pool +
  source tracking), `sidebar.rs` (Library album tree + menus + drag-drop),
  `foldertree.rs` (raw Folders tab), `albumtree.rs` (disk→album auto-sync),
  `actions.rs` (scan queue + worker), `aitag.rs` (AI tagging worker pool),
  `viewer.rs`, `properties.rs`, `toolbar.rs`, `status.rs`, `settings.rs`,
  `settings_ai.rs`, `tagmanager.rs`, `shortcuts.rs`, `dialogs.rs`,
  `controller.rs` (cancel token), `thumbcache.rs` (LRU texture cache),
  `prefs.rs`, `photo_object.rs`, `util.rs`.

## 3. Parity and post-parity work

The Rust application is at feature parity with the old Go application, plus
several improvements the Go version did not have. All Go UI features are ported,
including:

- The album tree with sub-albums and the "New folders" grouping.
- Right-click context menus (create/rename/delete album, move to album).
- Drag-and-drop of folders into albums and album re-parenting.
- The raw filesystem "Folders" tab.
- Grid refresh after scan and after rotation (re-queries the source).
- An in-memory LRU texture cache for fast scroll/re-entry.

Improvements added after parity (not in Go):

- Scan queue (`src/ui/actions.rs`, `AppState::scan_queue`). Adding a folder
  while a scan runs appends to a shared queue. The scan worker drains the queue.
  Before, a second add cancelled the first scan (both apps had this bug; Rust
  fixed it).
- Live sidebar population during a scan: the scan progress callback sends a
  reload every 200 photos, and once per finished root.
- Auto-album tree (`src/ui/albumtree.rs`, `sync_disk_tree`). After a root is
  scanned, the on-disk directory hierarchy is mirrored into the Library album
  tree: the root basename becomes a top-level album, intermediate directories
  become nested sub-albums, each scanned folder is filed into its album.
  It never re-files a folder that is already in an album, so user edits and
  manual placements survive a rescan.
- Folders tab: a folder icon per row; library roots use a distinct
  `drive-harddisk-symbolic` icon and show their full path in bold.

There are no known parity gaps. One historical setting, `thumb.regen`
(regenerate on slider move), is stored but never consulted — this matches the
Go behavior exactly and is not a regression.

## 3a. Open behaviors / possible follow-ups

The user accepted these; a fresh agent should not "fix" them without being
asked:

- During a scan, folders discovered mid-scan appear under "New folders" until
  the root finishes; `sync_disk_tree` only runs when a root completes, then it
  reorganizes them into the album tree. Making the tree build fully live would
  require running the sync per-directory (more DB writes).
- A rescan does NOT re-assert the disk-mirrored tree over manual album edits;
  it only files folders that are not already in an album. This was a deliberate
  choice (preserve user edits). The user has not asked to change it.
- `thumb.regen` remains a stored-but-unused setting (matches Go). Wiring it up
  would be a genuine new feature, not a bug fix.

## 4. Build, test, run

- Build: `cargo build`
- Test: `cargo test`
- Run: `cargo run`

The GUI needs a display. It does not run in a headless CI container. CI builds
and tests only.

## 5. CI and versioning

- `.gitea/workflows/build.yaml` runs on push to `main`. It reads the version
  from `Cargo.toml`, bumps the build number, commits it with `[skip ci]`, runs
  `cargo test --release` and `cargo build --release`, and publishes one rolling
  pre-release binary. This is the only workflow.
- The runner has no passwordless sudo. CI does not run `apt-get`. It adds the
  installed cargo bin directory to `GITHUB_PATH` and verifies `cargo` and
  `gtk4` are present.
- `src/version.rs` mirrors the Cargo version with `env!("CARGO_PKG_VERSION")`.
- Do not change the build number by hand. See AGENTS.md RULE THREE.

## 6. Schema note

The Rust schema is a fresh start. It has `orientation` and `ai_status` inline.
There is no migration from the Go databases. The user rebuilds the library by
rescanning.

## 7. Dependencies of note

- gtk4-rs 0.7 (`v4_10`). Do not upgrade past the GLib the system ships.
- rusqlite (`bundled`) — bundled SQLite includes FTS5.
- reqwest uses `rustls-tls` (no system OpenSSL).
- image, fast_image_resize, kamadak-exif, sha2, base64, serde, serde_json, dirs.

## 8. Known technical notes

- `Pixbuf` is not `Send`. Workers send raw bytes to the UI thread; the UI thread
  decodes to a texture or pixbuf.
- Background workers talk to the UI with `glib::MainContext::channel`. This is
  deprecated in glib 0.18 but works. A future change may move to
  `async-channel` + `spawn_future_local`.
- The grid uses a `PhotoObject` GObject with a `texture` property. A worker sets
  the texture on the UI thread; the bound `Image` observes `notify::texture`.
  A notify handler takes two arguments (object, ParamSpec) — use
  `connect_notify_local`, not a one-argument closure.
- Shared state moved into background threads must be `Send`. GTK/GObject types
  are not `Send`. `AppState` holds thread-shared data as `Arc<Mutex<...>>`
  (`ai_manager`, `scan_queue`); use the `*_arc()` accessors to clone a handle
  into a worker thread. Do not move an `Rc` or a widget into `thread::spawn`.
- The grid remembers its `Source` (a scanned folder id, a raw dir, or none) so
  `reload_from_source()` can re-query after a scan or a rotation. `show_folder`
  and `show_raw_folder` set the source; `show_photos` sets it to `None`.
- `Library` and `Thumbs` wrap the SQLite `Connection` in a `Mutex`; every call
  locks. This also serializes the multi-statement tag/album writes. Do not add a
  second connection without keeping the single-writer guarantee for `thumbs`.

## 9. Rules

- Follow RULE ZERO: do not loop on guesses. Ask the user one question and wait.
- Commit and push after each change (RULE ONE).
- Keep AGENTS.md and README.md correct (RULE TWO).
