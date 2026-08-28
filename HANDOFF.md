# HANDOFF

This document is for an agent with no memory of the last session. It uses
Simplified Technical English (ASD-STE100, Strict). Read AGENTS.md first. Read
ROADMAP.md for planned features. Read section 0000000 first — it describes the
most recent work (the duplicate image finder). Then read section 000000 — it
describes stylised face recognition for art, and the album Face type. Then read
section 00000 — it describes the human facial detection and recognition system
that the stylised system mirrors. Then read section 0000 — it describes four
small features, console logging, and scan-time performance and freeze fixes.
Then read section 000 — it describes non-destructive editing and color levels.
Then read section 00 — it describes four earlier follow-up features. Then read
section 0 — it describes the Immich integration. The later sections describe
earlier features and are still correct.

## 0000000. Duplicate image finder (most recent work — read this first)

This section describes the last session. The work is complete. The application
builds clean. `cargo test` passes 66 tests plus 2 ignored tests. The work is on
`main`. The work is pushed. Every CI build in the session was green. The version
was 0.0.76 at the end of the session. CI increases the version on each push. Do
not change the version by hand.

The session added a duplicate image finder. The user starts it from the toolbar
"Tools" menu. A dialog sets the scope and the similarity level. A background scan
groups the duplicate photos. The result view shows one group per row. The user
marks the copy to delete, then deletes the marked files.

### 0000000.1 What the finder does

The finder finds two kinds of duplicate:

1. Exact duplicates. Two files with the same SHA-256 content hash. The hash is
   already stored in `photos.hash`.
2. Near duplicates. Two files that look the same but differ in size,
   compression, or a small edit. The finder compares a 64-bit perceptual hash
   (a dHash). It matches two photos when the Hamming distance of their dHashes is
   at most a threshold.

### 0000000.2 The perceptual hash (dHash)

`src/phash.rs` holds the dHash. `dhash_rgb` reduces an RGB image to a 9x8
grayscale grid and sets one bit per adjacent-column brightness compare. This
gives 64 bits. `dhash_file` decodes a file first with `thumb::decode_oriented_rgb`.
`hamming` counts the differing bits.

The dHash is stored in a new column `photos.phash`. The type is INTEGER. SQLite
has no unsigned type, so the code bit-casts the u64 to i64 on write and back on
read (`phash as i64`, `get::<_, i64>() as u64`). A value of 0 means "not yet
computed".

The scanner computes the dHash during Phase 2 enrichment. `enrich_file_with_image`
in `src/scan.rs` decodes the file one time, then builds the dHash from the same
pixels. The `Enrichment` struct carries the `phash`. `Library::enrich_photo`
writes it. Existing photos have `phash = 0`. The finder backfills them on the
first scan (it decodes each such file one time and stores the result).

### 0000000.3 The grouping engine

`src/dedup.rs` holds the engine. `find_duplicates(photos, threshold, cancel)`
returns a list of `DupGroup`. A `DupGroup` holds the group photos and the id of
the photo to keep (`keep_id`). The engine does two passes over a union-find:

1. Exact pass. It unions photos that share a non-empty `hash`.
2. Near pass. It unions photos whose dHashes are within `threshold`. This pass is
   O(n^2). The scope is one album or one folder set, not the whole library, so
   this is acceptable. A threshold of 0 skips the near pass. Photos with
   `phash == 0` never match in the near pass.

The engine keeps only groups with more than one member. `choose_keep` picks the
best copy: larger pixel area first, then more lossless format
(`format_rank`: png/tiff/bmp > webp > jpg), then larger file size, then older
`added_at`, then lower id. The comparison is total, so ties break the same way
every run. Unit tests cover the exact pass, the near pass, and the ranking.

### 0000000.4 The database layer

`src/db/duplicates.rs` holds three methods on `Library`:

- `photos_in_folders(folder_ids)` reads the non-missing photos in a folder set.
- `set_photo_phash(id, phash)` writes a backfilled dHash.
- `delete_photo_hard(id, path)` deletes the row, then removes the file. It
  deletes the row first, so a failed file delete still leaves the library
  consistent. A `NotFound` file error is ignored, because a gone file is the
  goal. The row delete cascades to tags, edits, faces, and album membership
  through the `ON DELETE CASCADE` foreign keys.

Schema note. `photos.phash` is in `src/db/schema.sql`. The `migrate` function in
`src/db/library.rs` adds the column with `ALTER TABLE ... ADD COLUMN` for old
databases. A new index `idx_photos_hash` speeds the exact bucketing. The
`PHOTO_COLS` constant (three copies: `virtual_albums.rs`, `faces.rs`,
`style_faces.rs`) and the two inline SELECTs in `library.rs` gained `phash` at
the end. `map_photo` reads `phash` at column index 16.

### 0000000.5 The user flow and the UI

The toolbar "Tools" button opens a menu with "Find Duplicates…"
(`src/ui/toolbar.rs`). The item calls `actions::find_duplicates`
(`src/ui/actions.rs`). That function shows a dialog. The dialog has a similarity
slider (0 to 16, the maximum Hamming distance) and a scope choice: the current
folder, selected albums (recursive, through `folders_under_album`), or the whole
library. On "Find Duplicates" it resolves the folder ids and calls
`dedup_scan::find_duplicates`.

`src/ui/dedup_scan.rs` runs the scan off the GTK main thread. It copies the
`aitag.rs` pattern: a `Msg` enum, a `glib::MainContext::channel`, a worker
thread, and progress on the status bar. A new `Controller` field `dedup_job` on
`AppState` gives cancel support (the status-bar Stop button calls
`dedup_job.stop()`). The thread backfills missing dHashes, then runs the engine,
then sends the groups to the main thread. `present_results` builds the group
entries and calls `grid.show_duplicates`.

### 0000000.6 The result view (important design detail)

The result view is NOT the normal `GridView`. The user asked for a single box
around each whole group. A flowing `GridView` cannot draw one box around a set of
cells that wrap across rows. So the grid holds a second view for duplicate mode:

- `Grid` gained fields `scroller`, `dup_container`, `dup_state`, and the
  duplicate action bar (`dup_bar`, `dup_label`, `dup_delete_btn`).
- `show_duplicates` builds a vertical stack of `gtk4::Frame` boxes, one per
  group. Each frame holds a `FlowBox` of thumbnail overlays. The frame has the
  CSS class `dup-group-frame` (a bordered, tinted box). Then it swaps the
  scroller child from the `GridView` to `dup_container`.
- Each thumbnail is an `Overlay` of an `Image` and a red X `Label`
  (CSS class `dup-x`). A per-thumbnail `PhotoObject` drives the async thumbnail
  load through the shared worker pool (`enqueue_thumb`).
- Each thumbnail has its own `GestureClick`. This is the fix for a real bug: the
  first version marked cells through `connect_selection_changed`, which does not
  fire when the user clicks an already-selected cell. The per-thumbnail gesture
  fires on every click. `toggle_dup_mark(group, photo_id)` marks the clicked
  photo, moves the mark from another photo, or clears the mark when the marked
  photo is clicked.
- `dup_state` holds the live mark state (`Cell<bool>` per thumbnail) and the X
  widget, so a click updates the view with no rebuild.
- The action bar has a "Delete marked" button. It calls the `on_dup_delete`
  callback with the marked photos. `dedup_scan` shows one confirm dialog, then
  calls `delete_photo_hard` for each marked photo, then leaves duplicate mode.

`exit_dup_mode` restores the normal `GridView` child. `set_photos` (used by every
normal view loader) calls `exit_dup_mode` first, so opening any folder or album
leaves duplicate mode.

The CSS is in `install_css` in `src/ui/app.rs`. It is loaded one time for the
default display. It holds `dup-x` (the red badge) and `dup-group-frame` (the
group border).

### 0000000.7 What is deferred

- RAW+JPEG pairing is not done. The scanner does not scan RAW files
  (`IMAGE_EXTS` in `src/scan.rs` has no RAW extensions). So a RAW/JPEG pair
  cannot appear as a duplicate yet. Add RAW support first.
- A richer per-group review (side-by-side) is not done. The framed grid with the
  red-X marking is the review.
- The near pass is O(n^2). For a whole-library scan on a very large library, add
  a dHash prefilter bucket (for example, bucket by the high bits) before the
  pairwise compare.

### 0000000.8 Disk note (RULE FOUR)

The build cache filled the disk in this session. `target/debug/incremental`
grew to 13 GB. The session added RULE FOUR to AGENTS.md. Delete
`target/debug/incremental` after each test cycle. CI builds with `--release`, so
CI is not the cause. Check free space with `df -h /`.

## 000000. Stylised face recognition and album Face type

This section describes an earlier session. The work is complete. The application
builds clean. `cargo test` passes 60 tests plus 2 ignored tests. The work is on
`main`. The work is pushed. Every CI build in the session was green. The version
was 0.0.73 at the end of the session. CI increases the version on each push. Do
not change the version by hand.

The session added two features:

