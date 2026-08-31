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
- Each server node also has a **Timeline** child that shows every asset on the
  server (newest first), fetched with `POST /search/metadata` and no album
  filter (`Client::timeline_assets`, `immich::show_timeline`).
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
shared pipeline is `src/edit.rs`; the edit UI is the "Edit" tab of the right-hand
properties panel (`src/ui/editor.rs`), revealed by the viewer's Edit button or
by right-click → Edit in the grid (which opens the photo, then the tab). Baked
export lives in `src/ui/export.rs`.

### Edits (implemented)
- Flip horizontal / vertical.
- Straighten (arbitrary small angle, with auto-crop of the empty corners).
- Crop (numeric per-mille rectangle, plus an interactive drag overlay — see
  "Interactive crop" below).
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
- Export: "Export copy…" (edit tab) or right-click → "Export edited copy…" bakes
  edits at full resolution. A dialog picks format (JPEG/PNG) and JPEG quality,
  remembered in settings. One photo opens a Save dialog; several open a folder
  chooser and each is written as `<stem>-edited.<ext>`.
- Immich: editing works on Immich photos too. The full-resolution asset is
  downloaded (`asset_original`) for the histogram, auto-levels, and export;
  view-time edits apply to the preview.

## Color levels

**Status: implemented.** Per-channel (R/G/B) black/white/gamma levels, aimed at
images scanned from negatives that have skewed color casts.

### Behaviour (implemented)
- Per-channel input black point, white point, and gamma.
- **Auto levels**: derive per-channel black/white points from the image
  histogram (0.5% tail clip) to remove a color cast in one click.
- **Live histogram**: each channel shows a log-scaled histogram of the original
  with draggable black, white, and gamma markers; dragging updates the view
  live. Numeric spin buttons stay in sync.
- **Presets**: save the current levels as a named preset (`level_presets`),
  choose a preset to apply, and delete presets. Presets store levels only.
- **Apply to folder**: merge a preset's levels into every photo in the current
  folder, changing only the levels part of each photo's edit and preserving
  crop/rotate/flip/brightness (`Library::apply_levels_to_folder`).

### Open questions / to decide
- Whether edits should sync back to Immich (currently local-only; export bakes
  a new file the user can re-upload).

### Interactive crop (implemented)
The editor has a "Crop by dragging on the image" toggle. When on, the viewer
shows the image uncropped and overlays a draggable rectangle (a `DrawingArea`
stacked on the viewer `Picture`). Dragging selects the crop; releasing writes
the per-mille rectangle back into the numeric spin buttons and commits. The
overlay maps pointer coordinates to image coordinates accounting for
`ContentFit::Contain` letterboxing. See `src/ui/viewer.rs` (`set_crop_mode`,
`image_rect`, `update_crop_from_drag`) and `src/ui/editor.rs` (`build_crop`).

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

**Status: implemented.** Play the current grid view (or the current
multi-selection) as a full-screen slideshow. A "Play slideshow" button in the
toolbar starts it; right-click the button for options (per-image duration,
shuffle, loop), which persist in `library.db` settings
(`slideshow.secs`/`shuffle`/`loop`). The slideshow runs in the viewer
(`src/ui/viewer.rs`): it enters fullscreen, hides the control bar, and advances
on a `glib::timeout_add_seconds_local` timer. Keys: Space pauses/resumes,
Escape (or the Close binding) stops and leaves fullscreen; the arrow keys still
navigate. Non-destructive edits show in the slideshow (the viewer renders the
edited view). Any set the grid can show plays: normal albums, virtual albums,
folder view, Immich albums/timeline, or the current selection.

### Behaviour (implemented)
- Slideshow of an album's images (or any current photo set).
- Adjustable per-image duration.
- Shuffle mode (random order via an in-process Fisher–Yates, no extra dep).
- Repeat/loop mode; with loop off the show stops on the last image.
- Playback controls: pause/resume (Space), next/previous (arrows), exit (Esc).

