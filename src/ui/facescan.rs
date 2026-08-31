//! Face detection: background worker session over photos needing a scan.
//!
//! This mirrors the AI tagging pattern in `aitag.rs`. A coordinator thread
//! prepares the runtime and the models, then a worker pool detects and embeds
//! faces. A final step clusters the new embeddings. Progress posts to the GTK
//! main thread through a channel.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::db::{FaceGroup, Library};
use crate::face::cluster::{self, ClusterItem};
use crate::face::{models, runtime, FacePipeline};
use crate::model::Face;

use super::state::{show_error, show_message, AppState};

/// A status update posted from the coordinator to the UI thread.
enum Msg {
    Message(String),
    Progress(f64),
    Scanning(bool),
    Error(String),
    /// New faces were clustered; refresh the People UI now.
    Refresh,
    /// How many new photos this scan added to each existing group, keyed the
    /// way the People view identifies a group.
    Counts(HashMap<FaceGroup, i64>),
    Done,
}

/// How many photo ids to pull for one scan session.
const SCAN_BATCH: i64 = 100_000;

/// Re-cluster and refresh the People UI after this many photos are scanned, so
/// groups appear progressively instead of only at the end.
const REFRESH_EVERY: usize = 20;

/// Start a background face-detection session over all photos that need one.
pub fn scan_faces(state: &Rc<AppState>) {
    scan_faces_impl(state, false);
}

/// Start a scan without message boxes when there is nothing to do. Kept for the
/// opt-in auto-scan path and external callers.
#[allow(dead_code)]
pub fn scan_faces_quiet(state: &Rc<AppState>) {
    scan_faces_impl(state, true);
}

fn scan_faces_impl(state: &Rc<AppState>, quiet: bool) {
    let cfg = state.face_config.borrow().clone();
    if !cfg.enabled {
        if !quiet {
            show_message(
                state,
                "Face detection",
                "Face detection is off. Turn it on in Settings → Faces.",
            );
        }
        return;
    }
    if !cfg.models_ready() {
        if !quiet {
            show_message(
                state,
                "Face detection",
                "The face models are not downloaded. Open Settings → Faces and \
                 download them first.",
            );
        }
        return;
    }
    if state.face_job.running() {
        if !quiet {
            show_message(state, "Face detection", "A face scan is already running.");
        }
        return;
    }

    let ids = match state.lib.photos_needing_face_scan(SCAN_BATCH) {
        Ok(v) => v,
        Err(e) => {
            if !quiet {
                show_error(state, &e.to_string());
            }
            return;
        }
    };
    if ids.is_empty() {
        if !quiet {
            show_message(state, "Face detection", "No photos need a face scan.");
        }
        return;
    }

    run_scan(state, ids, cfg);
}