1. A second face system for stylised art (anime, cartoon, furry). It mirrors the
   human face system in `src/face/` but uses different models and a different
   clustering method. A named group is a "character", not a "person".
2. An album "Face type" (Photo or Art). The user marks an album, and the mark
   controls which face method scans that album's photos. A right-click menu
   scans or rescans an album.

See ROADMAP.md section "Facial detection & recognition" for the feature summary
of both systems.

### 000000.1 Why two systems, not one

SFace (the human embedder) is trained on photographs. It gives poor vectors on
drawings. DINOv2 (the stylised embedder) is not tuned for photograph identity.
The two embedders make vectors of different lengths (128 vs 384) in different
spaces, so they cannot share one cluster pool. The user asked about merging the
two. The decision, after analysis, was to keep them separate. The only shared
part is the ONNX Runtime loader (`src/face/runtime.rs`). The user considered
using HDBSCAN for photos too. The decision was no, because SFace has a
calibrated cosine threshold that HDBSCAN would discard, and HDBSCAN would put
valid photo faces into a noise bucket.

### 000000.2 Stylised models

The models are an anime YOLOv8-nano detector (deepghs, MIT, 12 MB) and DINOv2
ViT-S/14 (onnx-community export, Apache 2.0, 384-D, 88 MB fp32). Both are pinned
to fixed Hugging Face commits with verified SHA-256 in `src/styleface/models.rs`.
The catalog also offers a larger detector (YOLOv8-small, 44 MB) and a smaller
embedder (DINOv2 fp16, 44 MB) as alternatives. Warning: the fp16 embedder outputs
f16 tensors, but `embedder.rs` extracts f32. Do not select fp16 without adding
f16 handling. fp32 is the default and is safe. The models download into
`~/.local/share/pichouse/models/` on first use, next to the human models.

### 000000.3 Stylised inference

`src/styleface/detector.rs` runs YOLOv8: it letterboxes the oriented RGB image
into a 640x640 RGB NCHW tensor (values 0..1, gray padding), reads `output0`
`[1, 5, anchors]` (cx, cy, w, h, score), applies NMS, and returns per-mille
boxes. There are NO landmarks. `src/styleface/embedder.rs` runs DINOv2: it
enlarges the box 25 percent on each side, crops a square, resizes to 224x224,
normalises with the ImageNet mean and standard deviation, runs the model, takes
the CLS token (index 0 of `last_hidden_state`), and L2-normalises the 384-float
vector. `src/styleface/mod.rs` has `StyleFacePipeline` (`detect_and_embed`).

The coordinate rule is the same as the human system: a box is per-mille (0..1000)
of the photo AFTER `photos.orientation` rotation. Face crops reuse
`thumb::render_face_crop`, which adds its own 30 percent margin.

### 000000.4 Stylised clustering (HDBSCAN)

`src/styleface/cluster.rs` uses the `hdbscan` crate (version 0.12, MIT/Apache).
`Cargo.toml` gained this one dependency. The algorithm runs on L2-normalised
384-D vectors with the euclidean metric, `min_cluster_size = 2`, `min_samples =
1`. The design goal is many small groups, per the roadmap. HDBSCAN marks unclear
faces as noise; a noise face gets `cluster_id = -1` (constant `NOISE_CLUSTER_ID`).
The UI shows the noise group as "Unclear". A named character anchors a stable
cluster (`CHARACTER_CLUSTER_BASE + character_id`). Before HDBSCAN runs, an
unnamed face very near a named character joins it (constant
`CHARACTER_JOIN_MAX_DIST`), so characters pull in new matches. Rejections are
honoured, like the human system.

### 000000.5 Stylised storage

`src/db/schema.sql` gained `characters`, `style_faces`, `style_face_scan`, and
`style_face_rejections`. They mirror `persons`/`faces`/`face_scan`/
`face_rejections`. `style_faces` has NO landmarks column (the stylised embedder
uses the box only). `photos.style_face_status` is added by `migrate`.
`src/db/style_faces.rs` holds all access, mirroring `src/db/faces.rs`. A
character is a `crate::model::Character`; a stylised face is a
`crate::model::StyleFace`. Stylised face crops live in `style-face-thumbs.db`
(`src/db/style_face_thumbs.rs`), which reuses the `FaceThumbs` struct with a
separate file.

### 000000.6 Stylised UI

The UI mirrors the human "People" UI, with "character" vocabulary.

- Settings pane `src/ui/settings_characters.rs`: enable toggle, opt-in
  auto-scan, detector dropdown, embedder dropdown, Download models, Scan, and
  Delete all stylised face data. Registered in `src/ui/settings.rs`. Keys are
  `styleface.*` in `src/ui/prefs.rs`; `load_styleface_config` reads them.
  `AppState` holds `style_face_config`, `style_face_job`, and
  `style_face_thumbs`.
- Scan `src/ui/stylefacescan.rs`: mirrors `facescan.rs` (channel, coordinator,
  worker pool, progressive re-cluster, `download_models`, `run_scan`).
- Sidebar `src/ui/sidebar.rs`: a Characters header (id `charactersheader`, item
  prefix `character:`) appears once any stylised face exists. Rename and delete
  a character from the row menu. Helper `character_id_of`.
- Characters view `src/ui/charactersview.rs`: a center-stack child named
  `characters`, mirroring `facesview.rs`. It shows the noise group as "Unclear".
  `AppState`: `show_characters`, `refresh_characters_if_active`.
- Character dialogs `src/ui/characters.rs`: `name_style_cluster_dialog` (name or
  merge a cluster).
- Grid `src/ui/grid.rs`: `Source::Character` and `Source::StyleCluster`, with
  `show_character`/`show_style_cluster` and re-query arms.

### 000000.7 Album Face type (Photo / Art)

A user marks an album as Inherit, Photo, or Art. The mark controls the face
method. The kind is three-state and inherits down the tree. An album with
Inherit takes its parent's kind. A top-level album with Inherit resolves to
Photo.

- Model/DB: `crate::model::AlbumKind` (Inherit=0, Photo=1, Art=2, with
  `from_i64`/`as_i64`). `Album` gained a `kind` field. `albums.kind` column
  added in `schema.sql` and in `migrate`.
- Resolve: `Library::album_effective_kind` walks the parent chain and returns 1
  (Photo) or 2 (Art). `Library::folders_under_album` returns all folder ids in
  the album subtree. `Library::photo_effective_face_kind` gives a photo's kind
  (its folder's album's kind, default Photo when in no album).
- Scoped scan: `photos_needing_face_scan_in(folder_ids, limit)` and the style
  twin scan only a folder set. `clear_face_scan_in` / `clear_style_face_scan_in`
  clear prior faces and scan state for a rescan. The scan bodies were refactored
  into a public `run_scan(state, ids, cfg)` so any id list can be scanned.
- Router `src/ui/albumscan.rs`: `scan_album_faces(state, album_id, rescan)`
  resolves the album's folders, reads the effective kind, and dispatches to the
  human pipeline (Photo) or the stylised pipeline (Art). It shows a message when
  the routed system is off or its models are missing.
- Autoscan now routes by kind. `albumscan::autoscan_routed` splits eligible
  photos by `photo_effective_face_kind` and feeds each pipeline its own list, so
  a photo is scanned by exactly one method (no double-scan). `freshness.rs` calls
  it. The old `scan_faces_quiet`/`scan_style_faces_quiet` are now unused but kept
  with `#[allow(dead_code)]`.
- Sidebar: an album row shows a `(Photo)` or `(Art)` suffix when its OWN kind is
  explicit (not the inherited kind). The album context menu gained a "Face type"
  submenu (title shows the current value; three items Inherit/Photo/Art) and
  "Scan faces in album" / "Rescan faces in album". Actions:
  `album-kind-inherit/photo/art`, `scan-album-faces`, `rescan-album-faces`.
  Methods `set_album_kind` and `scan_album_faces` on the sidebar.

### 000000.8 Design decisions the user made this session

- Use the `hdbscan` crate for stylised faces, not a hand-written clusterer. The
  user preferred a maintained external crate.
- Keep two separate face systems. Do not merge them and do not use HDBSCAN for
  photos.
- Album kind is three-state with inheritance. Scan routes by kind (one item, not
  a method chooser). The sidebar shows a badge. The Face type submenu uses plain
  items with the current value in the title (not radio checkmarks, to avoid a
  stateful-action refactor). Album scan offers both Scan and Rescan. Autoscan
  routes by kind too.

### 000000.9 Open follow-ups (not built)

- No automatic art-vs-photo classifier. The album kind is the routing signal. A
  photo in no album defaults to Photo. This is deliberate.
- The sidebar badge shows an album's own explicit kind, not the inherited
  effective kind. A child that inherits "Art" shows no badge. Showing the
  effective kind on every album is a possible follow-up.
- No per-face reject/reassign UI for stylised faces (the human system has one in
  the viewer). `reject_style_face_from_character` and `recluster_now` exist in
  the backend for this future work, marked `#[allow(dead_code)]`.
- Roadmap Phases 3–5 for stylised faces (nearest-part incremental assign, merge
  suggestions, a learned metric) are not built.
