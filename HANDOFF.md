# HANDOFF

This document is for an agent with no memory of the last session. It uses
Simplified Technical English (ASD-STE100, Strict). Read AGENTS.md first. Read
ROADMAP.md for planned features.

## 1. Current task

The project moves from Go to Rust. The user validated the Rust spike. The user
started the full port. You do the port now.

The work is on the `rust-port` branch. Do the port in milestones. Commit and push
each milestone to `rust-port`. Do not push to `main`. Merge to `main` only when
the port reaches feature parity.

The user said: switch CI to Rust now. CI is already Rust. See section 5.

## 2. Branches

- `main` holds the old Go application. Do not change `main` now.
- `rust-port` holds the Rust port. All new work goes here.
- `spike/rust-gtk4-grid` holds the validated spike. Do not change it. The spike
  proved the hard UI part: a GTK4 GridView of recycled thumbnail cells backed by
  a custom GObject list model, with JPEG blobs from SQLite. Use its pattern for
  the UI grid.

## 3. Milestones — state

Done and pushed on `rust-port`:

- M0: Rust scaffolding. `Cargo.toml`, `src/main.rs`, `src/version.rs`. Rust CI.
- M1: `src/model.rs`. Domain types. Enums for AiStatus, TagSource, ScanStatus.
- M2: `src/db/`. rusqlite. Library over `library.db`; Thumbs over
  `thumbs-<N>.db`. Albums, tags, FTS5 search. Mutex serializes each connection.
- M3: `src/scan.rs`. Recursive image walk, EXIF date, dimensions, SHA-256 hash.
  Cancel via `Arc<AtomicBool>`.
- M4: `src/thumb.rs`. decode/rotate/resize (fast_image_resize Catmull-Rom)/JPEG.
  Per-size cache. Generator with lazily opened stores.
- M5: `src/ai/`. Ollama blocking client, config, tagger, subprocess manager.

All core modules have unit tests. Run `cargo test`. 20 tests pass.

Not done:

- M6: the GTK4 UI. This is the last and largest milestone (~3700 Go LOC in
  `internal/ui/`). Port it in sub-steps. See section 4.
- M7: parity pass. Delete the Go files (`cmd/`, `internal/`, `go.mod`,
  `go.sum`). Final doc rewrite. Merge `rust-port` to `main`.

## 4. M6 UI — plan

Build the UI with gtk4-rs 0.7 (`v4_10` feature). Port these `internal/ui/`
files. Do the sub-steps in order:

1. App, window, layout skeleton. Paned, Stack, StackSwitcher, status bar. Add an
   `on_ui` helper (glib idle_add) for background-to-UI-thread messages.
2. Grid plus thumbnail worker pool. Use a `gio::ListStore` of a `PhotoObject`
   GObject (from the spike). Keep the generation token and the cell-recycle
   guard. Decode JPEG to a pixbuf/texture on the UI thread.
3. Sidebar (album/folder tree, expansion persistence), foldertree,
   drag-and-drop, native GMenu context menus.
4. Viewer (navigation, rotate to DB then invalidate thumbs), properties panel
   (tabs, tags), toolbar (zoom slider, search, AI menu).
5. Settings windows (library folders, thumbnails, data location, shortcuts), AI
   settings, tag manager.
6. Configurable shortcuts, prefs load/save, dialogs.

Preserve this behaviour (parity risks):

- The 4-worker thumbnail pool with a generation token and a recycle guard.
- Cancellable scan and AI controllers. One Stop button cancels both.
- Thumbs single-writer serialization (already in `src/db/thumbs.rs`).
- FTS5 manual maintenance (already in `src/db/tags.rs`).
- Orientation is display and DB only. Never write it to the source file.
- Per-size thumbnail DB files. XDG data/config directory resolution.

## 5. CI and versioning

`.gitea/workflows/build.yaml` is Rust now. It:

- reads the version from `Cargo.toml` (`[package] version`);
- bumps the build number on each non-docs push to `main`;
- commits the bump with `[skip ci]`;
- runs `cargo test --release` and `cargo build --release`;
- publishes one rolling pre-release.

`src/version.rs` mirrors the Cargo version with `env!("CARGO_PKG_VERSION")`. Do
not change the build number by hand. See AGENTS.md RULE THREE.

Note: CI triggers on push to `main` only. Pushes to `rust-port` do not run CI.
CI runs first when you merge `rust-port` to `main`.

## 6. Schema decision

The Rust schema is a fresh start. Existing Go databases are not migrated. The
user rebuilds the library by rescanning. The Rust schema has `orientation` and
`ai_status` columns inline. There is no runtime migration code.

## 7. Dependencies of note

- gtk4-rs 0.7 (`v4_10`). Debian 13 ships GLib 2.84 / GTK 4.18. Do not upgrade
  past the GLib the system ships.
- rusqlite (`bundled`). Bundled SQLite includes FTS5. There is no separate
  `fts5` feature in this rusqlite version.
- reqwest uses `rustls-tls` (not native OpenSSL), to avoid a system OpenSSL
  dependency on the runner.
- image, fast_image_resize, kamadak-exif, sha2, base64, serde, serde_json, dirs.

## 8. How to work

- Build: `cargo build`. Test: `cargo test`. Run: `cargo run`.
- Commit and push each milestone to `rust-port`.
- Keep AGENTS.md and README.md correct as you go.
- Follow RULE ZERO: do not loop on guesses. Ask the user one question and wait.
