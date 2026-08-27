//! Immich UI glue: background album and asset fetches, and grid display.
//!
//! Every HTTP call runs on a background thread. Results return to the GTK main
//! thread through a `glib::MainContext::channel`. This mirrors the AI tagging
//! wiring in `src/ui/aitag.rs`.

use std::rc::Rc;
use std::sync::atomic::Ordering;

use gtk4::glib;

use crate::model::{ImmichAlbum, ImmichAsset, Photo};

use super::state::AppState;

/// Where an album upload should land on the server.
pub enum UploadTarget {
    /// Create a new album with this name.
    NewAlbum(String),
    /// Add to an existing album with this Immich album id.
    ExistingAlbum(String),
}

/// A progress message from the upload coordinator to the GTK main thread.
enum UploadMsg {
    Progress { done: usize, total: usize },
    Done { uploaded: usize, duplicate: usize, failed: usize },
    Error(String),
}

/// Show the "Upload to Immich" dialog for a local album, then start the upload.
///
/// The dialog lets the user pick a server, and either create a new album
/// (default named after the local album) or add to an existing one.
pub fn show_upload_dialog(state: &Rc<AppState>, album_id: i64, album_name: &str) {
    use gtk4::prelude::*;
    use gtk4::{
        Box as GtkBox, Button, CheckButton, DropDown, Entry, Label, Orientation, StringList, Window,
    };

    let servers = state.lib.immich_servers().unwrap_or_default();
    if servers.is_empty() {
        state
            .status()
            .set_message("Add an Immich server first (Settings → Immich).");
        return;
    }

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    root.append(&Label::new(Some(&format!(
        "Upload album \"{album_name}\" to Immich."
    ))));

    // Server picker.
    let server_names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    let server_list = StringList::new(&server_names);
    let server_drop = DropDown::new(Some(server_list), gtk4::Expression::NONE);
    let server_row = GtkBox::new(Orientation::Horizontal, 6);
    server_row.append(&Label::new(Some("Server")));
    server_row.append(&server_drop);
    root.append(&server_row);

    // New vs. existing album.
    let new_radio = CheckButton::with_label("Create new album");
    new_radio.set_active(true);
    let existing_radio = CheckButton::with_label("Add to existing album");
    existing_radio.set_group(Some(&new_radio));
    root.append(&new_radio);

    let name_entry = Entry::new();
    name_entry.set_text(album_name);
    name_entry.set_hexpand(true);
    root.append(&name_entry);

    root.append(&existing_radio);

    // Existing-album picker, filled from the cached album list for the chosen
    // server. Rebuilt when the server changes.
    let existing_list = StringList::new(&[]);
    let existing_drop = DropDown::new(Some(existing_list.clone()), gtk4::Expression::NONE);
    existing_drop.set_sensitive(false);
    root.append(&existing_drop);

    // Track album ids parallel to the existing-album dropdown rows.
    let existing_ids: Rc<std::cell::RefCell<Vec<String>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));

    let fill_existing = {
        let state = state.clone();
        let servers = servers.clone();
        let existing_list = existing_list.clone();
        let existing_ids = existing_ids.clone();
        Rc::new(move |server_index: u32| {
            while existing_list.n_items() > 0 {
                existing_list.remove(0);
            }
            existing_ids.borrow_mut().clear();
            let Some(server) = servers.get(server_index as usize) else {
                return;
            };
            let cache = state.immich_albums.borrow();
            if let Some(albums) = cache.get(&server.id) {
                let mut albums: Vec<crate::model::ImmichAlbum> = albums.clone();
                albums.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                for a in albums {
                    existing_list.append(&a.name);
                    existing_ids.borrow_mut().push(a.id);
                }
            }
        })
    };
    fill_existing(0);
    {
        let fill_existing = fill_existing.clone();
        server_drop.connect_selected_notify(move |d| fill_existing(d.selected()));
    }

    // Toggle which input is active with the radio choice.
    {
        let name_entry = name_entry.clone();
        let existing_drop = existing_drop.clone();
        new_radio.connect_toggled(move |b| {
            let new_mode = b.is_active();
            name_entry.set_sensitive(new_mode);
            existing_drop.set_sensitive(!new_mode);
        });
    }

    let ok = Button::with_label("Upload");
    ok.add_css_class("suggested-action");
    let cancel = Button::with_label("Cancel");
    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&cancel);
    buttons.append(&ok);
    root.append(&buttons);

    let window = Window::builder()
        .title("Upload to Immich")
        .modal(true)
        .default_width(400)
        .child(&root)
        .build();
    if let Some(w) = state.window() {
        window.set_transient_for(Some(&w));
    }

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let state = state.clone();
        let window = window.clone();
        let album_name = album_name.to_string();
        let servers = servers.clone();
        let new_radio = new_radio.clone();
        let name_entry = name_entry.clone();
        let existing_drop = existing_drop.clone();
        let existing_ids = existing_ids.clone();
        let server_drop = server_drop.clone();
        ok.connect_clicked(move |_| {
            let Some(server) = servers.get(server_drop.selected() as usize) else {
                return;
            };
            let target = if new_radio.is_active() {
                let name = name_entry.text().to_string();
                if name.trim().is_empty() {
                    return;
                }
                UploadTarget::NewAlbum(name)
            } else {
                let idx = existing_drop.selected() as usize;
                let Some(id) = existing_ids.borrow().get(idx).cloned() else {
                    return;
                };
                UploadTarget::ExistingAlbum(id)
            };
            upload_album(&state, album_id, &album_name, server.id, target);
            window.close();
        });
    }

    window.set_visible(true);
}

