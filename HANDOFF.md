# HANDOFF

This document is for an agent with no memory of the last session. It uses
Simplified Technical English (ASD-STE100, Strict). Read AGENTS.md first. Read
ROADMAP.md for planned features. Read section 0000000000000000000 first — it
describes the most recent work (found and fixed the real cause of the Phase 1
scan slowdown past 1M photos: a per-file re-stat that grew expensive on a
network mount). Then read section 000000000000000000 — it
describes an earlier, partial fix (stopping O(total-photos) sidebar work during
a scan) and the diagnostic logging that led to the real fix above. Then read
section 00000000000000000 — it describes bounding on-demand
grid work to the true visible window. Then read section 0000000000000000 — it
describes viewport-driven enrichment and a Tools "Generate Thumbnails" pass;
automatic enrichment is removed. Then read
section 000000000000000 — it
describes a during-scan sidebar-refresh performance fix for very large
libraries. Then read section 00000000000000 — it
describes a large-library performance overhaul plus five
features: a separate read connection, a demand-driven thumbnail grid, a scan
resume cursor, sort and filename options, and a banned-matches system. Then
read section 0000000000000 — it
describes a scan-tree responsiveness overhaul (a
postpone-thumbnails option, live tree growth during a scan, and a fix for
lost focus and swallowed clicks during a scan). Then read
section 000000000000 — it describes GitHub release prep, the Unlicense
switch, and the first named release, v0.1.0. Then read section 00000000000 —
it describes four context-menu and character-view fixes. Then read
section 0000000000 — it describes a named-release workflow that mirrors to
GitHub. Then read
section 000000000 — it describes four fixes (a sidebar crash, an album drag bug,
a scan freeze, and a smaller release binary). Then read section 00000000 — it describes the
CCIP embedder swap and six face and character features.
Then read section 0000000 — it describes the duplicate image finder. Then read
section 000000 — it
describes stylised face recognition for art, and the album Face type. Then read
section 00000 — it describes the human facial detection and recognition system
that the stylised system mirrors. Then read section 0000 — it describes four
small features, console logging, and scan-time performance and freeze fixes.
Then read section 000 — it describes non-destructive editing and color levels.
Then read section 00 — it describes four earlier follow-up features. Then read
section 0 — it describes the Immich integration. The later sections describe
earlier features and are still correct.

## 0000000000000000000. Found and fixed the real Phase 1 scan slowdown (most recent work — read this first)

This section describes the last session. All work is complete, on `main`, and
pushed. Two functional commits (diagnostic logging, then the fix). CI
version-bump commits sit between pushes. Ignore those.

The previous session (section 000000000000000000 below) fixed the sidebar
aggregate queries, but the user reported the scan still slowed down at 1M+
photos, including on a resume run that mostly skips directories, and on both a
CIFS and an NFSv4 mount. That ruled out SQLite and the sidebar. Read
section 000000000000000000 first for that background, then this section for
the real cause and fix.

### 0000000000000000000.1 Diagnostic logging pinpointed the cause

Added `ScanMetrics` to `src/scan.rs`: per-directory phase timing (`read_dir`
plus the entry `file_type` loop, the directory stat for the resume cursor, the
per-file stat, the resume cursor DB read, the DB batch) and a rolling summary at
`info` every 200 directories or 15 seconds (dirs/s, files/s, average per-phase
milliseconds, cumulative totals). Also logs a slow `read_dir` (>= 250 ms), a
slow `folder_scan_cursor` (>= 50 ms), and the WAL page count at each periodic
checkpoint (`Library::checkpoint`, `src/db/library.rs`).

The user ran a scan with `-vvv` and shared the `scan rate:` line from early
(200 directories in) and late (5847 directories, 1.42M files in). Every phase
was flat or small except one:

```
early: file_stat 0.09 ms/dir
late:  file_stat 209.37 ms/dir   (all other phases flat or small)
```

`file_stat` climbed about 2300x while `read_dir`, `dir_stat`, `cursor`, and `db`
did not. This is the whole slowdown.

### 0000000000000000000.2 Root cause and fix

`structure_photo` (`src/scan.rs`) called `std::fs::metadata(path)` a second
time for every image file, to read its size and mtime. The directory listing
(`std::fs::read_dir`) already fetches this information for each entry. On a
network mount (NFS with READDIRPLUS, CIFS) the second, separate `stat` call
round-trips to the server once the client's attribute cache entry for that file
(populated by the `read_dir` response) has been evicted — and across a walk of
over a million files, the cache keeps getting evicted, so the round-trip cost
climbs. This reproduced on both CIFS and NFSv4, confirming it is the app's extra
stat, not one mount protocol.

Fix, in `src/scan.rs`:
- The directory-entry loop now captures each image file's `Metadata` via
  `entry.metadata()` at listing time (reusing the attributes the directory
  listing already returned), instead of only the path.
- `structure_photo(folder_id, path, meta)` takes that `Metadata` directly and no
  longer calls `std::fs::metadata` itself.
- A file that vanishes between the listing and this call is skipped, the same
  as before. No separate fallback re-stat was added: it would hit the same
  vanished file and stat the network again for no benefit, defeating the fix.

Verify: re-run the scan with `-vvv` and grep
`"scan rate:|wal checkpoint|slow read_dir|slow folder_scan_cursor|db lock: waited"`.
`file_stat` should stay near 0 through the whole scan instead of climbing, and
dirs/s should stay high (bounded mainly by `read_dir`).

## 000000000000000000. Stop the Phase 1 scan slowdown at 1M+ photos

This section describes the last session. All work is complete, on `main`, and
pushed. One functional commit. CI version-bump commits sit between pushes.
Ignore those.

The user reported that the Phase 1 tree scan (structure only, no EXIF or hash)
slowed down badly after about one million photos. A basic file-tree scan should
stay fast.

Diagnosis: the tree walk and the per-row inserts are not the problem. The
progressive slowdown came from the 5-second sidebar refresh that runs during a
scan. Each tick recomputed two aggregates over the whole `photos` table:
`folder_photo_counts` (a `GROUP BY` over every row) and `new_photos_count` (a
join over recent photos with a per-candidate correlated prefix subquery). Both
are O(total photos). The per-batch `invalidate_count_cache` cleared the cache
before every reload, so both recomputed from scratch each tick. The
`tree_signature` skip-guard did not help, because new folders bump the folder
count on almost every tick during a scan.

Fixes:

- A (`src/ui/sidebar.rs`): during a scan (`state.scan.running()`), skip
  `folder_photo_counts` and `new_photos_count`. Folder rows show no photo count
  and New Files shows 0 until the scan ends. `node_label` now omits the ` (N)`
  suffix when the count map has no entry for a folder. At scan end the forced
  reload (`reload_folders_force`) runs with the scan no longer active and
  recomputes the real counts once. The reload during a scan is now O(folders).
- B1 (`src/db/library.rs`, `new_photos_count`): short-circuit to 0 when no root
  has finished its first scan (max `first_scan_done_at` is 0). During the
  initial import nothing is "new" yet, so this skips the heavy join and
  correlated subquery entirely.
- C3 (`src/db/library.rs`, `open_at`): raise `cache_size` from 64 MiB to 256 MiB
  on both connections, so the `photos.path` unique index (probed on every
  `ON CONFLICT(path)` insert) stays cached longer at 1M+ rows.
- C2 (`src/db/library.rs`, `checkpoint()`, and `src/ui/actions.rs`): add a
  best-effort `PRAGMA wal_checkpoint(TRUNCATE)` and call it between roots and on
  a 30-second timer during a long single-root scan. The UI read connection holds
  snapshots that block the automatic passive checkpoint, so without this the WAL
  grows without bound and every read pays a longer WAL scan.

Held for later (not done): C1, deferring or dropping the non-essential
`photos` indexes (`idx_photos_scan_state`, `idx_photos_added_at`,
`idx_photos_hash`) during Phase 1. It needs a schema/migration change and is the
riskiest. Revisit only if A + B1 + C2 + C3 do not flatten the curve.

Verify: run the scan with `-vvv` and watch the `sidebar.reload ... (folder_counts
..., new_photos_count ...)` debug line. `folder_counts` and `new_photos_count`
should now read near 0 during the scan (the queries are skipped), and the reload
time should stay flat across 100k -> 1M instead of climbing.

Note on testing: the app is a GTK4 desktop binary and needs a display, so an
agent running headless cannot exercise the real scan+reload path. The
authoritative check is the user's `-vvv` timing line before and after.

## 00000000000000000. Bound on-demand grid work to the true visible window

This section describes the last session. All work is complete, on `main`, and
pushed. One functional commit. CI version-bump commits sit between pushes.
Ignore those.

The previous session made enrichment "viewport-driven" by enqueuing from the
`GridView` factory `bind`. The user tested it: during a Phase 1 scan they opened
a 280-photo folder with a 3-wide, 6-tall viewport and did not scroll, yet all
280 photos enriched. A `-vvv` log showed the enrich worker draining the folder
in file order (001, 002, ... with no gaps), which proves the factory `bind`
fired for the whole folder, not only the ~18 visible cells.

Root cause: GTK's `GridView` binds far more cells than are visible. When a grid
is freshly populated and its viewport height is not yet allocated, GTK measures
by realising the whole model. So the `bind` signal is not a reliable "is
visible" test. `drop_enrich_request` on unbind could not help, because all ids
were bound and flushed inside the 180 ms debounce window.

Fix, in `src/ui/grid.rs`:
- Added `visible_index_range()`. It computes the on-screen store-index window
  from the scroller's vertical adjustment (`value`, `page_size`) and the cell
  geometry (columns from `grid_view.allocated_width()`, row height from the
  thumbnail size plus `CELL_SPACING`). It adds a margin of `VISIBLE_MARGIN_ROWS`
  (2) rows above and below. When the geometry is not ready (width or page size
  is 0, right after a folder opens), it returns `[0, VISIBLE_FALLBACK)` where
  `VISIBLE_FALLBACK` is 60, so the first screen still fills.