- The fp16 embedder catalog entry needs f16 tensor handling before use.

## 00000. Facial detection and recognition (read after section 000000)

This section describes an earlier session (the human face system). The stylised
face system in section 000000 mirrors this design. The work is complete. The
application builds. `cargo test` passed 55 tests plus 2 ignored tests at that
time. The work is on `main`. The work is pushed. The version was 0.0.70 at that
time. CI increases the version on each push. Do not change the version by hand.

The feature detects faces, groups the same person across the library, lets the
user name people, and shows each person's photos. All processing stays local.
See ROADMAP.md section "Facial detection & recognition" for the feature summary.

### 00000.1 Backend and the ONNX Runtime

The face pipeline uses `ort` (ONNX Runtime bindings) with the `load-dynamic`
feature. The crate is pinned to `=2.0.0-rc.10`. That release-candidate ABI needs
ONNX Runtime 1.22.x. `ndarray` builds the input tensors. `flate2` and `tar`
extract the runtime archive in process.

The ONNX Runtime shared library is NOT built into the binary and NOT committed.
The build and CI do not need it. Face detection is off by default. The first
time the user turns it on, the app downloads ONNX Runtime 1.22.0 from the
official Microsoft release into `~/.local/share/pichouse/runtime/`, with a
verified SHA-256, and loads it with `dlopen`. See `src/face/runtime.rs`. The URL
and the hash are constants there. `ort::init` runs once per process, guarded by
a `Once`.

Two `ort` release candidates were rejected before rc.10: rc.13 broke on the
`ureq` TLS feature, and rc.10 with the binary downloader needed system OpenSSL,
which the project avoids. `load-dynamic` plus a downloaded library avoids both.

Do not upgrade `ort` without checking the ABI version and the build. Do not add
the `download-binaries` feature — it drags in a build-time HTTP/TLS stack.

### 00000.2 Models

The models are YuNet (detector, MIT) and SFace (embedding, 128-D, Apache 2.0),
both from the OpenCV Zoo. They run well on an older CPU. They download into
`~/.local/share/pichouse/models/` on first use, pinned to opencv_zoo commit
`47534e2` with verified SHA-256. See the catalog in `src/face/models.rs`. The
catalog is extensible; higher-accuracy ArcFace (512-D, non-commercial) and a
custom-path option are listed in ROADMAP as follow-ups, not built.

### 00000.3 Inference

`src/face/detector.rs` runs YuNet: it letterboxes the oriented RGB image into a
static 640x640 BGR NCHW tensor, decodes the 12 raw heads (`cls/obj/bbox/kps` at
strides 8/16/32), applies NMS, and returns per-mille boxes and 5 landmarks.
`src/face/embedder.rs` runs SFace: it aligns the face to a 112x112 template with
an Umeyama similarity transform, warps with bilinear sampling, runs the model,
and L2-normalizes the 128-float vector. `src/face/mod.rs` has `FacePipeline`,
which ties the two together (`detect_and_embed`). `src/face/inference_test.rs`
is an ignored test that verifies both on real photos. Verified numbers:
detection score 0.946, same-person cosine 0.67, different-person 0.06.

Session extraction uses `DynValue::try_extract_tensor::<f32>()`. SFace input is
BGR, `[0,255]` float, NCHW; the input name is read from the session, not
hardcoded.

### 00000.4 Coordinate rule (important)

A face box and its landmarks are in per-mille (0..1000) of the photo AFTER
`photos.orientation` rotation and BEFORE any non-destructive edit. Every stored
box, face crop, and viewer overlay uses that same space. `Photo` has no EXIF
orientation field, so this convention is fixed.

### 00000.5 Storage

`src/db/schema.sql` has `persons`, `faces`, `face_scan`, and `face_rejections`.
`photos.face_status` is added by `migrate`. The schema runs on every open, so
existing databases gain the new tables automatically. `src/db/faces.rs` holds
all person and face access: CRUD, `photos_of_person`, `photos_in_cluster`,
`faces_for_clustering`, `person_representative_face`, `total_face_count`,
`unnamed_clusters`, `set_face_person`, `set_face_cluster`,
`reject_face_from_person`, `face_rejection_map`, and `delete_all_face_data`.
Embeddings pack as little-endian f32 blobs. Face-crop thumbnails live in
`face-thumbs.db` (`src/db/face_thumbs.rs`), keyed by face id. `Face` and
`Person` types are in `src/model.rs`.

### 00000.6 Clustering

`src/face/cluster.rs` groups embeddings by cosine similarity (default threshold
0.363, the SFace value). A named person anchors a stable cluster
(`PERSON_CLUSTER_BASE + person_id`), so new matching faces attach automatically
across scans. A `ClusterItem` carries `rejected` person ids; the assignment step
never puts a face into a person's cluster it was rejected from. This makes a
correction durable.

### 00000.7 Background scan

`src/ui/facescan.rs` mirrors the AI-tagging pattern (`aitag.rs`): a channel, a
coordinator thread, a worker pool, and a `Controller` cancel flag
(`state.face_job`). Workers decode the oriented image (capped long side 1600 via
`thumb::decode_oriented_rgb`), detect, embed, and write face rows. The scan is
PROGRESSIVE: every 20 photos it re-clusters and posts `Msg::Refresh`, so groups
appear during the scan, not only at the end. `download_models` fetches the
runtime and both models in the background and writes their paths and dim into
settings. `recluster_now` re-clusters after a manual correction and refreshes
the UI. `scan_faces` has a `quiet` variant for the opt-in auto-scan after a
reconcile (see `freshness.rs`, gated by `face.autoscan`, off by default).

### 00000.8 UI

- Settings pane `src/ui/settings_faces.rs`: enable toggle, opt-in auto-scan (off
  by default), embedding-model dropdown with a license note and a re-scan
  warning, Download models, Scan for faces now, and Delete all face data
  (privacy reset). Registered with one `stack.add_titled` line in
  `src/ui/settings.rs`. The `face.*` keys and `load_face_config` are in
  `src/ui/prefs.rs`; `AppState` holds `face_config` and `face_job`.
- Sidebar `src/ui/sidebar.rs`: a People header appears once any face exists
  (id `peopleheader`, item prefix `person:`). Clicking the header opens the
  Faces view. A person row shows that person's photos. A right-click menu
  renames or deletes a person.
- Faces view `src/ui/facesview.rs`: a center-stack view (`center_stack` child
  name `faces`). It shows one tile per group — named people first, then the
  largest unnamed clusters. A tile's IMAGE opens the group's photos. For an
  unnamed group, the tile's LABEL opens the name dialog. Tile size follows the
  thumbnail slider (`prefs.active_size()`, clamped 72..320). `AppState`
  methods: `show_faces`, `refresh_faces_if_active`.
- Grid `src/ui/grid.rs`: `Source::Person` and `Source::Cluster`, with
  `show_person`/`show_cluster` and re-query arms in `reload_from_source`. The
  grid header has a back button (`set_back`/`hide_back`) that the person and
  cluster views set to return to the Faces view.
- People dialogs `src/ui/people.rs`: `name_cluster_dialog` (name or merge a
  cluster) and `assign_face_dialog` (assign one face to a person or a new
  person, and, when the face is named, a "remove from this person" button that
  calls `reject_face_from_person`).
- Viewer overlay `src/ui/viewer.rs`: a Show faces button toggles a second
  transparent `DrawingArea` over the picture. It draws each face box (green
  when named, yellow when not) with the name. Clicking a box opens the assign
  dialog. When on, the image renders uncropped so boxes map to the oriented
  frame.
- Face crops: `AppState::face_crop_jpeg` renders (via
  `thumb::render_face_crop`) at 320 px and caches in `face-thumbs.db`.
- Person rule: `RuleField::Person` in `src/model.rs` and
  `src/db/virtual_albums.rs`, plus a "Contains person" entry in
  `src/ui/vrules.rs`. A smart virtual album can hold "contains person X".

### 00000.9 Rejection design decision

