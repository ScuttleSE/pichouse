# pichouse

A Picasa-like photo library application for Linux, written in Rust.

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
- **Fast two-phase import.** For large imports the folder tree and grid appear
  almost immediately: Phase 1 records only the file/folder structure, and a
  background Phase 2 fills in EXIF date, dimensions, the content hash, and the
  thumbnail per photo. Un-enriched photos show a filename placeholder until
  their thumbnail lands. Opening a folder moves its photos to the front of the
  enrichment queue, and an interrupted import resumes on the next launch.
- **Library freshness.** pichouse keeps the library in step with disk. It
  reconciles disk against the database on startup, on demand (the Refresh
  Library toolbar button), and on a periodic timer: files added on disk appear,
  and files removed are marked "missing" (shown dimmed) so their tags survive a
  temporary unmount or move. A file that reappears — including under a new name
  (a move) — reuses its existing row. On local folders an inotify watcher reacts
  quickly; on network drives (NFS/SMB), where inotify cannot see remote changes,
  the periodic reconcile keeps things fresh.
- **New Files view.** A "New Files" entry at the top of the Library tab collects
  files added to your library folders *after* the initial scan. Selecting it
  shows them grouped by folder (a header per folder, thumbnails below). Entries
  drop off automatically after two weeks. Removed files are marked "missing" and
  shown dimmed so their tags survive a temporary unmount or move.
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
- **Non-destructive editing.** An "Edit" tab in the right-hand panel (next to Pic
  Info and Tags) applies flip, straighten, crop, brightness/contrast, and
  per-channel color levels. Edits are stored in `library.db` and applied at view
  time and to thumbnails; the original file on disk is never changed. Open it
  from the viewer's Edit button or right-click → Edit on a thumbnail. Toggle
  "View original", "Revert all", or "Export copy…" to bake the edits into a new
  file. Immich photos can be edited too (the full asset is fetched on demand).
- **Baked export.** Right-click one or more thumbnails → "Export edited copy…" to
  write new files with edits applied. A dialog picks JPEG/PNG and JPEG quality,
  remembered for next time; several photos export into a chosen folder.
- **Color levels for negative scans.** Per-channel (R/G/B) black/white/gamma with
  a live, draggable histogram per channel and a one-click Auto levels (from the
  histogram) to fix color casts. Save named levels presets and apply a preset to
  a whole folder at once.

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

## Duplicate image finder

Open the toolbar **Tools** menu and pick **Find Duplicates…**. Choose a scope
(current folder, selected albums, or the whole library) and a similarity level.

- Byte-identical files match by their stored SHA-256 hash.
- Visually similar files (resized, re-compressed, minor edits) match by a
  64-bit perceptual hash (dHash). The slider sets the maximum Hamming distance.
- Each group auto-selects the "worse" copy for deletion and keeps the best one
  (larger pixel area, then more lossless format, then larger file, then older).
- Results show one group per row, each inside a framed box. The marked
  "worse" copy shows a red **X**. Click any photo to move the X to it. Click the
  X to unmark. The **Delete marked** button then removes the marked files.

## Immich integration (optional)