- Added `is_object_visible()` (find the object's index in the store, test the
  range) and gated `ensure_thumb_for` with it, so a full-model bind pass no
  longer queues a decode job for every photo.
- Gated the debounced enrich flush with `filter_ids_to_visible()`, which keeps
  only the buffered ids whose photo is in the visible window and discards the
  rest.
- Added a scroll-settle handler on the scroller's vertical adjustment
  (`schedule_visible_refresh` -> `refresh_visible_window`, debounced 200 ms).
  Because `bind` is no longer the authoritative visible signal, this is what
  fills thumbnails and enrichment for the window the user stops on.

New tuning constants in `src/ui/grid.rs`: `CELL_SPACING` (14 px, an estimate for
the inter-item gap plus caption row), `VISIBLE_MARGIN_ROWS` (2), and
`VISIBLE_FALLBACK` (60).

Notes and open items:
- The geometry is approximate (exact caption height and GridView spacing are not
  read). The 2-row margin and the 60-item fallback absorb the imprecision. Worst
  case a few extra rows do work.
- The one-time cost after a folder opens before the first allocation is up to
  60 photos (the fallback), not the whole folder. A later refinement could wait
  for the first allocation before doing any work, but 60 was the agreed value.
- A worklist-cancel on unbind (removing an already-enqueued off-screen id from
  `state.enrich_queue`) was considered and skipped: the flush gate already stops
  off-screen ids at the source, and front-of-queue ids run within about half a
  second.
- Verify on the real library: scan running, open the 280-photo folder, no
  scroll, expect only the visible window plus margin (or the 60 fallback) to
  enrich, then stop. Scroll and stop, and the new window fills.

## 0000000000000000. Viewport-driven enrichment and Tools "Generate Thumbnails"

This section describes the last session. All work is complete, on `main`, and
pushed. One functional commit. CI version-bump commits sit between pushes.
Ignore those.

The user reported a flood of "Reading photo info" while opening folders during
a Phase 1 scan. Opening four or five large folders read every photo in them.

Diagnosis: Phase 2 enrichment (thumbnails, EXIF, hash) is a worker pool that is
separate from thumbnail rendering. Opening a folder called
`enrich::prioritize_folder`, which queued every un-enriched photo in the folder.
Several other paths also started bulk enrichment on their own (startup, scan
end, reconcile, inotify, the Postpone setting). The grid does not need
enrichment to show a thumbnail: `thumb::get_edited` decodes an un-enriched photo
from its path on the fly, but does not persist it without a hash.

The user chose: enrichment follows the viewport, enrichment never starts on its
own, and a Tools button starts a deliberate, throttled, disposable full pass.

### 0000000000000000.1 Removed every automatic enrichment trigger

- `src/ui/app.rs`: removed `ensure_running` at startup and `prioritize_folder`
  on folder open.
- `src/ui/actions.rs`: the `ReloadAndEnrich` handler no longer starts
  enrichment at scan end.
- `src/ui/freshness.rs` and `src/ui/watcher.rs`: reconcile and inotify no longer
  enqueue enrichment (they still reload and auto-upload to Immich).
- `src/ui/settings.rs`: removed the "Postpone thumbnail scan" checkbox and its
  `ensure_running` call. `prefs::KEY_POSTPONE_THUMBS` is deprecated (kept inert).

### 0000000000000000.2 Viewport enrichment

- `src/ui/grid.rs`: added `on_enrich_request`, `set_on_enrich_request`,
  `request_enrich`, and `drop_enrich_request`. The factory `bind` calls
  `request_enrich(id)` when a bound cell's photo has an empty hash (not yet
  enriched). Ids collect in `enrich_buffer` and flush once, about 180 ms after
  the last realize burst, via `glib::timeout_add_local_once`. `unbind` drops a
  not-yet-flushed id.
- `src/ui/app.rs`: wired the callback to `enrich::enqueue_visible`.
- `src/ui/enrich.rs`: `enqueue_visible` pushes ids to the FRONT of the worklist
  (skipping ids already queued) and starts the pool. So opening a folder during
  a scan enriches only the on-screen cells, not the whole folder.

### 0000000000000000.3 Tools "Generate Thumbnails" (nice, disposable pass)

- `src/ui/enrich.rs`: `generate_all` seeds the worklist from
  `photos_needing_enrichment(None)` and starts the pool with `enrich_nice` set.
  `stop` cancels the pass and clears the worklist. `running` reports the state.
  The worker sleeps `NICE_SLEEP_MS` (40 ms) after each photo while `enrich_nice`
  is set, so the pass does not choke the system or a running Phase 1 scan.
- `src/ui/state.rs`: added `enrich_nice: Arc<AtomicBool>` and
  `gen_thumbs_action: RefCell<Option<gio::SimpleAction>>`.
- `src/ui/toolbar.rs`: added a stateful "Generate Thumbnails" toggle
  (`tools.gen_thumbs`) in the Library section. Off starts `generate_all`; on
  calls `stop`. The action is stored in `AppState` so the worker turns it off
  when the pass finishes on its own (`Msg::Finished` in `enrich.rs`).
- The pass is in memory only. Stopping the app discards it. Re-running resumes
  from the photos still needing enrichment, because `generate_all` re-seeds from
  the same query and enrichment is idempotent.

### 0000000000000000.4 Notes and open items

- The explicit per-scope actions still work: Settings "Scan Thumbnails Now"
  (per root, `enqueue_root`), and the folder right-click "Scan/Rescan all
  thumbnails" (`enqueue_folder` / `rescan_folder`).
- A folder never viewed, and never covered by a Generate Thumbnails pass, keeps
  un-enriched photos (no hash, no EXIF date, no phash). The duplicate finder
  backfills phash on demand for its scope, so that path still works.
- `AppState::pause_enrichment` is now unused (grid.rs pushes the shared
  `pause_until` directly). It is kept with `#[allow(dead_code)]`.
- Verify: during a Phase 1 scan, open several large folders without scrolling
  and confirm only an on-screen batch enriches. Then run Tools > Generate
  Thumbnails and confirm it is slow and does not choke, and that stopping the
  app discards it.

## 000000000000000. During-scan sidebar refresh performance

This section describes the last session. All work is complete, on `main`, and
pushed. One functional commit. CI's automatic version-bump commits sit between
pushes. Ignore those.

The user reported that the initial Phase 1 folder scan slowed down as it
progressed. At about 500000 files the tree-walk looked slower and slower.

Diagnosis: the walk speed was near constant. The cost was the periodic sidebar
refresh. During a scan the scan thread posts a `ReloadOnly` message on a timer.
The message rebuilt both sidebars. Each rebuild ran about twelve queries. Most
queries took the writer lock, so they queued behind the constant
`insert_structure_batch` writes. Each rebuild also replaced every root row in
the tree model and redid expansion, selection, and focus over the whole tree.
The cost grew with the folder and album count, so each refresh tick got heavier
as the database grew.

The fix has two phases.

### 000000000000000.1 Phase A — lock, throttle, and skip redundant work

- Routed the sidebar-reload read queries to the read-only connection
  (`self.lock()` to `self.read_lock()`): `folders` (`db/library.rs`), `albums`
  and `folder_albums` (`db/albums.rs`), `virtual_albums` (`db/virtual_albums.rs`),
  `persons` and `total_face_count` (`db/faces.rs`), `characters` and
  `total_style_face_count` (`db/style_faces.rs`), `immich_servers` and
  `linked_immich_folders` (`db/immich.rs`). A scan write no longer blocks a
  sidebar refresh, because WAL lets the read connection read while the writer
  commits.
- Raised the during-scan tree-refresh interval to 5 seconds
  (`SCAN_TREE_REFRESH` in `src/ui/actions.rs`).
- In the `ReloadOnly` handler (`src/ui/actions.rs`), skip
  `grid().reload_from_source()` while a scan runs and no folder is open. The
  initial import rarely writes the folder the user views.
- The count cache (`folder_photo_counts`, `new_photos_count`) already clears on
  a write and recomputes on the next read. With the 5 second throttle this
  recomputes at most once per refresh tick, not once per directory. No new
  dirty-flag struct was needed.

### 000000000000000.2 Phase B — do not rebuild the whole tree each tick

- Added `Library::tree_signature()` (`src/db/library.rs`). It returns the
  folder count and the album count as two `COUNT(*)` reads on the read
  connection.
- `Sidebar::reload` (`src/ui/sidebar.rs`) now holds `last_tree_signature`. While
  a scan runs, `reload` compares the live signature against the last rebuild and
  returns early when neither count grew. So an idle refresh tick does no
  `TreeData` build and no tree-model work.
- The root list is now spliced only where it changed. The old code did
  `list_root.splice(0, n, all_roots)`, which replaced every root row each tick.
  The new code compares the current root strings to the new ones and splices
  only the differing tail. During a scan new folders land as children of
  existing album roots, so the root list is unchanged and the splice does
  nothing.
- Because the root splice no longer recreates rows each tick, an expanded
  album's child `StringList` would not gain the folders filed under it during a
  scan. `refresh_expanded_children` (`src/ui/sidebar.rs`) walks the realized
  rows and, for each expanded row, splices its child list to match `child_ids`.
  It snapshots the rows first, then applies the splices, because a splice
  changes the flattened item count.
- The scan end must always refresh, even if the last tick left the counts
  unchanged. `app::reload_folders_force` and `Sidebar::invalidate_signature`
  clear the guard and force a full rebuild. The `ReloadAndEnrich` handler
  (`src/ui/actions.rs`), sent once after the scan drains, now calls the force
  path.

### 000000000000000.3 Accepted trade-offs and open items

- During a scan the sidebar updates less often (about every 5 seconds) and a
  folder's photo-count label may lag until a row rebinds or the scan ends. The
  user accepted this.
- A folder renamed on disk during a scan (same counts) may not refresh its
  label until the end-of-scan sweep. The end-of-root `sync_disk_tree` plus the
  forced final reload correct the tree when the scan finishes.
- `photos_in_virtual_album` and `virtual_album_photo_count` stay on the writer
  lock (multiple lock sites, low volume). A later change could move them.