/// Scan an explicit list of photo ids with the human face pipeline. Used by the
/// whole-library scan and the album-scoped scan. The caller ensures the config
/// is enabled and ready, and that no scan is running.
pub fn run_scan(state: &Rc<AppState>, ids: Vec<i64>, cfg: crate::face::FaceConfig) {
    let cancel = state.face_job.begin();
    let status = state.status();
    status.set_scanning(true);
    status.set_message("Preparing face models…");
    status.set_progress(0.0);

    // Reset the "new photos" badge from any previous scan, then snapshot each
    // existing group's photos so the finished scan can tell how many photos
    // it added to them.
    state.face_group_new_counts.borrow_mut().clear();
    let before_groups = state.lib.group_photo_ids().unwrap_or_default();

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |msg| {
            let status = state.status();
            match msg {
                Msg::Message(m) => status.set_message(&m),
                Msg::Progress(p) => status.set_progress(p),
                Msg::Scanning(s) => status.set_scanning(s),
                Msg::Error(e) => show_error(&state, &e),
                Msg::Refresh => {
                    if let Some(sb) = state.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
                    state.refresh_faces_if_active();
                }
                Msg::Counts(c) => {
                    *state.face_group_new_counts.borrow_mut() = c;
                }
                Msg::Done => {
                    state.face_job.finish();
                    // Refresh the People section after new faces and clusters.
                    if let Some(sb) = state.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
                    state.refresh_faces_if_active();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        // Ensure the ONNX Runtime library is present, then initialize it.
        let _ = tx.send(Msg::Message("Preparing ONNX Runtime…".into()));
        if let Err(e) = runtime::ensure_runtime() {
            fail(&tx, &format!("ONNX Runtime download failed: {e}"));
            return;
        }
        if let Err(e) = runtime::init_runtime() {
            fail(&tx, &format!("ONNX Runtime init failed: {e}"));
            return;
        }

        // Load the pipeline once and share it. The sessions serialize inside.
        let pipeline = match FacePipeline::load(
            &cfg.detector_path,
            &cfg.embedding_path,
            cfg.min_score,
        ) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                fail(&tx, &format!("Load models failed: {e}"));
                return;
            }
        };

        let total = ids.len();
        let _ = tx.send(Msg::Message(format!("Scanning faces 0/{total}…")));

        let done = Arc::new(Mutex::new((0usize, 0usize))); // (done, errors)
        let jobs: Arc<Mutex<std::collections::VecDeque<i64>>> =
            Arc::new(Mutex::new(ids.into_iter().collect()));

        let workers = cfg.concurrency.max(1);
        let cfg_threshold = cfg.cluster_threshold;
        // Guard against two workers running a full recluster at the same time.
        let reclustering = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        for _ in 0..workers {
            let jobs = jobs.clone();
            let pipeline = pipeline.clone();
            let lib = lib.clone();
            let cancel = cancel.clone();
            let done = done.clone();
            let tx = tx.clone();
            let reclustering = reclustering.clone();
            handles.push(std::thread::spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let id = {
                    let mut q = jobs.lock().unwrap();
                    q.pop_front()
                };
                let Some(id) = id else { return };

                let ok = scan_one_photo(&lib, &pipeline, id);

                let (d, _e) = {
                    let mut g = done.lock().unwrap();
                    g.0 += 1;
                    if !ok {
                        g.1 += 1;
                    }
                    (g.0, g.1)
                };
                let _ = tx.send(Msg::Progress(d as f64 / total as f64));
                let _ = tx.send(Msg::Message(format!("Scanning faces {d}/{total}…")));

                // Progressive grouping: every REFRESH_EVERY photos, re-cluster
                // and ask the UI to refresh so new people appear during the
                // scan. The boundary check makes exactly one worker do it.
                if d % REFRESH_EVERY == 0
                    && reclustering
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    if let Err(e) = recluster(&lib, cfg_threshold) {
                        log::warn!("progressive clustering: {e}");
                    }
                    reclustering.store(false, Ordering::Release);
                    let _ = tx.send(Msg::Refresh);
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }

        // Cluster the whole library's embeddings so new faces join the right
        // group and named people pull matching faces in.
        let _ = tx.send(Msg::Message("Grouping faces…".into()));
        if let Err(e) = recluster(&lib, cfg.cluster_threshold) {
            log::warn!("clustering: {e}");
        }

        // Diff against the pre-scan snapshot so each existing group that
        // gained photos can show how many are new.
        let after_groups = lib.group_photo_ids().unwrap_or_default();
        let _ = tx.send(Msg::Counts(new_photo_counts(&before_groups, &after_groups)));

        let (d, e) = *done.lock().unwrap();
        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Progress(-1.0));
        let final_msg = if cancel.load(Ordering::Relaxed) {
            format!("Face scan stopped ({d}/{total} done)")
        } else if e > 0 {
            format!("Face scan complete: {} scanned, {} failed", d - e, e)
        } else {
            format!("Face scan complete: {d} scanned")
        };
        let _ = tx.send(Msg::Message(final_msg));
        let _ = tx.send(Msg::Done);
    });
}

/// For each group present before the scan, count the photos in it now that
/// were not in it before. Groups the scan did not touch, and groups that did
/// not exist before the scan, are left out.
fn new_photo_counts(
    before: &HashMap<FaceGroup, HashSet<i64>>,
    after: &HashMap<FaceGroup, HashSet<i64>>,
) -> HashMap<FaceGroup, i64> {
    let mut out = HashMap::new();
    for (group, before_photos) in before {
        let added = match after.get(group) {
            Some(after_photos) => after_photos.difference(before_photos).count(),
            None => 0,
        };
        if added > 0 {
            out.insert(*group, added as i64);
        }
    }
    out
}

