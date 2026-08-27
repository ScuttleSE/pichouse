# AGENTS.md

## RULE ZERO — THE MOST IMPORTANT RULE

Do not speculate in a loop. Do not run in circles.

- When the cause of a problem is not clear after a couple of file reads, STOP.
- Ask the user one targeted question. Wait for the answer.
- Do not chain guesses. Do not say "let me check one more thing" again and again.
- The moment you loop, hypothesize, or repeat searches without a clear answer, STOP. This is not conditional on you noticing. Do not loop in the first place.
- This rule is hard. There is NO way around it. You may only continue past a loop by asking the user first.
- Breaking this rule is the worst failure. It wastes the user's time and tokens.

---

## RULE ONE — ALWAYS COMMIT AND PUSH

After completing any change, commit and push all changes. Do not leave work uncommitted. Pushing triggers CI.

---

## RULE TWO — HAND OFF BEFORE THE CONTEXT IS TOO LARGE

When the context becomes too large, write a handoff document for a new context.

- Write the document for an agent that starts with no memory of this session.
- Put in the document the current task, the state of the work, the next steps, and the open questions.
- Combine the handoff with a technical AGENTS.md.
- Write the technical AGENTS.md in Simplified Technical English (ASD-STE100), Strict mode.
- Use short sentences. Use active voice. Use one instruction per sentence. Give each word one meaning.
- Commit and push the handoff (see RULE ONE).

---

## RULE THREE — VERSIONING

The version format is major.minor.build. The series starts at 0.0.0.

- Store the version in `Cargo.toml` as the `[package] version`.
  `src/version.rs` mirrors it at build time with `env!("CARGO_PKG_VERSION")`.
- CI increases the build number by 1 on each push to `main`.
- A documentation-only push does not increase the build number. A push is
  documentation-only when it changes markdown (`*.md`) files only. CI skips the
  version bump, the build, and the release for such a push.
- CI commits the new version back with `[skip ci]` in the message.
- Do not increase the build number by hand.
- The user asks for a release. A release increases the major, the minor, or the build.
- Ask the user which part to increase if the user does not say.
- For a major release, increase major by 1. Set minor to 0. Set build to 0.
- For a minor release, increase minor by 1. Set build to 0.
- For a build release, increase build by 1.
- CI keeps one rolling pre-release from the latest `main` build.

---

## Project

**pichouse** — a Picasa-like photo library GUI application for Linux, written in Rust.

- Add one or more Library folders; they are scanned into a local SQLite database.
- Browsing the library reflects the cached DB state by default.
- Thumbnails are generated on first view and cached in per-size SQLite DBs.
- UI layout mimics Picasa 3, with modern styling.

## Tech stack

