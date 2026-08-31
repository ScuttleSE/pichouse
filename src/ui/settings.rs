//! Settings window with a stack sidebar: Library Folders, Thumbnails, AI
//! Tagging, Data Location, and Shortcuts.

use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, FileDialog, Grid as GtkGrid, Label, ListItem,
    ListView, Orientation, ScrolledWindow, Separator, SignalListItemFactory, SingleSelection,
    SpinButton, Stack, StackSidebar, StringList, StringObject, Window,
};

use super::dialogs::confirm;
use super::prefs;
use super::state::{show_error, show_message, AppState};

/// Show the settings window.
pub fn show_settings(state: &Rc<AppState>) {
    let window = Window::builder()
        .title("Settings")
        .modal(true)
        .default_width(680)
        .default_height(460)
        .build();
    if let Some(win) = state.window() {
        window.set_transient_for(Some(&win));
    }

    let stack = Stack::new();
    stack.set_vexpand(true);
    stack.add_titled(&folder_pane(state, &window), Some("folders"), "Library Folders");
    stack.add_titled(&thumb_pane(state), Some("thumbs"), "Thumbnails");
    stack.add_titled(&slideshow_pane(state), Some("slideshow"), "Slideshow");
    stack.add_titled(&appearance_pane(state), Some("appearance"), "Appearance");
    stack.add_titled(
        &super::settings_ai::ai_pane(state),
        Some("ai"),
        "AI Tagging",
    );
    stack.add_titled(
        &super::settings_immich::immich_pane(state),
        Some("immich"),
        "Immich",
    );
    stack.add_titled(
        &super::settings_faces::faces_pane(state),
        Some("faces"),
        "Faces",
    );
    stack.add_titled(
        &super::settings_characters::characters_pane(state),
        Some("characters"),
        "Characters",
    );
    stack.add_titled(&storage_pane(state, &window), Some("storage"), "Data Location");
    stack.add_titled(
        &shortcut_pane(state, &window),
        Some("shortcuts"),
        "Shortcuts",
    );

    let sidebar = StackSidebar::new();
    sidebar.set_stack(&stack);

    let body = GtkBox::new(Orientation::Horizontal, 0);
    body.append(&sidebar);
    body.append(&stack);
    window.set_child(Some(&body));
    window.set_visible(true);
}

