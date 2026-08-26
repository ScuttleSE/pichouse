# pichouse — Feature Roadmap

Running list of features to add down the road. This is a capture document for
ideas as they come up, not a committed plan or schedule. Items here are
unordered by priority unless noted.

## Immich integration

Interface with an [Immich](https://immich.app/) server so the library can work
alongside a self-hosted Immich instance.

### Browse
- Browse Immich albums from within pichouse.
- View the contents of an Immich album (photos/assets) inside the app.

### Upload
- Upload a pichouse album to Immich as a **new** album (created on the Immich
  server).
- Upload a pichouse album into an **existing** Immich album (add to it rather
  than create).

### Sync
- Tag a local album as "synced" with an Immich album.
- When a synced album is tagged, adding pictures to it automatically uploads
  them to Immich in the background.
- Sync tags back and forth between pichouse and Immich (two-way tag sync).

### Open questions / to decide
- Authentication: API key vs. user login; where credentials are stored.
- Server configuration: single server or multiple; where the URL is set
  (settings UI).
- Mapping between pichouse folders/albums and Immich albums.
- Deduplication: how to avoid re-uploading assets already on the server.
- Read-only browse vs. two-way sync (out of scope for now unless stated).
- Background upload: queue, retry, and status reporting for automatic uploads.
- Tag sync semantics: conflict resolution when a tag changes on both sides;
  how pichouse tag sources (AI vs. user) map to Immich tags.
- How "synced" state is persisted (per-album flag in library.db) and what
  happens when the link is broken or the remote album is deleted.

## Non-destructive image editing

Basic image editing, Picasa-style. Edits never modify the original file on
disk; they are stored in the database and applied at view time.

### Edits
- Basic operations: crop, rotate, and similar (e.g. straighten, flip; more
  adjustments like brightness/contrast/color TBD).

### Behaviour
- Edits are stored in the database, not written to the original file
  (non-destructive).
- When an image is viewed, the **edited** version is the default view.
- Option to view the **original** (before edits).
- Option to **revert** the edits (discard, restoring the original as the view).

### Open questions / to decide
- Edit storage format: per-photo edit record / edit stack (ordered list of
  operations) vs. a single derived-state blob; where it lives in library.db.
- How edits interact with thumbnails (regenerate edited thumbnails? cache
  both?) and with the thumb cache keyed by original hash.
- How edits interact with Immich upload/sync (upload original, edited, or
  both; do edits sync?).
- Whether an "export edited copy" (bake edits into a new file) is offered.

## RAW + JPEG pairing

Many libraries contain a RAW file alongside a JPEG of the same shot. These
should be treated as one photo, not two.

### Behaviour
- When a RAW and a JPEG represent the same image, pair them so the photo
  appears **only once** in the thumbnail grid.
- The JPEG is the visible/front image; the RAW sits "behind" it (associated,
  but not shown separately).

### Open questions / to decide
- Pairing rule: match by basename (e.g. `IMG_1234.jpg` + `IMG_1234.cr2`) in the
  same folder; how to handle multiple RAW extensions and case.
- Which RAW formats to recognize (cr2, cr3, nef, arw, dng, raf, orf, rw2, ...).
- What happens if only a RAW exists (no JPEG sidecar) — is it shown directly,
  and how is a thumbnail generated (RAW decode/embedded preview)?
- How the pairing is stored in library.db (a link/reference on the photo row).
- Interaction with edits (edits apply to the JPEG view) and with Immich
  upload/sync (upload JPEG, RAW, or both).
- Behaviour when the pair is broken (one file deleted or moved).

## Fast two-phase import

For large imports, get the file/folder structure into the app immediately, then
enrich image metadata as a background second step. (Today the scanner does the
expensive per-photo work inline — EXIF decode, dimension decode, and a full
SHA-256 hash of every file — so the structure does not appear until all of it
has been processed.)

### Phase 1 — structure only (fast)
- Walk the tree and insert folder rows + photo rows using only cheap `os.Stat`
  data (path, filename, folder_id, size, mod_time).
- Skip EXIF, dimension decode, and hashing entirely.
- Folder tree and thumbnail grid populate almost immediately, even for tens of
  thousands of files.
- Photos start with `hash=''`, `width/height=0`, `taken_at=0`.

### Phase 2 — enrichment (background, incremental)
- A background worker selects photos still needing metadata
  (`WHERE hash = '' OR width = 0`) and fills in, per photo: EXIF `taken_at`,
  `width`/`height`, `hash` (SHA-256), then thumbnails.
- Folder `year` is refined from the earliest `taken_at` once known (instead of
  being computed up-front as a blocker).
- Rows update in place; the UI refreshes as data lands.

### On-demand priority (open an unscanned album -> jump the queue)
- If, after Phase 1, the user opens an album/folder whose images are not yet
  enriched, those images are moved to the **top of the Phase 2 worklist** so
  their info and thumbnails are generated first.
- Priority should follow what the user is currently viewing; when they navigate
  away, the queue returns to normal order for the rest.

### Open questions / to decide
- Hash-keyed thumbnails: the thumb cache is keyed by `hash`, so thumbnails
  cannot generate until Phase 2 hashes a photo. Show a placeholder cell until
  enriched (simplest, Picasa-like), or generate against a temporary key and
  re-key once hashed.
- `scan_state` values / status to represent the phases (e.g. structured,
  enriching, done) and how resume-after-cancel works.
- Worklist model: how the priority queue is represented (in-memory vs. a DB
  column), and how "currently viewed" is signalled from the UI to the worker.
- Interaction with RAW+JPEG pairing (pair during Phase 1 or Phase 2?).

## Library freshness (keep the library in sync with disk)

Detect changes on disk in existing library folders as quickly as possible:
files added to or removed from known paths should be picked up and reflected in
the library without a manual full rescan.

### Detection
- Watch library folders for filesystem changes (add/remove/rename) and react
  promptly.
- Newly added files are scanned in (structure first, then enriched, per the
  two-phase import) and appear in the grid.
- Removed files are detected and their photo rows handled (see below).

### Library-view handling of added/removed files
- Work out how added and removed files are presented in the Library view.
- Added: new photos surface in their folder/album; consider a visual "new"
  indicator.
- Removed: decide whether the photo row is deleted immediately, marked
  "missing" (kept so tags/edits survive a temporary unmount or move), or
  offered for cleanup — and how missing items are shown in the grid.

### Open questions / to decide
- Watch mechanism: OS filesystem notifications (inotify on Linux) vs. periodic
  rescan/polling, and how to handle very large trees and watch-limit ceilings.
- Move vs. delete+add: detecting a moved/renamed file so tags/edits follow it
  rather than being lost.
- Debouncing bursts of changes (e.g. a bulk copy in progress) before scanning.
- Whether removed files are hard-deleted, soft-marked "missing", or trashed,
  and any user confirmation for cleanup.
- Reconciliation on startup / when a folder reappears after being offline.
- Interaction with hash-keyed thumbnails and with RAW+JPEG pairing when one
  side of a pair changes.
