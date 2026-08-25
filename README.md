# pichouse

A Picasa-like photo library application for Linux, written in Go.

pichouse lets you add one or more library folders, scans them into a local
SQLite database, generates cached thumbnails, and lets you browse your photos
through a Picasa-style interface: a collapsible folder tree on the left, a
thumbnail grid in the center, and a properties panel on the right. A separate
raw filesystem folder view is also available.

## Features (milestone 1)

- Add one or more library folders.
- Folders scanned into a local SQLite database (`library.db`).
- Browsing reflects the cached database state by default.
- Thumbnails generated on first view and cached separately (`thumbs.db`).
- Folder tree grouped by year with item counts.
- Thumbnail grid with adjustable thumbnail size.
- Properties panel for the selected photo.
- Separate raw filesystem folder view.

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
