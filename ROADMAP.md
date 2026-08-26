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

## Virtual albums

Albums whose contents are hand-picked **individual photos** drawn from any
number of different (normal, folder-backed) albums — not tied to a single
folder on disk.

Note: the current schema's `albums`/`album_folders` groups whole *folders*.
Virtual albums are a distinct concept that groups individual *photos*.

### Behaviour
- A virtual album can contain photos added from different albums/folders.
- Virtual albums appear **alongside** normal albums in the Library view, but
  with a slightly different icon to distinguish them.
- Purely virtual/organisational: they do not move or copy files on disk.

### Rule-based (smart) virtual albums
- A virtual album can have **rules** that automatically add matching photos,
  e.g. "all pictures with the tag 'vacation' and date between 2010-05-01 and
  2010-08-01".
- Rules match on photo attributes: tags, date/date-range, and potentially
  folder, filename, camera/EXIF, etc.
- Membership updates as the library changes (new matching photos are added
  automatically).
- Support combining conditions (AND/OR) and multiple rules per album.
- A virtual album may mix rule-matched photos with manually added ones (TBD),
  or be purely rule-based vs. purely manual.

### Open questions / to decide
- Storage: a new membership table (e.g. `virtual_album_photos(album_id,
  photo_id, position)`) vs. extending the existing albums model.
- Rule storage and evaluation: how rules are persisted (structured conditions
  vs. a query expression) and evaluated (live query at view time vs. a
  materialized membership refreshed on change).
- How manual additions/removals coexist with rules (pins/exclusions).
- Whether a photo can belong to multiple virtual albums (expected: yes).
- Ordering within a virtual album (manual position vs. sort).
- What happens to a virtual-album entry when the underlying photo is removed or
  goes "missing" (see Library freshness).
- Interaction with edits (which view is shown) and with Immich upload/sync
  (can a virtual album be uploaded/synced as an Immich album?).
- UI for adding photos to a virtual album (drag-drop, context menu).

## Slideshows

Play an album (or any photo set) as a full-screen slideshow.

### Behaviour
- Slideshow of an album's images.
- Adjustable per-image duration (how long each image is shown).
- Shuffle mode (random order).
- Repeat/loop mode.
- Standard playback controls: play/pause, next/previous, exit.

### Open questions / to decide
- Transitions between images (none/crossfade) and whether that is configurable.
- Whether edits (non-destructive) are shown in the slideshow (expected: yes).
- Which sets can be played (normal albums, virtual albums, folder view,
  current selection).
- Fit/scale handling for mixed aspect ratios and portrait/landscape.
- Optional Ken Burns / pan-zoom effect (nice-to-have).
- Behaviour on the last image when repeat is off (stop vs. exit).

## Adult / mature content tagging

Some libraries contain adult (NSFW) images. Support tagging and organising this
content properly, using a local AI model suited to the task. All processing
stays local (in keeping with the existing Ollama-based, offline AI tagging).

### Behaviour
- Find/select a local AI model that tags adult images accurately, including
  explicit/detailed tags where appropriate.
- Integrate it into the existing AI tagging pipeline (per-photo tags, AI vs.
  user source, confirm/reject flow).
- Detect and flag adult content so it can be filtered/hidden in the UI (safe
  mode / blur / hidden-by-default albums).

### Open questions / to decide
- Which local model(s): general vision model with an explicit prompt vs. a
  dedicated NSFW tagger/classifier; where it is hosted (Ollama or a separate
  backend).
- Tag vocabulary: free-form explicit tags vs. a controlled set; how these
  interact with the normal tag namespace.
- A dedicated "adult" flag on photos/albums vs. relying on tags alone.
- UI controls: a global safe/reveal toggle, per-album marking, blurred
  thumbnails, and whether adult content is excluded from slideshows/exports by
  default.
- Interaction with Immich sync (does adult tagging/flagging propagate?).
- Access control: optional gating (PIN/hidden) for adult albums.
