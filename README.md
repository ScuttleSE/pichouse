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
- Left sidebar tabs: a Library tree grouped by year with item counts, and a
  Folders tree that drills into each added root's subfolders live from disk.
- Thumbnail grid with a size slider that snaps to preset sizes.
- Properties panel for the selected photo.

## Tech stack

- **Language:** Go (module `git.hemmalab.se/scuttle/pichouse`)
- **GUI:** [Fyne](https://fyne.io) v2 (default X11/GLFW driver; runs via
  XWayland on Wayland sessions)
- **Database:** `modernc.org/sqlite` (pure-Go)
- **EXIF:** `github.com/rwcarlsen/goexif`

Databases are stored in `~/.local/share/pichouse/`.

## System prerequisites (Debian 13)

    sudo apt-get update && sudo apt-get install -y gcc libgl1-mesa-dev xorg-dev libxxf86vm-dev libwayland-dev libxkbcommon-dev

On a Wayland session, ensure XWayland is present (default on GNOME/KDE):

    sudo apt-get install -y xwayland

## Build and run

    go build ./...
    go run ./cmd/pichouse

## Test

    go test ./...

## Continuous integration

Pushing to `main` triggers a Gitea Actions workflow that builds a
`linux/amd64` binary and publishes a rolling pre-release.
