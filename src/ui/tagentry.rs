//! A tag entry with autocomplete.
//!
//! The widget is an `Entry` and a suggestion popover. The popover shows the
//! existing tags that start with the typed text (then tags that contain it),
//! the most-used tags first. Up and Down move in the list. Tab or Enter
//! accepts a suggestion. Esc closes the list. Enter with no open list sends
//! the text to the `on_submit` callback.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{Entry, Label, ListBox, Popover, PositionType};

use crate::db::Library;

const MAX_SUGGESTIONS: usize = 10;

pub struct TagEntry {
    pub entry: Entry,
    popover: Popover,
    list: ListBox,
    lib: Arc<Library>,
    /// The tag vocabulary (name, count), loaded on focus.
    vocab: RefCell<Vec<(String, i64)>>,
    on_submit: RefCell<Option<Box<dyn Fn(String)>>>,
    /// Set while the code changes the text, so `changed` does not reopen the
    /// list.
    muted: std::cell::Cell<bool>,
}

impl TagEntry {
    pub fn new(lib: Arc<Library>, placeholder: &str) -> Rc<Self> {
        let entry = Entry::new();
        entry.set_placeholder_text(Some(placeholder));
        entry.set_hexpand(true);
        let list = ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Browse);
        let popover = Popover::new();
        popover.set_child(Some(&list));
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(PositionType::Bottom);
        popover.set_can_focus(false);
        popover.set_parent(&entry);
        let this = Rc::new(Self {
            entry,
            popover,
            list,
            lib,
            vocab: RefCell::new(Vec::new()),
            on_submit: RefCell::new(None),
            muted: std::cell::Cell::new(false),
        });

        // Remove the popover with the entry, to avoid a GTK warning.
        {
            let pop = this.popover.clone();
            this.entry.connect_destroy(move |_| pop.unparent());
        }

        // Load the vocabulary when the entry gets the focus.
        {
            let w = Rc::downgrade(&this);
            let focus = gtk4::EventControllerFocus::new();
            focus.connect_enter(move |_| {
                if let Some(t) = w.upgrade() {
                    t.reload_vocab();
                }
            });
            let w = Rc::downgrade(&this);
            focus.connect_leave(move |c| {
                // Wait a little, so a click on a suggestion still works.
                let w = w.clone();
                let c = c.clone();
                gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
                    if let Some(t) = w.upgrade() {
                        if !c.contains_focus() {
                            t.popover.popdown();
                        }
                    }
                });
            });
            this.entry.add_controller(focus);
        }

        {
            let w = Rc::downgrade(&this);
            this.entry.connect_changed(move |_| {
                if let Some(t) = w.upgrade() {
                    if !t.muted.get() {
                        t.update_suggestions();
                    }
                }
            });
        }

        // Keys: Up/Down move, Tab accepts, Esc closes.
        {
            let w = Rc::downgrade(&this);
            let keys = gtk4::EventControllerKey::new();
            keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(t) = w.upgrade() else {
                    return gtk4::glib::Propagation::Proceed;
                };
                let open = t.popover.is_visible();
                use gtk4::gdk::Key;
                match key {
                    Key::Down if open => t.move_sel(1),
                    Key::Up if open => t.move_sel(-1),
                    Key::Tab if open => t.accept_selected(),
                    Key::Escape if open => t.popover.popdown(),
                    _ => return gtk4::glib::Propagation::Proceed,
                }
                gtk4::glib::Propagation::Stop
            });
            this.entry.add_controller(keys);
        }

        // Enter: accept the selected suggestion, or submit the text.
        {
            let w = Rc::downgrade(&this);
            this.entry.connect_activate(move |_| {
                let Some(t) = w.upgrade() else { return };
                if t.popover.is_visible() && t.list.selected_row().is_some() {
                    t.accept_selected();
                }
                t.submit();
            });
        }

        // A click on a suggestion accepts it and submits it.
        {
            let w = Rc::downgrade(&this);
            this.list.connect_row_activated(move |_, row| {
                let Some(t) = w.upgrade() else { return };
                t.list.select_row(Some(row));
                t.accept_selected();
                t.submit();
            });
        }
        this
    }

    /// Set the callback for Enter. The entry clears after the call.
    /// Set the text without opening the suggestion list.
    pub fn set_text(&self, text: &str) {
        self.muted.set(true);
        self.entry.set_text(text);
        self.muted.set(false);
    }

    pub fn set_on_submit<F: Fn(String) + 'static>(&self, f: F) {
        *self.on_submit.borrow_mut() = Some(Box::new(f));
    }

    /// Send the text to `on_submit` and clear the entry.
    pub fn submit(&self) {
        let text = self.entry.text().trim().to_string();
        self.popover.popdown();
        if text.is_empty() {
            return;
        }
        // With no callback, the entry is a plain value field. Keep the text.
        let Some(cb) = self.on_submit.borrow_mut().take() else { return };
        cb(text);
        *self.on_submit.borrow_mut() = Some(cb);
        self.muted.set(true);
        self.entry.set_text("");
        self.muted.set(false);
        // A new tag can exist now.
        self.reload_vocab();
    }

    pub fn reload_vocab(&self) {
        let mut v: Vec<(String, i64)> = self
            .lib
            .all_tags()
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.name, t.count))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        *self.vocab.borrow_mut() = v;
    }

    fn update_suggestions(&self) {
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        let q = self.entry.text().trim().to_lowercase();
        // Open the list only while the user types in the entry.
        let typing = self.entry.root().and_then(|r| r.focus()).map(|f| f.is_ancestor(&self.entry)).unwrap_or(false);
        if q.is_empty() || !typing {
            self.popover.popdown();
            return;
        }
        let vocab = self.vocab.borrow();
        let mut hits: Vec<&(String, i64)> =
            vocab.iter().filter(|(n, _)| n.to_lowercase().starts_with(&q)).collect();
        if hits.len() < MAX_SUGGESTIONS {
            hits.extend(vocab.iter().filter(|(n, _)| {
                let l = n.to_lowercase();
                !l.starts_with(&q) && l.contains(&q)
            }));
        }
        // Do not suggest the exact text alone.
        hits.retain(|(n, _)| n.to_lowercase() != q);
        hits.truncate(MAX_SUGGESTIONS);
        if hits.is_empty() {
            self.popover.popdown();
            return;
        }
        for (name, count) in hits {
            let l = Label::new(Some(&format!("{name}  ({count})")));
            l.set_xalign(0.0);
            unsafe {
                l.set_data("tag-name", name.clone());
            }
            self.list.append(&l);
        }
        self.list.select_row(self.list.row_at_index(0).as_ref());
        self.popover.set_width_request(self.entry.width().max(160));
        self.popover.popup();
        // The popover must not take the focus from the entry.
        self.entry.grab_focus_without_selecting();
        self.entry.set_position(-1);
    }

    fn move_sel(&self, d: i32) {
        let cur = self.list.selected_row().map(|r| r.index()).unwrap_or(-1);
        if let Some(r) = self.list.row_at_index((cur + d).max(0)) {
            self.list.select_row(Some(&r));
        }
    }

    /// Put the selected suggestion into the entry.
    fn accept_selected(&self) {
        let Some(row) = self.list.selected_row() else { return };
        let Some(label) = row.child() else { return };
        let name: Option<String> = unsafe { label.data::<String>("tag-name").map(|p| p.as_ref().clone()) };
        if let Some(name) = name {
            self.muted.set(true);
            self.entry.set_text(&name);
            self.entry.set_position(-1);
            self.muted.set(false);
        }
        self.popover.popdown();
    }
}