fn folder_pane(state: &Rc<AppState>, parent: &Window) -> GtkBox {
    let model = StringList::new(&[]);
    let reload = {
        let state = state.clone();
        let model = model.clone();
        Rc::new(move || {
            while model.n_items() > 0 {
                model.remove(0);
            }
            if let Ok(folders) = state.lib.library_folders() {
                for f in folders {
                    model.append(&f.path);
                }
            }
        })
    };
    reload();

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
    scroll.set_child(Some(&list));

    let scan_now = Button::with_label("Scan Thumbnails Now");
    let pending_label = Label::new(None);
    pending_label.set_xalign(0.0);
    let update_pending: Rc<dyn Fn()> = {
        let state = state.clone();
        let selection = selection.clone();
        let scan_now = scan_now.clone();
        let pending_label = pending_label.clone();
        Rc::new(move || {
            let path = selection
                .selected_item()
                .and_downcast::<StringObject>()
                .map(|o| o.string().to_string());
            match path {
                Some(p) => {
                    let n = state
                        .lib
                        .photos_needing_enrichment_under(&p)
                        .map(|v| v.len())
                        .unwrap_or(0);
                    pending_label.set_text(&format!("{n} pending"));
                    scan_now.set_sensitive(n > 0);
                }
                None => {
                    pending_label.set_text("");
                    scan_now.set_sensitive(false);
                }
            }
        })
    };
    update_pending();
    {
        let update_pending = update_pending.clone();
        selection.connect_selection_changed(move |_, _, _| {
            update_pending();
        });
    }
    {
        let state = state.clone();
        let selection = selection.clone();
        let update_pending = update_pending.clone();
        scan_now.connect_clicked(move |_| {
            if let Some(obj) = selection.selected_item().and_downcast::<StringObject>() {
                super::enrich::enqueue_root(&state, &obj.string());
            }
            update_pending();
        });
    }
    let scan_now_row = GtkBox::new(Orientation::Horizontal, 6);
    scan_now_row.append(&scan_now);
    scan_now_row.append(&pending_label);

    let add = Button::with_label("Add Folder…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let reload = reload.clone();
        let update_pending = update_pending.clone();
        add.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            // Open at the last folder the user picked, if any.
            if let Ok(last) = state.lib.get_setting(prefs::KEY_LAST_LIB_DIR, "") {
                if !last.is_empty() {
                    dialog.set_initial_folder(Some(&gio::File::for_path(&last)));
                }
            }
            let state = state.clone();
            let reload = reload.clone();
            let update_pending = update_pending.clone();
            dialog.select_folder(
                Some(&parent),
                gio::Cancellable::NONE,
                move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            // Remember the parent folder for the next open.
                            if let Some(par) = path.parent() {
                                let _ = state.lib.set_setting(
                                    prefs::KEY_LAST_LIB_DIR,
                                    &par.to_string_lossy(),
                                );
                            }
                            super::actions::add_library_folder(
                                &state,
                                &path.to_string_lossy(),
                            );
                            reload();
                            update_pending();
                        }
                    }
                },
            );
        });
    }

    let remove = Button::with_label("Remove");
    remove.add_css_class("destructive-action");
    {
        let state = state.clone();
        let selection = selection.clone();
        let parent = parent.clone();
        let reload = reload.clone();
        let update_pending = update_pending.clone();
        remove.connect_clicked(move |_| {
            let Some(obj) = selection.selected_item().and_downcast::<StringObject>() else {
                return;
            };
            let path = obj.string().to_string();
            let state2 = state.clone();
            let reload = reload.clone();
            let update_pending = update_pending.clone();
            confirm(
                &state,
                Some(&parent),
                "Remove folder",
                &format!("Remove \"{path}\" from the library? Scanned entries for this folder will be deleted."),
                move || {
                    if let Err(e) = state2.lib.remove_library_folder(&path) {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    super::app::reload_folders(&state2);
                    // If the grid was showing a folder that just got removed,
                    // clear it so stale thumbnails are not shown or clickable.
                    state2.clear_grid_if_folder_gone();
                    reload();
                    update_pending();
                },
            );
        });
    }

    let help = Label::new(Some("Folders added here are scanned into your library."));
    help.set_xalign(0.0);
    help.set_wrap(true);

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.append(&add);
    buttons.append(&remove);

    let autoscan = CheckButton::with_label("Scan folders immediately after adding");
    autoscan.set_active(
        state
            .lib
            .get_setting(prefs::KEY_AUTOSCAN_ON_ADD, "1")
            .map(|v| v == "1")
            .unwrap_or(true),
    );
    {
        let state = state.clone();
        autoscan.connect_toggled(move |b| {
            let _ = state.lib.set_setting(
                prefs::KEY_AUTOSCAN_ON_ADD,
                prefs::bool_to_str(b.is_active()),
            );
        });
    }

    let root = pane_box();
    root.append(&help);
    root.append(&buttons);
    root.append(&autoscan);
    root.append(&scroll);
    root.append(&scan_now_row);
    root
}

fn slideshow_pane(state: &Rc<AppState>) -> GtkBox {
    let root = pane_box();
    let intro = Label::new(Some("Slideshow playback options."));
    intro.set_xalign(0.0);
    root.append(&intro);

    let secs = state
        .lib
        .get_setting(
            prefs::KEY_SLIDESHOW_SECS,
            &prefs::DEFAULT_SLIDESHOW_SECS.to_string(),
        )
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(prefs::DEFAULT_SLIDESHOW_SECS)
        .clamp(1, 120);
    let shuffle_on = state
        .lib
        .get_setting(prefs::KEY_SLIDESHOW_SHUFFLE, "0")
        .map(|v| v == "1")
        .unwrap_or(false);
    let loop_on = state
        .lib
        .get_setting(prefs::KEY_SLIDESHOW_LOOP, "1")
        .map(|v| v == "1")
        .unwrap_or(true);

    let secs_row = GtkBox::new(Orientation::Horizontal, 6);
    let secs_label = Label::new(Some("Seconds per image"));
    secs_label.set_xalign(0.0);
    secs_label.set_size_request(140, -1);
    let secs_spin = SpinButton::with_range(1.0, 120.0, 1.0);
    secs_spin.set_value(secs as f64);
    secs_row.append(&secs_label);
    secs_row.append(&secs_spin);
    root.append(&secs_row);
    {
        let state = state.clone();
        secs_spin.connect_value_changed(move |s| {
            let _ = state.lib.set_setting(
                prefs::KEY_SLIDESHOW_SECS,
                &(s.value().round() as i32).to_string(),
            );
        });
    }

    let shuffle = CheckButton::with_label("Shuffle");
    shuffle.set_active(shuffle_on);
    {
        let state = state.clone();
        shuffle.connect_toggled(move |b| {
            let _ = state.lib.set_setting(
                prefs::KEY_SLIDESHOW_SHUFFLE,
                prefs::bool_to_str(b.is_active()),
            );
        });
    }
    root.append(&shuffle);

    let loop_chk = CheckButton::with_label("Loop");
    loop_chk.set_active(loop_on);
    {
        let state = state.clone();
        loop_chk.connect_toggled(move |b| {
            let _ = state.lib.set_setting(
                prefs::KEY_SLIDESHOW_LOOP,
                prefs::bool_to_str(b.is_active()),
            );
        });
    }
    root.append(&loop_chk);

    root
}

