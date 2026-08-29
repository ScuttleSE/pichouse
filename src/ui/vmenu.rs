//! Grid right-click context menu: add selected photos to a virtual album.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::PopoverMenu;

use super::dialogs::prompt_text;
use super::grid::Grid;
use super::sidebar::Sidebar;
use super::state::{show_error, AppState};

/// Install the right-click context menu on the grid. Menu actions operate on
/// the grid's current multi-selection.
pub fn install_grid_context_menu(state: &Rc<AppState>, grid: &Rc<Grid>, sidebar: &Rc<Sidebar>) {
    // Actions live in a group scoped to the grid view.
    let group = gio::SimpleActionGroup::new();
    let vt = glib::VariantTy::STRING;

    // A shared popover, rebuilt per right-click.
    let pop: Rc<RefCell<Option<PopoverMenu>>> = Rc::new(RefCell::new(None));

    // Add selected photos to an existing virtual album (target = album id).
    {
        let act = gio::SimpleAction::new("add-to", Some(vt));
        let state = state.clone();
        let grid = grid.clone();
        let sidebar = sidebar.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, param| {
            dismiss(&pop);
            let Some(album_id) = param
                .and_then(|p| p.str())
                .and_then(|s| s.parse::<i64>().ok())
            else {
                return;
            };
            let ids: Vec<i64> = local_photo_ids(&grid);
            if ids.is_empty() {
                return;
            }
            if let Err(e) = state.lib.add_photos_to_virtual_album(album_id, &ids) {
                show_error(&state, &e.to_string());
                return;
            }
            sidebar.reload_deferred();
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Create a new virtual album from the selection.
    {
        let act = gio::SimpleAction::new("new-from-selection", None);
        let state = state.clone();
        let grid = grid.clone();
        let sidebar = sidebar.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let ids: Vec<i64> = local_photo_ids(&grid);
            if ids.is_empty() {
                return;
            }
            let state2 = state.clone();
            let grid2 = grid.clone();
            let sidebar2 = sidebar.clone();
            prompt_text(
                &state,
                None,
                "New Virtual Album",
                "Album name:",
                "",
                move |name| {
                    let id = match state2.lib.create_virtual_album(&name, 0) {
                        Ok(id) => id,
                        Err(e) => {
                            show_error(&state2, &e.to_string());
                            return;
                        }
                    };
                    if let Err(e) = state2.lib.add_photos_to_virtual_album(id, &ids) {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    sidebar2.reload_deferred();
                    grid2.reload_from_source();
                },
            );
        });
        group.add_action(&act);
    }

    // Remove selected photos from the virtual album currently being viewed.
    {
        let act = gio::SimpleAction::new("remove-from-current", None);
        let state = state.clone();
        let grid = grid.clone();
        let sidebar = sidebar.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(album_id) = grid.current_virtual_album() else {
                return;
            };
            let ids: Vec<i64> = local_photo_ids(&grid);
            if ids.is_empty() {
                return;
            }
            if let Err(e) = state.lib.remove_photos_from_virtual_album(album_id, &ids) {
                show_error(&state, &e.to_string());
                return;
            }
            sidebar.reload_deferred();
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Remove selected photos from the character group currently being viewed.
    // A plain remove clears the character link but keeps the cluster, so a
    // later re-cluster may group the photo again.
    {
        let act = gio::SimpleAction::new("remove-from-character", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(character_id) = grid.current_character() else {
                return;
            };
            for id in local_photo_ids(&grid) {
                if let Err(e) = state.lib.remove_photo_from_character(id, character_id) {
                    show_error(&state, &e.to_string());
                    return;
                }
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Ban selected photos from the character group. A ban records a rejection,
    // so a re-cluster never groups these photos under this character again.
    {
        let act = gio::SimpleAction::new("ban-from-character", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(character_id) = grid.current_character() else {
                return;
            };
            for id in local_photo_ids(&grid) {
                if let Err(e) = state.lib.ban_photo_from_character(id, character_id) {
                    show_error(&state, &e.to_string());
                    return;
                }
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Remove selected photos from the person currently being viewed. A remove
    // clears the person link but keeps the cluster, so a later re-cluster may
    // group the photo again.
    {
        let act = gio::SimpleAction::new("remove-from-person", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(person_id) = grid.current_person() else {
                return;
            };
            for id in local_photo_ids(&grid) {
                if let Err(e) = state.lib.remove_photo_from_person(id, person_id) {
                    show_error(&state, &e.to_string());
                    return;
                }
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Remove selected photos from the unnamed face cluster being viewed.
    {
        let act = gio::SimpleAction::new("remove-from-cluster", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(cluster_id) = grid.current_cluster() else {
                return;
            };
            for id in local_photo_ids(&grid) {
                if let Err(e) = state.lib.remove_photo_from_cluster(id, cluster_id) {
                    show_error(&state, &e.to_string());
                    return;
                }
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Remove selected photos from the unnamed style cluster being viewed.
    {
        let act = gio::SimpleAction::new("remove-from-style-cluster", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(cluster_id) = grid.current_style_cluster() else {
                return;
            };
            for id in local_photo_ids(&grid) {
                if let Err(e) = state.lib.remove_photo_from_style_cluster(id, cluster_id) {
                    show_error(&state, &e.to_string());
                    return;
                }
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Mark selected photos unimportant. A skipped photo is excluded from every
    // future face scan and leaves every face group at once.
    {
        let act = gio::SimpleAction::new("skip-face-scan", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let ids = local_photo_ids(&grid);
            if ids.is_empty() {
                return;
            }
            if let Err(e) = state.lib.set_photos_skip_face_scan(&ids, true) {
                show_error(&state, &e.to_string());
                return;
            }
            grid.reload_from_source();
        });
        group.add_action(&act);
    }

    // Edit the selected photo: open it in the viewer and reveal the Edit tab.
    {
        let act = gio::SimpleAction::new("edit", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let photos = grid.selected_photos();
            let Some(first) = photos.into_iter().find(|p| p.id != 0) else {
                return;
            };
            // Open the full picture, then switch the right panel to Edit.
            state.open_viewer(vec![first], 0);
            state.properties().open_edit_tab();
        });
        group.add_action(&act);
    }

    // Show the folder that a photo comes from. Loads that folder into the grid.
    {
        let act = gio::SimpleAction::new("show-source-folder", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let photos = grid.selected_photos();
            let Some(first) = photos.into_iter().find(|p| p.id != 0) else {
                return;
            };
            let folder = match state.lib.folder_by_id(first.folder_id) {
                Ok(Some(f)) => f,
                Ok(None) => return,
                Err(e) => {
                    show_error(&state, &e.to_string());
                    return;
                }
            };
            super::app::load_folder_into_grid(&state, &folder);
        });
        group.add_action(&act);
    }

    // Export baked copies of the selected photos.
    {
        let act = gio::SimpleAction::new("export", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let items: Vec<(crate::model::Photo, crate::model::PhotoEdit)> = grid
                .selected_photos()
                .into_iter()
                .filter(|p| p.id != 0)
                .map(|p| {
                    let edit = state.lib.photo_edit(p.id).unwrap_or_default();
                    (p, edit)
                })
                .collect();
            if items.is_empty() {
                return;
            }
            super::export::export_photos(&state, items);
        });
        group.add_action(&act);
    }

    // Copy the selected photo (baked, full resolution) to the clipboard.
    {
        let act = gio::SimpleAction::new("copy", None);
        let state = state.clone();
        let grid = grid.clone();
        let pop = pop.clone();
        act.connect_activate(move |_, _| {
            dismiss(&pop);
            let Some(photo) = grid.selected_photos().into_iter().next() else {
                return;
            };
            copy_photo_to_clipboard(&state, &grid, photo);
        });
        group.add_action(&act);
    }

    // Install the action group on the grid's root box. The context-menu popover
    // parents to this same root box (see below), and GTK resolves menu actions
    // by walking up from the popover's parent. The action group must live on
    // that parent, not on the descendant GridView, or every item greys out.
    grid.widget().insert_action_group("grid", Some(&group));

    // On right-click, build the menu from the current virtual albums and pop it
    // up at the pointer over the grid view.
    let state = state.clone();
    let grid_weak = Rc::downgrade(grid);
    grid.set_on_context_menu(move |x, y| {
        let Some(grid) = grid_weak.upgrade() else {
            return;
        };
        // The menu offers "Copy image" for any selected photo (local or
        // Immich), plus virtual-album and edit/export actions for local photos.
        // Show it whenever at least one photo is selected.
        if grid.selected_photos().is_empty() {
            return;
        }
        let menu = build_menu(&state, &grid);
        dismiss(&pop);
        let popover = PopoverMenu::from_model_full(&menu, gtk4::PopoverMenuFlags::NESTED);
        popover.set_has_arrow(false);
        // Parent the popover to the grid's root box, not the scrolling GridView.
        // A popover parented to the scrolled content can be clipped to the
        // visible height, which forces the menu to scroll. The root box gives it
        // the full height, so a short menu never needs scrolling. Translate the
        // click point from GridView space into the root box's space.
        let anchor = grid.grid_view();
        let root = grid.widget();
        let (rx, ry) = anchor
            .translate_coordinates(root, x, y)
            .unwrap_or((x, y));
        popover.set_parent(root);
        // Open the menu upward when the click is in the lower half, so a tall
        // menu opens toward the free space and does not need scrolling.
        if (ry as i32) > root.height() / 2 {
            popover.set_position(gtk4::PositionType::Top);
        } else {
            popover.set_position(gtk4::PositionType::Bottom);
        }
        let rect = gdk::Rectangle::new(rx as i32, ry as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
        *pop.borrow_mut() = Some(popover);
    });
}

/// Build the context menu: a submenu of virtual albums plus "New … from
/// selection".
fn build_menu(state: &Rc<AppState>, grid: &Rc<Grid>) -> gio::Menu {
    let menu = gio::Menu::new();
    let albums = state.lib.virtual_albums().unwrap_or_default();

    let selected_all = grid.selected_photos();
    let selected_local = selected_all.iter().filter(|p| p.id != 0).count();

    // Tools apply to the selection. Copy works for any single photo (local or
    // Immich); Edit/Export need a local photo.
    let tools = gio::Menu::new();
    if selected_all.len() == 1 {
        tools.append(Some("Copy image"), Some("grid.copy"));
    }
    if selected_local == 1 {
        tools.append(Some("Edit…"), Some("grid.edit"));
        tools.append(Some("Show source folder"), Some("grid.show-source-folder"));
    }
    if selected_local >= 1 {
        tools.append(Some("Export edited copy…"), Some("grid.export"));
    }
    menu.append_section(None, &tools);

    // Group actions apply to local photos in a face-group view. "Do not scan"
    // applies in any view.
    let group_tools = gio::Menu::new();
    if selected_local >= 1 {
        if grid.current_person().is_some() {
            group_tools.append(
                Some("Remove from this person"),
                Some("grid.remove-from-person"),
            );
        }
        if grid.current_cluster().is_some() {
            group_tools.append(
                Some("Remove from this group"),
                Some("grid.remove-from-cluster"),
            );
        }
        if grid.current_character().is_some() {
            group_tools.append(
                Some("Remove from this character"),
                Some("grid.remove-from-character"),
            );
            group_tools.append(
                Some("Not this character (ban)"),
                Some("grid.ban-from-character"),
            );
        }
        if grid.current_style_cluster().is_some() {
            group_tools.append(
                Some("Remove from this group"),
                Some("grid.remove-from-style-cluster"),
            );
        }
        group_tools.append(
            Some("Do not scan these (mark unimportant)"),
            Some("grid.skip-face-scan"),
        );
    }
    if group_tools.n_items() > 0 {
        menu.append_section(None, &group_tools);
    }

    // The remaining sections are virtual-album operations, which apply only to
    // local photos. Skip them for an Immich-only selection.
    if selected_local == 0 {
        return menu;
    }

    let album_menu = gio::Menu::new();
    if albums.is_empty() {
        album_menu.append(
            Some("New Virtual Album from selection…"),
            Some("grid.new-from-selection"),
        );
        menu.append_section(None, &album_menu);
        return menu;
    }

    let add_section = gio::Menu::new();
    for a in &albums {
        // Indent sub-albums with a marker so nesting is legible in a flat list.
        let depth = album_depth(&albums, a.id);
        let prefix = "    ".repeat(depth);
        let label = format!("{prefix}{}", a.name);
        let action = format!("grid.add-to::{}", a.id);
        add_section.append(Some(&label), Some(&action));
    }
    album_menu.append_submenu(Some("Add to Virtual Album"), &add_section);
    album_menu.append(
        Some("New Virtual Album from selection…"),
        Some("grid.new-from-selection"),
    );
    // Offer removal only while a virtual album is being viewed.
    if grid.current_virtual_album().is_some() {
        album_menu.append(
            Some("Remove from this album"),
            Some("grid.remove-from-current"),
        );
    }
    menu.append_section(None, &album_menu);
    menu
}

/// Nesting depth of a virtual album within the given set (0 for top-level).
fn album_depth(albums: &[crate::model::VirtualAlbum], id: i64) -> usize {
    let mut depth = 0;
    let mut cur = id;
    while let Some(a) = albums.iter().find(|x| x.id == cur) {
        if a.parent_id == 0 {
            break;
        }
        depth += 1;
        cur = a.parent_id;
        if depth > 32 {
            break;
        }
    }
    depth
}

/// The ids of the currently selected **local** photos. Immich photos have id 0
/// and cannot be members of a virtual album, which stores `photos.id`.
fn local_photo_ids(grid: &Rc<Grid>) -> Vec<i64> {
    grid.selected_photos()
        .iter()
        .map(|p| p.id)
        .filter(|&id| id != 0)
        .collect()
}

fn dismiss(pop: &Rc<RefCell<Option<PopoverMenu>>>) {
    if let Some(p) = pop.borrow_mut().take() {
        p.popdown();
        if p.parent().is_some() {
            p.unparent();
        }
    }
}

/// The bytes needed to bake a photo off the main thread. Extracted on the main
/// thread (which owns the non-`Send` `AppState`), then moved to a worker.
enum CopySource {
    /// A local file on disk at this path.
    Local(String),
    /// An Immich asset: server base URL, API key, and asset id.
    Immich(String, String, String),
}

/// Bake the given photo (edits + orientation) at full resolution and put the
/// result on the system clipboard as an image. The load/decode/bake runs on a
/// background thread; the clipboard is set on the GTK main thread.
fn copy_photo_to_clipboard(state: &Rc<AppState>, grid: &Rc<Grid>, photo: crate::model::Photo) {
    // Resolve the source and the edit on the main thread.
    let source = if let Some(rest) = photo.path.strip_prefix("immich://") {
        let Some((sid, asset)) = rest.split_once('/') else {
            return;
        };
        let Ok(server_id) = sid.parse::<i64>() else {
            return;
        };
        let Ok(Some(server)) = state.lib.immich_server(server_id) else {
            return;
        };
        CopySource::Immich(server.base_url, server.api_key, asset.to_string())
    } else {
        CopySource::Local(photo.path.clone())
    };
    // Local photos carry a stored edit; Immich photos (id 0) have none.
    let edit = if photo.id != 0 {
        state.lib.photo_edit(photo.id).unwrap_or_default()
    } else {
        crate::model::PhotoEdit::default()
    };
    let orientation = photo.orientation;

    state.status().set_message("Copying image to clipboard…");
    let clipboard = grid.grid_view().clipboard();

    let (tx, rx) =
        glib::MainContext::channel::<Option<(Vec<u8>, i32, i32)>>(glib::Priority::DEFAULT);
    std::thread::spawn(move || {
        let baked = bake_source(&source, orientation, &edit);
        let payload = baked.map(|img| {
            let (w, h) = (img.width() as i32, img.height() as i32);
            (img.into_raw(), w, h)
        });
        let _ = tx.send(payload);
    });

    let state = state.clone();
    rx.attach(None, move |payload| {
        match payload {
            Some((raw, w, h)) => {
                let bytes = glib::Bytes::from_owned(raw);
                let texture = gdk::MemoryTexture::new(
                    w,
                    h,
                    gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    (w * 4) as usize,
                );
                clipboard.set_texture(&texture);
                state.status().set_message("Image copied to clipboard.");
            }
            None => state
                .status()
                .set_message("Could not copy the image to the clipboard."),
        }
        glib::ControlFlow::Break
    });
}

/// Load a copy source, apply orientation and edits, and return baked RGBA.
fn bake_source(
    source: &CopySource,
    orientation: i32,
    edit: &crate::model::PhotoEdit,
) -> Option<image::RgbaImage> {
    let img = match source {
        CopySource::Local(path) => image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?
            .to_rgba8(),
        CopySource::Immich(base_url, api_key, asset_id) => {
            let client = crate::immich::Client::new(base_url, api_key);
            let bytes = client.asset_original(asset_id).ok()?;
            image::load_from_memory(&bytes).ok()?.to_rgba8()
        }
    };
    let img = super::export::rotate_full(img, orientation);
    Some(crate::edit::apply_edits(img, edit))
}
