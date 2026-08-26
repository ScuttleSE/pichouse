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
    stack.add_titled(
        &super::settings_ai::ai_pane(state),
        Some("ai"),
        "AI Tagging",
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

    let add = Button::with_label("Add Folder…");
    {
        let state = state.clone();
        let parent = parent.clone();
        let reload = reload.clone();
        add.connect_clicked(move |_| {
            let dialog = FileDialog::new();
            let state = state.clone();
            let reload = reload.clone();
            dialog.select_folder(
                Some(&parent),
                gio::Cancellable::NONE,
                move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            super::actions::add_library_folder(
                                &state,
                                &path.to_string_lossy(),
                            );
                            reload();
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
        remove.connect_clicked(move |_| {
            let Some(obj) = selection.selected_item().and_downcast::<StringObject>() else {
                return;
            };
            let path = obj.string().to_string();
            let state2 = state.clone();
            let reload = reload.clone();
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
                    reload();
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

    let root = pane_box();
    root.append(&help);
    root.append(&buttons);
    root.append(&scroll);
    root
}

fn thumb_pane(state: &Rc<AppState>) -> GtkBox {
    let root = pane_box();
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