- **Language:** Rust (2021 edition, binary crate `pichouse`)
- **GUI:** GTK4 via [gtk4-rs](https://gtk-rs.org/) (`gtk4` crate). Native desktop.
  **Pinned to gtk4-rs 0.7.x with the `v4_10` feature**, which targets GLib 2.84
  (Debian 13). Newer gtk4-rs needs a newer GLib than Debian 13 ships — do not
  upgrade this dependency without also upgrading GLib.
- **DB:** `rusqlite` with the `bundled` feature (bundled SQLite includes FTS5).
  Two files: `library.db` (metadata) and per-size `thumbs-<N>.db` (thumbnail
  blobs), stored in `~/.local/share/pichouse/`.
- **Images:** `image` (decode/encode) + `fast_image_resize` (Catmull-Rom resize).
- **EXIF:** `kamadak-exif`
- **AI tagging:** `reqwest` (blocking, rustls-tls) + `serde` (Ollama HTTP client).
- **Hashing:** `sha2` (content hash used as the thumbnail cache key).

## System prerequisites (Debian 13; also required on the Gitea runner)

    sudo apt-get update && sudo apt-get install -y gcc pkg-config libgtk-4-dev libgirepository1.0-dev

GTK4 (>= 4.10) must be present at runtime; Debian 13 ships GTK 4.18.

## Build / run / test

    cargo build
    cargo run
    cargo test

## Layout

    src/main.rs          entry point
    src/version.rs       Version constant (mirrors Cargo.toml, read by CI)
    src/model.rs         shared types
    src/db/              SQLite schema + access (library.db, thumbs-<N>.db,
                         immich-thumbs-<server_id>.db); includes
                         virtual_albums.rs (virtual album CRUD, membership, and
                         rule evaluation), immich_thumbs.rs (per-server Immich
                         thumbnail cache), edits.rs (non-destructive per-photo
                         edits) and presets.rs (levels presets)
    src/scan.rs          filesystem scanner (Phase 1 structure walk + Phase 2
                         per-file enrich helper)
    src/reconcile.rs     library freshness: diff disk against the DB per folder
    src/thumb.rs         thumbnail generation + cache (applies edits at render)
    src/edit.rs          non-destructive edit pipeline (flip, straighten, crop,
                         levels, brightness/contrast) + auto-levels
    src/ai/              local AI tagging backend (Ollama HTTP client, tagger)
    src/immich/          Immich server integration (blocking HTTP client)
    src/ui/              GTK4 UI (app, state, grid, sidebar, viewer, editor,
                         export, properties, toolbar, status, settings,
                         settings_ai, settings_immich, aitag, immich, tagmanager,
                         shortcuts, dialogs, actions, controller, prefs,
                         photo_object, util, enrich, freshness, watcher,
                         newfiles, vrules, vmenu)
    .gitea/workflows/    CI (build/test/release on push to main)

## Architecture patterns

Read this section before you explore the code. It gives the reusable patterns
and the file and name anchors. It does not give line numbers. Line numbers
change. Names do not. Use grep to find a name.

### Settings

The application has no config file. Settings are key/value rows in the
`settings` table in `library.db`. Read a setting with `Library::get_setting`.
Write a setting with `Library::set_setting`. The setting key names are string
constants in `src/ui/prefs.rs`.

The application loads the AI config once at startup in `prefs::load_ai_config`.
`AppState` holds the config in a `RefCell`.

The settings dialog is a `Stack` with a `StackSidebar`. Each pane is a function
that returns a `GtkBox`. Panes register in `src/ui/settings.rs` with
`stack.add_titled`. A pane writes the in-memory `RefCell` and the DB setting on
each widget change. The dialog has no Save button.

To add a settings pane, write a new pane function and add one `stack.add_titled`
line in `src/ui/settings.rs`.

### DB schema and migration

The schema is `src/db/schema.sql`. Every table uses
`CREATE TABLE IF NOT EXISTS`. There is no schema version table.

`Library::open_at` runs the schema, then runs `migrate`. `migrate` is in
`src/db/library.rs`. `migrate` is idempotent. `migrate` adds columns only. It
reads `PRAGMA table_info` and adds a missing column with
`ALTER TABLE ... ADD COLUMN`.

To add a table, add a `CREATE TABLE IF NOT EXISTS` block to `schema.sql`.
To add a column to an existing table, add the column to `migrate`.

The `PHOTO_COLS` constant and the `map_photo` row-mapper are shared across the
query files.

### Sidebar sections

The sidebar tree is in `src/ui/sidebar.rs`. The tree is a `TreeListModel` over
string ids. Each id has a prefix, for example `album:<id>` or `valbum:<id>`.
The tree persists its expansion in the DB under a settings key.

The "Virtual Albums" section is the template for a new section. Its header id
is `VIRTUAL_HEADER_ID`. `reload` reads the DB into a `TreeData` struct and
builds the root id list. `child_ids` maps a node id to its child ids.
`node_label` gives a node its text and icon. `on_selection_changed` dispatches
by id prefix.

To add a section, do these steps:
1. Add id constants for the header and the item prefix.
2. Add data fields to `TreeData` and fill them in `reload`.
3. Push the header id to the root id list in `reload`.
4. Handle the ids in `child_ids`, `node_label`, and `on_selection_changed`.

### Grid entry points

The grid is in `src/ui/grid.rs`. The `Grid` holds a `Source` enum. The variants
are `Folder`, `RawDir`, `VirtualAlbum`, and `None`.

The loaders are `show_folder`, `show_virtual_album`, and `show_photos`.
`show_photos` shows an ad-hoc list of photos. `show_photos` has no re-queryable
source. `show_photos` is the simplest entry point for a remote photo set.

`AppState::show_virtual_album` in `src/ui/state.rs` is the wiring template. It
sets the current view, calls the grid loader, and updates the status bar.

A photo in the grid is a `PhotoObject`. `PhotoObject` is a GObject wrapper of a
`model::Photo`. `PhotoObject::from_photo` builds one. The grid fills the
`texture` property from the thumbnail cache.

### Background HTTP pattern

The AI backend is the template for background HTTP work. `src/ai/client.rs`
wraps a blocking `reqwest` client with a `base_url`. It sends requests and
deserializes JSON responses into `serde` structs.

`src/ui/aitag.rs` shows how to run the work off the GTK main thread:
1. Define a `Msg` enum for progress and results.
2. Create a channel with `glib::MainContext::channel`.
3. Attach the receiver with `rx.attach`. The receiver runs on the GTK main
   thread. It updates the UI.
4. Spawn a coordinator thread. The coordinator owns a `Client`. It starts a
   worker pool. Each worker does blocking HTTP and sends `Msg` values through a
   cloned sender.
5. Cancel the work with a `Controller`. A `Controller` holds an
   `Arc<AtomicBool>`. `AppState` holds the `Controller`.

Reuse this pattern for the Immich client.

## CI

`.gitea/workflows/build.yaml` builds on push to `main` on the `debian-go` runner,
reads the version from `Cargo.toml`, runs `cargo test --release` and `cargo build
--release`, and publishes a rolling pre-release. Build-only — it does not launch
the GUI. The runner host must have the system prerequisites installed (see above)
plus a Rust toolchain (`cargo`).

## Conventions

- All major changes are committed and pushed (push triggers CI).
- Keep AGENTS.md and README.md updated as the build progresses.
- Do regular handoffs to HANDOFF.md when the working context gets large.
