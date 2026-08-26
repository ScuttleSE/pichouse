# HANDOFF

This document is for an agent with no memory of the last session. It uses
Simplified Technical English (ASD-STE100, Strict). Read AGENTS.md first. Read
ROADMAP.md for planned features.

## 1. Current task

The user studies a rewrite of pichouse from Go to Rust. The user keeps the Go
app on `main`. The user builds a Rust spike on a branch.

## 2. Branches

- `main` holds the Go application. `main` is the product branch.
- `spike/rust-gtk4-grid` holds the Rust spike. Do not merge the spike into
  `main` now.
- Merge the spike into `main` only when the user converts the app to Rust.
- Do not change the spike branch now. Leave it as it is.

## 3. State of the Rust spike (branch `spike/rust-gtk4-grid`)

The spike proves the hardest part of a Rust port. The hardest part is the GTK4
thumbnail grid.

- The spike is in `spike/rust-grid/`.
- The spike uses `gtk4-rs` (`gtk4` crate 0.9), `gdk-pixbuf`, `glib`, `gio`, and
  `rusqlite` (bundled, FTS5-capable).
- The spike shows a `GridView`. The `GridView` uses a `SignalListItemFactory`
  and a custom GObject item type `PhotoObject`.
- The spike reads the real pichouse databases. It reads `library.db` for photo
  rows. It reads `thumbs-320.db` for thumbnail JPEG blobs.
- The spike decodes JPEG blobs with `PixbufLoader`.
- The spike builds and links against system GTK 4.18 on this machine.
- The spike needs a populated library to show thumbnails. No library exists
  yet. Run the Go app first to build a library and thumbnails. Then run the
  spike.
- Override the data directory with the environment variable
  `PICHOUSE_DATA_DIR`.

Result: the spike removes the main risk. Every other Go dependency has a direct
Rust crate. See ROADMAP.md and the spike README for detail.

## 4. Rust toolchain

- The user's account has rustup and cargo in `/home/scuttle/.cargo`. This is
  for local builds only.
- CI runs as the user `gitea-runner`, uid 1001, home `/home/gitea-runner`. CI
  does not see the developer's cargo.
- The spike CI installs rustup into the runner user home if cargo is missing.

## 5. Spike CI (branch only)

- The file is `.gitea/workflows/build-rust-spike.yaml`. It exists on the spike
  branch only.
- It runs on push to `spike/rust-gtk4-grid`.
- It checks GTK dev libraries with `pkg-config`. It installs nothing when the
  libraries are present. The runner has the libraries.
- The runner user has no passwordless sudo. Do not call `sudo` without a guard.
  Use `sudo -n` only, and only when a dependency is missing.
- The spike CI builds with `cargo build --release`. It does not publish a
  release. The binary is at
  `spike/rust-grid/target/release/pichouse-spike`.

## 6. Versioning (on `main`, in effect now)

Read AGENTS.md RULE THREE for the policy.

- The version format is major.minor.build. The series started at 0.0.0.
- The current version is 0.0.1.
- The version is in `internal/version/version.go` as `Version`.
- CI increases the build number by 1 on each push to `main`.
- CI commits the new version back. The commit message contains `[skip ci]`.
  Gitea skips a commit that contains `[skip ci]`. This stops a loop.
- Do not increase the build number by hand.
- A release increases major, minor, or build. The user asks for a release.
- Ask the user which part to increase if the user does not say.
- For a major release, increase major by 1. Set minor to 0. Set build to 0.
- For a minor release, increase minor by 1. Set build to 0.
- For a build release, increase build by 1.

## 7. CI facts you must know (they caused failures before)

- Gitea does not support `||` in a `${{ }}` expression. The expression
  `${{ secrets.GITEA_TOKEN || github.token }}` makes the workflow invalid.
  An invalid workflow creates NO run. Use `${{ secrets.GITEA_TOKEN }}` alone.
- A repository secret `GITEA_TOKEN` exists. It has write access. CI uses it to
  push the version-bump commit back to `main`.
- The Go release workflow `.gitea/workflows/build.yaml` does this on each push
  to `main`: it reads the version, increases the build, commits the version
  with `[skip ci]`, pushes to `main`, builds the binary, and updates one
  rolling pre-release with the tag `rolling`.
- The Go build needs cgo and GTK. The build is slow. Wait for it.

## 8. Next steps

- If the user continues the Rust port, follow the plan in this file section 9.
- If the user adds features, put ideas in ROADMAP.md. Keep ROADMAP.md
  structured. See its existing sections.
- Commit ROADMAP.md and AGENTS.md changes to `main`. Do not put them on the
  spike branch unless the user says so.

## 9. Rust port plan (for later, when the user commits to it)

Port bottom-up. Keep a build that runs at each step.

1. Scaffold a Cargo workspace. Mirror the Go layout as modules.
   Crates: `rusqlite` (bundled), `image`, `walkdir`, `sha2`, `reqwest`,
   `serde`, `serde_json`, `regex`, `kamadak-exif`, `gtk4`, `chrono`.
2. Port `internal/model`. Use structs and enums. Store times as i64.
3. Port `internal/db`. Copy `schema.sql`. Use one `Connection` behind a Mutex
   for `library.db`. Use one connection for the thumbnail database. Keep the
   FTS5 rebuild and the FTS query builder.
4. Port `internal/thumb`. Decode, scale (CatmullRom), rotate, encode (JPEG
   quality 85). Add `EncodeForAI`.
5. Port `internal/scan`. Walk in two passes. Read dimensions. Compute SHA-256.
   Read EXIF date. Derive the folder year.
6. Port `internal/ai`. Client for `/api/tags` and `/api/generate`. Manage an
   `ollama serve` subprocess. Clean tags with regex.
7. Port `internal/ui`. Build in this order: app and window, toolbar and status,
   sidebar and folder tree (`TreeListModel`), grid (`GridView` with a
   subclassed model and async thumbnails), properties, dialogs, drag and drop,
   keyboard shortcuts.
8. Change CI to `cargo build --release`. Keep the same system GTK packages.

## 10. Hard points in the Rust UI port

- The `GridView` and `TreeListModel` need a custom subclassed GObject model in
  `gtk4-rs`. This is more code than the Go version. The Go version uses a
  `StringList` and a parallel slice. Do not copy that shortcut.
- `gtk4-rs` widgets are not `Send`. Move async results to the main thread with
  `glib::MainContext::spawn_local` and an `async-channel`. Do not share widgets
  across threads.
- The existing Go code has no custom Cairo drawing and no custom GObject
  subclassing. This makes the port simpler.

## 11. Data model facts

- The pichouse data directory is `~/.local/share/pichouse/`.
- `library.db` holds metadata. See `internal/db/schema.sql`.
- The thumbnail cache is `thumbs-<size>.db`. The default size is 320. So the
  file is `thumbs-320.db`.
- The thumbnail cache table is `thumbnails(photo_hash PRIMARY KEY, size, jpeg
  BLOB, created_at)`.
- The thumbnail key is the photo SHA-256 hash, column `photos.hash`.
- A photo has no thumbnail until the scanner computes its hash.