- Verify on the real 500000-plus scan that the `sidebar.reload ... Xs` debug
  timing stays flat across the scan instead of climbing.

## 00000000000000. Large-library performance overhaul and five features

This section describes the last session. All work is complete, on `main`, and
pushed. The session made eight functional commits. CI's automatic
version-bump commits sit between pushes as usual. Ignore those.

The user reported that a 2-million-photo library made the application slow. The
user also asked for five features. The work fixed the slow paths first, then
added the features. The release build is clean. All tests pass (68 pass, 2
ignored).

### 00000000000000.1 Separate read connection and PRAGMA tuning

Problem: the scan thread and the UI thread shared one `Mutex<Connection>`. A
UI read waited behind an in-flight scan write. The log showed
`db lock: waited 2.6s` during a scan.

Fix, in `src/db/library.rs`:
- `Library` now holds a second connection, `read_conn: Mutex<Connection>`. It
  opens the same file read-only. WAL mode lets it read while the writer
  connection commits.
- A new `read_lock()` helper mirrors `lock()`. Hot UI read methods take
  `read_lock()`: `folder_photo_counts`, `photos_in_folder`, `new_photos_count`,
  `new_photos_grouped`, `missing_photo_count`, and the new duplicate-ban read
  methods.
- `open_at` sets writer PRAGMAs: `synchronous=NORMAL`, `busy_timeout=5000`,
  `cache_size=-65536`, `temp_store=MEMORY`, `mmap_size=268435456`,
  `wal_autocheckpoint=2000`. The read connection sets `query_only`, a
  busy_timeout, a cache, and mmap.

### 00000000000000.2 Scalar new-files count and a count cache

Problem: `new_photos_count` called `new_photos_grouped`, which loaded and
sorted every candidate row only to sum the lengths. It took 2.4 s. The startup
reloads two sidebars, so it ran twice.

Fix, in `src/db/library.rs`:
- `new_photos_count` is now a scalar SQL `COUNT`. A correlated subquery finds
  each photo's owning-root boundary (the max `first_scan_done_at` of any root
  whose path is a prefix of the folder path) and counts photos added strictly
  after a non-zero boundary. This mirrors `new_photos_grouped` without loading
  the rows.
- `Library` holds a `count_cache: Mutex<CountCache>`. It caches
  `folder_photo_counts` and the last `new_photos_count`. The two back-to-back
  sidebar reloads share one computation. `invalidate_count_cache` clears it.
  Every photo write path calls it: `insert_structure_batch`,
  `upsert_photo_structure`, `apply_reconcile_plan`, `delete_missing_photos`,
  `mark_first_scan_done`, and `delete_photo_hard`.

Note: the two startup sidebars still each call `reload`. The cache makes the
second call cheap. It does not remove the second call.

### 00000000000000.3 Demand-driven thumbnail grid

Problem: opening a non-enriched folder enqueued a thumbnail job for every photo
in the folder at once. On a huge folder this flooded the worker pool.

Fix, in `src/ui/grid.rs`:
- The grid enqueues a thumbnail job only when the `GridView` factory binds a
  cell. A bind covers the visible range plus the `GridView` overscan (one or
  two extra rows). The new method `ensure_thumb_for` serves the texture cache,
  then enqueues only if the cell has no texture and no pending job.
- `connect_unbind` calls `drop_pending_for` to forget an in-flight job for a
  cell that scrolled away (only when the cell holds no texture yet), so the
  worker result is discarded.
- `rebuild` and `set_photos_preserving` now only serve the texture cache. They
  do not enqueue jobs. The factory bind fills the rest on demand.
- `build_factory` now takes a `Weak<Grid>`. The real factory is installed in
  `into_rc`, after the `Rc<Grid>` exists, so the bind closure can reach the
  grid. `set_thumb_size` now takes `self: &Rc<Grid>`.

Caveat: a currently-visible cell whose hash changes mid-view (enrichment fills
the hash) does not re-enqueue until it re-binds on scroll. This is rare and
acceptable.

### 00000000000000.4 Scan resume cursor and startup resume popup

Problem: an interrupted first scan had no resume. A re-run re-walked the whole
tree and re-inserted every photo. The user also asked how to continue an
interrupted first scan, and warned that remaining folders must not wrongly land
in "New Files".

Fix:
- `src/db/library.rs` adds `folder_scan_cursor(path)`. It returns
  `(stored_mtime, is_done)` for a directory, joining `folders` with
  `scan_state`.
- `src/scan.rs` `scan_dir` now skips the folder upsert and the batch insert for
  a directory that is `Done` with an unchanged mtime. It still recurses into
  subdirectories. So a re-run continues fast over folders it already recorded.
- `src/db/library.rs` adds `interrupted_scan_roots()`. It lists roots with
  `first_scan_done_at = 0` that already hold at least one photo (a partial
  first scan). A never-scanned root with no photos is excluded.
- `src/ui/actions.rs` adds public `resume_scan(state, paths)`. It enqueues the
  partial roots.
- `src/ui/app.rs` `populate_deferred` shows a popup at startup when
  `interrupted_scan_roots()` is non-empty: "The initial scan of ... was
  interrupted. Resume now?". Yes calls `resume_scan`.

The boundary stays unset until a root's walk completes. So a resumed scan does
not flood "New Files". The scanner stamps `first_scan_done_at` only on a full
`Ok` completion (see `src/ui/actions.rs`).

### 00000000000000.5 Filename tooltip, Show Filenames toggle, extended sort

In `src/ui/grid.rs`:
- Every cell now has a filename tooltip.
- The cell is a vertical box: the thumbnail overlay on top and an optional
  filename caption below. The caption is hidden unless "Show Filenames" is on.
- New setting key `KEY_SHOW_FILENAMES` in `src/ui/prefs.rs`. Grid field
  `show_filenames: Cell<bool>`. Public methods `set_show_filenames` and
  `show_filenames`.
- The `SortOrder` enum grew from two variants to six: `DateDesc`, `DateAsc`,
  `NameAsc`, `NameDesc`, `SizeDesc`, `SizeAsc`. It is now `pub` with
  `from_setting`, `as_setting`, `dropdown_index`, and `from_dropdown_index`.
  `sort_photos` handles all six. Size sort uses `photo.size`.
- Public `set_sort_order(self: &Rc<Grid>, order)` persists the choice, syncs the
  header dropdown, and re-sorts. `sort_order_setting()` returns the current
  value string.

In `src/ui/toolbar.rs`, the Tools menu gained:
- A stateful "Show Filenames" toggle action (`tools.show_filenames`).
- A "Sort By" submenu with six items. A stateful string action (`tools.sort`)
  drives the grid and carries the active order as its state, so GTK draws the
  check mark. The header dropdown and the Tools submenu stay in sync.

### 00000000000000.6 Right-click folder thumbnail scan

In `src/ui/sidebar.rs`, the folder context menu gained two items:
- "Scan all thumbnails (unfinished)" calls `enrich::enqueue_folder`.
- "Rescan all thumbnails (all)" calls `enrich::rescan_folder`.

`src/ui/enrich.rs` adds `enqueue_folder` (photos still needing enrichment in the
folder) and `rescan_folder` (reset then re-queue all). `src/db/library.rs` adds
`reset_folder_enrichment(folder_id)`. It sets `scan_state = 0` for the folder's
non-missing photos and returns their ids, so the enrichment worker rebuilds
every thumbnail.

### 00000000000000.7 Banned matches (duplicate finder)

The user asked for a collection of "banned" matched photos, with an un-ban that
lets them match again.

Schema, in `src/db/schema.sql`: a new table `dup_bans(photo_a, photo_b,
banned_at, PRIMARY KEY(photo_a, photo_b))`. The pair is stored normalised (low
id first). A photo delete cascades the ban away.

Engine, in `src/dedup.rs`: `find_duplicates` now takes a
`banned: &HashSet<(i64, i64)>`. It skips a union for a banned pair in both the
exact pass and the near pass. Helper `norm_pair(a, b)` gives the normalised key.

Caveat: the grouping is transitive union-find. A banned pair is not unioned
directly, but a third photo that matches both can still bridge them. This is
rare and acceptable for now.

DB access, in `src/db/duplicates.rs`: `ban_dup_pair`, `unban_dup_pair`,
`banned_dup_pairs` (the set for the engine), `banned_dup_photo_pairs` (both
photos per pair, for the view), `banned_dup_count`, and `clear_all_dup_bans`.

UI review, in `src/ui/grid.rs` and `src/ui/dedup_scan.rs`: the duplicate action
bar gained a "Not duplicates" button. It bans each marked photo against its
group's kept copy (`marked_ban_pairs`, `on_dup_ban`, `set_on_dup_ban`). The
scan loads `banned_dup_pairs` and passes them to the engine.

Sidebar, in `src/ui/sidebar.rs`: a new leaf section "Banned Matches"
(`BANNED_MATCHES_ID`). It follows the "Missing Files" leaf template: an id
constant, a `TreeData` count field `banned_matches_count`, a `reload` push when
the count is over zero, a `node_label` branch, an `on_selection_changed`
dispatch to `AppState::show_banned_matches`, and a right-click "Clear Banned
Matches…" action (`clear-banned` -> `clear_banned_matches` ->
`clear_all_dup_bans`). `AppState::show_banned_matches` (in `src/ui/state.rs`)
shows the banned photos in the grid.

### 00000000000000.8 Open items

- The interrupted-scan continuation runs through the scan queue (`resume_scan`),
  not "Refresh Library". "Refresh Library" is the disk reconcile
  (`freshness::reconcile_now`), which is a no-op while a root's first scan is
  unfinished. The startup popup is the intended resume path.
- The banned-pair transitive-bridge caveat (section 00000000000000.7) stands.
- The two startup sidebars still each reload. The count cache makes the second
  cheap. A later change could dedupe the second reload.
- Test the demand-driven thumbnails on the real 2-million-photo library on the
  user's machine, during fast scroll.

## 0000000000000. Scan-tree responsiveness: postpone thumbnails, live tree growth, and a focus/click fix

