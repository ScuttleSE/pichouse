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
- `src/ui/` — the full GTK4 UI (see AGENTS.md for the file list).

## 3. What is deferred

The Go sidebar had a richer feature set than the current Rust sidebar. The Rust
sidebar shows a flat list of scanned folders. These Go features are NOT yet
ported:

- The album tree with sub-albums.
- The "New folders" grouping under the Library root.
- Drag-and-drop of folders into albums.
- Right-click context menus (create album, move to album).
- The separate raw filesystem "Folders" tab (`foldertree`).

The database layer already supports albums (`src/db/albums.rs`). Only the UI for
them is missing. Add these as a follow-up if the user asks.

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

## 9. Rules

- Follow RULE ZERO: do not loop on guesses. Ask the user one question and wait.
- Commit and push after each change (RULE ONE).
- Keep AGENTS.md and README.md correct (RULE TWO).
