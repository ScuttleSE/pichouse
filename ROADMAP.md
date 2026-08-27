# pichouse — Feature Roadmap

Running list of features to add down the road. This is a capture document for
ideas as they come up, not a committed plan or schedule. Items here are
unordered by priority unless noted.

**Completed** (see `HANDOFF.md` §10 for details): *Fast two-phase import* and
*Library freshness*.

## Picasa-style sidebar tree

Restyle the **Library** tab's tree to match Picasa 3: split it into distinct,
collapsible **sections** with headers, each showing a count, with per-item
counts and tiny thumbnail icons.

Note: the left panel keeps its existing top-level **tabs** — **Library** and
**Folders** (the stack switcher in `layout.go`). This change is *inside* the
Library tab, not a merge of the two tabs. The raw filesystem view stays in the
separate **Folders** tab.

### Sections (headers) inside the Library tab
- Split the Library tree into headed, collapsible sections, e.g.:
  - **Albums** — pichouse albums (folder-backed and, later, virtual).
  - **Faces / People** — named people from facial recognition (ties into
    Facial detection & recognition).
  - **Immich** — albums from a connected Immich server (ties into Immich
    integration).
- Each section header looks like a Picasa section header and shows a **count**
  of items in that section, e.g. `Albums (4)`, `People (1)`.
- Each section is **collapsible** (expand/collapse via the header triangle).

### Per-item display
- Each album/person row shows the **number of images** it contains, in
  parentheses after the name, e.g. `Recently Updated (250)`.
- Each album row shows a **tiny thumbnail icon** — a small thumbnail of the
  **first image** in that album — in place of a generic folder icon.

### Open questions / to decide
- Which image is "first" for the album thumbnail (sort order: filename, date
  taken, manual) and how it updates when the album changes.
- Thumbnail icon size and where the small icon comes from (reuse the thumb
  cache at a smaller size vs. a dedicated tiny thumb).
- How the image count is computed and kept fresh (live query vs. cached count on
  the album row).
- How the headed sections map onto the current sidebar tree model (the tree
  currently has a single root list; sections need header rows or grouping).
