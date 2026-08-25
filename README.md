# pichouse

A Picasa-like photo library application for Linux, written in Go.

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


## Tech stack

- **Language:** Go (module `git.hemmalab.se/scuttle/pichouse`)
- **GUI:** GTK4 via [gotk4](https://github.com/diamondburned/gotk4) v0.3.1
  (native desktop; requires cgo; targets the system GLib 2.84 on Debian 13)
- **Database:** `modernc.org/sqlite` (pure-Go)
- **EXIF:** `github.com/rwcarlsen/goexif`

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

    go build ./...
    go run ./cmd/pichouse

## Test

    go test ./...

## Continuous integration

Pushing to `main` triggers a Gitea Actions workflow that builds a
`linux/amd64` binary and publishes a rolling pre-release.
