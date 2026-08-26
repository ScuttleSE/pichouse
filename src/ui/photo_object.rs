//! A GObject wrapping a single photo row, so it can live in a `gio::ListStore`
//! and be bound by the GridView factory.

use gtk4::glib;

mod imp {
    use std::cell::RefCell;

    use gtk4::gdk;
    use gtk4::glib;
    use gtk4::glib::Properties;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::PhotoObject)]
    pub struct PhotoObject {
        #[property(get, set)]
        pub id: RefCell<i64>,
        #[property(get, set)]
        pub hash: RefCell<String>,
        #[property(get, set)]
        pub path: RefCell<String>,
        #[property(get, set)]
        pub filename: RefCell<String>,
        #[property(get, set)]
        pub orientation: RefCell<i32>,
        /// `true` when the file is gone from disk (soft "missing"); the cell is
        /// shown dimmed.
        #[property(get, set)]
        pub missing: RefCell<bool>,
        /// The decoded thumbnail, or `None` until a worker fills it in. The
        /// bound `Image` observes this property.
        #[property(get, set, nullable)]
        pub texture: RefCell<Option<gdk::Texture>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PhotoObject {
        const NAME: &'static str = "PichousePhotoObject";
        type Type = super::PhotoObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for PhotoObject {}
}

glib::wrapper! {
    pub struct PhotoObject(ObjectSubclass<imp::PhotoObject>);
}

impl PhotoObject {
    /// Build a `PhotoObject` from a domain `Photo`.
    pub fn from_photo(p: &crate::model::Photo) -> Self {
        glib::Object::builder()
            .property("id", p.id)
            .property("hash", p.hash.clone())
            .property("path", p.path.clone())
            .property("filename", p.filename.clone())
            .property("orientation", p.orientation)
            .property("missing", p.missing)
            .build()
    }
}
