//! Phase 2 of the two-phase import: a background worker pool that enriches
//! photos recorded as structure-only by Phase 1.
//!
//! Each worker pops a photo id from a shared priority worklist, computes its
//! EXIF taken date, pixel dimensions, and content hash, writes them to the
//! database, then generates its thumbnail (now that the hash — the thumbnail
//! cache key — exists). The visible grid re-queries periodically so
//! placeholders are replaced by real thumbnails as data lands.
//!
//! On-demand priority: opening a folder whose photos are not yet enriched moves
//! those ids to the front of the worklist, so what the user is looking at is
//! filled in first.

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::db::Library;
use crate::model::PhotoScanState;
use crate::scan;
use crate::thumb::Generator;

use super::state::AppState;

/// How many photos are enriched concurrently.
const ENRICH_WORKERS: usize = 3;

/// How often (in enriched photos) to refresh the visible grid/sidebar.
const REFRESH_EVERY: usize = 12;

/// A status update posted from a worker coordinator to the UI thread.
enum Msg {
    /// One photo finished; carries its folder id so the UI can refresh.
    #[allow(dead_code)] // `folder_id` documents the message payload.
    Progress { folder_id: i64, done: usize, total: usize },
    /// A folder's photos are all enriched; refine its year and reload sidebars.
    FolderDone(i64),
    /// All work drained; the worklist is empty.
    Finished,
}

/// Ensure the background enrichment worker pool is running, seeding the
/// worklist from every photo still needing enrichment. Idempotent: if a session
/// is already active, new ids appended to the shared queue are picked up without
/// starting a second pool.
pub fn ensure_running(state: &Rc<AppState>) {
    let ids = state
        .lib
        .photos_needing_enrichment(None)
        .unwrap_or_default();
    append_ids(state, ids);
    start_if_idle(state);
}

/// Append ids (from a just-scanned root or a reconcile) to the back of the
/// worklist and start the pool if it is idle.
pub fn enqueue(state: &Rc<AppState>, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    append_ids(state, ids);
    start_if_idle(state);
}

/// Move a folder's un-enriched photos to the FRONT of the worklist so the folder
/// the user just opened is enriched first, then start the pool if idle.
pub fn prioritize_folder(state: &Rc<AppState>, folder_id: i64) {
    let ids = state
        .lib
        .photos_needing_enrichment(Some(folder_id))
        .unwrap_or_default();
    if ids.is_empty() {
        return;
    }
    {
        let mut q = state.enrich_queue.lock().unwrap();
        // Remove any of these ids already queued, then push them to the front
        // (preserving their order) so they run before everything else.
        let front: std::collections::HashSet<i64> = ids.iter().copied().collect();
        q.retain(|id| !front.contains(id));
        for id in ids.into_iter().rev() {
            q.push_front(id);
        }
    }
    start_if_idle(state);
}

/// Append ids to the back of the shared worklist, skipping ids already queued.
fn append_ids(state: &Rc<AppState>, ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    let mut q = state.enrich_queue.lock().unwrap();
    let present: std::collections::HashSet<i64> = q.iter().copied().collect();
    for id in ids {
        if !present.contains(&id) {
            q.push_back(id);
        }
    }
}

/// Start the worker pool unless a session is already running.
fn start_if_idle(state: &Rc<AppState>) {
    if state.enrich_job.running() {
        return;
    }
    if state.enrich_queue.lock().unwrap().is_empty() {
        return;
    }
    start_workers(state);
}

