//! Album-scoped face scanning, routed by the album's Face type.
//!
//! An album has a kind (Inherit / Photo / Art). The effective kind decides which
//! face method scans the album's photos. A Photo album uses the human face
//! system. An Art album uses the stylised (anime/cartoon/furry) system. This
//! module resolves the album subtree's folders and photo ids, then dispatches to
//! the correct pipeline.

use std::rc::Rc;

use super::state::{show_message, AppState};

/// The scan batch cap, matching the whole-library scans.
const SCAN_BATCH: i64 = 100_000;

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
        2 => scan_art(state, &folders, rescan),
        _ => scan_photo(state, &folders, rescan),
    }
}

/// Route the human face pipeline over the album's folders.
fn scan_photo(state: &Rc<AppState>, folders: &[i64], rescan: bool) {
    let cfg = state.face_config.borrow().clone();
    if !cfg.enabled {
        show_message(
            state,
            "Face detection",
            "This album is marked Photo, but face detection is off. Turn it on in \
             Settings → Faces.",
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
    let ids = state
        .lib
        .photos_needing_face_scan_in(folders, SCAN_BATCH)
        .unwrap_or_default();
    if ids.is_empty() {
        show_message(state, "Face detection", "No photos in this album need a scan.");
        return;
    }
    super::facescan::run_scan(state, ids, cfg);
}

/// Route the stylised face pipeline over the album's folders.
fn scan_art(state: &Rc<AppState>, folders: &[i64], rescan: bool) {
    let cfg = state.style_face_config.borrow().clone();
    if !cfg.enabled {
        show_message(
            state,
            "Stylised face detection",
            "This album is marked Art, but stylised face detection is off. Turn it \
             on in Settings → Characters.",
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
    let ids = state
        .lib
        .photos_needing_style_face_scan_in(folders, SCAN_BATCH)
        .unwrap_or_default();
    if ids.is_empty() {
        show_message(
            state,
            "Stylised face detection",
            "No photos in this album need a scan.",
        );
        return;
    }
    super::stylefacescan::run_scan(state, ids, cfg);
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