fn thumb_pane(state: &Rc<AppState>) -> GtkBox {    let root = pane_box();
    let intro = Label::new(Some("Thumbnail slider preset sizes (pixels)."));
    intro.set_xalign(0.0);
    root.append(&intro);

    let labels = ["Smallest", "Small", "Large", "Largest"];
    let mut spins = Vec::new();
    let sizes = state.prefs.borrow().sizes.clone();
    for (i, lbl) in labels.iter().enumerate() {
        let row = GtkBox::new(Orientation::Horizontal, 6);
        let name = Label::new(Some(lbl));
        name.set_xalign(0.0);
        name.set_size_request(90, -1);
        let spin = SpinButton::with_range(32.0, 2048.0, 16.0);
        spin.set_value(*sizes.get(i).unwrap_or(&160) as f64);
        row.append(&name);
        row.append(&spin);
        root.append(&row);
        spins.push(spin);
    }

    let apply = Button::with_label("Apply Sizes");
    {
        let state = state.clone();
        let spins = spins.clone();
        apply.connect_clicked(move |_| {
            let new_sizes: Vec<i32> = spins.iter().map(|s| s.value() as i32).collect();
            {
                let mut prefs = state.prefs.borrow_mut();
                prefs.sizes = new_sizes.clone();
            }
            let _ = state
                .lib
                .set_setting(prefs::KEY_THUMB_SIZES, &prefs::format_sizes(&new_sizes));
            state.apply_thumb_prefs();
            let active = state.prefs.borrow().active_size();
            state.grid().set_thumb_size(active);
        });
    }
    root.append(&apply);
    root.append(&Separator::new(Orientation::Horizontal));

    let regen = CheckButton::with_label("Regenerate thumbnails when moving the slider");
    regen.set_active(state.prefs.borrow().regen_on_move);
    {
        let state = state.clone();
        regen.connect_toggled(move |b| {
            state.prefs.borrow_mut().regen_on_move = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_REGEN_ON_MOVE, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&regen);

    let save_all = CheckButton::with_label("Cache all sizes on generation");
    save_all.set_active(state.prefs.borrow().save_all_sizes);
    {
        let state = state.clone();
        save_all.connect_toggled(move |b| {
            state.prefs.borrow_mut().save_all_sizes = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_SAVE_ALL_SIZES, prefs::bool_to_str(b.is_active()));
            state.apply_thumb_prefs();
        });
    }
    root.append(&save_all);
    root.append(&Separator::new(Orientation::Horizontal));

    // New Files window (days).
    let nf_box = GtkBox::new(Orientation::Horizontal, 6);
    nf_box.append(&Label::new(Some("New Files window (days):")));
    let nf_spin = SpinButton::with_range(1.0, 365.0, 1.0);
    nf_spin.set_value(state.prefs.borrow().new_max_age_days as f64);
    {
        let state = state.clone();
        nf_spin.connect_value_changed(move |s| {
            let days = s.value() as i64;
            state.prefs.borrow_mut().new_max_age_days = days;
            let _ = state
                .lib
                .set_setting(prefs::KEY_NEW_MAX_AGE_DAYS, &days.to_string());
            state.refresh_new_files_if_active();
            if let Some(sb) = state.sidebar.borrow().as_ref() {
                sb.reload();
            }
        });
    }
    nf_box.append(&nf_spin);
    root.append(&nf_box);
    root.append(&Separator::new(Orientation::Horizontal));

    let clear = Button::with_label("Clear Thumbnail Cache");
    clear.add_css_class("destructive-action");
    {
        let state = state.clone();
        clear.connect_clicked(move |btn| {
            let parent = btn.root().and_downcast::<Window>();
            let state2 = state.clone();
            confirm(
                &state,
                parent.as_ref(),
                "Clear cache",
                "Delete all cached thumbnails? They will be regenerated on demand.",
                move || {
                    if let Err(e) = state2.gen.clear_all() {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    state2.grid().clear_texture_cache();
                    state2.grid().refresh_current();
                    show_message(&state2, "Thumbnails", "Thumbnail cache cleared.");
                },
            );
        });
    }
    root.append(&clear);

    root.append(&Separator::new(Orientation::Horizontal));
    let cleanup = Button::with_label("Clean Up Missing Photos");
    cleanup.add_css_class("destructive-action");
    cleanup.set_tooltip_text(Some(
        "Permanently remove photos that are no longer on disk (marked missing).",
    ));
    {
        let state = state.clone();
        cleanup.connect_clicked(move |btn| {
            let parent = btn.root().and_downcast::<Window>();
            let n = state.lib.missing_photo_count().unwrap_or(0);
            if n == 0 {
                show_message(&state, "Clean Up Missing", "No missing photos to remove.");
                return;
            }
            let state2 = state.clone();
            confirm(
                &state,
                parent.as_ref(),
                "Clean up missing",
                &format!(
                    "Permanently delete {n} missing photo(s) from the library? \
                     Their tags and album memberships are also removed. \
                     This does not touch any files on disk."
                ),
                move || {
                    match state2.lib.delete_missing_photos() {
                        Ok(deleted) => {
                            state2.refresh_new_files_if_active();
                            if let Some(sb) = state2.sidebar.borrow().as_ref() {
                                sb.reload();
                            }
                            state2.grid().refresh_current();
                            show_message(
                                &state2,
                                "Clean Up Missing",
                                &format!("Removed {deleted} missing photo(s)."),
                            );
                        }
                        Err(e) => show_error(&state2, &e.to_string()),
                    }
                },
            );
        });
    }
    root.append(&cleanup);
    root
}

fn appearance_pane(state: &Rc<AppState>) -> GtkBox {
    let root = pane_box();
    let intro = Label::new(Some("Appearance"));
    intro.set_xalign(0.0);
    root.append(&intro);

    let theme = CheckButton::with_label("Use recommended theme (Adwaita)");
    theme.set_active(state.prefs.borrow().theme_override);
    {
        let state = state.clone();
        theme.connect_toggled(move |b| {
            let on = b.is_active();
            state.prefs.borrow_mut().theme_override = on;
            let _ = state
                .lib
                .set_setting(prefs::KEY_THEME_OVERRIDE, prefs::bool_to_str(on));
            // Apply live.
            super::app::apply_theme(on);
        });
    }
    root.append(&theme);

    let hint = Label::new(Some(
        "Leave this on if the folder tree will not expand. Some environments \
         (e.g. remote desktops) ship a broken system theme that hides the tree \
         expander. Uncheck to use your system theme. A restart may be needed for \
         the change to fully apply.",
    ));
    hint.set_xalign(0.0);
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    root.append(&hint);

    root
}

fn storage_pane(state: &Rc<AppState>, parent: &Window) -> GtkBox {
    let root = pane_box();
    let intro = Label::new(Some(
        "Where the pichouse databases are stored. Takes effect after restart.",
    ));
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    root.append(&intro);

    let current = crate::db::data_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_label = Label::new(Some(&current));
    path_label.set_xalign(0.0);
    path_label.set_wrap(true);
    path_label.set_selectable(true);
    root.append(&path_label);

    let choose = Button::with_label("Choose Folder…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let path_label = path_label.clone();
        choose.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            let state = state.clone();
            let path_label = path_label.clone();
            dialog.select_folder(Some(&parent), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let p = path.to_string_lossy().into_owned();
                        if let Err(e) = crate::db::write_configured_data_dir(&p) {
                            show_error(&state, &e.to_string());
                            return;
                        }
                        path_label.set_text(&p);
                        show_message(
                            &state,
                            "Data Location",
                            "Data location updated. Restart pichouse for it to take effect.",
                        );
                    }
                }
            });
        });
    }
    root.append(&choose);
    root
}

