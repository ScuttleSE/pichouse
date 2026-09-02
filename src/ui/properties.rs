//! Right-hand properties panel: a tabbed panel with "Pic Info" and "Tags".

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, Label, Notebook, Orientation, ScrolledWindow, Separator,
};

use crate::model::{Photo, Tag, TagSource};

use super::editor::EditPanel;
use super::state::{show_error, AppState};
use super::util::{escape_markup, format_unix, human_size};

/// The right-hand properties panel.
pub struct Properties {
    root: Notebook,
    title: Label,
    location: Label,
    size: Label,
    date: Label,
    dims: Label,

    tag_list: GtkBox,
    tag_entry: Entry,
    tag_now: Button,

    edit: Rc<EditPanel>,

    current: RefCell<Option<Photo>>,
    state: RefCell<Option<Rc<AppState>>>,
}

impl Properties {
    /// Build the panel. `bind_state` must be called once before use.
    pub fn new() -> Rc<Properties> {
        let title = bold_label("Properties");
        let location = value_label();
        let size = value_label();
        let date = value_label();
        let dims = value_label();

        let info = GtkBox::new(Orientation::Vertical, 4);
        info.set_margin_top(8);
        info.set_margin_bottom(8);
        info.set_margin_start(8);
        info.set_margin_end(8);
        info.append(&title);
        info.append(&Separator::new(Orientation::Horizontal));
        info.append(&field("Location", &location));
        info.append(&field("File Size", &size));
        info.append(&field("File Date", &date));
        info.append(&field("Dimensions", &dims));

        let tag_list = GtkBox::new(Orientation::Vertical, 2);
        let tag_entry = Entry::new();
        let tag_now = Button::with_label("Tag this photo with AI");

        let notebook = Notebook::new();
        notebook.set_size_request(260, -1);
        notebook.append_page(&info, Some(&Label::new(Some("Pic Info"))));

        let edit = EditPanel::new();

        let props = Rc::new(Properties {
            root: notebook,
            title,
            location,
            size,
            date,
            dims,
            tag_list,
            tag_entry,
            tag_now,
            edit,
            current: RefCell::new(None),
            state: RefCell::new(None),
        });

        let tags_tab = props.build_tags_tab();
        props
            .root
            .append_page(&tags_tab, Some(&Label::new(Some("Tags"))));
        props
            .root
            .append_page(props.edit.widget(), Some(&Label::new(Some("Edit"))));

        props.clear();
        props
    }

