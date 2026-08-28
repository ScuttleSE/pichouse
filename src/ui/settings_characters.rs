//! Stylised face (character) settings pane.
//!
//! Each control writes the in-memory config and the db setting on change. The
//! pane downloads the models and starts a scan. The feature is off by default.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, CheckButton, DropDown, Label, Orientation, Separator, StringList};

use crate::styleface::models;

use super::prefs;
use super::state::AppState;

/// Build the Characters settings pane.
pub fn characters_pane(state: &Rc<AppState>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let intro = Label::new(Some(
        "Detect stylised faces in anime, cartoon, and furry art, and group them \
         by character. All processing stays on your machine. The models download \
         on first use (about 100 MB).",
    ));
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    root.append(&intro);

    let cfg = state.style_face_config.borrow().clone();

    let enabled = CheckButton::with_label("Enable stylised face detection");
    enabled.set_active(cfg.enabled);
    {
        let state = state.clone();
        enabled.connect_toggled(move |b| {
            state.style_face_config.borrow_mut().enabled = b.is_active();
            let _ = state.lib.set_setting(
                prefs::KEY_STYLEFACE_ENABLED,
                prefs::bool_to_str(b.is_active()),
            );
        });
    }
    root.append(&enabled);

    let autoscan = CheckButton::with_label("Scan new photos automatically");
    autoscan.set_active(cfg.autoscan);
    {
        let state = state.clone();
        autoscan.connect_toggled(move |b| {
            state.style_face_config.borrow_mut().autoscan = b.is_active();
            let _ = state.lib.set_setting(
                prefs::KEY_STYLEFACE_AUTOSCAN,
                prefs::bool_to_str(b.is_active()),
            );
        });
    }
    root.append(&autoscan);

    root.append(&Separator::new(Orientation::Horizontal));

    let cat = models::catalog();

    // Detector dropdown.
    let det_entries: Vec<_> = cat
        .iter()
        .filter(|e| e.kind == models::ModelKind::Detector)
        .cloned()
        .collect();
    let det_labels: Vec<&str> = det_entries.iter().map(|e| e.label).collect();
    let det_list = StringList::new(&det_labels);
    let det_drop = DropDown::new(Some(det_list), gtk4::Expression::NONE);
    let cur_det = state
        .lib
        .get_setting(prefs::KEY_STYLEFACE_DETECTOR_ID, models::DEFAULT_DETECTOR_ID)
        .unwrap_or_else(|_| models::DEFAULT_DETECTOR_ID.to_string());
    if let Some(pos) = det_entries.iter().position(|e| e.id == cur_det) {
        det_drop.set_selected(pos as u32);
    }
    let det_row = GtkBox::new(Orientation::Horizontal, 6);
    det_row.append(&Label::new(Some("Detector model")));
    det_row.append(&det_drop);
    root.append(&det_row);

    // Embedder dropdown.
    let embed_entries: Vec<_> = cat
        .iter()
        .filter(|e| e.kind == models::ModelKind::Embedding)
        .cloned()
        .collect();
    let embed_labels: Vec<&str> = embed_entries.iter().map(|e| e.label).collect();
    let embed_list = StringList::new(&embed_labels);
    let embed_drop = DropDown::new(Some(embed_list), gtk4::Expression::NONE);
    let cur_embed = state
        .lib
        .get_setting(
            prefs::KEY_STYLEFACE_EMBEDDING_ID,
            models::DEFAULT_EMBEDDING_ID,
        )
        .unwrap_or_else(|_| models::DEFAULT_EMBEDDING_ID.to_string());
    if let Some(pos) = embed_entries.iter().position(|e| e.id == cur_embed) {
        embed_drop.set_selected(pos as u32);
    }
    let embed_row = GtkBox::new(Orientation::Horizontal, 6);
    embed_row.append(&Label::new(Some("Embedding model")));
    embed_row.append(&embed_drop);
    root.append(&embed_row);

    let warn = Label::new(Some(
        "Changing a model needs a full re-scan so all faces use the new model.",
    ));
    warn.set_xalign(0.0);
    warn.set_wrap(true);
    warn.add_css_class("dim-label");
    root.append(&warn);

    let btn_row = GtkBox::new(Orientation::Horizontal, 6);
    let download = Button::with_label("Download models");
    let scan = Button::with_label("Scan for stylised faces now");
    btn_row.append(&download);
    btn_row.append(&scan);
    root.append(&btn_row);

    let hint = Label::new(Some(
        "Manage and name characters in the Characters section of the Library \
         sidebar.",
    ));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    root.append(&hint);

    {
        let state = state.clone();
        let det_entries = det_entries.clone();
        let embed_entries = embed_entries.clone();
        let det_drop2 = det_drop.clone();
        let embed_drop2 = embed_drop.clone();
        download.connect_clicked(move |_| {
            let di = det_drop2.selected() as usize;
            let ei = embed_drop2.selected() as usize;
            let (Some(det), Some(embed)) = (det_entries.get(di), embed_entries.get(ei)) else {
                return;
            };
            super::stylefacescan::download_models(
                &state,
                det.id.to_string(),
                embed.id.to_string(),
            );
        });
    }

    {
        let state = state.clone();
        scan.connect_clicked(move |_| {
            super::stylefacescan::scan_style_faces(&state);
        });
    }

    root.append(&Separator::new(Orientation::Horizontal));

    let reset = Button::with_label("Delete all stylised face data");
    reset.add_css_class("destructive-action");
    reset.set_halign(gtk4::Align::Start);
    {
        let state = state.clone();
        reset.connect_clicked(move |_| {
            let state2 = state.clone();
            super::dialogs::confirm(
                &state,
                None,
                "Delete all stylised face data",
                "Delete every detected stylised face, character, and grouping? \
                 Photos on disk are not affected. This cannot be undone.",
                move || {
                    delete_all(&state2);
                },
            );
        });
    }
    root.append(&reset);

    root
}

/// Clear every stylised face, character, and grouping, plus the crop cache.
fn delete_all(state: &Rc<AppState>) {
    if let Err(e) = state.lib.delete_all_style_face_data() {
        super::state::show_error(state, &e.to_string());
        return;
    }
    *state.style_face_thumbs.borrow_mut() = None;
    if let Err(e) = crate::db::remove_style_face_thumbs_database() {
        log::warn!("remove style face thumbs db: {e}");
    }
    if let Some(sb) = state.sidebar.borrow().as_ref() {
        sb.reload_deferred();
    }
    super::state::show_message(
        state,
        "Stylised face data deleted",
        "All detected stylised faces, characters, and groupings were removed.",
    );
}
