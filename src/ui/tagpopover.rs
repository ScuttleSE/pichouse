//! The tag popover of a grid cell.
//!
//! The popover lists the tags of the target photos. With more than one
//! target photo, each tag shows a count ("cat (3/5)"). A user tag uses the
//! normal text color. An unconfirmed AI tag uses the `tag-ai` color (muted
//! blue) and has a confirm button. Each tag has a remove button. The entry at
//! the bottom adds a tag to all target photos, with autocomplete.
//!
//! When the PTR tag database is on, a second list shows the PTR tags that the
//! photos do not have yet, in the `tag-ptr` color. Each has an add button.

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

    let root = GtkBox::new(Orientation::Vertical, 3);
    root.set_width_request(190);
    popover.add_css_class("tag-popover");

    let title = Label::new(None);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    root.append(&title);

    let list = GtkBox::new(Orientation::Vertical, 0);
    let scroll = ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(300);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    // PTR tags (potential tags). Hidden when the PTR is off.
    let ptr_box = GtkBox::new(Orientation::Vertical, 0);
    root.append(&ptr_box);

    let entry = TagEntry::new(lib.clone(), "Add a tag…");
    root.append(&entry.entry);
    popover.set_child(Some(&root));

    let ids = Rc::new(ids);
    let reload: Rc<Box<dyn Fn()>> = {
        let (lib, ids, list, title, on_changed, ptr_box) =
            (lib.clone(), ids.clone(), list.clone(), title.clone(), on_changed.clone(), ptr_box.clone());
        Rc::new_cyclic(|weak: &std::rc::Weak<Box<dyn Fn()>>| {
            let weak = weak.clone();
            Box::new(move || {
                while let Some(c) = list.first_child() {
                    list.remove(&c);
                }
                let n = ids.len();
                title.set_text(&if n == 1 { "Tags".to_string() } else { format!("Tags of {n} photos") });
                let tags = lib.tags_for_photos(&ids).unwrap_or_default();
                fill_ptr(&ptr_box, &lib, &ids, &tags, &on_changed, &weak);
                if tags.is_empty() {
                    let l = Label::new(Some("No tags yet."));
                    l.set_xalign(0.0);
                    l.add_css_class("dim-label");
                    list.append(&l);
                }
                for t in tags {
                    let row = GtkBox::new(Orientation::Horizontal, 2);
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
                        b.add_css_class("tag-row-btn");
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

/// Fill the PTR list: the PTR tags that not all target photos have.
fn fill_ptr(
    ptr_box: &GtkBox,
    lib: &Arc<Library>,
    ids: &Rc<Vec<i64>>,
    have: &[crate::db::tags::GroupTag],
    on_changed: &Rc<dyn Fn()>,
    weak: &std::rc::Weak<Box<dyn Fn()>>,
) {
    while let Some(c) = ptr_box.first_child() {
        ptr_box.remove(&c);
    }
    let Some(map) = super::ptrui::lookup(lib, ids) else { return };
    let n = ids.len();
    // Tag name -> the photos with it in the PTR.
    let mut by_tag: std::collections::BTreeMap<String, Vec<i64>> = Default::default();
    for (id, tags) in &map {
        for t in tags {
            by_tag.entry(t.clone()).or_default().push(*id);
        }
    }
    by_tag.retain(|name, pids| {
        let on = have.iter().find(|g| &g.name == name).map(|g| g.count).unwrap_or(0);
        (on as usize) < pids.len()
    });
    let head = GtkBox::new(Orientation::Horizontal, 2);
    let l = Label::new(Some(&format!("PTR tags ({})", by_tag.len())));
    l.set_xalign(0.0);
    l.set_hexpand(true);
    l.add_css_class("heading");
    head.append(&l);
    if by_tag.is_empty() {
        let none = Label::new(Some(if map.values().all(|v| v.is_empty()) { "No PTR match." } else { "All PTR tags added." }));
        none.add_css_class("dim-label");
        head.append(&none);
        ptr_box.append(&head);
        return;
    }
    let all = Button::with_label("Add all");
    all.set_has_frame(false);
    all.set_tooltip_text(Some("Add all PTR tags to the photos"));
    head.append(&all);
    ptr_box.append(&head);

    let run = |lib: &Arc<Library>, items: Vec<(Vec<i64>, String)>, on_changed: &Rc<dyn Fn()>, weak: &std::rc::Weak<Box<dyn Fn()>>| {
        for (pids, name) in items {
            for id in pids {
                let _ = lib.add_photo_tags(id, &[name.clone()], crate::model::TagSource::User);
            }
        }
        on_changed();
        if let Some(r) = weak.upgrade() {
            (*r)();
        }
    };
    {
        let items: Vec<(Vec<i64>, String)> = by_tag.iter().map(|(k, v)| (v.clone(), k.clone())).collect();
        let (lib, on_changed, weak) = (lib.clone(), on_changed.clone(), weak.clone());
        all.connect_clicked(move |_| run(&lib, items.clone(), &on_changed, &weak));
    }
    for (name, pids) in by_tag {
        let row = GtkBox::new(Orientation::Horizontal, 2);
        let text = if n == 1 { name.clone() } else { format!("{name} ({}/{n})", pids.len()) };
        let l = Label::new(Some(&text));
        l.set_xalign(0.0);
        l.set_hexpand(true);
        l.set_wrap(true);
        l.add_css_class("tag-ptr");
        l.set_tooltip_text(Some("PTR tag (not added)"));
        row.append(&l);
        let b = Button::from_icon_name("list-add-symbolic");
        b.set_has_frame(false);
        b.add_css_class("tag-row-btn");
        b.set_tooltip_text(Some("Add this PTR tag"));
        let (lib, on_changed, weak) = (lib.clone(), on_changed.clone(), weak.clone());
        b.connect_clicked(move |_| run(&lib, vec![(pids.clone(), name.clone())], &on_changed, &weak));
        row.append(&b);
        ptr_box.append(&row);
    }
}