fn shortcut_pane(state: &Rc<AppState>, parent: &Window) -> GtkBox {
    let root = pane_box();
    let intro = Label::new(Some("Viewer keyboard shortcuts."));
    intro.set_xalign(0.0);
    root.append(&intro);

    let grid = GtkGrid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);

    // Keep the per-action key labels so Reset can refresh them live.
    let mut key_labels: Vec<(super::shortcuts::Action, Label)> = Vec::new();
    for (row, (action, _def)) in super::shortcuts::defaults().into_iter().enumerate() {
        let name = Label::new(Some(action.label()));
        name.set_xalign(0.0);
        let keyval = state.shortcuts.borrow().keyval(action);
        let key_label = Label::new(Some(&super::shortcuts::keyval_label(keyval)));
        key_label.set_xalign(0.0);
        key_label.set_size_request(120, -1);
        let change = Button::with_label("Change…");
        {
            let state = state.clone();
            let parent = parent.clone();
            let key_label = key_label.clone();
            change.connect_clicked(move |_| {
                capture_shortcut(&state, &parent, action, key_label.clone());
            });
        }
        grid.attach(&name, 0, row as i32, 1, 1);
        grid.attach(&key_label, 1, row as i32, 1, 1);
        grid.attach(&change, 2, row as i32, 1, 1);
        key_labels.push((action, key_label));
    }
    root.append(&grid);

    let reset = Button::with_label("Reset to Defaults");
    {
        let state = state.clone();
        reset.connect_clicked(move |_| {
            for (action, def) in super::shortcuts::defaults() {
                state.shortcuts.borrow_mut().set(action, def);
                let name = super::shortcuts::keyval_label(def);
                let _ = state
                    .lib
                    .set_setting(&format!("keybind.{}", action_key(action)), &name);
                // Live-refresh the matching key label.
                if let Some((_, label)) = key_labels.iter().find(|(a, _)| *a == action) {
                    label.set_text(&name);
                }
            }
            state.viewer().refresh_tooltips();
        });
    }
    root.append(&reset);
    root
}

