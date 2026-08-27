//! Immich UI glue: background album and asset fetches, and grid display.
//!
//! Every HTTP call runs on a background thread. Results return to the GTK main
//! thread through a `glib::MainContext::channel`. This mirrors the AI tagging
//! wiring in `src/ui/aitag.rs`.

use std::rc::Rc;

use gtk4::glib;

use crate::model::{ImmichAlbum, ImmichAsset, Photo};

use super::state::AppState;

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