/// Spawn the coordinator + worker pool for one enrichment session.
fn start_workers(state: &Rc<AppState>) {
    let cancel = state.enrich_job.begin();
    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);

    // UI-thread receiver: refresh the grid/sidebar as data lands.
    {
        let state = state.clone();
        let mut since_refresh = 0usize;
        rx.attach(None, move |msg| {
            match msg {
                Msg::Progress { done, total, .. } => {
                    since_refresh += 1;
                    if since_refresh >= REFRESH_EVERY {
                        since_refresh = 0;
                        state.grid().reload_from_source();
                        state.refresh_new_files_if_active();
                    }
                    state
                        .status()
                        .set_message(&format!("Reading photo info {done}/{total}…"));
                }
                Msg::FolderDone(folder_id) => {
                    // Refine the folder year from the earliest known taken date.
                    if let Ok(Some(earliest)) = state.lib.earliest_taken_at(folder_id) {
                        let year = crate::scan::year_of(earliest);
                        let _ = state.lib.set_folder_year(folder_id, year);
                        super::app::reload_folders(&state);
                    }
                }
                Msg::Finished => {
                    state.enrich_job.finish();
                    state.grid().reload_from_source();
                    state.refresh_new_files_if_active();
                    // If new work arrived while finishing, restart.
                    if !state.enrich_queue.lock().unwrap().is_empty() {
                        start_workers(&state);
                    } else {
                        state.status().set_message("Library up to date.");
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    let gen = state.gen.clone();
    let queue = state.enrich_queue.clone();

    std::thread::spawn(move || {
        let done = Arc::new(Mutex::new(0usize));
        // Track, per folder, how many of its photos remain un-enriched so we can
        // fire FolderDone exactly once when a folder is fully enriched.
        let folder_seen: Arc<Mutex<std::collections::HashSet<i64>>> =
            Arc::new(Mutex::new(std::collections::HashSet::new()));

        let mut handles = Vec::new();
        for w in 0..ENRICH_WORKERS {
            let lib = lib.clone();
            let gen = gen.clone();
            let queue = queue.clone();
            let cancel = cancel.clone();
            let tx = tx.clone();
            let done = done.clone();
            let folder_seen = folder_seen.clone();
            let builder = std::thread::Builder::new().name(format!("enrich{w}"));
            if let Ok(h) = builder.spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let (id, remaining_in_queue) = {
                    let mut q = queue.lock().unwrap();
                    let id = q.pop_front();
                    (id, q.len())
                };
                let Some(id) = id else { return };

                let folder_id = enrich_one(&lib, &gen, id);

                let d = {
                    let mut g = done.lock().unwrap();
                    *g += 1;
                    *g
                };
                // Total is computed live so appending a second folder's photos
                // grows it rather than showing a stale first-folder count.
                let total = d + remaining_in_queue;
                let _ = tx.send(Msg::Progress {
                    folder_id,
                    done: d,
                    total,
                });

                // If this folder now has no more photos needing enrichment,
                // announce it once so its year is refined.
                if folder_id != 0 {
                    let remaining = lib
                        .photos_needing_enrichment(Some(folder_id))
                        .map(|v| v.len())
                        .unwrap_or(1);
                    if remaining == 0 {
                        let first = {
                            let mut seen = folder_seen.lock().unwrap();
                            seen.insert(folder_id)
                        };
                        if first {
                            let _ = tx.send(Msg::FolderDone(folder_id));
                        }
                    }
                }
            }) {
                handles.push(h);
            }
        }
        for h in handles {
            let _ = h.join();
        }
        let _ = tx.send(Msg::Finished);
    });
}

/// Enrich one photo: compute EXIF/dimensions/hash, store them, then generate its
/// thumbnail. Returns the photo's folder id (0 if the photo vanished).
fn enrich_one(lib: &Library, gen: &Generator, id: i64) -> i64 {
    let p = match lib.photo_by_id(id) {
        Ok(Some(p)) => p,
        _ => return 0,
    };
    let _ = lib.set_photo_scan_state(id, PhotoScanState::Enriching);
    log::trace!("enrich {} ({}) …", id, p.path);
    let t = std::time::Instant::now();
    match scan::enrich_file(std::path::Path::new(&p.path)) {
        Some(enr) => {
            let hash_ms = t.elapsed();
            let _ = lib.enrich_photo(id, enr.taken_at, enr.width, enr.height, &enr.hash);
            // Generate (and cache) the thumbnail now that the hash exists.
            let t_thumb = std::time::Instant::now();
            let _ = gen.get(&enr.hash, std::path::Path::new(&p.path), p.orientation);
            if hash_ms.as_millis() >= 500 || t_thumb.elapsed().as_millis() >= 500 {
                log::debug!(
                    "enrich {} slow: enrich_file {:.2?}, thumbnail {:.2?} ({})",
                    id,
                    hash_ms,
                    t_thumb.elapsed(),
                    p.path
                );
            }
        }
        None => {
            // File could not be read; leave it structured so a later pass (or a
            // reconcile) can retry, but do not spin on it now.
            let _ = lib.set_photo_scan_state(id, PhotoScanState::Structured);
        }
    }
    p.folder_id
}
