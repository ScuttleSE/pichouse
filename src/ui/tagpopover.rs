//! The tag popover of a grid cell.
//!
//! The popover lists the tags of the target photos. With more than one
//! target photo, each tag shows a count ("cat (3/5)"). A user tag uses the
//! normal text color. An unconfirmed AI tag uses the `tag-ai` color (muted
//! blue) and has a confirm button. Each tag has a remove button. The entry at
//! the bottom adds a tag to all target photos, with autocomplete.

use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Orientation, Popover, ScrolledWindow};

use super::tagentry::TagEntry;
use crate::db::Library;

/// Open the popover on `anchor`. `on_changed` runs after each change.
pub fn show(anchor: &impl IsA<gtk4::Widget>, lib: Arc<Library>, ids: Vec<i64>, on_changed: Rc<dyn Fn()>) {
    let popover = Popover::new();
    popover.set_parent(anchor);
    popover.set_position(gtk4::PositionType::Bottom);

    let root = GtkBox::new(Orientation::Vertical, 6);
    root.set_margin_top(6);
    root.set_margin_bottom(6);
    root.set_margin_start(6);
    root.set_margin_end(6);
    root.set_width_request(260);

    let title = Label::new(None);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    root.append(&title);

    let list = GtkBox::new(Orientation::Vertical, 2);
    let scroll = ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(300);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    let entry = TagEntry::new(lib.clone(), "Add a tag…");
    root.append(&entry.entry);
    popover.set_child(Some(&root));

    let ids = Rc::new(ids);
    let reload: Rc<Box<dyn Fn()>> = {
        let (lib, ids, list, title, on_changed) = (lib.clone(), ids.clone(), list.clone(), title.clone(), on_changed.clone());
        Rc::new_cyclic(|weak: &std::rc::Weak<Box<dyn Fn()>>| {
            let weak = weak.clone();
            Box::new(move || {
                while let Some(c) = list.first_child() {
                    list.remove(&c);
                }
                let n = ids.len();
                title.set_text(&if n == 1 { "Tags".to_string() } else { format!("Tags of {n} photos") });
                let tags = lib.tags_for_photos(&ids).unwrap_or_default();
                if tags.is_empty() {
                    let l = Label::new(Some("No tags yet."));
                    l.set_xalign(0.0);
                    l.add_css_class("dim-label");
                    list.append(&l);
                }
                for t in tags {
                    let row = GtkBox::new(Orientation::Horizontal, 4);
                    let text = if n == 1 { t.name.clone() } else { format!("{} ({}/{n})", t.name, t.count) };
                    let l = Label::new(Some(&text));
                    l.set_xalign(0.0);
                    l.set_hexpand(true);
                    l.set_wrap(true);
                    if t.ai_only {
                        l.add_css_class("tag-ai");
                        l.set_tooltip_text(Some("AI tag (not confirmed)"));
                    }
                    row.append(&l);
                    let act = |icon: &str, tip: &str, f: Box<dyn Fn(&Library, &[i64], &str)>| {
                        let b = Button::from_icon_name(icon);
                        b.set_has_frame(false);
                        b.set_tooltip_text(Some(tip));
                        let (lib, ids, name, weak, on_changed) = (lib.clone(), ids.clone(), t.name.clone(), weak.clone(), on_changed.clone());
                        b.connect_clicked(move |_| {
                            f(&lib, &ids, &name);
                            on_changed();
                            if let Some(r) = weak.upgrade() {
                                (*r)();
                            }
                        });
                        b
                    };
                    if t.ai_only {
                        row.append(&act("object-select-symbolic", "Confirm this AI tag", Box::new(|l, i, n| {
                            let _ = l.confirm_tag_on_photos(i, n);
                        })));
                    }
                    row.append(&act("window-close-symbolic", "Remove this tag", Box::new(|l, i, n| {
                        let _ = l.remove_tag_from_photos(i, n);
                    })));
                    list.append(&row);
                }
            })
        })
    };
    (*reload)();

    {
        let (lib, ids, on_changed, reload) = (lib.clone(), ids.clone(), on_changed.clone(), reload.clone());
        entry.set_on_submit(move |name| {
            let _ = lib.add_tags_to_photos(&ids, &[name]);
            on_changed();
            (*reload)();
        });
    }

    // Keep the entry alive while the popover lives. Remove the popover on
    // close.
    {
        let keep = entry.clone();
        popover.connect_closed(move |p| {
            let _ = &keep;
            let p = p.clone();
            gtk4::glib::idle_add_local_once(move || p.unparent());
        });
    }
    popover.popup();
    entry.entry.grab_focus();
}