This section describes the last session. All work is complete, on `main`,
and pushed, except where noted. Four fix commits plus one feature commit;
CI's automatic version-bump commits sit between them as usual (see
section 000000000000.6 in the next section — ignore those).

### 0000000000000.1 Added a "postpone thumbnail scan" option

User request: on a large or slow library root, the full two-phase import
(structure scan, then EXIF/hash/thumbnail enrichment) could run for hours
before it was needed. Added, in Settings → Library Folders: a checkbox
"Postpone thumbnail scan until a folder is opened" (default off) and a
"Scan Thumbnails Now" button with a live pending-count label for the
selected root.

When on, enrichment only ever runs for the one folder the user opens
(`enrich::prioritize_folder`, already existed for on-demand priority) — bulk
enrichment (`ensure_running`/`enqueue`) is gated off by a new `postponed()`
check. The button bypasses the gate explicitly via a new
`enrich::enqueue_root`. New DB method: `Library::photos_needing_enrichment_under`
(`src/db/library.rs`, matches by path prefix like `remove_library_folder`
does). New setting key `KEY_POSTPONE_THUMBS` (`src/ui/prefs.rs`). Commit
`4be26e3`.

### 0000000000000.2 The structure scan now populates the Library tree live, not only after it finishes

Two follow-up bug reports drove this. First: even with live status text, the
sidebar tree itself only grew once a whole root finished scanning. Cause:
`Scanner::scan_folder` (`src/scan.rs`) ran two full sequential passes over
the *entire* root — a discovery pass that walked the whole tree into an
in-memory map before writing anything, then a recording pass that inserted
each directory's photos and filed it into the tree. On a large or slow root,
discovery alone could take most of the scan, during which the tree was
frozen.

Fix: merged discovery and recording into one recursive walk (`scan_dir`,
replacing the old two-pass body and the now-deleted free function
`collect_images`). Each directory's photos are written and filed into the
tree the instant that directory is visited, so the Library sidebar now grows
live throughout the scan. Trade-off (confirmed with the user): the Phase 1
progress bar is gone, since the total photo count is no longer known ahead
of time — the status bar instead shows a running count, e.g. "Scanning
`/mnt/nas/2019/summer` (4,213 found)". Phase 2 enrichment's own progress bar
is unaffected; it always computed its total from a DB query, not a
pre-walk.

An earlier, smaller attempt at the same underlying problem (add a live
status message during a still-separate discovery pass, commit `69de6aa`) is
superseded by this merge and no longer exists as separate code — mentioned
here only so its commit hash isn't a mystery in `git log`. Final
architecture: commit `cf40433`.

### 0000000000000.3 Fixed the Library tree stealing focus and swallowing clicks during a scan

Once the tree started refreshing live (previous item), the user reported
keyboard focus getting stolen while navigating the tree during a scan. A
first attempt (commit `16e0576`) captured and restored the selected row ids
and whether the tree had keyboard focus, around `Sidebar::reload()`'s
teardown/rebuild (`list_root.splice` replaces every root row with a fresh
GObject, which is what destroys the focused/selected row in the first
place). **This attempt did not work** — the user reported focus was still
stolen, and, new symptom, clicking a folder to view its thumbnails now did
nothing at all (no freeze, no error).

Two Explore agents ran down both root causes precisely (see the plan file
this session used, now stale/reusable:
`~/.claude/plans/i-want-to-adjust-quizzical-hellman.md`, if it still exists):

- **Reloads fired too often.** The scan's `on_folder` callback
  (`src/ui/actions.rs`) sent `Msg::ReloadOnly` every 4 folders discovered —
  on a fast disk, often enough to fire multiple times per second. A GTK
  mouse click is a press-then-release gesture resolved against the widget it
  started on; a reload tearing down that row *between* press and release
  silently drops the click. This is the primary cause of both symptoms.
- **`grab_focus()` targets the wrong thing.** Confirmed by reading the
  vendored gtk4-rs 0.7.3 source: the correct per-row-by-position focus API,
  `ListView::scroll_to(pos, ListScrollFlags::FOCUS, ..)`, is gated behind
  `#[cfg(feature = "v4_12")]` — unavailable under this project's `v4_10` pin
  (`Cargo.toml`: "Do not upgrade past the GLib version Debian 13 ships").
  `self.list_view.grab_focus()` instead moves focus to GTK's own internal
  list-focus-position tracker, invalidated by the splice — not to the row
  just reselected.

Fix (commit `434bdf1`):
- `src/ui/actions.rs`: replaced the per-4-folders counter with a wall-clock
  throttle, `SCAN_TREE_REFRESH = Duration::from_millis(750)` — comfortably
  longer than any click or keypress, so a reload should essentially never
  land mid-gesture again.
- `src/ui/sidebar.rs`: added `pending_focus_id: RefCell<Option<String>>`.
  `reload()` records the previously-focused row's id into it (instead of
  calling `grab_focus()` on the container); `bind_row` grabs focus itself
  (`expander.grab_focus()`) the moment it binds a widget to that id — `bind`
  fires synchronously for visible rows as part of the model-change signal,
  which a same-frame `grab_focus()` call cannot rely on — then clears the
  pending id so it only fires once.

### 0000000000000.4 Note: clicking a top-level Album still does nothing (separate, pre-existing gap)

Investigated, then explicitly ruled out of scope for this session after
asking the user directly: `Sidebar::on_selection_changed`
(`src/ui/sidebar.rs`) has no `album_id_of` branch at all. Selecting a real
Album row (a user-created album, as opposed to a scanned folder) has never
shown anything, in any version, scanning or not — a permanent gap, not a
scan-specific regression. The intended UX (README.md) is to expand an Album
and click its child folders individually; a combined "click an album, see
every photo under it" view was never built. `Library::folders_under_album`
(`src/db/albums.rs`) already returns the right recursive folder set if this
is ever built — it is just not wired to the grid. Not touched this session.

### 0000000000000.5 graphify knowledge graph added to the repo (uncommitted)

Ran `/graphify .` this session: built a 1756-node / 4298-edge knowledge
graph of the codebase into `graphify-out/` (`graph.json`, `graph.html`,
`GRAPH_REPORT.md`). As part of its own automation, the skill also added a
"## graphify" section to `AGENTS.md` and created `CLAUDE.md`,
`.claude/settings.json`, and `.opencode/`. **None of this is committed.**
It is unrelated to this session's actual code changes and was left
untouched for review, not reverted and not committed — decide either way
next session.

### 0000000000000.6 Files changed this session

- `src/ui/prefs.rs` — `KEY_POSTPONE_THUMBS`.
- `src/db/library.rs` — `photos_needing_enrichment_under` + test.
- `src/ui/enrich.rs` — `postponed()` gate on `ensure_running`/`enqueue`,
  `enqueue_root()`, adjusted `prioritize_folder` browse-pause logic.
- `src/ui/settings.rs` — the postpone checkbox and "Scan Thumbnails Now" row.
- `README.md` — documented the postpone option.
- `src/scan.rs` — merged `collect_images` and the two-pass `scan_folder`
  body into one recursive `scan_dir` walk; deleted the now-unused `Progress`
  struct.
- `src/ui/actions.rs` — matching simplification of the scan-progress
  bookkeeping, plus the `SCAN_TREE_REFRESH` throttle.
- `src/ui/sidebar.rs` — `suppress_selection_notify`, `pending_focus_id`, and
  the focus/selection restore logic in `reload()` and `bind_row`.
- Not committed (see 0000000000000.5): `AGENTS.md`, `CLAUDE.md`, `.claude/`,
  `.opencode/`, `graphify-out/`.

### 0000000000000.7 Verification

`cargo build` clean after every commit (54 warnings throughout — the same
pre-session baseline; no new warnings introduced, one accidental new warning
caught and fixed before committing). `cargo test`: 67 passed, 2 ignored,
after every commit.

**Not verified: any of the actual GUI behavior.** This working environment
has no display, so the live tree growth, the running-count status text, and
the focus/click fix could only be verified by reading code and reasoning
about GTK's documented behavior, not by running the app. The next session
should confirm, against a real scan of a large or slow library root: the
tree visibly grows folder-by-folder during the scan; clicking folders in the
tree reliably opens them while a scan is running; and keyboard arrow-key
navigation of the tree keeps its position and focus across a scan's
background refreshes instead of resetting every ~750ms.

---

## 000000000000. GitHub release prep, the Unlicense switch, and v0.1.0

This section describes an earlier session. All work is complete, on `main`,
and pushed. Named release `v0.1.0` is live on Gitea and on GitHub.

### 000000000000.1 Removed an accidentally committed session transcript

Commit `54e81f4` had swept in `session-ses_fb61.md`, a 4931-line AI session
transcript with private data (internal host name, local shell prompt, raw
chat). Commit `93ff3fb` deletes the file and adds `session-*.md` to
`.gitignore`.

### 000000000000.2 GitHub mirror now pushes a filtered snapshot, not full history

`.gitea/workflows/release.yaml` used to run `git push github HEAD:main`,
which would have sent the full Gitea history — including `AGENTS.md`,
`HANDOFF.md`, and `.gitea/` — to GitHub. Commit `61cd375` changes the "Push
code and tag to GitHub" step (now "Push filtered snapshot to GitHub"): it
builds one fresh commit from the current tree with those paths removed, and
force-pushes only that commit to GitHub's `main` and the release tag.
Gitea's own history is untouched.

### 000000000000.3 Added a LICENSE file and updated the README

Commit `9505416`: adds `LICENSE`, documents the Immich integration and the
People/Characters split in `README.md` (both existed in code but were
undocumented), replaces the internal-Gitea-CI mention with a GitHub-facing
"Releases" section, and rewords `ROADMAP.md` line 367 ("the Gitea runner" →
"the CI runner").

### 000000000000.4 Switched the license from MIT to the Unlicense