/// Upload a local album's photos to an Immich server in the background.
///
/// The coordinator uploads each photo, skips ones with no readable file, and
/// treats server-reported duplicates as already present. It then creates a new
/// album or adds the assets to an existing one. Progress and the final result
/// show in the status bar. The `immich_upload` controller cancels the run.
pub fn upload_album(
    state: &Rc<AppState>,
    album_id: i64,
    album_name: &str,
    server_id: i64,
    target: UploadTarget,
) {
    let Ok(Some(server)) = state.lib.immich_server(server_id) else {
        return;
    };
    let photos = state.lib.photos_in_album(album_id).unwrap_or_default();
    if photos.is_empty() {
        state
            .status()
            .set_message(&format!("{album_name}: no photos to upload."));
        return;
    }

    let cancel = state.immich_upload.begin();
    state
        .status()
        .set_message(&format!("Uploading {} photos to Immich…", photos.len()));
    state.status().set_progress(0.0);

    let album_name = album_name.to_string();
    let (tx, rx) = glib::MainContext::channel::<UploadMsg>(glib::Priority::DEFAULT);
    std::thread::spawn(move || {
        let client = crate::immich::Client::new(&server.base_url, &server.api_key);
        let total = photos.len();
        let mut asset_ids: Vec<String> = Vec::new();
        let mut uploaded = 0usize;
        let mut duplicate = 0usize;
        let mut failed = 0usize;

        for (i, p) in photos.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let path = std::path::Path::new(&p.path);
            let created = if p.taken_at > 0 { p.taken_at } else { p.mod_time };
            match client.upload_asset(path, &p.filename, created, p.mod_time) {
                Ok(outcome) => {
                    if outcome.duplicate {
                        duplicate += 1;
                    } else {
                        uploaded += 1;
                    }
                    asset_ids.push(outcome.asset_id);
                }
                Err(_) => failed += 1,
            }
            let _ = tx.send(UploadMsg::Progress {
                done: i + 1,
                total,
            });
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(UploadMsg::Done {
                uploaded,
                duplicate,
                failed,
            });
            return;
        }

        // Place the uploaded (and duplicate) assets into the target album.
        let album_res = match target {
            UploadTarget::NewAlbum(name) => client.create_album(&name, &asset_ids).map(|_| ()),
            UploadTarget::ExistingAlbum(id) => client.add_assets_to_album(&id, &asset_ids),
        };
        match album_res {
            Ok(()) => {
                let _ = tx.send(UploadMsg::Done {
                    uploaded,
                    duplicate,
                    failed,
                });
            }
            Err(e) => {
                let _ = tx.send(UploadMsg::Error(format!("album step failed: {e}")));
            }
        }
    });

    let state = state.clone();
    rx.attach(None, move |msg| match msg {
        UploadMsg::Progress { done, total } => {
            state.status().set_progress(done as f64 / total as f64);
            state
                .status()
                .set_message(&format!("Uploading to Immich… {done}/{total}"));
            glib::ControlFlow::Continue
        }
        UploadMsg::Done {
            uploaded,
            duplicate,
            failed,
        } => {
            state.immich_upload.finish();
            state.status().set_progress(0.0);
            state.status().set_message(&format!(
                "Immich upload done: {uploaded} new, {duplicate} already present, {failed} failed."
            ));
            // Refresh album list so a newly created album appears.
            refresh_albums(&state);
            glib::ControlFlow::Break
        }
        UploadMsg::Error(e) => {
            state.immich_upload.finish();
            state.status().set_progress(0.0);
            state.status().set_message(&format!("Immich upload error: {e}"));
            glib::ControlFlow::Break
        }
    });
}

