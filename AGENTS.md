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

- Store the version in `internal/version/version.go` as `Version`.
- CI increases the build number by 1 on each push to `main`.
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

**pichouse** — a Picasa-like photo library GUI application for Linux, written in Go.

- Add one or more Library folders; they are scanned into a local SQLite database.
- Browsing the library reflects the cached DB state by default.
- A separate raw filesystem "folder view" is also available.
- Thumbnails are generated on first view and cached in a separate SQLite DB.
- UI layout mimics Picasa 3 (see `picasa.png`), with modern styling.

## Tech stack

- **Language:** Go (module `git.hemmalab.se/scuttle/pichouse`)
- **GUI:** GTK4 via [gotk4](https://github.com/diamondburned/gotk4) (`github.com/diamondburned/gotk4/pkg`). Native desktop; requires cgo. **Pinned to v0.3.1**, which targets GLib 2.84 (Debian 13). Newer gotk4 (v0.4.x) requires GLib >= 2.88, which Debian 13 does not ship — do not upgrade this dependency without also upgrading GLib.
- **DB:** `modernc.org/sqlite` (pure-Go). Two files: `library.db` (metadata), `thumbs.db` (thumbnail blobs), stored in `~/.local/share/pichouse/`.
- **EXIF:** `github.com/rwcarlsen/goexif`

## System prerequisites (Debian 13; also required on the Gitea runner)

    sudo apt-get update && sudo apt-get install -y gcc pkg-config libgtk-4-dev libgirepository1.0-dev

GTK4 (>= 4.10) must be present at runtime; Debian 13 ships GTK 4.18.

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
    internal/ai/         local AI tagging backend (Ollama HTTP client, tagger)
    internal/model/      shared types
    internal/ui/         GTK4 UI (app, layout, sidebar, foldertree, grid, properties, toolbar, status, settings, aitag, tagmanager)
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