The user chose "name-first only" for removing a face from a group. Removal is
offered ONLY for a NAMED person (the assign dialog shows "Not <person> — remove
from this person"). Removing records a durable rejection so a re-scan never
re-attaches the face to that person. An UNNAMED group has no stable identity
across scans, so it offers no removal. The workflow is: name the group, then
remove outliers.

### 00000.10 Open follow-ups (not built)

- A cache-only clear for face crops, so a slider or model change refreshes crops
  without a full "Delete all face data". Old 160 px crops from an earlier build
  stay until regenerated.
- Higher-accuracy optional models (ArcFace 512-D) and a custom `.onnx` path.
- Immich has its own people feature; a mapping is out of scope for now.
- A dedicated split control for a face inside an unnamed group (today the user
  names the group first).

## 0000. Timeline, copy, crop overlay, slideshows, logging, and freeze fixes

This section describes the last session. The work is complete. The application
builds. `cargo test` passes 43 tests. The work is on `main`. The work is pushed.
The version is 0.0.54. CI increases the version on each push. Do not change the
version by hand.

The user confirmed the freeze fixes work. The user gave a `gdb` backtrace that
found the last freeze.

### 0000.1 Four small features

The session added four features from ROADMAP.md.

1. **Immich Timeline.** Each Immich server in the sidebar has a **Timeline**
   child node. The node shows every asset on the server, newest first. The new
   method is `Client::timeline_assets` in `src/immich/client.rs`. It calls
   `POST /search/metadata` with no `albumIds` filter and pages through the whole
   library. It shares the `search_assets` helper with `album_assets`. The UI
   function is `immich::show_timeline` in `src/ui/immich.rs`. It maps the assets
   to `immich://` photos and shows them with the grid's ad-hoc `show_photos`.
   The sidebar node uses the id prefix `immichtimeline:<server_id>` and the
   parser `immich_timeline_id_of` in `src/ui/sidebar.rs`.

2. **Copy image to clipboard.** The grid right-click menu has a **Copy image**
   item. It bakes the full-resolution edited image on a background thread and
   sets a `gdk::MemoryTexture` on the clipboard. It works for local and Immich
   photos. The code is in `src/ui/vmenu.rs` (`copy_photo_to_clipboard`,
   `bake_source`). `export::rotate_full` is now `pub(crate)`. The menu shows for
   an Immich-only selection.

3. **Interactive crop overlay.** The Edit tab has a "Crop by dragging on the
   image" toggle. The viewer wraps its `Picture` in a `gtk4::Overlay` with a
   transparent `DrawingArea`. The user drags a rectangle. On release the viewer
   converts the rectangle to a per-mille crop and writes it back to the numeric
   spin buttons, then commits. The coordinate mapping accounts for the
   `ContentFit::Contain` letterbox. The code is in `src/ui/viewer.rs`
   (`set_crop_mode`, `image_rect`, `update_crop_from_drag`) and
   `src/ui/editor.rs` (`build_crop`).

4. **Slideshows.** The toolbar has a **Play slideshow** button. A right-click on
   the button opens a popover with the per-image duration, a shuffle toggle, and
   a loop toggle. The settings persist in `library.db` under the keys
   `slideshow.secs`, `slideshow.shuffle`, and `slideshow.loop` (see
   `src/ui/prefs.rs`). The slideshow runs in the viewer
   (`src/ui/viewer.rs`): it enters fullscreen, hides the control bar, and
   advances on a `glib::timeout_add_seconds_local` timer. Space pauses. Escape
   stops. The arrow keys still move between photos. The slideshow plays the
   current grid view or the current multi-selection.

ROADMAP.md marks Slideshows and the interactive crop as done. Virtual albums
were already done.

### 0000.2 Console logging with CLI flags

The application now logs to the console (stderr). The crates are `log` and
`env_logger` (default features off, to keep the link small). `src/main.rs`
parses the verbosity from the command line before it starts the GUI:

- no flag: `warn` (the default)
- `-v`: `info`
- `-vv`: `debug`
- `-vvv`: `trace`
- `-q` or `--quiet`: `error`
- `-h` or `--help`: print usage and exit

`src/ui/app.rs::run` calls `app.run_with_args(&[])`. This stops GTK from parsing
the flags and rejecting them. The flags are consumed in `main` before the GUI
starts.

Every log line has the thread name (`main`, `scan`, `enrich0`…). Operations log
*before* they start, so the last line before a stall names the operation that
hung. `Library::lock` (in `src/db/library.rs`) wraps the shared
`Mutex<Connection>` and warns when a lock wait is more than 200 ms. All library
DB call sites use `self.lock()`. The `Thumbs` and `ImmichThumbs` databases keep
their own separate connections and do not use this helper.

### 0000.3 Scan-time performance and freeze fixes

The user reported that scanning a new library folder froze the UI. The session
made these fixes, in order:

1. **Decode each file once (`src/scan.rs`, `src/thumb.rs`, `src/ui/enrich.rs`).**
   Enrichment read each file three times (hash, dimensions, thumbnail decode).
   The new `scan::enrich_file_with_image` reads the file one time, hashes the
   bytes in memory, parses EXIF from the same bytes, and decodes one time.
   `Generator::cache_from_image` makes the thumbnail from those pixels. The old
   `enrich_file`, `taken_at`, `dimensions`, and `hash_file` stay for tests and a
   fallback; they have `#[allow(dead_code)]`.

2. **Fewer enrichment workers.** `ENRICH_WORKERS` is 2 (was 3). Parallel large
   decodes on a slow disk are net slower.

3. **Pause background work while browsing.** `AppState` has
   `enrich_pause_until: Arc<AtomicU64>` (a wall-clock millis deadline). The
   enrichment workers and the scan thread sleep while `now < enrich_pause_until`.
   Opening a folder sets a 3-second pause (`AppState::pause_enrichment`, called
   from `enrich::prioritize_folder`). Each thumbnail that lands in the grid
   re-arms a 2-second pause (in the grid's done handler, `src/ui/grid.rs`, using
   the constant `BROWSE_PAUSE_MS`). So the scan and enrichment stay paused while
   the visible folder still makes thumbnails. `Scanner::scan_folder` takes the
   pause flag as a parameter.

4. **No grid flash and no focus theft (`src/ui/grid.rs`).** A background reload
   used `rebuild()`, which does `store.remove_all()` and re-appends. That reset
   the selection and focus, so a click was almost impossible during a scan. The
   new `set_photos_preserving` diffs the incoming photos against the current
   store by file path (a stable id that does not change when a photo gains its
   hash). When the set matches in order, it updates the existing objects in
   place and keeps the selection. It falls back to a full `rebuild` only when
   the set changes. `reload_from_source` uses `set_photos_preserving`.
   `rebuild` also carries over each cell's texture by file path so a cell never
   blanks.

5. **Histogram off the main thread (`src/ui/editor.rs`).** `EditPanel::load`
   computed the histogram on the main thread. That decodes the full image and
   blocked the UI. The new `load_histogram_async` computes the histogram on a
   background thread and posts the result back, with a generation guard. The new
   `histogram_from_source` and `HistSource` do the work without the non-`Send`
   `AppState`.

6. **The hard freeze: infinite recursion (`src/ui/viewer.rs`,
   `src/ui/editor.rs`).** A `gdb` backtrace of the frozen application showed
   about 700 nested frames of `Viewer::show → Properties::show →
   EditPanel::load → Viewer::set_show_original → Viewer::show`. The cause:
   `EditPanel::load` called `viewer.set_show_original`, and `set_show_original`
   always re-rendered with `viewer.show`, which re-shows the properties panel,
   which re-loads the editor. Two guards fix it:
   - `EditPanel::load` returns early when the photo is the same (same `id` and
     `path`).
   - `Viewer::set_show_original` re-renders only when the flag changes.

### 0000.4 Not done in this session

- The user asked to document the exact Immich API-key permissions in README.md.
  This is not done yet. The needed permissions are: `album.read`, `asset.read`,
  `asset.view`, `timeline.read` for browse and view; and `asset.upload`,
  `album.create`, `album.update` for upload and two-way folder sync. The
  application never deletes on the server. Add this to README.md.
- ROADMAP.md has a new item: "Move to llama.cpp instead of Ollama". This is a
  plan only. No code exists for it.

### 0000.5 Verification

- `cargo build` — builds. The remaining warnings are deprecation warnings from
  the glib channel API and the GTK dialogs. Section 8 describes this future
  migration. Do not treat the deprecation warnings as new work.
- `cargo test` — 43 tests pass.
- To debug a future freeze: run the frozen process through `gdb -p <pid> -batch
  -ex "thread apply all bt"` and read the **main** thread (Thread 1). The worker
  threads are often only idle on a channel receive and are not the cause.

## 000. Non-destructive editing and color levels

This section describes the last session. The session added non-destructive
image editing and color levels. The work is complete. The application builds.
`cargo test` passes 43 tests. The work is on `main`. The work is pushed. The
version is 0.0.44.

The remaining `cargo build` warnings are about 34. Each remaining warning is a
deprecation warning. The deprecation warnings come from the glib channel API and
the GTK dialogs. Section 8 describes this future migration. Do not treat the
deprecation warnings as new work here.

The user confirmed the feature works.

### 000.1 What the feature does

The user edits a photo without a change to the file on disk. The application
stores the edits in `library.db`. The application applies the edits at view time
and when it makes a thumbnail. The edits are:

- Flip horizontal and flip vertical.
- Straighten by a small angle, with an auto-crop of the empty corners.
- Crop, as a per-mille rectangle.
- Brightness and contrast.
- Per-channel (red, green, blue) color levels: black point, white point, gamma.

The 90-degree rotation is a separate, older feature. It stays on
`photos.orientation`. The application applies the 90-degree rotation first. The
application applies the new edits after.

Color levels help photos scanned from negatives. These photos have skewed color.
The levels panel shows these controls:

- A live histogram for each channel. The histogram has three draggable markers:
  black, white, and gamma. A drag updates the view immediately.
- Spin buttons for the same values. The spin buttons and the markers stay in
  step.
- An "Auto levels" button. It reads the histogram and sets each channel's black
  and white points to remove a color cast. It clips 0.5% at each tail.
- A preset chooser. The user saves the current levels as a named preset. The
  user applies a preset. The user deletes a preset. Presets store levels only.
- An "Apply to folder" button. It merges the current levels into every photo in
  the folder. It keeps each photo's other edits (crop, rotate, flip,
  brightness).

