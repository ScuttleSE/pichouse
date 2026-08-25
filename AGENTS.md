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

## Project

**pichouse** — a Picasa-like photo library GUI application for Linux, written in Go.

- Add one or more Library folders; they are scanned into a local SQLite database.
- Browsing the library reflects the cached DB state by default.
- A separate raw filesystem "folder view" is also available.
- Thumbnails are generated on first view and cached in a separate SQLite DB.
- UI layout mimics Picasa 3 (see `picasa.png`), with modern styling.

## Tech stack

- **Language:** Go (module `git.hemmalab.se/scuttle/pichouse`)
- **GUI:** Fyne v2 (default X11/GLFW driver; runs via XWayland on Wayland sessions)
- **DB:** `modernc.org/sqlite` (pure-Go). Two files: `library.db` (metadata), `thumbs.db` (thumbnail blobs), stored in `~/.local/share/pichouse/`.
- **EXIF:** `github.com/rwcarlsen/goexif`

## System prerequisites (Debian 13; also required on the Gitea runner)

    sudo apt-get update && sudo apt-get install -y gcc libgl1-mesa-dev xorg-dev libxxf86vm-dev libwayland-dev libxkbcommon-dev

On a Wayland session, ensure XWayland is present (default on GNOME/KDE): `sudo apt-get install -y xwayland`.

## Build / run / test

    go build ./...
    go run ./cmd/pichouse
    go test ./...

## Layout

    cmd/pichouse/        entry point
    internal/version/    Version constant (read by CI)
    internal/db/         SQLite schema + access (library.db, thumbs.db)
    internal/scan/       filesystem scanner
    internal/thumb/      thumbnail generation + cache
    internal/model/      shared types
    internal/ui/         Fyne UI (app, sidebar, grid, properties, toolbar, folderview)
    .gitea/workflows/    CI (build on push to main, rolling pre-release)

## CI

`.gitea/workflows/build.yaml` builds on push to `main` on the `debian-go` runner (this machine),
reads the version from `internal/version/version.go`, builds `./cmd/pichouse` with `CGO_ENABLED=1`,
and publishes a rolling pre-release. Build-only — it does not launch the GUI. The runner host must
have the system prerequisites installed (see above).

## Conventions

- All major changes are committed and pushed (push triggers CI).
- Keep AGENTS.md and README.md updated as the build progresses.
- Do regular handoffs to HANDOFF.md when the working context gets large.
- **Never commit `build.yaml` (the reference template) or `picasa.png`.** They are gitignored.
  Delete both once they are no longer needed.
