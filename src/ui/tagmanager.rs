//! Tag Manager window: rename, merge, and delete tags globally.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Label, ListItem, ListView, Orientation, ScrolledWindow,
    SignalListItemFactory, SingleSelection, StringList, StringObject, Window,
};

use super::dialogs::{confirm, prompt_text};
use super::state::{show_error, AppState};

/// Show the Tag Manager window.
pub fn show_tag_manager(state: &Rc<AppState>) {
    let model = StringList::new(&[]);
    let names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

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
    let list_view = ListView::new(Some(selection.clone()), Some(factory));
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list_view));

    let reload = {
        let state = state.clone();
        let model = model.clone();
        let names = names.clone();
        Rc::new(move || {
            let tags = state.lib.all_tags().unwrap_or_default();
            let mut n = names.borrow_mut();
            n.clear();
            // Clear the model.
            while model.n_items() > 0 {
                model.remove(0);
            }
            for t in &tags {
                n.push(t.name.clone());
                model.append(&format!("{}  ({})", t.name, t.count));
            }
        })
    };
    reload();

    let selected_name = {
        let selection = selection.clone();
        let names = names.clone();
        move || -> Option<String> {
            let pos = selection.selected();
            names.borrow().get(pos as usize).cloned()
        }
    };

    let rename = Button::with_label("Rename…");
    {
        let state = state.clone();
        let selected_name = selected_name.clone();
        let reload = reload.clone();
        rename.connect_clicked(move |btn| {
            let Some(name) = selected_name() else { return };
            let parent = btn.root().and_downcast::<Window>();
            let state2 = state.clone();
            let reload = reload.clone();
            let name2 = name.clone();
            prompt_text(
                &state,
                parent.as_ref(),
                "Rename tag",
                &format!("New name for \"{name}\":"),
                &name,
                move |new_name| {
                    if let Err(e) = state2.lib.rename_tag(&name2, &new_name) {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    reload();
                    state2.grid().refresh_current();
                    state2.properties().reload_tags();
                },
            );
        });
    }

    let merge = Button::with_label("Merge into…");
    {
        let state = state.clone();
        let selected_name = selected_name.clone();
        let reload = reload.clone();
        merge.connect_clicked(move |btn| {
            let Some(name) = selected_name() else { return };
            let parent = btn.root().and_downcast::<Window>();
            let state2 = state.clone();
            let reload = reload.clone();
            let name2 = name.clone();
            prompt_text(
                &state,
                parent.as_ref(),
                "Merge tag",
                &format!("Merge \"{name}\" into which tag?"),
                "",
                move |dst| {
                    if let Err(e) = state2.lib.merge_tags(&name2, &dst) {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    reload();
                    state2.grid().refresh_current();
                    state2.properties().reload_tags();
                },
            );
        });
    }

    let delete = Button::with_label("Delete");
    delete.add_css_class("destructive-action");
    {
        let state = state.clone();
        let selected_name = selected_name.clone();
        let reload = reload.clone();
        delete.connect_clicked(move |btn| {
            let Some(name) = selected_name() else { return };
            let parent = btn.root().and_downcast::<Window>();
            let state2 = state.clone();
            let reload = reload.clone();
            let name2 = name.clone();
            confirm(
                &state,
                parent.as_ref(),
                "Delete tag",
                &format!("Delete \"{name}\" from all photos?"),
                move || {
                    if let Err(e) = state2.lib.delete_tag(&name2) {
                        show_error(&state2, &e.to_string());
                        return;
                    }
                    reload();
                    state2.grid().refresh_current();
                    state2.properties().reload_tags();
                },
            );
        });
    }

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.append(&rename);
    buttons.append(&merge);
    buttons.append(&delete);

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&buttons);
    root.append(&scroll);

    let window = Window::builder()
        .title("Tag Manager")
        .modal(true)
        .default_width(420)
        .default_height(520)
        .child(&root)
        .build();
    if let Some(win) = state.window() {
        window.set_transient_for(Some(&win));
    }
    window.set_visible(true);
}