### Open questions / to decide
- Transitions between images (none/crossfade) and whether that is configurable.
 - Optional Ken Burns / pan-zoom effect (nice-to-have).

## Move to llama.cpp instead of Ollama

Replace the Ollama backend for local AI tagging with a direct llama.cpp
integration. The application uses Ollama today (see `src/ai/`, an HTTP client to
a local Ollama server). llama.cpp is the engine under Ollama. A direct
integration removes the Ollama layer.

### Reasons
- Fewer dependencies. The user does not install and run a separate Ollama server.
- More control. The application selects the model file, the context size, the
  GPU offload, and the thread count directly.
- Better packaging. The application can ship or point to one model file.

### Options to decide
- **Bundled server:** the application starts a `llama-server` process and talks
  to it over HTTP (the same request pattern as the current Ollama client). This
  is the smallest change to `src/ai/`.
- **In-process bindings:** link llama.cpp through a Rust crate (for example
  `llama-cpp-2` or `llama_cpp`). This removes the HTTP layer but adds a native
  build dependency and complicates the Debian 13 / CI build.

### Open questions
- Which vision models work with llama.cpp for tagging (the model plus its
  `mmproj` projector file), and where the user gets them.
- How the application finds or downloads the model file (no automatic download
  today).
- GPU support on the target machines, and the CPU fallback.
- The build and packaging effect on Debian 13 and the CI runner if the
  in-process option is chosen (a C++ toolchain and CUDA/Vulkan libraries).
- Migration of the existing AI settings (`src/ui/settings_ai.rs`, the `ai.*`
  keys in `library.db`) to the new backend's settings.
- Keep all processing local, as the current design requires.

## Facial detection & recognition

Detect faces in photos, group the same person's face across the library, let the
user name a person, and surface all their photos — Picasa "People"-style. Fits
alongside the existing local AI tagging pipeline; all processing stays local.

There are TWO parallel systems: a **human face** system for photographs and a
**stylised face** system for anime, cartoon, and furry art. They share only the
ONNX Runtime loader. Each has its own models, database tables, clustering, and
sidebar section. An album's **Face type** (Photo or Art) decides which system
scans that album's photos (see "Album Face type" below).

**Status: both systems implemented.** Off by default. The models and the ONNX
Runtime download into the data folder on first use.

### Human faces (implemented)

The pipeline is YuNet (detection, MIT) plus SFace (128-D embedding, Apache 2.0),
both from the OpenCV Zoo, run in-process through `ort` (ONNX Runtime, loaded at
run time). Faces, people, and per-photo scan state live in `library.db`
(`persons`, `faces`, `face_scan`, `face_rejections`); face-crop thumbnails live
in `face-thumbs.db`. A background worker session (`src/ui/facescan.rs`) detects
and embeds faces, then clusters embeddings by cosine similarity (greedy
nearest-centroid, SFace threshold 0.363); named people anchor stable clusters so
new matches attach automatically. The sidebar has a **People** section
(`person:<id>`); selecting a person shows every photo they appear in
(`Source::Person`). A **Faces** view (`src/ui/facesview.rs`) and dialogs
(`src/ui/people.rs`) turn unnamed clusters into named people and merge clusters.
A `RuleField::Person` lets a smart virtual album hold "contains person X". The
viewer has a **Show faces** overlay that draws each face box and assigns a face
on click. Settings (`src/ui/settings_faces.rs`) hold the enable toggle, an opt-in
auto-scan, the embedding-model choice, the model download, the scan action, and
a "Delete all face data" reset. See `src/face/` (config, runtime, models,
detector, embedder, cluster).

### Stylised faces — anime / cartoon / furry (implemented)