/// Send the failure sequence to the UI thread.
fn fail(tx: &glib::Sender<Msg>, msg: &str) {    let _ = tx.send(Msg::Scanning(false));
    let _ = tx.send(Msg::Progress(-1.0));
    let _ = tx.send(Msg::Message("Face detection unavailable".into()));
    let _ = tx.send(Msg::Error(msg.to_string()));
    let _ = tx.send(Msg::Done);
}

/// Detect and store faces for one photo. Returns whether it succeeded.
fn scan_one_photo(lib: &Library, pipeline: &FacePipeline, id: i64) -> bool {
    let p = match lib.photo_by_id(id) {
        Ok(Some(p)) => p,
        _ => return false,
    };
    let _ = lib.set_face_scan_state(id, 1);

    // Detect on the oriented image, capped for speed. Detection long side of
    // 1600 keeps small faces findable without decoding a huge full-res buffer.
    let (rgb, w, h) =
        match crate::thumb::decode_oriented_rgb(std::path::Path::new(&p.path), p.orientation, 1600) {
            Ok(v) => v,
            Err(_) => {
                let _ = lib.set_face_scan_state(id, 3);
                return false;
            }
        };

    let faces = match pipeline.detect_and_embed(&rgb, w, h) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("detect {}: {e}", p.filename);
            let _ = lib.set_face_scan_state(id, 3);
            return false;
        }
    };

    // Replace any prior faces for this photo, then insert the new ones.
    let _ = lib.clear_faces_for_photo(id);
    for f in &faces {
        let row = Face {
            photo_id: id,
            bbox_x: f.bbox_x,
            bbox_y: f.bbox_y,
            bbox_w: f.bbox_w,
            bbox_h: f.bbox_h,
            landmarks: f.landmarks.clone(),
            embedding: f.embedding.clone(),
            det_score: f.det_score,
            ..Default::default()
        };
        let _ = lib.insert_face(&row);
    }
    let _ = lib.set_face_scan_state(id, 2);
    true
}

/// Re-cluster in the background after a manual correction (for example a
/// rejection), then refresh the sidebar and Faces view. Cheap: it reads
/// embeddings and rewrites cluster ids only.
pub fn recluster_now(state: &Rc<AppState>) {
    let lib = state.lib.clone();
    let threshold = state.face_config.borrow().cluster_threshold;
    let (tx, rx) = glib::MainContext::channel::<()>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |_| {
            if let Some(sb) = state.sidebar.borrow().as_ref() {
                sb.reload_deferred();
            }
            state.refresh_faces_if_active();
            state.grid().reload_from_source();
            glib::ControlFlow::Break
        });
    }
    std::thread::spawn(move || {
        let _ = recluster(&lib, threshold);
        let _ = tx.send(());
    });
}

/// Re-cluster every embedded face in the library. Person-assigned faces anchor
/// stable clusters, so named people keep their identity across runs.
fn recluster(lib: &Library, threshold: f32) -> Result<(), String> {
    let rows = lib
        .faces_for_clustering()
        .map_err(|e| format!("read faces: {e}"))?;
    if rows.is_empty() {
        return Ok(());
    }
    let rejections = lib.face_rejection_map().unwrap_or_default();
    let items: Vec<ClusterItem> = rows
        .into_iter()
        .map(|(face_id, cluster_id, person_id, embedding)| ClusterItem {
            face_id,
            embedding,
            cluster_id,
            person_id,
            rejected: rejections.get(&face_id).cloned().unwrap_or_default(),
        })
        .collect();
    // Unnamed cluster ids start above any existing unnamed id to avoid reuse.
    let next = 1i64;
    let assignments = cluster::cluster(&items, threshold, next);
    let pairs: Vec<(i64, i64)> = assignments
        .into_iter()
        .map(|a| (a.face_id, a.cluster_id))
        .collect();
    lib.set_face_clusters(&pairs)
        .map_err(|e| format!("write clusters: {e}"))?;
    Ok(())
}