The user opens the edit controls in the "Edit" tab of the right-hand panel. The
tab is next to "Pic Info" and "Tags". The user reveals the tab with the viewer
Edit button, or with a right-click on a thumbnail, then "Edit".

The user exports a "baked" copy. The application applies the edits at full
resolution and writes a new file. The user starts an export with "Export copy…"
in the edit tab, or with a right-click on one or more thumbnails, then "Export
edited copy…". A dialog sets the format (JPEG or PNG) and the JPEG quality. The
application remembers these values for the next export. For one photo, a Save
dialog asks for the path. For more than one photo, a folder dialog asks for a
folder. The application writes each file as `<stem>-edited.<ext>`.

Immich photos work too. The application downloads the full-resolution asset with
`Client::asset_original`. It uses the asset for the histogram, for auto-levels,
and for export. View-time edits apply to the preview.

### 000.2 Where the code is

- `src/db/schema.sql` — two new tables: `photo_edits` (one row per photo) and
  `level_presets`. Each table uses `CREATE TABLE IF NOT EXISTS`.
- `src/model.rs` — `Levels`, `LevelPreset`, and `PhotoEdit` structs. All fields
  are integer-scaled. `Default` is the identity edit. Gamma is milli-units
  (1000 = 1.0). Crop is per-mille (0..1000). This keeps the structs `Eq`.
- `src/db/edits.rs` — `photo_edit`, `set_photo_edit` (bumps `edit_rev`),
  `clear_photo_edit`, and `apply_levels_to_folder`.
- `src/db/presets.rs` — `level_presets`, `save_level_preset`,
  `delete_level_preset`.
- `src/edit.rs` — the shared render pipeline `apply_edits`, plus `auto_levels`.
  Both the viewer and the thumbnail generator call `apply_edits`. It has unit
  tests.
- `src/thumb.rs` — `Generator::get_edited`. The cache key is `<hash>` for an
  identity edit, or `<hash>|<edit_rev>` for a real edit. `invalidate` deletes the
  plain hash and every edited variant (`delete_hash_and_edits` in
  `src/db/thumbs.rs`).
- `src/ui/editor.rs` — the `EditPanel`. It builds once. `EditPanel::load(photo)`
  binds it to a photo. `refresh_all` and `refresh_channels` push values into the
  widgets after auto-levels or a preset. `load_image_for_edit` loads a local
  file or an Immich original.
- `src/ui/export.rs` — the baked export. It reads and writes the format and
  quality settings. It bakes the edits and writes the file.
- `src/ui/properties.rs` — the right-hand panel. It owns the `EditPanel`. It adds
  the "Edit" tab. `show(photo)` calls `edit.load`. `open_edit_tab` switches to
  the tab (page index 2).
- `src/ui/viewer.rs` — the Edit button calls `properties().open_edit_tab()`. The
  decode path (`decode_edited`, `pixbuf_to_rgba`) applies the edit for display. A
  "view original" toggle shows the unedited image.
- `src/ui/vmenu.rs` — the grid right-click menu gains "Edit…" (one photo) and
  "Export edited copy…" (one or more photos).
- `src/ui/prefs.rs` — `KEY_EXPORT_FORMAT` and `KEY_EXPORT_JPEG_QUALITY`, with
  defaults.

### 000.3 Design rules to keep

- Do not put float fields on `Photo` or `PhotoEdit`. `Photo` derives `Eq`. Store
  scaled integers.
- Do not change the shared `PHOTO_COLS`/`map_photo` query strings for edits.
  Edits live in a separate table and a separate struct.
- An identity edit keeps no `photo_edits` row. `set_photo_edit` deletes the row
  for an identity edit.
- After a change to an edit, call `Generator::invalidate(hash)`, then
  `viewer().reload_current()`, then `grid().reload_from_source()`.
- The thumbnail cache key must include the edit revision for a real edit. Do not
  reuse the plain hash for an edited thumbnail.

### 000.4 Open items (not done — candidates for next work)

- Interactive crop overlay. The crop is numeric per-mille now. A drag-rectangle
  overlay on the viewer picture is a good next step.
- A background progress bar for "Apply to folder" on very large folders. The
  operation is synchronous now.
- Edits do not sync to Immich. Export writes a new file the user re-uploads.
- The `open_edit_tab` page index is the constant 2 (Pic Info, Tags, Edit). If a
  new tab changes the order, update this index.

### 000.5 Verification limit

The last session could not run the GTK GUI. It verified the work with `cargo
build` and `cargo test` only. The user later confirmed the feature works. A new
agent that changes this UI must still test by hand, because the tests do not
cover the GTK widgets.

## 00. Small follow-up features (earlier work)

This section describes an earlier session. The session did four small follow-up
tasks. Each task is complete. The application builds. `cargo test` passed 37
tests at that time (it passes 43 now). Each task is on `main`. Each task is
pushed.

The remaining `cargo build` warnings are 25. Each remaining warning is a
deprecation warning. The deprecation warnings come from the glib channel API and
the GTK `MessageDialog`. Section 8 describes this future migration. Do not treat
the deprecation warnings as new work here.

### 00.1 New Files window is a user setting

The New Files window was a constant. It is now a user setting.

- The setting is `Prefs.new_max_age_days`. The default is 14.
- The setting key is `ui.new_max_age_days` (`prefs::KEY_NEW_MAX_AGE_DAYS`).
- `Prefs::new_max_age_secs()` gives the value in seconds.
- The Thumbnails settings pane has a spin button. The range is 1 to 365 days.
- A change writes the pref and the DB setting. A change refreshes the New Files
  view and reloads the sidebar, so the count updates at once.
- The old constants `NEW_MAX_AGE_DAYS` and `NEW_MAX_AGE_SECS` are removed from
  `src/ui/newfiles.rs`. Read the window from the prefs.

### 00.2 Clean Up Missing Photos action

A new action hard-deletes photo rows that are marked missing.

- `Library::missing_photo_count()` counts rows where `missing = 1`.
- `Library::delete_missing_photos()` deletes rows where `missing = 1`. It
  returns the count. The `ON DELETE CASCADE` rule removes the tags and the
  virtual-album memberships. The pragma `foreign_keys=ON` is set at open, so the
  cascade runs.
- The action is a destructive button in the Thumbnails settings pane. The button
  uses the `confirm(...)` dialog first. The action does not touch any file on
  disk. After the delete, it reloads the sidebar, the grid, and the New Files
  view.

### 00.3 thumb.regen is wired

The `thumb.regen` setting was stored but unused. It is now wired.

- The setting is `Prefs.regen_on_move` (key `thumb.regen`). The checkbox is in
  the Thumbnails settings pane.
- When the setting is on, the toolbar zoom slider clears the grid in-memory
  texture cache before it resizes. So each cell re-renders at the new size. The
  code is in `src/ui/toolbar.rs`, the slider `connect_value_changed` handler.
- When the setting is off, the prior behavior is kept.
- This is not a full on-disk regenerate. A full regenerate is the "Clear
  Thumbnail Cache" button.

### 00.4 Dead-code warnings cleared

The dead-code warnings are cleared.

- One truly-unused import was removed (`gtk4::prelude` in
  `src/ui/shortcuts.rs`).
- Each intentional kept-API item now has `#[allow(dead_code)]` and a comment.
  Examples: `Library::upsert_photo`, `Library::set_thumb_ready`,
  `Thumbs::clear`, `Generator::size`, `Grid::thumb_size`,
  `NewFilesView::set_thumb_size`, `GenResult` duration fields, the `enrich::Msg`
  `folder_id` field, the `grid::Source::Immich` fields, and `ScanStatus::Error`
  / `ScanStatus::from_str`.
- Do not delete a kept-API item without a check. It may serve a future feature.

### 00.5 Open follow-ups (not done)

- Give the grid selection model a hands-on test pass (see section 12.4). The GUI
  needs a display; CI cannot do this.
- Migrate the glib channel API and the GTK `MessageDialog` to remove the 25
  deprecation warnings (see section 8). This is a larger, separate task.

## 0. Immich integration (read after section 00)

The Immich integration is the active work area. Read this section before the
older sections. The older sections describe earlier features and are still
correct.

### 0.1 State

The application builds. `cargo test` passes 37 tests. All work is on `main`.
Each change is committed and pushed. CI bumps the build number on each push.

The Immich integration has four done phases and two out-of-scope items. See
`ROADMAP.md`, section "Immich integration", for the phase list.

