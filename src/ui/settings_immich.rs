//! Immich settings pane: manage one or more Immich servers.
//!
//! Each server is a row in `immich_servers`. The pane lists the servers, edits
//! one at a time in a small form, and tests the connection on a background
//! thread. Saving a server rebuilds the sidebar Immich section and refreshes
//! its album cache.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, Label, ListItem, ListView, Orientation, ScrolledWindow,
    Separator, SignalListItemFactory, SingleSelection, SpinButton, StringList, StringObject,
};

use super::prefs;
use super::state::{show_error, show_message, AppState};

/// Build the Immich settings pane.
pub fn immich_pane(state: &Rc<AppState>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let intro = Label::new(Some(
        "Connect to one or more Immich servers. Browse their albums in the \
         library sidebar. The API key is stored in plain text.",
    ));
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    root.append(&intro);

    // The id of the server currently loaded in the form. 0 means "new server".
    let editing_id: Rc<RefCell<i64>> = Rc::new(RefCell::new(0));

    // Server list.
    let model = StringList::new(&[]);
    let selection = SingleSelection::new(Some(model.clone()));
    let factory = SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let label = Label::new(None);
        label.set_xalign(0.0);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        if let (Some(obj), Some(label)) = (
            item.item().and_downcast::<StringObject>(),
            item.child().and_downcast::<Label>(),
        ) {
            label.set_text(&obj.string());
        }
    });
    let list = ListView::new(Some(selection.clone()), Some(factory));
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_min_content_height(120);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    // Form fields.
    let name_entry = labeled_entry(&root, "Name", "My Immich");
    let url_entry = labeled_entry(&root, "URL", "http://host:2283");
    let key_entry = labeled_entry(&root, "API key", "");

    // Keep server ids parallel to the list rows so a selection maps to an id.
    let ids: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(Vec::new()));

    let reload_list = {
        let state = state.clone();
        let model = model.clone();
        let ids = ids.clone();
        Rc::new(move || {
            while model.n_items() > 0 {
                model.remove(0);
            }
            ids.borrow_mut().clear();
            if let Ok(servers) = state.lib.immich_servers() {
                for s in servers {
                    model.append(&format!("{}  ({})", s.name, s.base_url));
                    ids.borrow_mut().push(s.id);
                }
            }
        })
    };
    reload_list();

    // Load a selected server into the form.
    {
        let state = state.clone();
        let ids = ids.clone();
        let editing_id = editing_id.clone();
        let name_entry = name_entry.clone();
        let url_entry = url_entry.clone();
        let key_entry = key_entry.clone();
        selection.connect_selection_changed(move |sel, _, _| {
            let pos = sel.selected();
            let Some(&sid) = ids.borrow().get(pos as usize) else {
                return;
            };
            if let Ok(Some(s)) = state.lib.immich_server(sid) {
                *editing_id.borrow_mut() = s.id;
                name_entry.set_text(&s.name);
                url_entry.set_text(&s.base_url);
                key_entry.set_text(&s.api_key);
            }
        });
    }

    root.append(&Separator::new(Orientation::Horizontal));

    // Buttons.
    let new_btn = Button::with_label("New");
    let save_btn = Button::with_label("Save");
    let delete_btn = Button::with_label("Delete");
    delete_btn.add_css_class("destructive-action");
    let test_btn = Button::with_label("Test Connection");
    let status = Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);

    // New: clear the form.
    {
        let editing_id = editing_id.clone();
        let name_entry = name_entry.clone();
        let url_entry = url_entry.clone();
        let key_entry = key_entry.clone();
        let status = status.clone();
        new_btn.connect_clicked(move |_| {
            *editing_id.borrow_mut() = 0;
            name_entry.set_text("");
            url_entry.set_text("");
            key_entry.set_text("");
            status.set_text("");
        });
    }

    // Save: insert a new server or update the loaded one.
    {
        let state = state.clone();
        let editing_id = editing_id.clone();
        let name_entry = name_entry.clone();
        let url_entry = url_entry.clone();
        let key_entry = key_entry.clone();
        let reload_list = reload_list.clone();
        let status = status.clone();
        save_btn.connect_clicked(move |_| {
            let name = name_entry.text().to_string();
            let url = url_entry.text().to_string();
            let key = key_entry.text().to_string();
            if name.is_empty() || url.is_empty() {
                status.set_text("Enter a name and a URL.");
                return;
            }
            let id = *editing_id.borrow();
            let res = if id == 0 {
                state.lib.add_immich_server(&name, &url, &key).map(|s| {
                    *editing_id.borrow_mut() = s.id;
                })
            } else {
                state.lib.update_immich_server(id, &name, &url, &key)
            };
            if let Err(e) = res {
                show_error(&state, &e.to_string());
                return;
            }
            reload_list();
            status.set_text("Saved.");
            // Rebuild the sidebar section and refresh its album cache.
            if let Some(sb) = state.sidebar.borrow().as_ref() {
                sb.reload();
            }
            super::immich::refresh_albums(&state);
        });
    }

    // Delete: remove the loaded server.
    {
        let state = state.clone();
        let editing_id = editing_id.clone();
        let name_entry = name_entry.clone();
        let url_entry = url_entry.clone();
        let key_entry = key_entry.clone();
        let reload_list = reload_list.clone();
        let status = status.clone();
        delete_btn.connect_clicked(move |_| {
            let id = *editing_id.borrow();
            if id == 0 {
                return;
            }
            if let Err(e) = state.lib.delete_immich_server(id) {
                show_error(&state, &e.to_string());
                return;
            }
            // Remove the server's persistent thumbnail cache file.
            let _ = crate::db::remove_immich_thumbs_for_server(id);
            *editing_id.borrow_mut() = 0;
            name_entry.set_text("");
            url_entry.set_text("");
            key_entry.set_text("");
            reload_list();
            status.set_text("Deleted.");
            if let Some(sb) = state.sidebar.borrow().as_ref() {
                sb.reload();
            }
            super::immich::refresh_albums(&state);
        });
    }

    // Test: check the URL and key on a background thread.
    {
        let url_entry = url_entry.clone();
        let key_entry = key_entry.clone();
        let status = status.clone();
        test_btn.connect_clicked(move |_| {
            let url = url_entry.text().to_string();
            let key = key_entry.text().to_string();
            if url.is_empty() {
                status.set_text("Enter a URL first.");
                return;
            }
            status.set_text("Checking…");
            let (tx, rx) = glib::MainContext::channel::<String>(glib::Priority::DEFAULT);
            std::thread::spawn(move || {
                let client = crate::immich::Client::new(&url, &key);
                let msg = match client.test() {
                    Ok(true) => "Server OK. API key accepted.".to_string(),
                    Ok(false) => {
                        "Could not reach the server, or the API key is wrong.".to_string()
                    }
                    Err(e) => format!("Error: {e}"),
                };
                let _ = tx.send(msg);
            });
            let status = status.clone();
            rx.attach(None, move |msg| {
                status.set_text(&msg);
                glib::ControlFlow::Break
            });
        });
    }

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.append(&new_btn);
    buttons.append(&save_btn);
    buttons.append(&delete_btn);
    buttons.append(&test_btn);
    root.append(&buttons);
    root.append(&status);

    root.append(&Separator::new(Orientation::Horizontal));

    // Album page size: how many assets pichouse fetches per request.
    let page_row = GtkBox::new(Orientation::Horizontal, 6);
    let page_label = Label::new(Some("Album page size"));
    page_label.set_xalign(0.0);
    page_label.set_size_request(120, -1);
    let page_spin = SpinButton::with_range(10.0, 1000.0, 10.0);
    let current_page = state
        .lib
        .get_setting(
            prefs::KEY_IMMICH_PAGE_SIZE,
            &prefs::DEFAULT_IMMICH_PAGE_SIZE.to_string(),
        )
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(prefs::DEFAULT_IMMICH_PAGE_SIZE as f64);
    page_spin.set_value(current_page);
    {
        let state = state.clone();
        page_spin.connect_value_changed(move |s| {
            let v = s.value() as i32;
            let _ = state
                .lib
                .set_setting(prefs::KEY_IMMICH_PAGE_SIZE, &v.to_string());
        });
    }
    page_row.append(&page_label);
    page_row.append(&page_spin);
    root.append(&page_row);

    // Clear only the Immich thumbnail caches (separate from the local cache).
    let clear = Button::with_label("Clear Immich Thumbnail Cache");
    clear.add_css_class("destructive-action");
    {
        let state = state.clone();
        clear.connect_clicked(move |_| {
            if let Err(e) = crate::db::remove_all_immich_thumb_databases() {
                show_error(&state, &e.to_string());
                return;
            }
            state.grid().clear_texture_cache();
            state.grid().refresh_current();
            show_message(
                &state,
                "Immich",
                "Immich thumbnail cache cleared. Thumbnails re-download on demand.",
            );
        });
    }
    root.append(&clear);

    root
}

/// Add a labeled entry row to `parent` and return the `Entry`.
fn labeled_entry(parent: &GtkBox, caption: &str, placeholder: &str) -> Entry {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    let label = Label::new(Some(caption));
    label.set_xalign(0.0);
    label.set_size_request(80, -1);
    let entry = Entry::new();
    entry.set_hexpand(true);
    if !placeholder.is_empty() {
        entry.set_placeholder_text(Some(placeholder));
    }
    row.append(&label);
    row.append(&entry);
    parent.append(&row);
    entry
}