/// Refresh the album list for every Immich server in the background.
///
/// The function fetches all servers' albums on a worker thread, stores them in
/// `AppState::immich_albums`, and then rebuilds the sidebar on the main thread.
pub fn refresh_albums(state: &Rc<AppState>) {
    let servers = state.lib.immich_servers().unwrap_or_default();
    if servers.is_empty() {
        state.immich_albums.borrow_mut().clear();
        if let Some(sb) = state.sidebar.borrow().as_ref() {
            sb.reload();
        }
        return;
    }

    let (tx, rx) =
        glib::MainContext::channel::<(i64, Vec<ImmichAlbum>)>(glib::Priority::DEFAULT);
    for s in servers {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let client = crate::immich::Client::new(&s.base_url, &s.api_key);
            let albums = client.albums().unwrap_or_default();
            let _ = tx.send((s.id, albums));
        });
    }

    let state = state.clone();
    rx.attach(None, move |(server_id, albums)| {
        state.immich_albums.borrow_mut().insert(server_id, albums);
        if let Some(sb) = state.sidebar.borrow().as_ref() {
            sb.reload();
        }
        glib::ControlFlow::Continue
    });
}

/// Open an Immich album in the grid. Fetches the album's assets in the
/// background, maps them to `Photo` values with `immich://` paths, and shows
/// them. The grid downloads the thumbnails over HTTP.
pub fn show_album(state: &Rc<AppState>, server_id: i64, album_id: &str, name: &str) {
    let Ok(Some(server)) = state.lib.immich_server(server_id) else {
        return;
    };
    *state.current_folder.borrow_mut() = 0;
    state.show_grid();
    state
        .status()
        .set_message(&format!("{name} — loading from Immich…"));

    let album_id = album_id.to_string();
    let name = name.to_string();
    let page_size = state
        .lib
        .get_setting(
            super::prefs::KEY_IMMICH_PAGE_SIZE,
            &super::prefs::DEFAULT_IMMICH_PAGE_SIZE.to_string(),
        )
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(super::prefs::DEFAULT_IMMICH_PAGE_SIZE);
    let (tx, rx) = glib::MainContext::channel::<Vec<ImmichAsset>>(glib::Priority::DEFAULT);
    {
        let album_id = album_id.clone();
        std::thread::spawn(move || {
            let client = crate::immich::Client::new(&server.base_url, &server.api_key);
            let assets = client.album_assets(&album_id, page_size).unwrap_or_default();
            let _ = tx.send(assets);
        });
    }

    let state = state.clone();
    rx.attach(None, move |assets| {
        let photos: Vec<Photo> = assets
            .iter()
            .map(|a| Photo {
                path: super::grid::immich_path(server_id, &a.id),
                filename: a.filename.clone(),
                width: a.width,
                height: a.height,
                taken_at: a.taken_at,
                ..Default::default()
            })
            .collect();
        let count = photos.len();
        state
            .grid()
            .show_immich_album(server_id, &album_id, &name, photos);
        state
            .status()
            .set_message(&format!("{name} — {count} photos (Immich)"));
        glib::ControlFlow::Break
    });
}
