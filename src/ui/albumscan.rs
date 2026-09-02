//! Album-scoped face scanning, routed by the album's Face type.
//!
//! An album has a kind (Inherit / Photo / Art). The effective kind decides which
//! face method scans the album's photos. A Photo album uses the human face
//! system. An Art album uses the stylised (anime/cartoon/furry) system. This
//! module resolves the album subtree's folders and photo ids, then dispatches to
//! the correct pipeline.

use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;

use super::state::{show_message, AppState};

/// The scan batch cap, matching the whole-library scans.
const SCAN_BATCH: i64 = 100_000;

/// How often to poll for enrichment completion before starting a face scan.
const ENRICH_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Scan (or rescan) faces for one album and its sub-albums, routed by the
/// album's effective Face type. When `rescan` is true, prior scan state and
/// detected faces for the album's photos are cleared first.
pub fn scan_album_faces(state: &Rc<AppState>, album_id: i64, rescan: bool) {
    if album_id == 0 {
        return;
    }
    let folders = match state.lib.folders_under_album(album_id) {
        Ok(f) => f,
        Err(e) => {
            super::state::show_error(state, &e.to_string());
            return;
        }
    };
    if folders.is_empty() {
        show_message(
            state,
            "Scan faces",
            "This album has no folders to scan. Add folders to the album first.",
        );
        return;
    }

    let kind = state.lib.album_effective_kind(album_id).unwrap_or(1);
    match kind {
        2 => scan_art(state, &folders, rescan, "album"),
        _ => scan_photo(state, &folders, rescan, "album"),
    }
}

/// Scan (or rescan) faces for one or more folders, routed by each folder's
/// effective Face type (the album it belongs to, or Photo if it is in no
/// album). Folders go to the human face or the stylised pipeline by kind, and
/// each non-empty group starts its own scan. Works the same way as the
/// album-scoped scan: un-enriched photos are enriched (generating their
/// thumbnails) first, then scanned.
pub fn scan_folder_faces(state: &Rc<AppState>, folder_ids: &[i64], rescan: bool) {
    let mut photo_folders: Vec<i64> = Vec::new();
    let mut art_folders: Vec<i64> = Vec::new();
    for &fid in folder_ids {
        if fid == 0 {
            continue;
        }
        if state.lib.folder_effective_face_kind(fid).unwrap_or(1) == 2 {
            art_folders.push(fid);
        } else {
            photo_folders.push(fid);
        }
    }
    if !photo_folders.is_empty() {
        scan_photo(state, &photo_folders, rescan, "folder");
    }
    if !art_folders.is_empty() {
        scan_art(state, &art_folders, rescan, "folder");
    }
}

/// Route the human face pipeline over the given folders.
fn scan_photo(state: &Rc<AppState>, folders: &[i64], rescan: bool, label: &'static str) {
    let cfg = state.face_config.borrow().clone();
    if !cfg.enabled {
        show_message(
            state,
            "Face detection",
            &format!(
                "This {label} is marked Photo, but face detection is off. Turn it on \
                 in Settings → Faces."
            ),
        );
        return;
    }
    if !cfg.models_ready() {
        show_message(
            state,
            "Face detection",
            "The face models are not downloaded. Open Settings → Faces and download \
             them first.",
        );
        return;
    }
    if state.face_job.running() {
        show_message(state, "Face detection", "A face scan is already running.");
        return;
    }
    if rescan {
        if let Err(e) = state.lib.clear_face_scan_in(folders) {
            super::state::show_error(state, &e.to_string());
            return;
        }
    }
    let folders = folders.to_vec();
    ensure_enriched_then(state, folders.clone(), move |state| {
        let ids = state
            .lib
            .photos_needing_face_scan_in(&folders, SCAN_BATCH)
            .unwrap_or_default();
        if ids.is_empty() {
            show_message(
                state,
                "Face detection",
                &format!("No photos in this {label} need a scan."),
            );
            return;
        }
        super::facescan::run_scan(state, ids, cfg);
    });
}