User request. Commit `7ef615d`: `Cargo.toml` `license` is now
`"Unlicense"`; `LICENSE` holds the Unlicense text plus a note that it
covers only pichouse's own code. Downloaded models keep their own licenses
(YuNet MIT, SFace Apache 2.0, the anime detector MIT, CCIP CaFormer
OpenRAIL-M) — listed in the README "License" section too.

### 000000000000.5 Cut the first named release, v0.1.0

Followed AGENTS.md RULE THREE: pushed `main`, then `git tag v0.1.0` and
`git push origin v0.1.0`. Verified after the fact with the GitHub API and
`git ls-remote`: the GitHub release holds the correct binary and file tree.

### 000000000000.6 Note: a rolling pre-release can show a different version than a named release

Gitea's `rolling` release (from `build.yaml`, rebuilt on every push to
`main`) and a named release (from `release.yaml`, on a tag push) can finish
minutes apart with different version numbers if both trigger from the same
push. This is expected, not a bug — do not re-diagnose it.

### 000000000000.7 Files changed this session

- `session-ses_fb61.md` — deleted.
- `.gitignore` — added rule `session-*.md`.
- `LICENSE` — added, then rewritten for the Unlicense.
- `Cargo.toml` — `license` set to `"Unlicense"`. Version now `0.1.0`.
- `README.md` — Immich section, People/Characters section, License section,
  rewritten Releases section.
- `ROADMAP.md` — one wording fix.
- `.gitea/workflows/release.yaml` — GitHub push step now builds a filtered
  snapshot.
- `Cargo.lock` — version synced by CI.

### 000000000000.8 Verification

`cargo build` clean (pre-existing warnings only). Filtered-snapshot logic
dry-run before the real release. GitHub API confirmed the release, its
binary asset, and its file tree after the real release ran.

---

## 00000000000. Context-menu and character-view fixes

This section describes an earlier session. All work is complete, on `main`,
and pushed. Commit `5ab1015` holds all four fixes below.

### 00000000000.1 Grid right-click menu was greyed out

Cause: `src/ui/vmenu.rs` installed the `grid` action group on
`grid.grid_view()`, but the popover parents to `grid.widget()` — GTK
resolves menu actions by walking up from the popover parent, never down, so
it found no `grid.*` action. Fix: install the action group on
`grid.widget()` instead.

### 00000000000.2 New "Remove from…" items in a face-group menu

Added `Library::remove_photo_from_person` / `remove_photo_from_cluster`
(`src/db/faces.rs`), `Grid::current_person` / `current_cluster`
(`src/ui/grid.rs`), and two menu actions/items in `src/ui/vmenu.rs`
("Remove from this person" / "Remove from this group").

### 00000000000.3 Character-view "Do not scan" now honors the selection

Cause: the right-click `skip` action in `src/ui/charactersview.rs` captured
only the clicked tile's id. Fix: factored the id-gather logic into
`skip_keys(&[TileKey])`, used by both the button and the menu; the label is
now "Do not scan selected" in both places.

### 00000000000.4 Merge dialog pre-selects the last-merged character

Added `last_merged_character: RefCell<Option<i64>>` to `AppState`
(`src/ui/state.rs`); `characters.rs`'s merge dialog reads and writes it.
In-memory only, resets on restart.

### 00000000000.5 Files changed this session

`src/db/faces.rs`, `src/ui/grid.rs`, `src/ui/vmenu.rs`,
`src/ui/charactersview.rs`, `src/ui/state.rs`, `src/ui/app.rs`,
`src/ui/characters.rs`.

### 00000000000.6 Verification

`cargo build` clean. `cargo test`: 66 passed, 2 ignored.

### 00000000000.7 Open follow-ups (not built)

- The face-group "Remove from…" items are a plain remove, not a ban — a
  removed photo may re-group later. A durable per-person/cluster ban (like
  the character ban) would need a new rejection path in the grid menu.
- `last_merged_character` is transient. To persist across restarts, store it
  via `Library::set_setting` (see `src/ui/prefs.rs`).

## 0000000000. Named-release workflow with GitHub mirror

Added `.gitea/workflows/release.yaml`: a tag push matching `v*` syncs the
version into `Cargo.toml`, builds, tests, publishes a Gitea release, and
mirrors the code and release to GitHub (`ScuttleSE/pichouse`). Needs a
`GH_TOKEN` secret (GitHub PAT, `contents: write`) in the Gitea repo
settings. Full process is in AGENTS.md RULE THREE — do not push a tag
without the user's request.

(Superseded: the GitHub push step now builds a filtered snapshot instead of
mirroring full history — see section 000000000000.2.)

## 000000000. Four fixes

This section describes an earlier session. All work is complete and pushed.
Each fix is a separate commit.

### 000000000.1 Sidebar right-click crash during a scan

Cause: `show_row_menu` (`src/ui/sidebar.rs`) parented the row popover to the
per-row `TreeExpander`, which a concurrent reload could destroy, leaving a
dangling popover. Fix: parent to the `ListView` instead, translating the
click point across.

### 000000000.2 Album multi-drag moved only one album

Cause: the album drop path in `attach_row_drag` (`src/ui/sidebar.rs`) moved
only the dragged album, unlike the folder path. Fix: added
`selected_album_ids` and `reparent_albums`, mirroring the folder path.

### 000000000.3 UI freeze during a large face or art scan

Cause: `recluster` (`src/ui/stylefacescan.rs`, `src/ui/facescan.rs`) wrote
one DB transaction per face and could run concurrently across workers. Fix:
batch writers `set_face_clusters` / `set_style_face_clusters`
(`src/db/faces.rs`, `src/db/style_faces.rs`) plus an `AtomicBool` guard so
only one worker reclusters at a time.

### 000000000.4 Smaller release binary

Added `strip = true`, `lto = "thin"`, `codegen-units = 1` to
`[profile.release]` in `Cargo.toml` (documented there). ~23 MB → ~14 MB.

### 000000000.5 Files changed this session

`src/ui/sidebar.rs`, `src/db/faces.rs`, `src/db/style_faces.rs`,
`src/ui/facescan.rs`, `src/ui/stylefacescan.rs`, `Cargo.toml`.

### 000000000.6 Open follow-ups (not built)

- The progressive recluster still re-reads every face in the library every
  20 photos (O(N), grows with the library). A future agent could recluster
  only new faces.

## 00000000. CCIP embedder and face features

This section describes an earlier session. All work is complete and pushed.
`cargo test`: 66 passed, 2 ignored.

### 00000000.1 Stylised embedder swapped from DINOv2 to CCIP

Cause: DINOv2's CLS token encoded art style, not character identity, so two
different characters in the same style clustered together. CCIP
(`deepghs/ccip_onnx`, CaFormer-24 variant) is trained for anime character
re-identification instead.

Pinned model: repo `deepghs/ccip_onnx`, commit
`eb2acdd29af1703388d3d0c04221add322bc9110`, file
`ccip-caformer-24-randaug-pruned/model_feat.onnx` (~150 MB), SHA-256
`4ea118d16496274f4f6e08d3afc768cc592389e8f7f32f8732ce2215c228ac5f`. Input
`input` NCHW `[N,3,384,384]`, output `output` 768-D.

Preprocessing (`src/styleface/embedder.rs`): resize to 384x384 bilinear,
scale to 0..1, normalize with mean `(0.48145466, 0.4578275, 0.40821073)` /
std `(0.26862954, 0.26130258, 0.27577711)`, take the full 768-value output,
L2-normalize. Detector-box margin changed 0.25 → 0.10. Catalog default is
`ccip_caformer_24` (`src/styleface/models.rs`); `CHARACTER_JOIN_MAX_DIST`
changed 0.35 → 0.20 (`src/styleface/cluster.rs`) — a starting point, may
need tuning after real-world testing.

**Open task:** the user must test the CCIP result on a real library and
tune `CHARACTER_JOIN_MAX_DIST` if characters still merge or over-split. If
tuning is not enough, add CCIP's learned-metric distance
(`model_metrics.onnx`).

### 00000000.2 Model switch clears old stylised face data

A change of embedding model changes vector dimension (384 DINOv2 → 768
CCIP), so `download_models` (`src/ui/stylefacescan.rs`) calls
`lib.delete_all_style_face_data()` when the stored model id changes.

### 00000000.3 Download progress bar

Shared helper `read_with_progress` (`src/styleface/models.rs`) reads in 64
KB chunks and reports a fraction via `Content-Length`. Used by
`ensure_model_progress` (styleface and face) and
`face::runtime::ensure_runtime_progress`. Old no-progress functions
delegate to these with a no-op callback.

### 00000000.4 Viewer draws stylised character boxes

The viewer previously queried only the human `faces`/`persons` tables.
`Grid::is_style_source` (`src/ui/grid.rs`) flags a Character/StyleCluster
source; `Viewer::load_faces` (`src/ui/viewer.rs`) then reads
`style_faces_for_photo` instead. A box click opens
`characters::assign_style_face_dialog` or `people::assign_face_dialog`
depending on mode.

### 00000000.5 Un-match and ban one stylised face

`characters::assign_style_face_dialog` mirrors
`people::assign_face_dialog`: remove-and-ban (records a durable rejection
via `reject_style_face_from_character`), reassign, or make a new character.

### 00000000.6 Delete and ban a whole group

`Library::delete_person_and_ban` (`src/db/faces.rs`) and
`delete_character_and_ban` (`src/db/style_faces.rs`) reject every member
face, then delete the group. Sidebar menu items: "Delete and Ban Person" /
"Delete and Ban Character" (`src/ui/sidebar.rs`).

**Limit:** the ban stops re-grouping under that person/character, but does
not stop the detector from finding the face again — there is no per-photo
"do not detect" flag in the schema. A true per-photo ban needs a new
column, excluded in `photos_needing_face_scan` /
`photos_needing_style_face_scan`.

### 00000000.7 Missing Files sidebar section

New "Missing Files (N)" sidebar leaf, mirroring "New Files"
(`src/ui/sidebar.rs`, `AppState::show_missing_files` in
`src/ui/state.rs`, `Library::photos_missing`). "Clear Missing Files…"
hard-deletes the rows via `delete_missing_photos` (does not touch disk —
the files are already gone).