pichouse can connect to an [Immich](https://immich.app) server and browse its
albums and photos alongside your local library.

- **Connect** in *Settings → Immich*: enter your server URL and API key, then
  *Test Connection*.
- Immich albums appear in the sidebar next to your local Albums, and their
  photos load into the same thumbnail grid, viewer, and properties panel used
  for local photos.
- **Editing works too.** Immich photos support the same non-destructive
  editing as local photos (flip, straighten, crop, brightness/contrast, color
  levels) — the full-resolution asset is fetched on demand, and edits are
  applied at view time without modifying anything on the Immich server.
- Thumbnails for Immich photos are cached locally, per connected server
  (`immich-thumbs-<server-id>.db`), alongside the rest of pichouse's data.

## Facial recognition (local, optional)

pichouse can detect faces, group the same person across your library, and let
you name people — Picasa "People"-style. It is **off by default** and all
processing stays on your machine.

- **Backend:** in-process [ONNX Runtime](https://onnxruntime.ai) through the
  `ort` crate. The models are **YuNet** (face detection, MIT) and **SFace**
  (face embedding, Apache 2.0) from the OpenCV Zoo. They run comfortably on a
  CPU that is a few years old.
- **First use downloads everything.** The ONNX Runtime library and the two
  models download into the data folder (`~/.local/share/pichouse/`) the first
  time you enable faces, each with a verified SHA-256. Nothing is shipped in the
  binary and nothing is downloaded until you ask.
- **Enable it** in *Settings → Faces*: toggle it on, click *Download models*,
  then *Scan for faces now*. Turn on *Scan new photos automatically* if you want
  new imports scanned without asking.
- **Name people** with *Review people*: it shows unnamed groups as face crops.
  Name a group to create a person, or merge a group into an existing person.
- **Browse a person** from the **People** section in the Library sidebar:
  selecting a person shows every photo they appear in.
- **Smart albums** can use a *Contains person* rule, so a virtual album can hold
  every photo of a chosen person.
- **Privacy:** *Delete all face data* removes every face, person, and grouping.
  Photos on disk are never changed by face detection.

Faces, people, and per-photo scan state live in `library.db`; face-crop
thumbnails live in `face-thumbs.db`.

### People vs. Characters: two separate recognition pipelines

**People** (above) detects and groups *human* faces using YuNet/SFace.
**Characters** is a separate pipeline for *stylised* art — anime, cartoons,
illustrations, furries — using its own detector (an anime-tuned YOLOv8-nano)
and a CCIP CaFormer embedder trained for anime character re-identification
(both from [deepghs](https://huggingface.co/deepghs); the embedder is
OpenRAIL-M licensed, the detector MIT). Both pipelines live under *Settings →
Faces* and work the same way day-to-day (scan, review, name, browse), but
they run independently: a photo can be grouped by one pipeline, both, or
neither, depending on whether it contains real or drawn faces.

### Managing face and character groups

The People view and the Characters view show one tile per group. Both views add
faces during a scan. The Characters grid keeps a stable order. A new group
appends at the end. A tile does not move after it appears.

You manage a group in two places:

- **Group tile (double-click, single-click, right-click).** A double-click opens
  the group's photos. A single click selects the group. Select more groups with
  more single clicks. A shift-click selects every group between the last clicked
  group and the shift-clicked group. A selection bar then shows *Do not scan
  selected* and
  *Clear selection*. *Do not scan selected* marks every photo in the selected
  groups unimportant. A right-click on a tile opens a menu. A named tile offers
  *Rename*, *Clear name*, *Delete character*, and *Do not scan this group*.
  *Clear name* makes the group unnamed again and keeps its members. An unnamed
  tile offers *Name this group* and *Do not scan this group*. When more than one
  unnamed group is selected, *Name this group* names every selected group as one
  new character.
- **Photo grid (right-click).** Open a group, then right-click one or more
  photos. In a character group you can *Remove from this character* or
  *Not this character (ban)*. A ban records a rejection, so a re-scan never adds
  the photo to that character again. In an unnamed group you can
  *Remove from this group*.

*Do not scan these (mark unimportant)* marks the selected photos unimportant.
pichouse then excludes these photos from every future face scan, both human and
stylised. It also removes them from every face group at once. The mark is stored
in `photos.skip_face_scan`.


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

## Logging / troubleshooting

pichouse logs to the console (stderr). Verbosity is set by command-line flags on
the shipped binary:

    pichouse            # default: warnings and errors only
    pichouse -v         # info: scan start/end summaries
    pichouse -vv        # debug: per-directory DB timings + sidebar-reload breakdown
    pichouse -vvv       # trace: per-photo detail
    pichouse -q         # quiet: errors only
    pichouse --help     # show usage

The `-vv` level is the one to use when the UI stalls for several seconds while
scanning a new library folder. Every log line is tagged with the thread that
emitted it (`main`, `scan`, `enrich0`…), and operations log *before* they start,
so a freeze is easy to attribute: the last line before a stall names the thread
and the exact operation that hung (for example a `db lock: waiting for
connection` with no following `acquired`, or a `sidebar.reload: new_photos_count`
with no following timing). It also prints how long `collect_images` took,
per-directory `insert_batch` timings, a `sidebar.reload` breakdown
(`folder_counts`/`new_photos_count`/`va_counts`), and warns when acquiring the
shared database lock takes more than 200 ms — the usual cause of scan-time
freezes.

## Test

    cargo test

## Releases

Tagged releases (`vX.Y.Z`) publish a prebuilt `linux/amd64` binary as a
[GitHub Release](https://github.com/ScuttleSE/pichouse/releases). If you'd
rather build from source, see [Build and run](#build-and-run) above.

## License

pichouse's own source code is released into the public domain under the
[Unlicense](LICENSE) — do whatever you want with it.

This does **not** cover the third-party models the optional AI features
download at runtime (see [Facial recognition](#facial-recognition-local-optional)
and [Immich integration](#immich-integration-optional) above); nothing here
is shipped in the binary, but if you enable a feature and it downloads a
model, that model's own license applies to it:

- **YuNet** (face detection) — MIT
- **SFace** (face embedding) — Apache 2.0
- **Anime detector** (YOLOv8-nano, stylised faces) — MIT
- **CCIP CaFormer** (stylised face embedding) — OpenRAIL-M

pichouse's Rust dependencies each carry their own (permissive)
open-source licenses as well; see `Cargo.toml`.
