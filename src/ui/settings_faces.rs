//! Face recognition settings pane.
//!
//! Each control writes the in-memory config and the db setting on change. The
//! pane also downloads the models and starts a scan. Face detection is off by
//! default.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, CheckButton, DropDown, Label, Orientation, Separator, StringList};

use crate::face::models;

use super::prefs;
use super::state::AppState;

/// Build the Faces settings pane.
pub fn faces_pane(state: &Rc<AppState>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let intro = Label::new(Some(
        "Detect faces and group people. All processing stays on your machine. \
         The models download on first use.",
    ));
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    root.append(&intro);

    let cfg = state.face_config.borrow().clone();

    // Enable toggle.
    let enabled = CheckButton::with_label("Enable face detection");
    enabled.set_active(cfg.enabled);
    {
        let state = state.clone();
        enabled.connect_toggled(move |b| {
            state.face_config.borrow_mut().enabled = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_FACE_ENABLED, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&enabled);

    // Autoscan toggle (off by default).
    let autoscan = CheckButton::with_label("Scan new photos automatically");
    autoscan.set_active(cfg.autoscan);
    {
        let state = state.clone();
        autoscan.connect_toggled(move |b| {
            state.face_config.borrow_mut().autoscan = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_FACE_AUTOSCAN, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&autoscan);

    root.append(&Separator::new(Orientation::Horizontal));

    // Model selection. Only embedding choice is offered; the detector is fixed
    // to the default YuNet. The catalog holds both.
    let cat = models::catalog();
    let embed_entries: Vec<_> = cat
        .iter()
        .filter(|e| e.kind == models::ModelKind::Embedding)
        .cloned()
        .collect();
    let labels: Vec<&str> = embed_entries.iter().map(|e| e.label).collect();
    let model_list = StringList::new(&labels);
    let model_drop = DropDown::new(Some(model_list), gtk4::Expression::NONE);
    // Select the current embedding id.
    let cur_embed = state
        .lib
        .get_setting(prefs::KEY_FACE_EMBEDDING_ID, models::DEFAULT_EMBEDDING_ID)
        .unwrap_or_else(|_| models::DEFAULT_EMBEDDING_ID.to_string());
    if let Some(pos) = embed_entries.iter().position(|e| e.id == cur_embed) {
        model_drop.set_selected(pos as u32);
    }
    let model_row = GtkBox::new(Orientation::Horizontal, 6);
    model_row.append(&Label::new(Some("Embedding model")));
    model_row.append(&model_drop);
    root.append(&model_row);

    let license = Label::new(None);
    license.set_xalign(0.0);
    license.set_wrap(true);
    let set_license = {
        let license = license.clone();
        let entries = embed_entries.clone();
        move |idx: usize| {
            if let Some(e) = entries.get(idx) {
                license.set_text(&format!("License: {}", e.license));
            }
        }
    };
    set_license(model_drop.selected() as usize);
    root.append(&license);

    // A warning that a model change needs a re-scan.
    let warn = Label::new(None);
    warn.set_xalign(0.0);
    warn.set_wrap(true);
    warn.add_css_class("dim-label");
    root.append(&warn);

    // Download + scan buttons.
    let btn_row = GtkBox::new(Orientation::Horizontal, 6);
    let download = Button::with_label("Download models");
    let scan = Button::with_label("Scan for faces now");
    btn_row.append(&download);
    btn_row.append(&scan);
    root.append(&btn_row);

    let hint = Label::new(Some(
        "Manage and name people in the People section of the Library sidebar.",
    ));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    root.append(&hint);

    {
        let state = state.clone();
        let entries = embed_entries.clone();
        let model_drop2 = model_drop.clone();
        download.connect_clicked(move |_| {
            let idx = model_drop2.selected() as usize;
            let Some(embed) = entries.get(idx) else {
                return;
            };
            super::facescan::download_models(
                &state,
                models::DEFAULT_DETECTOR_ID.to_string(),
                embed.id.to_string(),
            );
        });
    }

    {
        let state = state.clone();
        scan.connect_clicked(move |_| {
            super::facescan::scan_faces(&state);
        });
    }

    root.append(&Separator::new(Orientation::Horizontal));

    // Privacy reset.
    let reset = Button::with_label("Delete all face data");
    reset.add_css_class("destructive-action");
    reset.set_halign(gtk4::Align::Start);
    {
        let state = state.clone();
        reset.connect_clicked(move |_| {
            let state2 = state.clone();
            super::dialogs::confirm(
                &state,
                None,
                "Delete all face data",
                "Delete every detected face, person, and grouping? Photos on disk are \
                 not affected. This cannot be undone.",
                move || {
                    delete_all_face_data(&state2);
                },
            );
        });
    }
    root.append(&reset);

    // React to a model change: update license, warn about a re-scan.
    {
        let state = state.clone();
        let entries = embed_entries.clone();
        let warn = warn.clone();
        model_drop.connect_selected_notify(move |d| {
            let idx = d.selected() as usize;
            set_license(idx);
            let Some(e) = entries.get(idx) else { return };
            let prev = state
                .lib
                .get_setting(prefs::KEY_FACE_EMBEDDING_ID, models::DEFAULT_EMBEDDING_ID)
                .unwrap_or_default();
            if prev != e.id {
                warn.set_text(
                    "This model differs from the one your faces used. Download it, \
                     then run a full re-scan so all faces use the new model.",
                );
            } else {
                warn.set_text("");
            }
        });
    }

    root
}

/// Clear every face, person, and grouping, plus the face-crop cache.
fn delete_all_face_data(state: &Rc<AppState>) {
    if let Err(e) = state.lib.delete_all_face_data() {
        super::state::show_error(state, &e.to_string());
        return;
    }
    // Drop the open face-thumbs handle, then remove the file.
    *state.face_thumbs.borrow_mut() = None;
    if let Err(e) = crate::db::remove_face_thumbs_database() {
        log::warn!("remove face thumbs db: {e}");
    }
    if let Some(sb) = state.sidebar.borrow().as_ref() {
        sb.reload_deferred();
    }
    super::state::show_message(
        state,
        "Face data deleted",
        "All detected faces, people, and groupings were removed.",
    );
}

