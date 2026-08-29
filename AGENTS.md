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

## RULE ZERO-A — ALL COMMUNICATION USES ASD-STE100 STRICT

Write all text in Simplified Technical English (ASD-STE100), Strict mode.

- This rule covers every text you write. It covers your chat answers to the
  user. It covers commit messages. It covers README.md, HANDOFF.md, ROADMAP.md,
  and all other documents. It covers code comments.
- Load the `asd-ste100` skill at the start of each session. Apply Strict mode.
- Use short sentences. Use active voice. Use one instruction per sentence. Give
  each word one meaning.
- Keep sentences to 20 words or fewer for instructions. Keep sentences to 25
  words or fewer for descriptions.
- Do not use semicolons. Do not use phrasal verbs. Do not use marketing words.
- Keep a hedge as a hedge. Do not change "may fail" into "fails".

---

## RULE ZERO-B — ANSWER FIRST, DO NOT VOMIT TEXT

Give the short answer first. Stop. Do not write a wall of text.

- When you ask the user to do a task, write only the task. Then wait.
- Do not add a plan, a hypothesis, or a "what I am looking for" section to a
  request for action. Wait for the result first.
- Do not restate the plan after each step. The user reads the plan once.
- Keep each reply short. Add detail only when the user asks for it.

---

## RULE ZERO-C — DIAGNOSE WITH DATA, NOT ASSUMPTIONS

Find the true cause before you claim a cause.

- Do not blame or clear a change without evidence. Get a measurement first.
- Do not assume the user's environment. The user runs the binary on a
  different machine. Local disk state, tools, and timing do not transfer.
- Trust the trace over the theory. When strace or gdb data conflicts with your
  code reading, the data wins.
- A "window did not appear" symptom means the main thread blocks. Find the
  main-thread stall (the futex or syscall gap), not a background worker.
- Confirm the fix solves the measured problem. Do not stop at "it builds".

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
- CI keeps one rolling pre-release from the latest `main` build.

### Named release

A named release is a permanent release on the release page. It also mirrors the
code and the release to GitHub (`ScuttleSE/pichouse`).

- The user asks for a named release and gives the exact version `X.Y.Z`.
- Create the tag and push it. Run `git tag vX.Y.Z`. Run `git push origin vX.Y.Z`.
- The tag push starts `.gitea/workflows/release.yaml`.
- The workflow writes `X.Y.Z` into `Cargo.toml` on `main`. It commits the change
  back with `[skip ci]`.
- The workflow builds, tests, and publishes a Gitea release with the binary.
- The workflow pushes the commit and the tag to GitHub. It publishes a GitHub
  release with the binary.
- The GitHub push needs a `GH_TOKEN` secret in the Gitea repo settings. The
  token is a GitHub PAT with `contents: write` on `ScuttleSE/pichouse`.
- Do not push a tag without the user's request.

---

## RULE FOUR — CLEAN THE DEBUG BUILD CACHE

The debug build cache fills the disk fast on this machine.

- The `target/debug/incremental` directory grows to many gigabytes. It is a
  disposable cache. Cargo rebuilds it on the next build.
- Delete it after each test cycle. Run `rm -rf target/debug/incremental`.
- Do not delete `target/release`. CI builds with `--release`.
- Check free space with `df -h /` if a build fails with a disk error.

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

### ONNX Runtime (facial recognition)

The face feature uses `ort` with the `load-dynamic` feature. It loads
`libonnxruntime.so` at run time. The library is not built into the binary. The
library is not committed. The build and CI do not need it.

Face detection is off by default. The first time the user turns it on, pichouse
downloads ONNX Runtime 1.22.0 from the official Microsoft release into the data
folder (`~/.local/share/pichouse/runtime/`), with a verified SHA-256. It then
loads the library from there. See `src/face/runtime.rs`. The URL and the hash
are constants in that file.

## Build / run / test

    cargo build
    cargo run
    cargo test

## Layout

    src/main.rs          entry point
    src/version.rs       Version constant (mirrors Cargo.toml, read by CI)
    src/model.rs         shared types
    src/db/              SQLite schema + access (library.db, thumbs-<N>.db,
                         immich-thumbs-<server_id>.db, face-thumbs.db);
                         includes virtual_albums.rs (virtual album CRUD,
                         membership, and rule evaluation), immich_thumbs.rs
                         (per-server Immich thumbnail cache), faces.rs (persons,
                         faces, face-scan state, clustering helpers),
                          face_thumbs.rs (face-crop cache), style_faces.rs
                           (characters, stylised faces, style-face-scan state,
                           HDBSCAN clustering helpers, group/photo management,
                           and the photos.skip_face_scan flag), style_face_thumbs.rs
                          (stylised face-crop cache), edits.rs
                          (non-destructive per-photo edits), presets.rs
                          (levels presets) and duplicates.rs (duplicate-finder
                          queries: in-scope photos, phash backfill, hard delete)
    src/scan.rs          filesystem scanner (Phase 1 structure walk + Phase 2
                         per-file enrich helper; also computes the dHash)
    src/phash.rs         perceptual hash (64-bit dHash) for the duplicate finder
    src/dedup.rs         duplicate finder engine (exact + near grouping, ranking)
    src/reconcile.rs     library freshness: diff disk against the DB per folder
    src/thumb.rs         thumbnail generation + cache (applies edits at render)
    src/edit.rs          non-destructive edit pipeline (flip, straighten, crop,
                         levels, brightness/contrast) + auto-levels
    src/ai/              local AI tagging backend (Ollama HTTP client, tagger)
    src/face/            local facial recognition (ort/ONNX Runtime): config,
                         runtime (library download+init), models (catalog+
                         download), detector (YuNet), embedder (SFace), cluster
    src/styleface/       stylised face recognition for anime/cartoon/furry art
                         (shares src/face/runtime): config, models (catalog+
                         download), detector (anime YOLOv8n), embedder (CCIP
                         CaFormer), cluster (HDBSCAN crate)
    src/immich/          Immich server integration (blocking HTTP client)
    src/ui/              GTK4 UI (app, state, grid, sidebar, viewer, editor,
                         export, properties, toolbar, status, settings,
                         settings_ai, settings_immich, settings_faces,
                         settings_characters, aitag, facescan, stylefacescan,
                         albumscan, people, characters, facesview,
                         charactersview, immich,
                         tagmanager, shortcuts, dialogs, actions, controller,
                         prefs, photo_object, util, enrich, freshness, watcher,
                         newfiles, vrules, vmenu, dedup_scan)
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

The "New Files" and "Missing Files" rows are leaf sections (no children). A
leaf needs an id constant, a `TreeData` count field, a `reload` push when the
count is over zero, a `node_label` branch, and an `on_selection_changed`
dispatch. "Missing Files" lists photos with `missing = 1` and offers a
right-click "Clear Missing Files…" action that calls `delete_missing_photos`.

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

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
