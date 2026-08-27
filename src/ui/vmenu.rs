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

    grid.grid_view().insert_action_group("grid", Some(&group));

    // On right-click, build the menu from the current virtual albums and pop it
    // up at the pointer over the grid view.
    let state = state.clone();
    let grid_weak = Rc::downgrade(grid);
    grid.set_on_context_menu(move |x, y| {
        let Some(grid) = grid_weak.upgrade() else {
            return;
        };
        // Only local library photos can join a virtual album. Immich photos
        // (id 0) are not rows in `photos`, so skip the menu when the selection
        // has no local photos.
        if local_photo_ids(&grid).is_empty() {
            return;
        }
        let menu = build_menu(&state, &grid);
        dismiss(&pop);
        let popover = PopoverMenu::from_model_full(&menu, gtk4::PopoverMenuFlags::NESTED);
        popover.set_has_arrow(false);
        popover.set_parent(grid.grid_view());
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
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

    // Edit / export apply to the selection regardless of virtual albums.
    let tools = gio::Menu::new();
    let selected = grid.selected_photos().iter().filter(|p| p.id != 0).count();
    if selected == 1 {
        tools.append(Some("Edit…"), Some("grid.edit"));
    }
    tools.append(Some("Export edited copy…"), Some("grid.export"));
    menu.append_section(None, &tools);

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