/// Route the stylised face pipeline over the given folders.
fn scan_art(state: &Rc<AppState>, folders: &[i64], rescan: bool, label: &'static str) {
    let cfg = state.style_face_config.borrow().clone();
    if !cfg.enabled {
        show_message(
            state,
            "Stylised face detection",
            &format!(
                "This {label} is marked Art, but stylised face detection is off. Turn \
                 it on in Settings → Characters."
            ),
        );
        return;
    }
    if !cfg.models_ready() {
        show_message(
            state,
            "Stylised face detection",
            "The stylised face models are not downloaded. Open Settings → Characters \
             and download them first.",
        );
        return;
    }
    if state.style_face_job.running() {
        show_message(
            state,
            "Stylised face detection",
            "A stylised face scan is already running.",
        );
        return;
    }
    if rescan {
        if let Err(e) = state.lib.clear_style_face_scan_in(folders) {
            super::state::show_error(state, &e.to_string());
            return;
        }
    }
    let folders = folders.to_vec();
    ensure_enriched_then(state, folders.clone(), move |state| {
        let ids = state
            .lib
            .photos_needing_style_face_scan_in(&folders, SCAN_BATCH)
            .unwrap_or_default();
        if ids.is_empty() {
            show_message(
                state,
                "Stylised face detection",
                &format!("No photos in this {label} need a scan."),
            );
            return;
        }
        super::stylefacescan::run_scan(state, ids, cfg);
    });
}

/// Enrich every un-enriched photo in the given folders, then invoke `then`.
/// Enrichment (thumbnails, hash, EXIF) only ever runs on request in this app,
/// so a face scan over folders nobody has browsed into would otherwise find
/// nothing to do; this makes the face-scan actions request it explicitly.
/// Runs `then` immediately when nothing needs enrichment. Otherwise queues the
/// missing photos on the shared enrichment worker and polls until they are
/// all done (or the app can no longer tell, on a DB error) before proceeding.
fn ensure_enriched_then<F>(state: &Rc<AppState>, folders: Vec<i64>, then: F)
where
    F: FnOnce(&Rc<AppState>) + 'static,
{
    let needs = state
        .lib
        .photos_needing_enrichment_in(&folders)
        .unwrap_or_default();
    if needs.is_empty() {
        then(state);
        return;
    }
    let total = needs.len();
    super::enrich::enqueue_ids(state, needs);
    state.status().set_message_transient(
        &format!("Generating thumbnails for {total} photo(s) before scanning for faces…"),
        3,
    );
    let state = state.clone();
    let mut then = Some(then);
    glib::source::timeout_add_local(ENRICH_POLL_INTERVAL, move || {
        match state.lib.photos_needing_enrichment_in(&folders) {
            Ok(remaining) if !remaining.is_empty() => glib::ControlFlow::Continue,
            _ => {
                if let Some(f) = then.take() {
                    f(&state);
                }
                glib::ControlFlow::Break
            }
        }
    });
}

/// Autoscan routed by album kind. Splits every photo that needs a scan into a
/// Photo list and an Art list by its album's effective kind, then feeds each
/// enabled and ready pipeline its own list. A photo goes to exactly one method,
/// so nothing is scanned twice. Quiet: it shows no message boxes. `want_face`
/// and `want_art` are the per-system autoscan opt-ins.
pub fn autoscan_routed(state: &Rc<AppState>, want_face: bool, want_art: bool) {
    let face_cfg = state.face_config.borrow().clone();
    let style_cfg = state.style_face_config.borrow().clone();
    let face_on =
        want_face && face_cfg.enabled && face_cfg.models_ready() && !state.face_job.running();
    let style_on =
        want_art && style_cfg.enabled && style_cfg.models_ready() && !state.style_face_job.running();
    if !face_on && !style_on {
        return;
    }

    // Collect the union of photos needing either pass, then route each by kind.
    let mut photo_ids: Vec<i64> = Vec::new();
    let mut art_ids: Vec<i64> = Vec::new();

    if face_on {
        for id in state
            .lib
            .photos_needing_face_scan(SCAN_BATCH)
            .unwrap_or_default()
        {
            match state.lib.photo_effective_face_kind(id).unwrap_or(1) {
                2 => {}
                _ => photo_ids.push(id),
            }
        }
    }
    if style_on {
        for id in state
            .lib
            .photos_needing_style_face_scan(SCAN_BATCH)
            .unwrap_or_default()
        {
            if state.lib.photo_effective_face_kind(id).unwrap_or(1) == 2 {
                art_ids.push(id);
            }
        }
    }

    if face_on && !photo_ids.is_empty() {
        super::facescan::run_scan(state, photo_ids, face_cfg);
    }
    if style_on && !art_ids.is_empty() {
        super::stylefacescan::run_scan(state, art_ids, style_cfg);
    }
}

