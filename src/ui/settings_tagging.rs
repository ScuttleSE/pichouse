//! Tagging settings pane: a "General" tab and an "AI" tab.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, DropDown, Label, Notebook, Orientation, StringList};

use super::prefs;
use super::state::AppState;

/// The tag icon modes. The value is the setting text.
pub const ICON_MODES: [(&str, &str); 4] = [
    ("auto", "Tagged photos always, untagged on mouseover"),
    ("hover", "Only on mouseover"),
    ("always", "Always"),
    ("off", "Off"),
];

/// The tag icon shapes. The value is the setting text.
pub const ICON_SHAPES: [(&str, &str); 3] = [("tag", "Tag"), ("dot", "Dot"), ("hash", "Hash sign (#)")];

pub fn tagging_pane(state: &Rc<AppState>) -> Notebook {
    let nb = Notebook::new();
    nb.append_page(&general_tab(state), Some(&Label::new(Some("General"))));
    nb.append_page(&super::settings_ai::ai_pane(state), Some(&Label::new(Some("AI"))));
    nb
}

fn general_tab(state: &Rc<AppState>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let head = Label::new(Some("Tag icon on thumbnails"));
    head.set_xalign(0.0);
    head.add_css_class("heading");
    root.append(&head);

    root.append(&choice_row(state, "Show the icon", prefs::KEY_TAG_ICON_MODE, "auto", &ICON_MODES));
    root.append(&choice_row(state, "Icon shape", prefs::KEY_TAG_ICON_SHAPE, "tag", &ICON_SHAPES));
    root
}

fn choice_row(
    state: &Rc<AppState>,
    title: &str,
    key: &'static str,
    default: &str,
    choices: &'static [(&'static str, &'static str)],
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    let l = Label::new(Some(title));
    l.set_xalign(0.0);
    l.set_width_chars(14);
    row.append(&l);
    let labels: Vec<&str> = choices.iter().map(|c| c.1).collect();
    let drop = DropDown::new(Some(StringList::new(&labels)), gtk4::Expression::NONE);
    drop.set_hexpand(true);
    let cur = state.lib.get_setting(key, default).unwrap_or_default();
    drop.set_selected(choices.iter().position(|c| c.0 == cur).unwrap_or(0) as u32);
    let state = state.clone();
    drop.connect_selected_notify(move |d| {
        let v = choices.get(d.selected() as usize).map(|c| c.0).unwrap_or(default_of(choices));
        let _ = state.lib.set_setting(key, v);
        apply(&state);
    });
    row.append(&drop);
    row
}

fn default_of(c: &'static [(&'static str, &'static str)]) -> &'static str {
    c[0].0
}

/// Read the tag icon settings and apply them to the grid.
pub fn apply(state: &Rc<AppState>) {
    let mode = state.lib.get_setting(prefs::KEY_TAG_ICON_MODE, "auto").unwrap_or_default();
    let shape = state.lib.get_setting(prefs::KEY_TAG_ICON_SHAPE, "tag").unwrap_or_default();
    state.grid().set_tag_icon(&mode, &shape);
}