/// Download the two selected face models (and the ONNX Runtime) in the
/// background, writing their resolved paths and the embedding dimension into
/// settings. Progress posts to the status bar. Calls `on_done` on the UI thread
/// when finished so the settings pane can refresh.
pub fn download_models(state: &Rc<AppState>, detector_id: String, embedding_id: String) {
    let status = state.status();
    status.set_scanning(true);
    status.set_message("Downloading face models…");

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |msg| {
            let status = state.status();
            match msg {
                Msg::Message(m) => status.set_message(&m),
                Msg::Scanning(s) => status.set_scanning(s),
                Msg::Error(e) => show_error(&state, &e),
                Msg::Done => {
                    // Reload the config from settings so the pane and workers
                    // see the new model paths.
                    let cfg = super::prefs::load_face_config(&state.lib);
                    *state.face_config.borrow_mut() = cfg;
                }
                Msg::Progress(p) => status.set_progress(p),
                Msg::Refresh => {}
                Msg::Counts(_) => {}
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Message("Downloading ONNX Runtime…".into()));
        {
            let tx = tx.clone();
            if let Err(e) = runtime::ensure_runtime_progress(&|p| {
                let _ = tx.send(Msg::Progress(p));
            }) {
                let _ = tx.send(Msg::Scanning(false));
                let _ = tx.send(Msg::Error(format!("ONNX Runtime download failed: {e}")));
                return;
            }
        }
        let _ = tx.send(Msg::Progress(-1.0));
        let _ = tx.send(Msg::Message("Downloading detector model…".into()));
        let det = {
            let tx = tx.clone();
            match models::ensure_model_progress(&detector_id, &|p| {
                let _ = tx.send(Msg::Progress(p));
            }) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(Msg::Scanning(false));
                    let _ = tx.send(Msg::Error(format!("Detector download failed: {e}")));
                    return;
                }
            }
        };
        let _ = tx.send(Msg::Progress(-1.0));
        let _ = tx.send(Msg::Message("Downloading embedding model…".into()));
        let emb = {
            let tx = tx.clone();
            match models::ensure_model_progress(&embedding_id, &|p| {
                let _ = tx.send(Msg::Progress(p));
            }) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(Msg::Scanning(false));
                    let _ = tx.send(Msg::Error(format!("Embedding download failed: {e}")));
                    return;
                }
            }
        };
        let _ = tx.send(Msg::Progress(-1.0));
        let dim = models::entry(&embedding_id).map(|e| e.embedding_dim).unwrap_or(0);

        let _ = lib.set_setting(super::prefs::KEY_FACE_DETECTOR_ID, &detector_id);
        let _ = lib.set_setting(super::prefs::KEY_FACE_EMBEDDING_ID, &embedding_id);
        let _ = lib.set_setting(super::prefs::KEY_FACE_DETECTOR_PATH, &det.to_string_lossy());
        let _ = lib.set_setting(super::prefs::KEY_FACE_EMBEDDING_PATH, &emb.to_string_lossy());
        let _ = lib.set_setting(super::prefs::KEY_FACE_EMBEDDING_DIM, &dim.to_string());

        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Message("Face models ready.".into()));
        let _ = tx.send(Msg::Done);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_only_photos_added_to_a_pre_existing_group() {
        let mut before = HashMap::new();
        before.insert(FaceGroup::Person(1), HashSet::from([10, 11]));
        before.insert(FaceGroup::Cluster(2), HashSet::from([20]));

        let mut after = HashMap::new();
        // Person 1 gained one new photo.
        after.insert(FaceGroup::Person(1), HashSet::from([10, 11, 12]));
        // Cluster 2 gained none.
        after.insert(FaceGroup::Cluster(2), HashSet::from([20]));
        // A brand new group is not "existing", so it is left out even though
        // it has photos.
        after.insert(FaceGroup::Cluster(3), HashSet::from([30]));

        let counts = new_photo_counts(&before, &after);
        assert_eq!(counts.get(&FaceGroup::Person(1)), Some(&1));
        assert_eq!(counts.get(&FaceGroup::Cluster(2)), None);
        assert_eq!(counts.get(&FaceGroup::Cluster(3)), None);
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn a_group_that_lost_its_photos_counts_zero_new() {
        let mut before = HashMap::new();
        before.insert(FaceGroup::Cluster(1), HashSet::from([10]));
        let after = HashMap::new();

        let counts = new_photo_counts(&before, &after);
        assert!(counts.is_empty());
    }
}
