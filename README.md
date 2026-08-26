# pichouse

A Picasa-like photo library application for Linux, written in Rust.

> **Rewrite in progress.** pichouse is being ported from Go to Rust on the
> `rust-port` branch. Core logic modules are ported first; the GTK4 UI last.
> The feature descriptions below describe the target application.

pichouse lets you add one or more library folders through a Settings dialog,
scans them into a local SQLite database, generates cached thumbnails, and lets
you browse your photos through a Picasa-style interface: a collapsible sidebar
on the left (with a year-grouped Library tab and a raw filesystem Folders tab),
a thumbnail grid in the center, and a properties panel on the right.

## Features (milestone 1)

- Add or remove library folders via a Settings dialog (extensible for more
  settings later).
- Folders scanned into a local SQLite database (`library.db`); scans can be
  stopped from the status bar.
- Browsing reflects the cached database state by default.
- Thumbnails generated on first view and cached separately (`thumbs.db`).
- Left sidebar tabs:
  - **Library** — a virtual organisation of scanned folders. Folders not yet
    filed appear under "New folders". Right-click to create Albums and
    Sub-Albums, then move folders into them (multi-select and drag-and-drop
    supported). Album membership is virtual and never moves files on disk.
  - **Folders** — a live filesystem tree that drills into each added root's
    subfolders. Thumbnails already generated during scanning are reused, so
    reopening a folder is fast.
- Thumbnail grid with a size slider that snaps to preset sizes.
- Properties panel for the selected photo.

## AI-based tagging (local, optional)

pichouse can generate keyword tags for your photos using a **local** vision
model. Nothing leaves your machine and no models are downloaded automatically.

- **Backend:** a local [Ollama](https://ollama.com) HTTP server
  (`127.0.0.1:11434`). Install Ollama and pull a vision model, e.g.
  `ollama pull moondream` (small/fast) or `ollama pull llava`.
- **Runs on CPU or GPU** — whichever Ollama is configured to use.
- **Enable it** in *Settings → AI Tagging*: toggle it on, choose the model, and
  optionally let pichouse start Ollama automatically. Use *Test Connection* to
  verify the server and model are available.
- **Run tagging** from the toolbar AI button: *Tag Current Folder* or *Tag
  Entire Library*. Progress shows in the status bar and can be stopped. You can
  also tag a single selected photo from the *Tags* tab of the properties panel.
- **Tag management:** the *Tags* tab lets you add user tags, confirm or remove
  AI tags per photo. The *Tag Manager* (toolbar AI menu) renames, merges, and
  deletes tags across the whole library.
- **Search:** the toolbar search box matches filenames **and** tags (via a
  full-text index), so typing `beach` finds every photo tagged `beach`.

All tags are stored in `library.db`. AI and user tags share one table and are
distinguished by a source flag.

### Controlling CPU/GPU load

Even when Ollama runs the model on the GPU, vision models do image
preprocessing (the CLIP/`mmproj` embedding step) and prompt prefill that may run
on the CPU, which can spike CPU usage during a batch.

**Can the vision preprocessing run on the GPU?** Sometimes — it is decided by
Ollama, not pichouse. It runs on GPU only when Ollama's llama.cpp build supports
GPU for the vision encoder *and* there is enough free VRAM to offload it. If the
language model already fills VRAM, Ollama pushes the overflow (encoder/prefill)
to the CPU. Check with `ollama ps` while tagging: "100% GPU" means only prefill
is on CPU; a split like "52%/48% CPU/GPU" means the model does not fully fit —
use a smaller model or quantization (e.g. `moondream` instead of `llava:13b`).

pichouse exposes knobs (Settings → AI Tagging) that reduce CPU load:

- **Concurrency** defaults to **1**. Parallel requests to a single local GPU do
  not improve throughput and make Ollama spin up extra runners or do parallel
  CPU prefill.
- **CPU threads** caps how many CPU threads Ollama may use (`0` = automatic).
- **Context size** caps the context window (`0` = model default); smaller
  reduces CPU-side prompt prefill.
- **Max tokens** caps generated tokens per image (default `128`). Some models
  (notably `llava`) can generate without stopping and never return; this cap
  guarantees tagging finishes.
- The model is kept resident between images (`keep_alive`) so it is not reloaded
  mid-batch.

Per-image timing (`load`, `prompt_eval`, `eval`) is written to the log so you
can see where time is spent.


## Tech stack

- **Language:** Rust (2021 edition, binary crate `pichouse`)
- **GUI:** GTK4 via [gtk4-rs](https://gtk-rs.org/) 0.7.x with the `v4_10`
  feature (native desktop; targets the system GLib 2.84 on Debian 13)
- **Database:** `rusqlite` with the bundled SQLite (includes FTS5)
- **Images:** `image` + `fast_image_resize` (Catmull-Rom resize)
- **EXIF:** `kamadak-exif`
- **AI tagging:** `reqwest` (blocking, rustls-tls) + `serde`

Databases are stored in `~/.local/share/pichouse/`.

## AI tagging prerequisites (optional)

AI tagging needs a local [Ollama](https://ollama.com) install with a vision
model pulled (e.g. `ollama pull moondream`). pichouse adds no build-time
dependencies for this — it talks to Ollama over local HTTP. The feature is off
by default and the app runs normally without Ollama present.

## System prerequisites (Debian 13)

    sudo apt-get update && sudo apt-get install -y gcc pkg-config libgtk-4-dev libgirepository1.0-dev

GTK4 (>= 4.10) must be present at runtime; Debian 13 ships GTK 4.18.

## Build and run

    cargo build
    cargo run

## Test

    cargo test

## Continuous integration

Pushing to `main` triggers a Gitea Actions workflow that builds a
`linux/amd64` binary and publishes a rolling pre-release.
