//! Left sidebar: a list of scanned folders. Selecting one loads it in the grid.
//!
//! This is the first navigation step. The richer album tree, drag-and-drop, and
//! context menus from the Go app are added in later sub-steps.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Label, ListItem, ListView, PolicyType, ScrolledWindow, SignalListItemFactory, SingleSelection,
};

use crate::model::Folder;

/// A folder row exposed to the list model as a GObject.
mod folder_object {
    use gtk4::glib;

    mod imp {
        use std::cell::RefCell;

        use gtk4::glib;
        use gtk4::glib::Properties;
        use gtk4::prelude::*;
        use gtk4::subclass::prelude::*;

        #[derive(Properties, Default)]
        #[properties(wrapper_type = super::FolderObject)]
        pub struct FolderObject {
            #[property(get, set)]
            pub id: RefCell<i64>,
            #[property(get, set)]
            pub name: RefCell<String>,
            #[property(get, set)]
            pub year: RefCell<i32>,
            #[property(get, set)]
            pub count: RefCell<i32>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for FolderObject {
            const NAME: &'static str = "PichouseFolderObject";
            type Type = super::FolderObject;
        }

        #[glib::derived_properties]
        impl ObjectImpl for FolderObject {}
    }

    glib::wrapper! {
        pub struct FolderObject(ObjectSubclass<imp::FolderObject>);
    }

    impl FolderObject {
        pub fn new(id: i64, name: &str, year: i32, count: i32) -> Self {
            glib::Object::builder()
                .property("id", id)
                .property("name", name)
                .property("year", year)
                .property("count", count)
                .build()
        }
    }
}

use folder_object::FolderObject;

/// The folder-list sidebar.
pub struct Sidebar {
    root: gtk4::Box,
    store: gio::ListStore,
    selection: SingleSelection,
}

impl Sidebar {
    /// Build the sidebar. `on_select` is called with a folder id when the user
    /// selects a folder row.
    pub fn new<F: Fn(i64) + 'static>(on_select: F) -> Sidebar {
        let store = gio::ListStore::new::<FolderObject>();
        let selection = SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let factory = SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
            let label = Label::new(None);
            label.set_xalign(0.0);
            label.set_margin_start(6);
            label.set_margin_end(6);
            label.set_margin_top(3);
            label.set_margin_bottom(3);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
            let Some(f) = item.item().and_downcast::<FolderObject>() else {
                return;
            };
            let Some(label) = item.child().and_downcast::<Label>() else {
                return;
            };
            let count = f.count();
            let year = f.year();
            let text = if year > 0 {
                format!("{}  ({}) — {}", f.name(), count, year)
            } else {
                format!("{}  ({})", f.name(), count)
            };
            label.set_text(&text);
        });

        let list_view = ListView::new(Some(selection.clone()), Some(factory));

        // Fire the callback when the selected row changes.
        let on_select = Rc::new(on_select);
        {
            let selection = selection.clone();
            let store = store.clone();
            let on_select = on_select.clone();
            selection.connect_selected_notify(move |sel| {
                let pos = sel.selected();
                if pos == gtk4::INVALID_LIST_POSITION {
                    return;
                }
                if let Some(obj) = store.item(pos).and_downcast::<FolderObject>() {
                    on_select(obj.id());
                }
            });
        }

        let header = Label::new(Some("Folders"));
        header.set_xalign(0.0);
        header.set_margin_start(8);
        header.set_margin_top(6);
        header.set_margin_bottom(4);
        header.add_css_class("heading");

        let scroller = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vexpand(true)
            .child(&list_view)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_size_request(280, -1);
        root.append(&header);
        root.append(&scroller);

        Sidebar {
            root,
            store,
            selection,
        }
    }

    /// The sidebar's root widget.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Replace the folder list. `counts` maps folder id to its photo count.
    pub fn set_folders(&self, folders: &[Folder], counts: &std::collections::HashMap<i64, i64>) {
        self.store.remove_all();
        for f in folders {
            let count = counts.get(&f.id).copied().unwrap_or(0) as i32;
            self.store
                .append(&FolderObject::new(f.id, &f.name, f.year, count));
        }
    }

    /// Select the first folder, if any, without firing a spurious callback for
    /// an empty list. Returns the selected folder id.
    pub fn select_first(&self) -> Option<i64> {
        if self.store.n_items() == 0 {
            return None;
        }
        self.selection.set_selected(0);
        self.store
            .item(0)
            .and_downcast::<FolderObject>()
            .map(|o| o.id())
    }
}