### 00000000.8 Grid context-menu clipped and scrolled

Cause: `src/ui/vmenu.rs` parented the popover to the `GridView`, which sits
inside a `ScrolledWindow` that clips it. Fix: parent to the grid root box
instead, translating the click point; the menu opens upward when clicked in
the lower half.

### 00000000.9 Files changed this session

`src/styleface/models.rs`, `src/styleface/embedder.rs`,
`src/styleface/cluster.rs`, `src/styleface/mod.rs`, `src/face/models.rs`,
`src/face/runtime.rs`, `src/db/faces.rs`, `src/db/style_faces.rs`,
`src/db/library.rs`, `src/db/schema.sql`, `src/ui/facescan.rs`,
`src/ui/stylefacescan.rs`, `src/ui/viewer.rs`, `src/ui/characters.rs`,
`src/ui/grid.rs`, `src/ui/state.rs`, `src/ui/sidebar.rs`,
`src/ui/vmenu.rs`, `AGENTS.md`.

## 0000000. Duplicate image finder

This section describes an earlier session. Work is complete and pushed.

The finder (toolbar Tools → Find Duplicates…) finds exact duplicates (SHA-256
match, `photos.hash`) and near duplicates (64-bit dHash, Hamming distance
under a threshold slider).

### 0000000.1 Perceptual hash

`src/phash.rs`: `dhash_rgb` reduces to a 9x8 grayscale grid, one bit per
adjacent-column brightness compare (64 bits total). Stored in
`photos.phash` (INTEGER, bit-cast u64↔i64 since SQLite has no unsigned
type; `0` means not yet computed). Computed during Phase 2 enrichment
(`enrich_file_with_image` in `src/scan.rs`) from the same decoded pixels
used for the thumbnail; existing photos are backfilled on first scan.

### 0000000.2 Grouping engine

`src/dedup.rs`: `find_duplicates(photos, threshold, cancel)` runs a
union-find over two passes (exact hash match, then near dHash match within
`threshold` — O(n²), scoped to one album/folder set). `choose_keep` ranks
copies: larger pixel area, then more lossless format, then larger file,
then older, then lower id (a total order, so ties break consistently).

### 0000000.3 Database layer

`src/db/duplicates.rs`: `photos_in_folders`, `set_photo_phash`,
`delete_photo_hard` (deletes the row first, then the file; a `NotFound`
file error is ignored). `photos.phash` added via `migrate`
(`src/db/library.rs`), new index `idx_photos_hash`.

### 0000000.4 UI and result view

`src/ui/actions.rs` (`find_duplicates` dialog: scope + similarity slider) →
`src/ui/dedup_scan.rs` (background scan, mirrors the `aitag.rs` worker
pattern, cancellable via `AppState::dedup_job`) → `Grid::show_duplicates`.

The result view is a custom layout, not the normal `GridView`, because one
box must wrap around each whole group: `Grid` gained `dup_container`,
`dup_state`, and a duplicate action bar. Each group is a `gtk4::Frame` of a
`FlowBox` of thumbnails; each thumbnail has its own `GestureClick` (a plain
`connect_selection_changed` does not fire on a click of an
already-selected cell). "Delete marked" calls `delete_photo_hard` per
marked photo. `exit_dup_mode` restores the normal `GridView`; every normal
view loader calls it via `set_photos`.

### 0000000.5 Deferred

- RAW+JPEG pairing: not done — the scanner does not scan RAW files yet.
- The near pass is O(n²) — a prefilter bucket (e.g. by dHash high bits)
  would help a whole-library scan on a very large library.

## 000000. Stylised face recognition and album Face type

This section describes an earlier session. Work is complete and pushed.
`cargo test`: 60 passed, 2 ignored.

Added a second face system for stylised art (anime/cartoon/furry, in
`src/styleface/`, mirroring `src/face/`), plus a per-album Face type
(Inherit/Photo/Art) that routes scanning to the right system. See
ROADMAP.md "Facial detection & recognition" for the feature summary.

### 000000.1 Why two systems, not one

SFace (human embedder) gives poor vectors on drawings; DINOv2 (the original
stylised embedder, later replaced by CCIP — section 00000000.1) is not
tuned for photographic identity. Different vector dimensions (128 vs
384/768) rule out a shared cluster pool. Decision: keep them separate;
only the ONNX Runtime loader (`src/face/runtime.rs`) is shared. HDBSCAN is
used for stylised faces only — SFace's calibrated cosine threshold works
better for photos.

### 000000.2 Stylised models (original pins, since replaced — see 00000000.1)

Anime YOLOv8-nano detector (deepghs, MIT, 12 MB). Embedder was DINOv2
ViT-S/14 at the time (later replaced by CCIP). Catalog also lists a larger
detector and a smaller fp16 embedder as untested alternatives — the fp16
entry needs f16 tensor handling before use (`embedder.rs` currently expects
f32 only).

### 000000.3 Inference

`src/styleface/detector.rs`: YOLOv8, letterboxed 640x640, NMS, per-mille
boxes (no landmarks). `src/styleface/embedder.rs`: box enlarged 25%,
resized square, ImageNet-normalized. Coordinate convention matches the
human system: per-mille of the photo after `photos.orientation` rotation.

### 000000.4 Clustering (HDBSCAN)

`src/styleface/cluster.rs`, `hdbscan` crate, L2-normalized vectors,
euclidean, `min_cluster_size = 2`, `min_samples = 1`. Noise faces get
`cluster_id = -1` (shown as "Unclear"). A named character anchors a stable
cluster id (`CHARACTER_CLUSTER_BASE + character_id`); an unnamed face near
a named character joins it before HDBSCAN runs
(`CHARACTER_JOIN_MAX_DIST`). Rejections are honoured.

### 000000.5 Storage

`characters`, `style_faces`, `style_face_scan`, `style_face_rejections` in
`src/db/schema.sql`, mirroring the human tables (no landmarks column).
`src/db/style_faces.rs` mirrors `src/db/faces.rs`. Crops live in
`style-face-thumbs.db`.

### 000000.6 UI

Mirrors the human "People" UI with "character" vocabulary:
`src/ui/settings_characters.rs`, `src/ui/stylefacescan.rs`,
`src/ui/charactersview.rs`, `src/ui/characters.rs`, plus `Source::Character`
/ `Source::StyleCluster` in `src/ui/grid.rs`.

### 000000.7 Album Face type (Photo / Art)

Three-state (`AlbumKind`: Inherit/Photo/Art), inherits down the album tree,
defaults to Photo at the top. `Library::album_effective_kind` resolves it;
`photo_effective_face_kind` gives a photo's routed kind.
`albumscan::scan_album_faces` and `autoscan_routed` dispatch to the human
or stylised pipeline accordingly, so a photo is scanned by exactly one
method. Sidebar: a `(Photo)`/`(Art)` suffix on explicit (non-inherited)
kinds, plus a "Face type" submenu and Scan/Rescan actions.

### 000000.8 Open follow-ups (not built)

- No automatic art-vs-photo classifier — album kind is the only routing
  signal.
- Sidebar badge shows an album's own explicit kind, not the inherited
  effective kind.
- No per-face reject/reassign UI for stylised faces yet (backend exists:
  `reject_style_face_from_character`, `recluster_now`, both
  `#[allow(dead_code)]`).
- Roadmap phases 3–5 for stylised faces (incremental assign, merge
  suggestions, a learned metric) are not built.

## 00000. Facial detection and recognition

