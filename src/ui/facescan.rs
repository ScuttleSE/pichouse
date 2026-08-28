//! Face detection: background worker session over photos needing a scan.
//!
//! This mirrors the AI tagging pattern in `aitag.rs`. A coordinator thread
//! prepares the runtime and the models, then a worker pool detects and embeds
//! faces. A final step clusters the new embeddings. Progress posts to the GTK
//! main thread through a channel.

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::db::Library;
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
    Done,
}

/// How many photo ids to pull for one scan session.
const SCAN_BATCH: i64 = 100_000;

/// Start a background face-detection session over all photos that need one.
pub fn scan_faces(state: &Rc<AppState>) {
    scan_faces_impl(state, false);
}

/// Start a scan without message boxes when there is nothing to do. Used by the
/// opt-in auto-scan after a library reconcile.
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

    let cancel = state.face_job.begin();
    let status = state.status();
    status.set_scanning(true);
    status.set_message("Preparing face models…");
    status.set_progress(0.0);

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
                Msg::Done => {
                    state.face_job.finish();
                    // Refresh the People section after new faces and clusters.
                    if let Some(sb) = state.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
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
        let mut handles = Vec::new();
        for _ in 0..workers {
            let jobs = jobs.clone();
            let pipeline = pipeline.clone();
            let lib = lib.clone();
            let cancel = cancel.clone();
            let done = done.clone();
            let tx = tx.clone();
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

/// Re-cluster every embedded face in the library. Person-assigned faces anchor
/// stable clusters, so named people keep their identity across runs.
fn recluster(lib: &Library, threshold: f32) -> Result<(), String> {
    let rows = lib
        .faces_for_clustering()
        .map_err(|e| format!("read faces: {e}"))?;
    if rows.is_empty() {
        return Ok(());
    }
    let items: Vec<ClusterItem> = rows
        .into_iter()
        .map(|(face_id, cluster_id, person_id, embedding)| ClusterItem {
            face_id,
            embedding,
            cluster_id,
            person_id,
        })
        .collect();
    // Unnamed cluster ids start above any existing unnamed id to avoid reuse.
    let next = 1i64;
    let assignments = cluster::cluster(&items, threshold, next);
    for a in assignments {
        let _ = lib.set_face_cluster(a.face_id, a.cluster_id);
    }
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
                Msg::Progress(_) => {}
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Message("Downloading ONNX Runtime…".into()));
        if let Err(e) = runtime::ensure_runtime() {
            let _ = tx.send(Msg::Scanning(false));
            let _ = tx.send(Msg::Error(format!("ONNX Runtime download failed: {e}")));
            return;
        }
        let _ = tx.send(Msg::Message("Downloading detector model…".into()));
        let det = match models::ensure_model(&detector_id) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(Msg::Scanning(false));
                let _ = tx.send(Msg::Error(format!("Detector download failed: {e}")));
                return;
            }
        };
        let _ = tx.send(Msg::Message("Downloading embedding model…".into()));
        let emb = match models::ensure_model(&embedding_id) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(Msg::Scanning(false));
                let _ = tx.send(Msg::Error(format!("Embedding download failed: {e}")));
                return;
            }
        };
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
