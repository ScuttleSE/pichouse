//! Stylised face detection: background worker session over photos.
//!
//! This mirrors `facescan.rs` for the anime/cartoon/furry face system. A
//! coordinator thread prepares the shared ONNX Runtime and the stylised models,
//! then a worker pool detects and embeds faces. A final step runs HDBSCAN over
//! the new embeddings. Progress posts to the GTK main thread through a channel.

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::db::Library;
use crate::face::runtime;
use crate::model::StyleFace;
use crate::styleface::incremental::{assign_new, group_noise, CentroidIndex};
use crate::styleface::{models, StyleFacePipeline};

use super::state::{show_error, show_message, AppState};

enum Msg {
    Message(String),
    Progress(f64),
    Scanning(bool),
    Error(String),
    Refresh,
    Done,
}

const SCAN_BATCH: i64 = 100_000;
const REFRESH_EVERY: usize = 20;

/// Start a background stylised-face-detection session over all photos that need
/// one.
pub fn scan_style_faces(state: &Rc<AppState>) {
    scan_impl(state, false);
}

/// Start a scan without message boxes when there is nothing to do. Kept for the
/// opt-in auto-scan path and external callers.
#[allow(dead_code)]
pub fn scan_style_faces_quiet(state: &Rc<AppState>) {
    scan_impl(state, true);
}

fn scan_impl(state: &Rc<AppState>, quiet: bool) {
    let cfg = state.style_face_config.borrow().clone();
    if !cfg.enabled {
        if !quiet {
            show_message(
                state,
                "Stylised face detection",
                "Stylised face detection is off. Turn it on in Settings → Characters.",
            );
        }
        return;
    }
    if !cfg.models_ready() {
        if !quiet {
            show_message(
                state,
                "Stylised face detection",
                "The stylised face models are not downloaded. Open Settings → \
                 Characters and download them first.",
            );
        }
        return;
    }
    if state.style_face_job.running() {
        if !quiet {
            show_message(
                state,
                "Stylised face detection",
                "A stylised face scan is already running.",
            );
        }
        return;
    }

    let ids = match state.lib.photos_needing_style_face_scan(SCAN_BATCH) {
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
            show_message(
                state,
                "Stylised face detection",
                "No photos need a stylised face scan.",
            );
        }
        return;
    }

    run_scan(state, ids, cfg);
}

/// Scan an explicit list of photo ids with the stylised pipeline. Used by the
/// whole-library scan and the album-scoped scan. The caller ensures the config
/// is enabled and ready, and that no scan is running.
pub fn run_scan(state: &Rc<AppState>, ids: Vec<i64>, cfg: crate::styleface::StyleFaceConfig) {
    let cancel = state.style_face_job.begin();
    let status = state.status();
    status.set_scanning(true);
    status.set_message("Preparing stylised face models…");
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
                Msg::Refresh => {
                    if let Some(sb) = state.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
                    state.refresh_characters_if_active();
                    state.grid().refresh_face_scanned();
                }
                Msg::Done => {
                    state.style_face_job.finish();
                    if let Some(sb) = state.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
                    state.refresh_characters_if_active();
                    state.grid().refresh_face_scanned();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Message("Preparing ONNX Runtime…".into()));
        if let Err(e) = runtime::ensure_runtime() {
            fail(&tx, &format!("ONNX Runtime download failed: {e}"));
            return;
        }
        if let Err(e) = runtime::init_runtime() {
            fail(&tx, &format!("ONNX Runtime init failed: {e}"));
            return;
        }

        let pipeline =
            match StyleFacePipeline::load(&cfg.detector_path, &cfg.embedding_path, cfg.min_score) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    fail(&tx, &format!("Load models failed: {e}"));
                    return;
                }
            };

        let total = ids.len();

        let done = Arc::new(Mutex::new((0usize, 0usize)));
        let jobs: Arc<Mutex<std::collections::VecDeque<i64>>> =
            Arc::new(Mutex::new(ids.into_iter().collect()));

        let workers = cfg.concurrency.max(1);
        let cfg_params = cfg.cluster_params();
        // Read the group centroids one time. A full HDBSCAN pass over a large
        // library takes hours, so the scan only puts each new face into the
        // nearest existing group. See `styleface::incremental`.
        let _ = tx.send(Msg::Message("Reading character groups…".into()));
        let index = {
            let cancel = cancel.clone();
            match CentroidIndex::load(&lib, &move || cancel.load(Ordering::Relaxed)) {
                Ok(Some(i)) => Some(Arc::new(Mutex::new(i))),
                Ok(None) => None,
                Err(e) => {
                    log::warn!("style centroids: {e}");
                    None
                }
            }
        };
        let _ = tx.send(Msg::Message(format!("Scanning stylised faces 0/{total}…")));
        let mut handles = Vec::new();
        for _ in 0..workers {
            let jobs = jobs.clone();
            let pipeline = pipeline.clone();
            let lib = lib.clone();
            let cancel = cancel.clone();
            let done = done.clone();
            let tx = tx.clone();
            let index = index.clone();
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
                let _ = tx.send(Msg::Message(format!("Scanning stylised faces {d}/{total}…")));

                if d % REFRESH_EVERY == 0 {
                    if let Some(index) = &index {
                        // try_lock: a worker that finds the lock taken skips
                        // this round. The next round or the final pass groups
                        // the faces.
                        if let Ok(mut idx) = index.try_lock() {
                            match assign_new(&lib, &mut idx, cfg_params) {
                                Ok(n) if n > 0 => {
                                    let _ = tx.send(Msg::Refresh);
                                }
                                Ok(_) => {}
                                Err(e) => log::warn!("progressive style grouping: {e}"),
                            }
                        }
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }

        if let Some(index) = &index {
            let _ = tx.send(Msg::Message("Grouping stylised faces…".into()));
            if let Err(e) = assign_new(&lib, &mut index.lock().unwrap(), cfg_params) {
                log::warn!("style grouping: {e}");
            }
        }

        let (d, e) = *done.lock().unwrap();
        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Progress(-1.0));
        let final_msg = if cancel.load(Ordering::Relaxed) {
            format!("Stylised face scan stopped ({d}/{total} done)")
        } else if e > 0 {
            format!("Stylised face scan complete: {} scanned, {} failed", d - e, e)
        } else {
            format!("Stylised face scan complete: {d} scanned")
        };
        let _ = tx.send(Msg::Message(final_msg));
        let _ = tx.send(Msg::Done);
    });
}