A parallel system for drawn faces. It uses an anime YOLOv8-nano detector
(deepghs, MIT, 12 MB) and DINOv2 ViT-S/14 (Apache 2.0, 384-D) — SFace gives bad
vectors on drawings, so a different embedder is required. It clusters with the
`hdbscan` crate (euclidean on L2-normalised vectors, `min_cluster_size = 2`,
leaf-like small groups); HDBSCAN marks unclear faces as noise (cluster `-1`),
shown in the UI as "Unclear". A named group is a **character**. Data lives in
`characters`, `style_faces`, `style_face_scan`, `style_face_rejections`, and
`style-face-thumbs.db`. The sidebar has a **Characters** section
(`character:<id>`); the center **Characters** view
(`src/ui/charactersview.rs`) names and merges clusters. Settings
(`src/ui/settings_characters.rs`) offer two detectors and two embedders. See
`src/styleface/` (config, models, detector, embedder, cluster) and
`src/ui/stylefacescan.rs`.

Design note: for art, the art style often matters more than the character, and
hair colour dominates. Two drawings of one character can land far apart, and two
characters by one artist can land close. The merge function is the fix: HDBSCAN
is tuned to make many small groups, and the user merges them in two clicks.

### Album Face type — routing (implemented)

An album has a **Face type**: Inherit, Photo, or Art. It is three-state and
inherits down the tree (a top-level Inherit resolves to Photo). Set it once at
the top; sub-albums and folders inherit. Right-click an album → a **Face type**
submenu sets the kind, and **Scan / Rescan faces in album** scans the album's
photos with the routed method (Photo → human system, Art → stylised system). The
opt-in auto-scan also routes by album kind, so each photo is scanned by exactly
one method (no double-scan). A photo in no album defaults to Photo. Album rows
show a `(Photo)`/`(Art)` badge when the kind is set explicitly. See
`src/model.rs` (`AlbumKind`), `src/db/albums.rs` (`album_effective_kind`,
`folders_under_album`, `photo_effective_face_kind`), and `src/ui/albumscan.rs`.

### Deferred follow-ups

- Higher-accuracy optional human models (ArcFace 512-D, non-commercial) and a
  custom `.onnx` path.
- A split control that pulls a mis-grouped face out of a person and re-clusters
  it (today a human face is reassigned by clicking it in the viewer). No per-face
  reject/reassign UI exists for stylised faces yet (the backend supports it).
- Stylised follow-ups from the source roadmap: nearest-part incremental assign
  for new images, "is this also X?" merge suggestions, and a learned metric
  trained from user merges.
- A cache-only clear for face crops (a slider or model change refreshes crops
  without a full "Delete all face data").
- An automatic art-vs-photo classifier. Today the album Face type is the routing
  signal; there is no per-image auto-detection.
- The stylised fp16 embedder catalog entry needs f16 tensor handling before use.
- Interaction with Immich's own people feature (kept separate for now).

## Duplicate image finder

Find duplicate (and near-duplicate) images within the library and help the user
clean them up by auto-selecting the "worse" copy for potential deletion.

### Status: implemented (v1)

The first version is in place. Open it from the toolbar "Tools" menu →
"Find Duplicates…".

- Scope: the current folder, selected albums (recursive, with sub-albums), or
  the entire library.
- Similarity: a slider sets the maximum Hamming distance (0..16) on a 64-bit
  dHash. `0` finds exact and visually identical copies. Byte-identical files
  always match by their stored SHA-256 hash.
- The dHash is computed during Phase 2 enrichment and stored in
  `photos.phash`. Existing photos are backfilled on the first scan.
- Groups auto-select the "worse" copy for deletion. "Better" means, in order:
  larger pixel area, more lossless format, larger file size, older added date.
- Results show one group per row. A framed box surrounds each group's photos.
  The auto-selected "worse" copy shows a red X. Click any photo in a group to
  move the X to it. Click the X to unmark the group. A "Delete marked" button in
  the results bar runs a single confirm, then hard deletes the marked files (and
  their rows, cascading to tags/edits/faces/album membership).

### Deferred / not yet done

- RAW+JPEG pairing: the app does not scan RAW files yet, so this concern does
  not apply. Add RAW support first.
- A richer per-group review UI (side-by-side view). The current review is the
  grouped grid with the red-X marking and the "Delete marked" bar.

### Original design notes

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