- Phase 1 — Browse. Done.
- Phase 2 — Full image viewer. Done.
- Phase 3 — Upload. Done.
- Phase 4a — Two-way folder sync. Done.
- Phase 4b — Tag sync. Out of scope. Do not build it.
- Phase 5 — Immich photos in virtual albums. Deferred. Do not build it now.

### 0.2 Architecture

Immich uses the background HTTP pattern. Read the "Background HTTP pattern"
part of AGENTS.md.

- `src/immich/client.rs` holds a blocking `reqwest` client. It sends an
  `x-api-key` header. It talks to the Immich REST API under the `/api` path.
- `src/immich/mod.rs` exports `Client`.
- `src/ui/immich.rs` runs all Immich work off the GTK main thread. It uses
  `glib::MainContext::channel` to return results to the main thread. This is the
  main Immich UI file.
- `src/db/immich.rs` holds the `immich_servers` and `immich_folder_links`
  table access.
- `src/db/immich_thumbs.rs` holds a per-server thumbnail cache. Each server has
  its own file `immich-thumbs-<server_id>.db`.
- `src/ui/settings_immich.rs` is the Settings pane. It manages servers and the
  album page size. It has a "Clear Immich Thumbnail Cache" button.

### 0.3 Data model

- Table `immich_servers(id, name, base_url, api_key, added_at)`. More than one
  server is supported. The API key is stored in plain text.
- Table `immich_folder_links(folder_id PK, server_id, immich_album_id,
  created_at)`. One local folder links to one Immich album for two-way sync.
- Both tables are in `src/db/schema.sql`. They use `CREATE TABLE IF NOT
  EXISTS`. No migrate step is needed for them.
- An Immich photo in the grid is a synthetic `model::Photo`. Its `id` is 0. Its
  `path` is `immich://<server_id>/<asset_id>`. It has no row in the `photos`
  table. For this reason an Immich photo cannot join a virtual album (that needs
  a real `photos.id`). The grid excludes Immich photos from virtual-album
  actions.

### 0.4 Key API facts (do not re-learn these)

- List an album's assets with `POST /search/metadata`, body
  `{"albumIds":[id], "size":N, "page":P}`. The response is
  `{"assets":{"items":[...], "nextPage":"<n>"|null}}`. The old
  `GET /albums/{id}` no longer returns assets on current Immich.
- Thumbnails come from `GET /assets/{id}/thumbnail?size=thumbnail`. The viewer
  preview comes from `size=preview`. The original file comes from
  `GET /assets/{id}/original`.
- Thumbnails and previews may be WebP. GTK `PixbufLoader` cannot decode WebP on
  the target machine. `decode_texture` in `src/ui/grid.rs` and `decode_pixbuf`
  in `src/ui/viewer.rs` fall back to the `image` crate for WebP. Keep this
  fallback.
- Upload with a multipart `POST /assets`. The response reports `created` or
  `duplicate`. Immich dedups by its own checksum. The local `photos.hash` is
  SHA-256 and is not used for Immich dedup.
- The `reqwest` `multipart` feature is enabled in `Cargo.toml`.

### 0.5 Sync behaviour

- Sync links a local **folder** to an Immich album. It does not link a pichouse
  album.
- Up direction: `immich::autoupload_added` uploads new local photos of a linked
  folder. It is called next to `enrich::enqueue` in `src/ui/freshness.rs` and
  `src/ui/watcher.rs`.
- Down direction: `immich::sync_folder_down` downloads album assets that are not
  yet local, writes them into the folder, then calls `freshness::reconcile_now`.
  `sync_all_down` runs at startup and every 5 minutes
  (`start_periodic_refresh`).
- Match is by original filename. This stops re-download loops and re-upload.
- Start sync from the local side: right-click a folder → "Sync with Immich
  album…". The dialog makes a new album or uses an existing one.
- Start sync from the Immich side: right-click an Immich album → "Sync to local
  folder…". The dialog downloads the album into a new subfolder of a chosen
  library root, then links that folder.
- A synced folder shows a `⇅` mark in the sidebar tree.

### 0.6 Sidebar node ids (Immich)

The sidebar tree uses string ids. See AGENTS.md "Sidebar sections".

- `immichheader` — the Immich section header.
- `immichserver:<server_id>` — one server.
- `immichalbum:<server_id>:<album_uuid>` — one album.
- Helper parsers in `src/ui/sidebar.rs`: `immich_server_id_of`,
  `immich_album_of`. Context-menu actions: `refresh-immich`, `album-to-local`,
  `upload-to-immich`, `sync-immich`, `unsync-immich`, `syncnow-immich`.

### 0.7 Known limits (do not treat as bugs)

- One upload or sync session at a time. The `state.immich_upload` controller
  cancels a prior session when a new one starts. Two syncs can cancel each
  other. A queue is a future improvement.
- Filename match, not content hash. Two different images with the same filename
  are treated as the same. Content-hash match is a future improvement.
- Sync is additive. A delete on one side does not delete on the other side.
- Virtual albums cannot be uploaded. Only folders and folder-backed albums
  upload.

### 0.8 Next steps (if the user asks)

- Consider a proper upload queue so parallel syncs do not cancel each other.
- Consider content-hash match for sync instead of filename match.
- Phase 5 (Immich photos in virtual albums) needs a schema change. See
  ROADMAP.md Phase 5 for the two options.

## 1. State

The Go to Rust port is complete. The Rust application replaces the Go
application. The Go code is deleted. All work is on `main`. The temporary
`rust-port` branch is deleted.

The application builds, tests pass, and the UI runs on the target machine
(Debian 13, GTK 4.18). `cargo test` passes 33 tests.

Two roadmap features are complete: fast two-phase import and library freshness.
A New Files view is complete. Section 10 describes them. Section 11 describes
the fixes from an earlier session.

The Virtual albums roadmap feature is complete. It is manual and rule-based.
Section 12 describes it. Section 12 also describes two related additions:
drag-and-drop of photos onto a virtual album, and persistence of the sidebar
tree view. Read section 12 first if you continue recent work.

## 10. Fast two-phase import and library freshness

### Two-phase import
- Phase 1 (`scan::Scanner::scan_folder`) records photo structure only: path,
  filename, folder id, size, mod time. It does no EXIF decode, no dimension
  decode, and no hashing. The folder tree and grid populate almost at once.
- Phase 2 (`ui::enrich`) is a background worker pool. It drains a shared
  worklist of photo ids (`AppState::enrich_queue`), computes EXIF `taken_at`,
  dimensions, and the SHA-256 hash per photo (`scan::enrich_file`), writes them
  (`Library::enrich_photo`), then generates the thumbnail. The grid re-queries
  periodically, so placeholders become thumbnails as data lands.
- A photo carries a `scan_state` column: 0=structured, 1=enriching, 2=done.
  `photos_needing_enrichment` selects `scan_state <> 2 AND missing = 0`.
- Opening a folder calls `enrich::prioritize_folder`, which moves that folder's
  un-enriched ids to the front of the worklist. The queue follows the view.
- On startup, `enrich::ensure_running` reseeds the worklist from the database,
  so an interrupted import resumes without a manual rescan.
- Bulk enrichment does NOT run during a scan. See section 11.1.
- The folder `year` starts from the folder mtime and is refined from the
  earliest known `taken_at` after a folder finishes enriching.
- Thumbnails need the hash (the cache key), so an un-enriched cell shows the
  filename-label placeholder until Phase 2 hashes it. No temporary key is used.

### Library freshness
- `reconcile.rs` diffs disk against the database per folder. New files are
  inserted (Phase 1) and queued for enrichment. Removed files are soft-marked
  `missing` (a `missing` column); the row and its tags/edits are kept. A
  reappeared file clears the flag. A new file whose size matches a missing row
  in the same root is treated as a move and re-points the existing row
  (`move_photo_path`), then is re-hashed to confirm identity.
- Reconciliation is the reliable path. It runs on startup, on demand (the
  Refresh Library toolbar button, `emblem-synchronizing-symbolic`), and on a
  periodic timer (`ui::freshness`, `PERIODIC` = 180 s). It works on network
  drives (NFS/SMB) and very large trees.
- `ui::watcher` adds an inotify fast path for local folders. It debounces event
  bursts (`DEBOUNCE` = 1500 ms) and reconciles only the affected directories.
  It degrades gracefully: if a watch cannot be added (e.g. the inotify watch
  limit), it logs and relies on the periodic reconcile. It is never required
  for correctness.
- IMPORTANT network-drive caveat: inotify does NOT see changes made by other
  machines on NFS/SMB mounts. The watcher may be silent there. The periodic
  reconcile is what catches remote changes. Do not remove the periodic
  reconcile in favor of inotify.
- Missing photos are shown dimmed in the grid (a `missing` property on
  `PhotoObject`).

### New Files view
- A photo carries `added_at` (set on the Phase 1 structure insert). Each library
  root carries `first_scan_done_at`, stamped once when its first scan completes
  (`Library::mark_first_scan_done`, called from the scan worker). A photo is
  "new" when `added_at > first_scan_done_at` for its owning root (matched by path
  prefix) AND it was added within the last `NEW_MAX_AGE_DAYS` (14) days. Age
  expiry is automatic; there is no per-file dismiss.