fn fail(tx: &glib::Sender<Msg>, msg: &str) {
    let _ = tx.send(Msg::Scanning(false));
    let _ = tx.send(Msg::Progress(-1.0));
    let _ = tx.send(Msg::Message("Stylised face detection unavailable".into()));
    let _ = tx.send(Msg::Error(msg.to_string()));
    let _ = tx.send(Msg::Done);
}

fn scan_one_photo(lib: &Library, pipeline: &StyleFacePipeline, id: i64) -> bool {
    let p = match lib.photo_by_id(id) {
        Ok(Some(p)) => p,
        _ => return false,
    };
    let (rgb, w, h) =
        match crate::thumb::decode_oriented_rgb(std::path::Path::new(&p.path), p.orientation, 1600) {
            Ok(v) => v,
            Err(_) => {
                let _ = lib.set_style_face_scan_state(id, 3);
                return false;
            }
        };

    let faces = match pipeline.detect_and_embed(&rgb, w, h) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("style detect {}: {e}", p.filename);
            let _ = lib.set_style_face_scan_state(id, 3);
            return false;
        }
    };

    // Keep ignored faces ignored: read their boxes before the clear. A new
    // face at the same place is ignored again.
    let rows: Vec<StyleFace> = faces
        .iter()
        .map(|f| StyleFace {
            photo_id: id,
            bbox_x: f.bbox_x,
            bbox_y: f.bbox_y,
            bbox_w: f.bbox_w,
            bbox_h: f.bbox_h,
            embedding: f.embedding.clone(),
            det_score: f.det_score,
            ..Default::default()
        })
        .collect();
    if let Err(e) = lib.replace_style_faces_for_photo(id, &rows) {
        log::warn!("store faces {}: {e}", p.filename);
        let _ = lib.set_style_face_scan_state(id, 3);
        return false;
    }
    true
}

/// Re-cluster in the background after a manual correction, then refresh the
/// sidebar and Characters view.
pub fn recluster_now(state: &Rc<AppState>) {
    let lib = state.lib.clone();
    let params = state.style_face_config.borrow().cluster_params();
    let (tx, rx) = glib::MainContext::channel::<()>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |_| {
            if let Some(sb) = state.sidebar.borrow().as_ref() {
                sb.reload_deferred();
            }
            state.refresh_characters_if_active();
            state.grid().reload_from_source();
            glib::ControlFlow::Break
        });
    }
    std::thread::spawn(move || {
        // Put ungrouped faces into the existing groups, then make new groups
        // from the noise faces. A full HDBSCAN pass over every face takes
        // hours on a large library, so this does not do one.
        match CentroidIndex::load(&lib, &|| false) {
            Ok(Some(mut idx)) => {
                if let Err(e) = assign_new(&lib, &mut idx, params) {
                    log::warn!("style grouping: {e}");
                }
            }
            Ok(None) => {}
            Err(e) => log::warn!("style centroids: {e}"),
        }
        if let Err(e) = group_noise(&lib, params) {
            log::warn!("style noise grouping: {e}");
        }
        let _ = tx.send(());
    });
}

/// Download the two selected stylised models (and the ONNX Runtime) in the
/// background, writing their resolved paths and the embedding dimension into
/// settings. Calls `on_done` on the UI thread by reloading the config.
pub fn download_models(state: &Rc<AppState>, detector_id: String, embedding_id: String) {
    let status = state.status();
    status.set_scanning(true);
    status.set_message("Downloading stylised face models…");

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
                    let cfg = super::prefs::load_styleface_config(&state.lib);
                    *state.style_face_config.borrow_mut() = cfg;
                }
                Msg::Progress(p) => status.set_progress(p),
                Msg::Refresh => {}
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
        let dim = models::entry(&embedding_id)
            .map(|e| e.embedding_dim)
            .unwrap_or(0);

        // A change of the embedding model changes the embedding dimension and
        // makes old vectors incompatible. Clear all stylised face data so the
        // next scan recomputes them.
        let old_embedding_id = lib
            .get_setting(super::prefs::KEY_STYLEFACE_EMBEDDING_ID, "")
            .unwrap_or_default();
        if old_embedding_id != embedding_id {
            let _ = lib.delete_all_style_face_data();
        }

        let _ = lib.set_setting(super::prefs::KEY_STYLEFACE_DETECTOR_ID, &detector_id);
        let _ = lib.set_setting(super::prefs::KEY_STYLEFACE_EMBEDDING_ID, &embedding_id);
        let _ = lib.set_setting(
            super::prefs::KEY_STYLEFACE_DETECTOR_PATH,
            &det.to_string_lossy(),
        );
        let _ = lib.set_setting(
            super::prefs::KEY_STYLEFACE_EMBEDDING_PATH,
            &emb.to_string_lossy(),
        );
        let _ = lib.set_setting(super::prefs::KEY_STYLEFACE_EMBEDDING_DIM, &dim.to_string());

        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Message("Stylised face models ready.".into()));
        let _ = tx.send(Msg::Done);
    });
}