- Whether the same headed-section styling and counts also apply in the
  **Folders** tab (Picasa's Folders section shows per-folder counts).
- Section order, default collapsed/expanded state, and persistence of that
  state across restarts.

## Immich integration

Interface with an [Immich](https://immich.app/) server so the library can work
alongside a self-hosted Immich instance.

**Status: Phase 1 (browse), Phase 2 (full image viewer), Phase 3 (upload), and
Phase 4a (linked folders + auto-upload) implemented.**
pichouse connects to one or more Immich servers (Settings → Immich; API-key
auth; servers stored in the `immich_servers` table). Each server appears as a
section in the Library sidebar, expanding to its albums with asset counts.
Opening an album lists its assets with `POST /search/metadata` (`albumIds`
filter, paged; the page size is configurable, default 100) and shows them in the
grid. Thumbnails download from the server and are cached on disk in a per-server
SQLite file (`immich-thumbs-<server_id>.db`, keyed by asset id); a dedicated
worker pool serves them disk-first, then over HTTP. Deleting a server removes its
thumbnail file; a separate "Clear Immich Thumbnail Cache" button clears all of
them. Double-clicking an Immich thumbnail opens the full image viewer, which
downloads the asset "preview" over HTTP. Thumbnails and previews may be WebP;
both the grid and the viewer decode WebP through the `image` crate when GTK's
pixbuf loader cannot. See `src/immich/` (blocking HTTP client),
`src/db/immich_thumbs.rs` (thumbnail cache), and `src/ui/immich.rs` (background
fetch + channel to the GTK main thread). Sync of tags (Phase 4b) is not yet
implemented; linked folders with auto-upload (Phase 4a) are.

### Phase 1 — Browse (done)
- When connected to an Immich server, its albums appear as a **separate
  section** in the Libraries tab (distinct from local library folders/albums).
- Browse Immich albums from within pichouse.
- View the contents of an Immich album (photos/assets) inside the app.
- Right-click the Immich header or a server row → "Refresh Albums" re-fetches
  the album list (picks up albums added or deleted on the server directly).

### Phase 2 — Full image viewer (done)
- Double-click an Immich thumbnail to open the full image viewer.
- The viewer downloads the asset "preview" over HTTP and decodes it (WebP
  supported). Navigation (prev/next) and view-only rotation work; rotation is
  not written back to the server.

### Phase 3 — Upload (done)
- Right-click a scanned **folder** or a **folder-backed album** → "Upload to
  Immich…" opens a dialog to pick a server and either **create a new album**
  (default named after the source) or **add to an existing album**. A folder
  uploads its own photos; an album uploads the union of its direct member
  folders' photos.
- The "Upload to Immich…" item appears only when a server exists and the node
  has photos (based on the cached scan counts), so empty folders and empty
  albums do not offer it.
- Uploads run in the background with progress in the status bar; the
  `immich_upload` controller cancels a run.
- Deduplication uses Immich's own checksum check: the upload endpoint reports
  each asset as `created` or `duplicate`, and duplicates are still added to the
  target album, so re-uploading is safe.
- Virtual albums are not yet uploadable.

### Phase 4a — Sync: linked folders, two-way (done)
- **No global sync by default** — only folders the user explicitly links are
  synced.
- Right-click a scanned folder → "Sync with Immich album…" links it to a
  chosen server and either a **new album** (created on the server) or an
  **existing album** (stored in `immich_folder_links`). A synced folder is
  marked with `⇅` in the tree.
- **Two-way.** On linking, and on a periodic timer and startup:
  - **Up:** new local photos in a linked folder upload to its album (from the
    freshness reconcile or the inotify watcher, via `immich::autoupload_added`).
  - **Down:** assets in the album that are not yet local download into the
    folder (`immich::sync_folder_down` / `sync_all_down`), then a reconcile
    turns them into local photos.
- Matching is by original filename: it stops re-download loops (a downloaded
  file is present next cycle) and re-upload (the forward path finds a
  server-side duplicate).
- "Sync Now" forces a two-way sync of a folder; "Unsync from Immich" removes the
  link (does not touch the server album).
- Sync can also be started from the Immich side: right-click an Immich album →
  "Sync to local folder…" downloads it into a new subfolder of a chosen library
  root and links that folder for two-way sync.
- The Immich album list auto-refreshes every 5 minutes (plus the manual
  "Refresh Albums" action).

### Phase 4b — Two-way tag sync (out of scope)
Not planned for now. Deliberately skipped. If revisited later, the open points
are: conflict resolution when a tag changes on both sides; how pichouse tag
sources (AI vs. user) map to Immich tags; and how per-asset identity is tracked
(local `photos.hash` ↔ Immich asset id) so tags land on the right asset.

### Phase 5 — Immich photos in virtual albums (future)
Let Immich assets be members of pichouse virtual albums. Today they cannot:
virtual-album membership stores `photos.id`, and Immich photos are synthetic
grid entries with `id = 0` and no row in the `photos` table. Adding one raises a
`FOREIGN KEY constraint failed` error, so the grid excludes Immich photos from
"add to virtual album" and from drag-and-drop onto a virtual album.

To support this, decide one of:
- Import the Immich asset as a local `photos` row (an `immich://` path, a
  nullable or sentinel `folder_id`, no local file on disk), so existing
  membership and rules work unchanged; or
- Add a membership table that can reference a remote asset (server id + asset
  uuid) alongside local `photos.id`, and teach the grid loader and rule
  evaluator to mix both.

Open points: how such a photo shows in the grid and viewer (already handled by
the `immich://` path), how it interacts with tags and rules, and what happens
when the remote asset or server is removed.

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

**Status: implemented.** Basic image editing, Picasa-style. Edits never modify
the original file on disk; they are stored in `library.db` (`photo_edits`, one
row per photo) and applied at view time and when generating thumbnails. The
shared pipeline is `src/edit.rs`; the edit panel is `src/ui/editor.rs`, opened
from the viewer's Edit button.

### Edits (implemented)
- Flip horizontal / vertical.
- Straighten (arbitrary small angle, with auto-crop of the empty corners).
- Crop (numeric per-mille rectangle; an interactive drag overlay is a follow-up).
- Brightness / contrast.
- Per-channel color levels (see "Color levels" below).
- 90-degree rotation stays on `photos.orientation` (pre-existing) and is applied
  before these edits.

### Behaviour (implemented)
- Edits are stored in the database, not written to the original file.
- The **edited** version is the default view.
- "View original" toggle shows the untouched image.
- "Revert all" discards the edits (removes the `photo_edits` row).
- "Export copy…" bakes the edits into a new JPEG/PNG on disk.

### Notes
- Thumbnails: the thumbnail cache key gains the edit revision (`<hash>|<rev>`);
  an identity edit reuses the plain `hash`, so pre-edit caches stay valid.
  Editing calls `Generator::invalidate` to drop stale edited thumbnails.
- Immich: edits apply to local files only; Immich previews are shown unedited
  and are not exported.

## Color levels

**Status: implemented.** Per-channel (R/G/B) black/white/gamma levels, aimed at
images scanned from negatives that have skewed color casts.

### Behaviour (implemented)
- Per-channel input black point, white point, and gamma.
- **Auto levels**: derive per-channel black/white points from the image
  histogram (0.5% tail clip) to remove a color cast in one click.
- **Presets**: save the current levels as a named preset (`level_presets`),
  choose a preset to apply, and delete presets. Presets store levels only.
- **Apply to folder**: merge a preset's levels into every photo in the current
  folder, changing only the levels part of each photo's edit and preserving
  crop/rotate/flip/brightness (`Library::apply_levels_to_folder`).

### Open questions / to decide
- A live histogram display in the levels panel (nice-to-have).
- Interactive crop overlay (drag rectangle) instead of numeric per-mille.
- Whether edits sync to Immich (upload original, edited, or both).

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

## Virtual albums

**Status: implemented (manual + rule-based).** Virtual albums group individual
photos across folders. Storage: `virtual_albums` (nestable, with an AND/OR
`rule_match`), `virtual_album_photos` (manual pins and exclusions), and
`virtual_album_rules` (structured tag/date/filename/folder conditions).
Membership is evaluated live at view time: rule matches combined per the match
mode, unioned with pins, minus exclusions (see `src/db/virtual_albums.rs`). The
sidebar shows a "Virtual Albums" section above normal albums; the grid supports
multi-selection with a right-click menu to add/remove photos and create albums
from a selection; a rules editor dialog (`src/ui/vrules.rs`) edits smart rules.

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

## Facial detection & recognition

Detect faces in photos, group the same person's face across the library, let the
user name a person, and surface all their photos — Picasa "People"-style. Fits
alongside the existing local AI tagging pipeline; all processing stays local.

### Behaviour
- Detect faces in library images (a per-photo face-detection pass).
- **Group** faces that belong to the same person automatically (face clustering
  by similarity).
- Let the user **name** a face/cluster (assign a person name), and confirm or
  correct grouping (merge/split clusters, reassign a face).
- Once named, a person's photos are available as a **virtual album** containing
  every image that person appears in (ties into Virtual albums).
- Membership updates as new matching faces are found in newly scanned photos.

### Open questions / to decide
- Backend: reuse the existing Ollama/AI stack vs. a dedicated local face
  pipeline (detector + embedding model, e.g. a face-embedding network + a
  clustering step). All local, consistent with offline AI tagging.
- Storage: how faces, bounding boxes, embeddings, clusters, and person names are
  persisted in library.db (e.g. `faces`, `persons` tables; embedding blobs).
- Clustering method and threshold, and how re-clustering works as the library
  grows (incremental assignment vs. periodic re-cluster).
- Person -> virtual album mapping: a rule-based virtual album ("contains person
  X") vs. a dedicated People section with its own UI.
- UI: a People view (named/unnamed faces), naming flow, and confirm/merge/split
  controls; showing face thumbnails cropped from the source image.
- Interaction with edits (detect on original), RAW+JPEG pairing (detect once per
  paired photo), and adult content (faces in NSFW images).
- Interaction with Immich (Immich has its own people/face feature — map to it or
  keep separate).
- Privacy: all face data stays local; how to delete/reset a person or all face
  data.

## Duplicate image finder

Find duplicate (and near-duplicate) images within the library and help the user
clean them up by auto-selecting the "worse" copy for potential deletion.

### Scope (where to search)
- Run the finder at different scope levels:
  - the **current album** only;
  - a **selected set of albums** (multi-select);
  - an **album and all its sub-albums** (recursive).

### Similarity
- Adjustable **similarity level** — from exact/near-exact duplicates to looser
  visual matches (e.g. same shot, different size/compression/crop).
- Detection should catch not just byte-identical files but visually similar
  images (resized, re-compressed, minor edits).

### Auto-selection for deletion
- When duplicates are found, **auto-select the "worse" copy** of each group as
  the candidate for deletion, leaving the "better" one kept.
- "Worse" is decided by quality/size heuristics, e.g.:
  - smaller pixel dimensions / lower resolution;
  - lossy vs. lossless format (prefer the lossless/original);
  - smaller file size / higher compression;
  - (potentially) lower bit depth, stripped metadata, etc.
- The user can review and adjust the selection before anything is deleted.

### Open questions / to decide
- Similarity method: exact hash (SHA-256, already stored) for identical files vs.
  a perceptual hash (pHash/dHash/aHash) for near-duplicates; whether to store the
  perceptual hash in library.db for fast repeat runs.
- How the adjustable similarity level maps to a threshold (e.g. Hamming distance
  on a perceptual hash) and its default.
- "Worse" ranking rules: exact ordering of the heuristics and how ties break;
  whether the rules are user-configurable.
- Interaction with RAW+JPEG pairing (a RAW/JPEG pair is not a duplicate) and with
  non-destructive edits (compare originals, not edited views).
- Cross-album duplicates: how a group spanning multiple albums is presented, and
  whether deleting removes the file or just an album membership/virtual entry.
- Deletion semantics: hard delete vs. trash vs. "missing" mark (see Library
  freshness), and required user confirmation.
- Performance: comparing large libraries efficiently (bucketing by hash/size
  first, then perceptual compare within buckets).
- UI: how duplicate groups are shown (side-by-side, grouped grid) and the
  review/confirm flow before deletion.

## Geolocation & maps

Use photo GPS EXIF data to show where photos were taken — per-photo on a map,
and a global map of the whole library.

### Per-photo map (properties panel)
- When a photo has geodata (GPS EXIF), add a **Map** tab in the right-hand
  properties panel showing that photo's location.
- The map is either **Google Maps** or **OpenStreetMap** (decide backend below).
- No Map tab (or a disabled/empty state) when the photo has no geodata.

### Global map view
- A global **map view** of the whole library, plotting every geotagged photo.
- **Cluster** markers where many photos were taken in the same area; zooming in
  expands clusters into finer clusters / individual photos.
- Clicking a marker/cluster shows the photos taken there (open in the grid or a
  popover).

### Open questions / to decide
- Map backend: OpenStreetMap (open, no API key, e.g. via a tile source /
  libshumate) vs. Google Maps (API key, terms); offline vs. online tiles.
- How the map is embedded in a GTK4 app (a native map widget like libshumate vs.
  a WebKitGTK web view); dependency and packaging impact on Debian 13.
- Where GPS is read/stored: extend the scanner/EXIF step to persist lat/lon on
  the photo row in library.db (currently EXIF gives taken_at, dimensions only).
- Clustering method and thresholds for the global view; server-side vs.
  client-side clustering for large libraries.
- How the global map view is launched (a top-level view/tab vs. a menu action)
  and how it interacts with the current album/selection (map the whole library
  vs. the current album/selection).
- Privacy: some photos have sensitive locations; whether to allow hiding/
  stripping geodata, and interaction with adult content and Immich sync.

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