    /// Give the panel access to shared state and wire tag actions.
    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state.clone());
        self.edit.bind_state(state.clone());

        // Add-tag on entry activate and button click.
        let this = self.clone();
        self.tag_entry.connect_activate(move |_| this.add_tag());
        // The add button is the last child of the entry's row; we connect it in
        // build_tags_tab by capturing self. Here we wire tag_now.
        let this = self.clone();
        self.tag_now.connect_clicked(move |_| {
            let cur = this.current.borrow().clone();
            if let (Some(state), Some(p)) = (this.state.borrow().clone(), cur) {
                super::aitag::tag_one_photo_now(&state, p);
            }
        });
    }

    /// The panel root widget.
    pub fn widget(&self) -> &Notebook {
        &self.root
    }

    /// Show or hide the panel.
    pub fn set_visible(&self, v: bool) {
        self.root.set_visible(v);
    }

    fn build_tags_tab(self: &Rc<Self>) -> GtkBox {
        let box_ = GtkBox::new(Orientation::Vertical, 6);
        box_.set_margin_top(8);
        box_.set_margin_bottom(8);
        box_.set_margin_start(8);
        box_.set_margin_end(8);

        let add_row = GtkBox::new(Orientation::Horizontal, 4);
        self.tag_entry.set_hexpand(true);
        self.tag_entry.set_placeholder_text(Some("Add a tag…"));
        let add_btn = Button::from_icon_name("list-add-symbolic");
        add_btn.set_tooltip_text(Some("Add tag"));
        let this = self.clone();
        add_btn.connect_clicked(move |_| this.add_tag());
        add_row.append(&self.tag_entry);
        add_row.append(&add_btn);
        box_.append(&add_row);

        box_.append(&self.tag_now);
        box_.append(&Separator::new(Orientation::Horizontal));

        let scroll = ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_child(Some(&self.tag_list));
        box_.append(&scroll);

        box_
    }

    fn add_tag(self: &Rc<Self>) {
        let cur = self.current.borrow().clone();
        let Some(p) = cur else { return };
        if p.id == 0 {
            return;
        }
        let name = self.tag_entry.text().to_string();
        if name.trim().is_empty() {
            return;
        }
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        if let Err(e) = state.lib.add_photo_tags(p.id, &[name], TagSource::User) {
            show_error(&state, &e.to_string());
            return;
        }
        self.tag_entry.set_text("");
        self.reload_tags();
    }

    /// Repopulate the tag list for the current photo.
    pub fn reload_tags(self: &Rc<Self>) {
        while let Some(child) = self.tag_list.first_child() {
            self.tag_list.remove(&child);
        }
        let cur = self.current.borrow().clone();
        let Some(p) = cur else { return };
        if p.id == 0 {
            return;
        }
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let tags = match state.lib.photo_tags(p.id) {
            Ok(t) => t,
            Err(_) => return,
        };
        if tags.is_empty() {
            let none = Label::new(Some("No tags yet."));
            none.set_xalign(0.0);
            self.tag_list.append(&none);
            return;
        }
        for t in tags {
            self.tag_list.append(&self.tag_row(&state, p.id, &t));
        }
    }

    fn tag_row(self: &Rc<Self>, state: &Rc<AppState>, photo_id: i64, t: &Tag) -> GtkBox {
        let row = GtkBox::new(Orientation::Horizontal, 4);

        let (marker, tip) = match (t.source, t.confirmed) {
            (TagSource::User, _) => ("•", "User tag"),
            (TagSource::Ai, true) => ("✓", "AI tag (confirmed)"),
            (TagSource::Ai, false) => ("◆", "AI tag"),
        };
        let badge = Label::new(Some(marker));
        badge.set_tooltip_text(Some(tip));

        let name = Label::new(Some(&t.name));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_wrap(true);

        row.append(&badge);
        row.append(&name);

        if t.source == TagSource::Ai && !t.confirmed {
            let confirm = Button::from_icon_name("object-select-symbolic");
            confirm.set_tooltip_text(Some("Confirm this AI tag"));
            confirm.set_has_frame(false);
            let this = self.clone();
            let state = state.clone();
            let tag_name = t.name.clone();
            confirm.connect_clicked(move |_| {
                if let Err(e) = state.lib.confirm_photo_tag(photo_id, &tag_name) {
                    show_error(&state, &e.to_string());
                    return;
                }
                this.reload_tags();
            });
            row.append(&confirm);
        }

        let remove = Button::from_icon_name("edit-delete-symbolic");
        remove.set_tooltip_text(Some("Remove tag from this photo"));
        remove.set_has_frame(false);
        let this = self.clone();
        let state = state.clone();
        let tag_name = t.name.clone();
        remove.connect_clicked(move |_| {
            if let Err(e) = state.lib.remove_photo_tag(photo_id, &tag_name) {
                show_error(&state, &e.to_string());
                return;
            }
            this.reload_tags();
        });
        row.append(&remove);

        row
    }

    /// Reset the panel to an empty state.
    pub fn clear(self: &Rc<Self>) {
        *self.current.borrow_mut() = None;
        self.title.set_markup("<b>Properties</b>");
        self.location.set_text("—");
        self.size.set_text("—");
        self.date.set_text("—");
        self.dims.set_text("—");
        self.tag_now.set_sensitive(false);
        self.reload_tags();
        self.edit.load(None);
    }

    /// Switch to the Edit tab. Called when the user starts editing a photo.
    pub fn open_edit_tab(self: &Rc<Self>) {
        // Edit is the third page (index 2): Pic Info, Tags, Edit.
        self.root.set_current_page(Some(2));
    }

    /// Populate the panel from a photo.
    pub fn show(self: &Rc<Self>, photo: &Photo) {
        *self.current.borrow_mut() = Some(photo.clone());
        self.title.set_markup(&format!(
            "<b>Properties of {}</b>",
            escape_markup(&photo.filename)
        ));
        let dir = std::path::Path::new(&photo.path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.location.set_text(&dir);
        self.size.set_text(&human_size(photo.size));
        let when = if photo.taken_at > 0 {
            photo.taken_at
        } else {
            photo.mod_time
        };
        self.date.set_text(&format_unix(when));
        if photo.width > 0 && photo.height > 0 {
            self.dims
                .set_text(&format!("{} x {}", photo.width, photo.height));
        } else {
            self.dims.set_text("—");
        }
        self.tag_now.set_sensitive(photo.id != 0);
        self.reload_tags();
        self.edit.load(Some(photo.clone()));
    }
}

fn bold_label(text: &str) -> Label {
    let l = Label::new(None);
    l.set_xalign(0.0);
    l.set_markup(&format!("<b>{}</b>", escape_markup(text)));
    l
}

fn value_label() -> Label {
    let l = Label::new(None);
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_selectable(true);
    l
}

fn field(caption: &str, value: &Label) -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 0);
    b.set_margin_top(6);
    b.append(&bold_label(caption));
    b.append(value);
    b
}