- The initial import of an existing library never counts as new, because those
  photos are recorded before the boundary is stamped.
- `Library::new_photos_grouped` returns `(Folder, Vec<Photo>)` groups, newest
  first. `new_photos_count` sums them for the sidebar.
- `ui::newfiles::NewFilesView` renders the grouped view (a folder header per
  group, thumbnails in a `FlowBox` below) with its own thumbnail worker pool. It
  is a named child (`newfiles`) of the center `Stack`.
- The sidebar (`ui::sidebar`) shows a "New Files (N)" row at the top of the
  Library tab when N > 0; selecting it calls `AppState::show_new_files`.
  Selecting a normal folder calls `show_grid` first, then loads the folder.
- The view refreshes live: reconciliation, the watcher, and enrichment call
  `AppState::refresh_new_files_if_active` after they reload.

### Schema migration
- `library.db` gained `photos.scan_state`, `photos.missing`, `photos.added_at`,
  and `library_folders.first_scan_done_at`. `Library::open_at` runs an additive
  `migrate` that adds the columns to an older database, marks already-hashed rows
  `scan_state = 2`, and stamps `first_scan_done_at` on existing roots so their
  photos are not treated as new. No rebuild is forced.

### Open follow-ups (not done; noted in code)
- Full hash-based move detection beyond the size heuristic.
- ~~A "clean up missing" action to hard-delete missing rows on user
  confirmation.~~ Done. A "Clean Up Missing Photos" button in the Thumbnails
  settings pane calls `Library::delete_missing_photos` after a confirm dialog.
- ~~Making `NEW_MAX_AGE_DAYS` a user setting (it is a constant now).~~ Done. It
  is `Prefs.new_max_age_days` (setting key `ui.new_max_age_days`), edited by a
  spin button in the Thumbnails settings pane.
- Interaction with future RAW+JPEG pairing (pair during Phase 1 or Phase 2).

## 11. Recent session: scan sequencing, tree, and theme

This section records fixes made after section 10. Read it before you change the
scan, the album tree, or reconciliation.

### 11.1 Enrichment does not run during a scan (tree first)
- The scan worker (`ui::actions`) sends two message types. `ReloadOnly`
  refreshes the sidebars and the grid but does NOT start Phase 2. It is sent on
  the periodic tick and after each root completes. `ReloadAndEnrich` refreshes
  and then calls `enrich::ensure_running`. It is sent one time, after the whole
  scan queue drains.
- Result: the file tree lands in Library first. Bulk "Reading photo info"
  starts only after the scan finishes.
- Exception: if the user opens a folder during the scan, that folder is enriched
  at once. `load_folder_into_grid` calls `enrich::prioritize_folder`, which
  front-loads that folder's ids and starts the pool. Keep this behavior.
- Do not add a call that starts bulk enrichment during the scan.

### 11.2 The album tree builds live during the scan
- The problem before: folders showed under "New folders" until the scan
  finished, because the album sync ran only at the end.
- `scan::Scanner::scan_folder` now takes a second closure, `on_folder`. The
  scanner calls it once per directory, right after it writes that directory's
  rows.
- The scan worker gives `on_folder` a per-root `albumtree::DiskAlbumMapper`. The
  mapper files each folder into its disk-mirrored album at once, so a folder
  never waits under "New folders". A completion sweep (`sync_disk_tree`) runs
  per root as a safety net. Both paths skip folders that are already in an album,
  so user placements are kept.
- `DiskAlbumMapper` caches albums by `(parent_id, name)` so repeated calls stay
  cheap.

### 11.3 Albums sort alphabetically
- `ui::sidebar::reload` sorts albums by name, case-insensitive, before it builds
  the tree. This is display-only. The `position` column is not changed.

### 11.4 No phantom 0-image folders; empty leaf folders are removed
- A directory that holds only subfolders (no images) is an album only. It never
  gets a `folders` row. The scanner records only directories that hold images.
- `reconcile.rs` matched this rule. In `reconcile_dir`, a directory with no
  images uses `Library::folder_id_by_path` (a non-creating lookup). If no row
  exists, reconcile skips the directory. So reconciliation no longer creates
  phantom 0-image folder rows for container directories.
- Decision (user): a folder with no images on disk AND no image-containing
  subfolders is removed outright (`Library::delete_folder`), even if it held
  photos before. `dir_has_images` guards this: a container is never removed.
  This wins over the "soft missing" rule when the LAST image in a folder is
  deleted. `Report` has a `removed` count.

### 11.5 Phase 1 inserts are batched (lock contention)
- `Library::insert_structure_batch` records a whole directory's photos in one
  transaction. Before, each photo took the single SQLite mutex twice. The tight
  loop starved the enrichment and thumbnail workers and the UI thread.
- The scan yields between directories. The DB has one connection behind one
  `Mutex`, so all DB access is serialized. Keep the batch pattern for any new
  bulk write during a scan.

### 11.6 Theme override (Adwaita) — fixes an unexpandable tree
- Some environments (for example Kasm remote desktops) ship a broken GTK3-era
  system theme. GTK4 cannot parse it. The theme gives no expander size and no
  expander image, so the folder tree cannot expand in either the Library or the
  Folders tab. The data is correct; only the theme is broken.
- `app::apply_theme(force_adwaita)` sets `gtk-theme-name` to `Adwaita` when the
  override is on, or resets the property when it is off. It runs at startup in
  `build_ui`, before any widget is built.
- Settings has an Appearance pane with a checkbox, "Use recommended theme
  (Adwaita)". It is on by default. The change applies live. The setting key is
  `ui.theme_override` (`prefs::KEY_THEME_OVERRIDE`, `Prefs::theme_override`).
- If a user reports a tree that will not expand, tell them to keep this on.

### 11.7 Other UI fixes in this session
- Viewer: `show()` clears the old image at once and tags each async load with a
  generation, so opening a second photo does not show the previous one.
- Sidebar: the context-menu popover crashed the app after a library folder was
  removed. Every menu action now calls `dismiss_menu` first, and the album
  actions use `reload_deferred` (an idle rebuild), so the tree is not rebuilt
  while the popover's row is recycled.
- Settings: removing a library folder calls `AppState::clear_grid_if_folder_gone`
  so the grid does not keep showing the deleted folder's thumbnails.
- Sidebar order: "New Files" then "New folders" then albums, all at the top.
- Status bar: the scan counter is cumulative across all queued folders, not per
  folder.

## 12. Recent session: virtual albums, drag-drop, tree persistence

This section records work made after section 11. Read it before you change the
virtual albums, the grid selection, or the sidebar tree state.

### 12.1 Virtual albums (manual + rule-based)
- Purpose: a virtual album groups individual photos. The photos come from any
  folder. A virtual album can nest under another virtual album. A photo can
  belong to many virtual albums.
- The code is in `src/db/virtual_albums.rs` (data) and several UI files.
- Storage: three tables in `library.db`.
  - `virtual_albums` — id, name, parent_id, position, rule_match. `rule_match`
    is 0 for AND, 1 for OR.
  - `virtual_album_photos` — album_id, photo_id, position, kind. `kind` is 0 for
    a manual pin, 1 for an exclusion.
  - `virtual_album_rules` — id, album_id, field, op, value. `field` is one of
    `tag`, `date_from`, `date_to`, `filename`, `folder`.
- Membership is computed live at view time. `photos_in_virtual_album` builds SQL
  from the rules, combines the rules with AND or OR per `rule_match`, unions the
  manual pins, then subtracts the exclusions.
- IMPORTANT: the membership subqueries select `photo_id AS id`. The alias is
  required. A missing alias caused a past bug where removals did not take effect.
- The three tables use `CREATE TABLE IF NOT EXISTS`. An older database gains them
  on open. There is no separate migration step.
- Model types are in `src/model.rs`: `VirtualAlbum`, `RuleMatch`, `RuleField`,
  `RuleOp`, `VirtualRule`.
- Sidebar (`src/ui/sidebar.rs`): a "Virtual Albums" section shows above the
  normal folder-albums. The section icon is `starred-symbolic`. Node ids use the
  prefix `valbum:`. The section header id is `virtualheader`. The row context
  menu creates, renames, deletes, adds a sub-album, and opens the rules editor.
  Drag one virtual album onto another to nest it.
- Grid (`src/ui/grid.rs`): the grid now uses `MultiSelection`. `selected_photos`
  returns the current selection. A right-click raises a context menu.
- Grid context menu (`src/ui/vmenu.rs`): it adds the selection to a virtual
  album, removes the selection from the album that is open, or makes a new album
  from the selection.
- Rules editor (`src/ui/vrules.rs`): a dialog with an AND/OR mode and a list of
  rule rows. Dates are typed as `YYYY-MM-DD`. The dialog converts a date to a
  Unix timestamp and back. `AppState::show_virtual_album` loads an album into the
  grid.

### 12.2 Drag-and-drop of photos onto a virtual album
- The grid is a drag source. The payload is a string `photos:<id>,<id>,...`. The
  ids are the current selection. GTK selects the pressed cell before the drag
  starts. So a drag over an unselected cell carries only that cell.