fn action_key(a: super::shortcuts::Action) -> &'static str {
    match a {
        super::shortcuts::Action::Prev => "prev",
        super::shortcuts::Action::Next => "next",
        super::shortcuts::Action::Rotate => "rotate",
        super::shortcuts::Action::Close => "close",
    }
}

fn capture_shortcut(
    state: &Rc<AppState>,
    parent: &Window,
    action: super::shortcuts::Action,
    key_label: Label,
) {
    let label = Label::new(Some(&format!(
        "Press a key for \"{}\"\n(Escape to cancel)",
        action.label()
    )));
    label.set_justify(gtk4::Justification::Center);
    label.set_margin_top(20);
    label.set_margin_bottom(20);

    let window = Window::builder()
        .title("Set shortcut")
        .modal(true)
        .default_width(320)
        .default_height(120)
        .child(&label)
        .build();
    window.set_transient_for(Some(parent));

    let key_ctrl = gtk4::EventControllerKey::new();
    {
        let state = state.clone();
        let window = window.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _keycode, _modifier| {
            let kv = keyval.into_glib();
            if keyval == gtk4::gdk::Key::Escape {
                window.close();
                return glib::Propagation::Stop;
            }
            state.shortcuts.borrow_mut().set(action, kv);
            let name = super::shortcuts::keyval_label(kv);
            let _ = state
                .lib
                .set_setting(&format!("keybind.{}", action_key(action)), &name);
            key_label.set_text(&super::shortcuts::keyval_label(kv));
            state.viewer().refresh_tooltips();
            window.close();
            glib::Propagation::Stop
        });
    }
    window.add_controller(key_ctrl);
    window.set_visible(true);
}

/// A standard settings pane box with margins.
fn pane_box() -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 8);
    b.set_margin_top(12);
    b.set_margin_bottom(12);
    b.set_margin_start(12);
    b.set_margin_end(12);
    b
}