This section describes an earlier session (the human face system; section
000000's stylised system mirrors this design). Work is complete and pushed.
`cargo test`: 55 passed, 2 ignored, at the time.

Detects faces, groups the same person across the library, lets the user
name people, shows each person's photos. All processing stays local. See
ROADMAP.md "Facial detection & recognition".

### 00000.1 Backend

`ort` (ONNX Runtime bindings), `load-dynamic` feature, pinned to
`=2.0.0-rc.10` (needs ONNX Runtime 1.22.x — later rc's broke the `ureq` TLS
feature or needed system OpenSSL, which this project avoids). The runtime
library is not built in or committed — it downloads to
`~/.local/share/pichouse/runtime/` on first use, with a verified SHA-256
(`src/face/runtime.rs`). Do not upgrade `ort` without checking the ABI
version, and do not add the `download-binaries` feature.

### 00000.2 Models

YuNet (detector, MIT) and SFace (embedding, 128-D, Apache 2.0), OpenCV Zoo,
pinned commit `47534e2` with verified SHA-256 (`src/face/models.rs`).
ArcFace (512-D, non-commercial) and a custom-path option are ROADMAP
follow-ups, not built.

### 00000.3 Inference

`src/face/detector.rs`: YuNet, 640x640 BGR NCHW, decodes 12 raw heads
(cls/obj/bbox/kps at strides 8/16/32), NMS, per-mille boxes + 5 landmarks.
`src/face/embedder.rs`: SFace, Umeyama-aligned 112x112, L2-normalized
128-D. Verified numbers: detection score 0.946, same-person cosine 0.67,
different-person 0.06 (`src/face/inference_test.rs`, ignored test).

### 00000.4 Coordinate rule (important)

A face box/landmarks are per-mille of the photo AFTER `photos.orientation`
rotation and BEFORE any non-destructive edit. Every stored box, crop, and
overlay uses this same space — `Photo` has no separate EXIF orientation
field, so this is a fixed convention.

### 00000.5 Storage

`persons`, `faces`, `face_scan`, `face_rejections` in `src/db/schema.sql`;
`photos.face_status` via `migrate`. `src/db/faces.rs` holds all access.
Embeddings pack as little-endian f32 blobs. Crops live in `face-thumbs.db`.

### 00000.6 Clustering

`src/face/cluster.rs`: cosine similarity, default threshold 0.363 (the
SFace value). A named person anchors a stable cluster id
(`PERSON_CLUSTER_BASE + person_id`). Rejected person ids are tracked per
face so a correction is durable across re-clusters.

### 00000.7 Background scan

`src/ui/facescan.rs` mirrors the AI-tagging pattern (channel, coordinator,
worker pool, `Controller` cancel flag). Progressive: re-clusters and
refreshes every 20 photos. `download_models` fetches runtime + models in
the background. Opt-in auto-scan after reconcile via `face.autoscan`
(off by default).

### 00000.8 UI

`src/ui/settings_faces.rs` (enable, auto-scan, model choice, download,
scan, delete-all), `src/ui/facesview.rs` (one tile per group),
`Source::Person`/`Source::Cluster` in `src/ui/grid.rs`,
`src/ui/people.rs` (name/merge/assign dialogs), a face-box overlay toggle
in `src/ui/viewer.rs`, and a "Contains person" virtual-album rule
(`RuleField::Person`).

### 00000.9 Rejection design decision

Removal from a group is offered only for a NAMED person — an unnamed group
has no stable identity across scans, so there is nothing durable to record
a rejection against. Workflow: name the group, then remove outliers.

### 00000.10 Open follow-ups (not built)

- A cache-only clear for face crops (today only "Delete all face data"
  clears them).
- Higher-accuracy optional models (ArcFace) and a custom `.onnx` path.
- Immich has its own people feature; mapping to it is out of scope for now.
- A split control for a face inside an unnamed group (today the user names
  the group first).

## 0000. Timeline, copy, crop overlay, slideshows, logging, and freeze fixes

This section describes an earlier session. Work is complete and pushed.
`cargo test`: 43 passed.

### 0000.1 Four small features

1. **Immich Timeline** — a Timeline child node per server, all assets
   newest-first. `Client::timeline_assets` (`src/immich/client.rs`),
   `immich::show_timeline` (`src/ui/immich.rs`).
2. **Copy image to clipboard** — grid menu item bakes the full-resolution
   edited image and sets it via `gdk::MemoryTexture`
   (`src/ui/vmenu.rs`: `copy_photo_to_clipboard`, `bake_source`). Works for
   local and Immich photos.
3. **Interactive crop overlay** — drag a rectangle on the viewer image; on
   release it converts to a per-mille crop (`src/ui/viewer.rs`:
   `set_crop_mode`, `image_rect`, `update_crop_from_drag`;
   `src/ui/editor.rs`: `build_crop`).
4. **Slideshows** — toolbar Play button, per-image duration/shuffle/loop
   settings persisted in `library.db` (`src/ui/prefs.rs`), runs fullscreen
   in the viewer on a `glib::timeout_add_seconds_local` timer.

### 0000.2 Console logging with CLI flags

`log` + `env_logger` (stderr). `src/main.rs` parses `-v`/`-vv`/`-vvv`/
`-q`/`--quiet`/`-h` before GTK starts (`app.run_with_args(&[])` stops GTK
from consuming the flags). Every log line is thread-tagged; operations log
before they start. `Library::lock` warns when a lock wait exceeds 200 ms.

### 0000.3 Scan-time performance and freeze fixes

- Decode each file once instead of three times
  (`scan::enrich_file_with_image`, `src/thumb.rs`, `src/ui/enrich.rs`).
- `ENRICH_WORKERS` reduced 3 → 2 (parallel large decodes on a slow disk
  were net slower).
- Background work pauses while browsing (`AppState::enrich_pause_until`,
  set by `pause_enrichment` on folder-open and re-armed by each landed
  thumbnail).
- `set_photos_preserving` (`src/ui/grid.rs`) diffs by file path and updates
  in place instead of `rebuild()`, so a background reload no longer resets
  selection/focus.
- Histogram computation moved off the main thread
  (`load_histogram_async`, `src/ui/editor.rs`).
- **Hard freeze, infinite recursion:** `EditPanel::load` →
  `Viewer::set_show_original` → `Viewer::show` → `Properties::show` →
  `EditPanel::load` looped (~700 frames, found via `gdb` backtrace). Fixed
  with two early-return guards in `src/ui/viewer.rs` / `src/ui/editor.rs`.

### 0000.4 Not done in this session

- README.md does not yet document the exact Immich API-key permissions
  needed (`album.read`, `asset.read`, `asset.view`, `timeline.read` for
  browse/view; `asset.upload`, `album.create`, `album.update` for
  upload/sync — the app never deletes on the server).
- "Move to llama.cpp instead of Ollama" is a ROADMAP idea only, no code.

### 0000.5 Verification

`cargo build` clean (remaining warnings are glib-channel/GTK-dialog
deprecations, tracked in section 8, not new work). `cargo test`: 43 passed.
To debug a future freeze: `gdb -p <pid> -batch -ex "thread apply all bt"`,
read the **main** thread — worker threads idling on a channel receive are
usually not the cause.

## 000. Non-destructive editing and color levels

This section describes an earlier session. Work is complete and pushed.
`cargo test`: 43 passed.

Edits (flip, straighten+auto-crop, crop, brightness/contrast, per-channel
R/G/B levels) are stored in `library.db` and applied at view time and
thumbnail time — the file on disk is never changed. The 90° rotation stays
a separate, older feature (`photos.orientation`), applied before the new
edits.

### 000.1 Where the code is

- `src/db/schema.sql` — `photo_edits`, `level_presets`.
- `src/model.rs` — `Levels`, `LevelPreset`, `PhotoEdit` (integer-scaled
  fields, so the structs stay `Eq`; gamma in milli-units, crop per-mille).
- `src/db/edits.rs`, `src/db/presets.rs` — CRUD + `apply_levels_to_folder`.
- `src/edit.rs` — shared render pipeline `apply_edits` + `auto_levels`
  (clips 0.5% per tail), used by both the viewer and the thumbnailer.
- `src/thumb.rs` — `Generator::get_edited`; cache key includes `edit_rev`
  for a real edit.
- `src/ui/editor.rs` — the `EditPanel` (histogram with draggable
  black/white/gamma markers, presets, "Apply to folder").
- `src/ui/export.rs` — baked export (`<stem>-edited.<ext>`, JPEG/PNG +
  quality remembered).
- `src/ui/properties.rs`, `src/ui/viewer.rs`, `src/ui/vmenu.rs` — Edit tab,
  Edit button, "view original" toggle, grid menu items.

Immich photos: the full-resolution asset is fetched via
`Client::asset_original` for histogram/auto-levels/export; view-time edits
apply to the preview.

### 000.2 Design rules to keep

- No float fields on `Photo`/`PhotoEdit` — store scaled integers.
- Do not add edit fields to the shared `PHOTO_COLS`/`map_photo` query —
  edits live in their own table.
- An identity edit keeps no `photo_edits` row (`set_photo_edit` deletes it).
- After an edit change: `Generator::invalidate(hash)`, then
  `viewer().reload_current()`, then `grid().reload_from_source()`.
- The thumbnail cache key must include the edit revision for a real edit.

### 000.3 Open items (not done)

- A background progress bar for "Apply to folder" on very large folders
  (synchronous today).
- Edits do not sync to Immich — export writes a new file the user
  re-uploads.
- `open_edit_tab`'s page index is the constant `2` — update it if a new tab
  changes the order.

### 000.4 Verification limit

That session could not run the GTK GUI — verified with `cargo build` /
`cargo test` only (the user confirmed by hand afterward). Tests do not
cover GTK widgets; test any UI change here by hand too.

## 00. Small follow-up features (earlier work)

This section describes an earlier session. Four small tasks, each complete
and pushed. `cargo test` passed 37 at the time (43 now).

### 00.1 New Files window is a user setting

`Prefs.new_max_age_days` (key `ui.new_max_age_days`, default 14), a spin
button in Thumbnails settings. Replaces the old `NEW_MAX_AGE_DAYS` /
`NEW_MAX_AGE_SECS` constants in `src/ui/newfiles.rs`.

### 00.2 Clean Up Missing Photos action

`Library::missing_photo_count` / `delete_missing_photos` (hard-deletes rows
with `missing = 1`, cascades via `ON DELETE CASCADE`). A confirm-gated
button in Thumbnails settings; does not touch files on disk.

### 00.3 thumb.regen is wired

`Prefs.regen_on_move` (key `thumb.regen`): when on, the toolbar zoom slider
clears the grid's in-memory texture cache before resizing
(`src/ui/toolbar.rs`), so cells re-render instead of scaling a cached
texture.

### 00.4 Dead-code warnings cleared

One genuinely-unused import removed; every intentional kept-API item now
has `#[allow(dead_code)]` plus a comment. Do not delete a kept-API item
without checking — it may serve a future feature.

### 00.5 Open follow-ups (not done)

- A hands-on multi-select test pass for the grid selection model (needs a
  display; CI cannot do this — see 12.4).
- Migrate the glib channel API and GTK `MessageDialog` off deprecated APIs
  (see section 8) — a larger, separate task.

## 0. Immich integration

### 0.1 State

`cargo test` passed 37 at the time. Phases 1–4a (browse, full viewer,
upload, two-way folder sync) are done. Phase 4b (tag sync) is out of
scope. Phase 5 (Immich photos in virtual albums) is deferred — see
ROADMAP.md "Immich integration".

### 0.2 Architecture

Background HTTP pattern (see AGENTS.md). `src/immich/client.rs` — blocking
`reqwest` client, `x-api-key` header, Immich REST API under `/api`.
`src/ui/immich.rs` runs all Immich work off the GTK main thread via
`glib::MainContext::channel`. `src/db/immich.rs` —
`immich_servers`/`immich_folder_links` tables. `src/db/immich_thumbs.rs` —
per-server thumbnail cache (`immich-thumbs-<server_id>.db`).
`src/ui/settings_immich.rs` — server management pane.

### 0.3 Data model

`immich_servers(id, name, base_url, api_key, added_at)` (API key stored in
plain text; multiple servers supported). `immich_folder_links(folder_id PK,
server_id, immich_album_id, created_at)` — one local folder ↔ one Immich
album. An Immich photo in the grid is a synthetic `Photo` with `id = 0` and
`path = immich://<server_id>/<asset_id>` — no `photos` row, so it can't join
a virtual album (excluded from those actions).

### 0.4 Key API facts

- List album assets: `POST /search/metadata`,
  `{"albumIds":[id],"size":N,"page":P}` → `{"assets":{"items":[...],
  "nextPage":...}}`. (The old `GET /albums/{id}` no longer returns assets.)
- Thumbnails: `GET /assets/{id}/thumbnail?size=thumbnail` (viewer preview:
  `size=preview`); original: `GET /assets/{id}/original`.
- Thumbnails/previews may be WebP, which GTK `PixbufLoader` can't decode on
  the target machine — `decode_texture`/`decode_pixbuf` fall back to the
  `image` crate. Keep this fallback.
- Upload: multipart `POST /assets`; Immich dedups by its own checksum, not
  the local SHA-256.

### 0.5 Sync behaviour

Sync links a local **folder** to an Immich album (not a pichouse album).
Up: `immich::autoupload_added` uploads new local photos. Down:
`immich::sync_folder_down` downloads new album assets, then reconciles;
`sync_all_down` runs at startup and every 5 minutes. Match is by original
filename (avoids re-download/re-upload loops). A synced folder shows a `⇅`
mark in the sidebar.

### 0.6 Known limits (not bugs)

- One upload/sync session at a time — a new one cancels the prior.
- Filename match, not content hash.
- Sync is additive — a delete on one side does not delete on the other.
- Only folders and folder-backed albums upload, not virtual albums.

### 0.7 Next steps (if the user asks)

- An upload queue so parallel syncs don't cancel each other.
- Content-hash match instead of filename match.
- Phase 5 needs a schema change — see ROADMAP.md Phase 5 for the options.

## 1. State

The Go-to-Rust port is complete; the Go code is deleted. Builds, tests pass,
and the UI runs on the target machine (Debian 13, GTK 4.18). `cargo test`
passed 33 at the time. Fast two-phase import, library freshness, the New
Files view, and virtual albums (manual + rule-based) are complete — see
sections 10–12.

## 10. Fast two-phase import and library freshness

**Two-phase import:** Phase 1 (`scan::Scanner::scan_folder`) records path/
name/folder/size/mtime only — no EXIF, dimensions, or hash — so the tree and
grid populate almost at once. Phase 2 (`ui::enrich`, a background worker
pool) computes EXIF, dimensions, and SHA-256 hash per photo
(`scan::enrich_file`), writes them, then generates the thumbnail.
`photos.scan_state` (0/1/2) tracks progress; opening a folder calls
`enrich::prioritize_folder` to front-load its ids; `enrich::ensure_running`
reseeds the worklist on startup so an interrupted import resumes.

**Library freshness:** `reconcile.rs` diffs disk against the database per
folder — new files insert, removed files soft-mark `missing` (tags/edits
kept), a reappeared or moved file (matched by size) re-points the existing
row. Runs on startup, on demand, and on a 180s timer
(`ui::freshness`) — this is the reliable path and the only one that sees
remote (NFS/SMB) changes. `ui::watcher` adds an inotify fast path for local
folders only (debounced 1500ms); it degrades gracefully and is never
required for correctness. Do not remove the periodic reconcile in favor of
inotify.

**New Files view:** a photo is "new" when `added_at` is after its root's
`first_scan_done_at` and within `NEW_MAX_AGE_DAYS` (see 00.1). Initial-import
photos never count as new. `ui::newfiles::NewFilesView` renders grouped-by-
folder results; the sidebar shows a "New Files (N)" row at the top of the
Library tab.

**Schema:** `photos.scan_state`, `photos.missing`, `photos.added_at`,
`library_folders.first_scan_done_at`, added additively by `Library::open_at`
`migrate` (no rebuild forced).

**Open follow-ups:** full hash-based move detection beyond the size
heuristic; RAW+JPEG pairing interaction (not yet decided which phase pairs
them).

## 11. Recent session: scan sequencing, tree, and theme

Read before changing the scan, the album tree, or reconciliation.

### 11.1 Enrichment does not run during a scan

`ReloadOnly` (periodic tick, per-root completion) refreshes the UI without
starting Phase 2; `ReloadAndEnrich` (sent once, after the whole scan queue
drains) starts it. Exception: opening a folder during a scan still enriches
it immediately via `enrich::prioritize_folder` — keep this behavior.

### 11.2 The album tree builds live during the scan

`scan::Scanner::scan_folder` takes an `on_folder` closure, called once per
directory right after its rows are written, which files it into the
disk-mirrored album tree at once (via a per-root
`albumtree::DiskAlbumMapper`) instead of waiting under "New folders" until
the scan ends. A completion sweep (`sync_disk_tree`) is a safety net; both
paths skip folders already in an album, preserving user placements.

### 11.3–11.7 Smaller fixes from the same session

- Albums sort alphabetically in the sidebar (display-only).
- A container directory with no images gets no `folders` row (was
  producing phantom 0-image folders); a folder with no images and no
  image-containing subfolders is now removed outright.
- `Library::insert_structure_batch` batches Phase-1 inserts per directory
  into one transaction (was starving other workers on the single SQLite
  mutex).
- `app::apply_theme(force_adwaita)` — Adwaita override for environments
  with a broken GTK3-era system theme that otherwise leaves the folder tree
  unexpandable (Settings → Appearance, on by default).
- Viewer: generation-tagged async loads, so opening a second photo can't
  show the first one's stale image. Sidebar popover crash after a library
  folder removal fixed by calling `dismiss_menu` before album actions.

## 12. Recent session: virtual albums, drag-drop, tree persistence

Read before changing virtual albums, grid selection, or sidebar tree state.

### 12.1 Virtual albums (manual + rule-based)

A virtual album groups photos from any folder, computed live at view time
(`src/db/virtual_albums.rs`: `photos_in_virtual_album` builds SQL from
rules AND/OR-combined per `rule_match`, unions manual pins, subtracts
exclusions). Three tables: `virtual_albums`, `virtual_album_photos`,
`virtual_album_rules`. **Important:** the membership subqueries need the
`photo_id AS id` alias — a missing alias previously caused removals not to
take effect. Sidebar section above folder-albums (prefix `valbum:`); grid
uses `MultiSelection` with a right-click menu to add/remove/create.

### 12.2 Drag-and-drop of photos onto a virtual album

Grid is a drag source (`photos:<id>,<id>,...` payload, current selection —
GTK selects the pressed cell before the drag starts, so dragging an
unselected cell carries only that cell). A drop leaves the target row
selected, so the add path calls `unselect_all` afterward — otherwise the
next click wouldn't fire `selection-changed` and the album wouldn't open.

### 12.3 The sidebar tree view survives a restart

Expanded node ids saved to `library.db` under key `sidebar_expanded`.
`save_expansion` now also removes collapsed ids (previously only inserted,
so an id stayed "expanded" forever); it runs at the start of each reload,
`persist_expansion` after.

### 12.4 Note: the grid selection model changed

`SingleSelection` → `MultiSelection` (needed for drag and "add to album").
The viewer/properties panel act on the first selected photo. Not a known
bug, but this change never got a hands-on multi-select test pass — test
viewer, properties, and rotation with a multi-selection if you touch this
area.

## 2. What is ported

All Go modules are ported to Rust — `src/model.rs` (domain types),
`src/db/` (rusqlite, one `Mutex`-guarded connection per database),
`src/scan.rs`, `src/thumb.rs`, `src/ai/` (Ollama), and the full GTK4 UI
under `src/ui/`.

## 3. Parity and post-parity work

Feature parity with the old Go app, plus improvements it didn't have: a
scan queue (`AppState::scan_queue`, so adding a folder mid-scan appends
instead of cancelling), live sidebar population during a scan, and the
auto-album tree (`src/ui/albumtree.rs::sync_disk_tree`, never re-files a
folder already in an album). No known parity gaps.

## 3a. Open behaviors / possible follow-ups

The user accepted these as-is — do not "fix" them without being asked:

- Folders discovered mid-scan sit under "New folders" until their root
  finishes (`sync_disk_tree` runs per completed root, not per directory).
- A rescan never re-asserts the disk-mirrored tree over manual album edits
  (deliberate — preserves user edits).

## 4. Build, test, run

`cargo build` / `cargo test` / `cargo run`. The GUI needs a display — it
does not run in headless CI; CI builds and tests only.

## 5. CI and versioning

`.gitea/workflows/build.yaml` runs on push to `main`: bumps the build
number, tests, builds `--release`, publishes one rolling pre-release. A
named release (tag push) uses `.gitea/workflows/release.yaml` instead — see
AGENTS.md RULE THREE and section 0000000000. `src/version.rs` mirrors
`Cargo.toml` via `env!("CARGO_PKG_VERSION")`. Do not change the build
number by hand.

## 6. Schema note

The Rust schema is a fresh start (no migration from the old Go databases —
the user rebuilds the library by rescanning).

## 7. Dependencies of note

gtk4-rs 0.7 (`v4_10` — do not upgrade past the system's GLib). rusqlite
(`bundled`, includes FTS5). reqwest with `rustls-tls` (no system OpenSSL).

## 8. Known technical notes

- `Pixbuf` is not `Send` — workers send raw bytes to the UI thread, which
  decodes.
- Background workers use `glib::MainContext::channel` (deprecated in glib
  0.18 but working; a future migration could move to `async-channel` +
  `spawn_future_local`).
- The grid's `PhotoObject` has a `texture` property; use
  `connect_notify_local` (two-argument closure), not a one-argument one.
- Thread-shared state must be `Send` — GTK/GObject types aren't. Use
  `Arc<Mutex<...>>` and the `*_arc()` accessors; never move an `Rc` or
  widget into `thread::spawn`.
- The grid remembers its `Source` so `reload_from_source()` can re-query
  after a scan or rotation.
- `Library` and `Thumbs` each wrap their `Connection` in a `Mutex` — every
  call locks, serializing multi-statement writes. Do not add a second
  connection without preserving the single-writer guarantee.

## 9. Rules

- Follow RULE ZERO: do not loop on guesses. Ask the user one question and
  wait.
- Commit and push after each change (RULE ONE).
- Keep AGENTS.md and README.md correct (RULE TWO).