- A virtual-album sidebar row accepts the payload. `photo_ids_of` parses it. The
  row then adds the photos to the album.
- The sidebar drop target accepts both MOVE and COPY actions. The grid drag
  source uses COPY. The drop fails if the actions do not overlap.
- Fix recorded: a drop leaves the target row selected. A following click on that
  row did not fire `selection-changed`, so the album did not open. The add path
  now calls `unselect_all` after the drop. The next click is a real change and
  opens the album.

### 12.3 The sidebar tree view survives a restart
- The set of expanded node ids is saved in `library.db`. The settings key is
  `sidebar_expanded` (`EXPANDED_SETTING_KEY`). The value is the ids joined by
  newline.
- `bind_state` calls `load_expansion` before the first reload.
  `restore_expansion` then expands the saved rows.
- `save_expansion` now removes collapsed ids too. Before, it only inserted, so an
  id stayed "expanded" forever. It runs at the start of each reload.
- `persist_expansion` writes the set to `library.db`. It runs after each reload.
  It also runs on every manual expand or collapse. `bind_row` connects
  `connect_expanded_notify` on each row for this. The handler is stored on the
  list item and is disconnected when the item is recycled.

### 12.4 Note: the grid selection model changed
- The grid changed from `SingleSelection` to `MultiSelection`. This was needed
  for the drag and the "add to album" menu.
- The viewer and the properties panel act on the first selected photo.
- This change did not get a hands-on multi-select test pass. It is not a known
  bug. Test viewer, properties, and rotation with a multi-selection if you touch
  this area.

### 12.5 Open follow-ups (not done)
- Give the grid selection model a hands-on test pass (see 12.4).
- ~~Clean up pre-existing dead-code warnings.~~ Done. Dead-code warnings are
  cleared: truly-unused imports removed, intentional kept-API items annotated
  with `#[allow(dead_code)]` and a comment. The ~25 remaining warnings are all
  glib-channel / MessageDialog deprecation warnings (a separate future
  migration, see section 8).

## 2. What is ported

All Go modules are ported to Rust:

- `src/model.rs` — domain types.
- `src/db/` — rusqlite over `library.db` and per-size `thumbs-<N>.db`. Albums,
  virtual albums (`virtual_albums.rs`), tags, FTS5 search. Each connection is
  behind a `Mutex`.
- `src/scan.rs` — recursive image walk, EXIF date, dimensions, SHA-256 hash,
  cancellation.
- `src/thumb.rs` — decode, rotate, resize (fast_image_resize Catmull-Rom),
  JPEG encode, per-size cache.
- `src/ai/` — Ollama blocking client, config, tagger, subprocess manager.
- `src/ui/` — the full GTK4 UI. Notable files: `app.rs` (window/layout/wiring),
  `state.rs` (shared `Rc<AppState>`), `grid.rs` (thumbnail grid + worker pool +
  source tracking), `sidebar.rs` (Library album tree + menus + drag-drop),
  `foldertree.rs` (raw Folders tab), `albumtree.rs` (disk→album auto-sync),
  `actions.rs` (scan queue + worker), `aitag.rs` (AI tagging worker pool),
  `viewer.rs`, `properties.rs`, `toolbar.rs`, `status.rs`, `settings.rs`,
  `settings_ai.rs`, `tagmanager.rs`, `shortcuts.rs`, `dialogs.rs`,
  `controller.rs` (cancel token), `thumbcache.rs` (LRU texture cache),
  `prefs.rs`, `photo_object.rs`, `util.rs`, `vmenu.rs` (grid virtual-album
  context menu), `vrules.rs` (virtual-album rules editor).

## 3. Parity and post-parity work

The Rust application is at feature parity with the old Go application, plus
several improvements the Go version did not have. All Go UI features are ported,
including:

- The album tree with sub-albums and the "New folders" grouping.
- Right-click context menus (create/rename/delete album, move to album).
- Drag-and-drop of folders into albums and album re-parenting.
- The raw filesystem "Folders" tab.
- Grid refresh after scan and after rotation (re-queries the source).
- An in-memory LRU texture cache for fast scroll/re-entry.

Improvements added after parity (not in Go):

- Scan queue (`src/ui/actions.rs`, `AppState::scan_queue`). Adding a folder
  while a scan runs appends to a shared queue. The scan worker drains the queue.
  Before, a second add cancelled the first scan (both apps had this bug; Rust
  fixed it).
- Live sidebar population during a scan: the scan progress callback sends a
  reload every 200 photos, and once per finished root.
- Auto-album tree (`src/ui/albumtree.rs`, `sync_disk_tree`). After a root is
  scanned, the on-disk directory hierarchy is mirrored into the Library album
  tree: the root basename becomes a top-level album, intermediate directories
  become nested sub-albums, each scanned folder is filed into its album.
  It never re-files a folder that is already in an album, so user edits and
  manual placements survive a rescan.
- Folders tab: a folder icon per row; library roots use a distinct
  `drive-harddisk-symbolic` icon and show their full path in bold.

There are no known parity gaps. One historical setting, `thumb.regen`
(regenerate on slider move), was stored but unused in the Go app and early Rust;
it is now wired (see section 3a).

## 3a. Open behaviors / possible follow-ups

The user accepted these; a fresh agent should not "fix" them without being
asked:

- During a scan, folders discovered mid-scan appear under "New folders" until
  the root finishes; `sync_disk_tree` only runs when a root completes, then it
  reorganizes them into the album tree. Making the tree build fully live would
  require running the sync per-directory (more DB writes).
- A rescan does NOT re-assert the disk-mirrored tree over manual album edits;
  it only files folders that are not already in an album. This was a deliberate
  choice (preserve user edits). The user has not asked to change it.
- `thumb.regen` is now wired: when "Regenerate thumbnails when moving the
  slider" is on, the toolbar slider clears the grid's in-memory texture cache
  before resizing, so cells re-render at the new size instead of scaling a
  cached texture. When off, the prior behavior is kept.

## 4. Build, test, run

- Build: `cargo build`
- Test: `cargo test`
- Run: `cargo run`

The GUI needs a display. It does not run in a headless CI container. CI builds
and tests only.

## 5. CI and versioning

- `.gitea/workflows/build.yaml` runs on push to `main`. It reads the version
  from `Cargo.toml`, bumps the build number, commits it with `[skip ci]`, runs
  `cargo test --release` and `cargo build --release`, and publishes one rolling
  pre-release binary. This is the only workflow.
- The runner has no passwordless sudo. CI does not run `apt-get`. It adds the
  installed cargo bin directory to `GITHUB_PATH` and verifies `cargo` and
  `gtk4` are present.
- `src/version.rs` mirrors the Cargo version with `env!("CARGO_PKG_VERSION")`.
- Do not change the build number by hand. See AGENTS.md RULE THREE.

## 6. Schema note

The Rust schema is a fresh start. It has `orientation` and `ai_status` inline.
There is no migration from the Go databases. The user rebuilds the library by
rescanning.

## 7. Dependencies of note

- gtk4-rs 0.7 (`v4_10`). Do not upgrade past the GLib the system ships.
- rusqlite (`bundled`) — bundled SQLite includes FTS5.
- reqwest uses `rustls-tls` (no system OpenSSL).
- image, fast_image_resize, kamadak-exif, sha2, base64, serde, serde_json, dirs.

## 8. Known technical notes

- `Pixbuf` is not `Send`. Workers send raw bytes to the UI thread; the UI thread
  decodes to a texture or pixbuf.
- Background workers talk to the UI with `glib::MainContext::channel`. This is
  deprecated in glib 0.18 but works. A future change may move to
  `async-channel` + `spawn_future_local`.
- The grid uses a `PhotoObject` GObject with a `texture` property. A worker sets
  the texture on the UI thread; the bound `Image` observes `notify::texture`.
  A notify handler takes two arguments (object, ParamSpec) — use
  `connect_notify_local`, not a one-argument closure.
- Shared state moved into background threads must be `Send`. GTK/GObject types
  are not `Send`. `AppState` holds thread-shared data as `Arc<Mutex<...>>`
  (`ai_manager`, `scan_queue`); use the `*_arc()` accessors to clone a handle
  into a worker thread. Do not move an `Rc` or a widget into `thread::spawn`.
- The grid remembers its `Source` (a scanned folder id, a raw dir, or none) so
  `reload_from_source()` can re-query after a scan or a rotation. `show_folder`
  and `show_raw_folder` set the source; `show_photos` sets it to `None`.
- `Library` and `Thumbs` wrap the SQLite `Connection` in a `Mutex`; every call
  locks. This also serializes the multi-statement tag/album writes. Do not add a
  second connection without keeping the single-writer guarantee for `thumbs`.

## 9. Rules

- Follow RULE ZERO: do not loop on guesses. Ask the user one question and wait.
- Commit and push after each change (RULE ONE).
- Keep AGENTS.md and README.md correct (RULE TWO).
