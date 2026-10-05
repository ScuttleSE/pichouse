//! A thumbnail widget for the grid. It crops the photo to fill the cell and
//! moves the crop so a focus box (a face) stays visible. It does not zoom
//! more than the cell needs. In fit mode, it shows the whole photo.

use std::cell::{Cell, RefCell};

use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

thread_local! {
    /// False: never crop. Every thumbnail shows the whole photo.
    static CROP: Cell<bool> = Cell::new(true);
}

/// Turn the square crop on or off for all thumbnails.
pub fn set_crop_enabled(on: bool) {
    CROP.with(|c| c.set(on));
}

/// A box in per-mille of the photo: x, y, w, h.
pub type Focus = (i32, i32, i32, i32);

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ThumbPic {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub focus: Cell<Option<Focus>>,
        pub fit: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ThumbPic {
        const NAME: &'static str = "PichouseThumbPic";
        type Type = super::ThumbPic;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for ThumbPic {}

    impl WidgetImpl for ThumbPic {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width(), obj.height());
            let Some(tex) = self.texture.borrow().clone() else {
                return;
            };
            let Some((x, y, dw, dh)) = obj.draw_rect(w, h) else {
                return;
            };
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            snapshot.append_texture(
                &tex,
                &graphene::Rect::new(x as f32, y as f32, dw as f32, dh as f32),
            );
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct ThumbPic(ObjectSubclass<imp::ThumbPic>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ThumbPic {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ThumbPic {
    pub fn new() -> ThumbPic {
        ThumbPic::default()
    }

    pub fn set_texture(&self, t: Option<&gdk::Texture>) {
        *self.imp().texture.borrow_mut() = t.cloned();
        self.queue_draw();
    }

    pub fn has_texture(&self) -> bool {
        self.imp().texture.borrow().is_some()
    }

    pub fn set_focus_box(&self, f: Option<Focus>) {
        if self.imp().focus.get() != f {
            self.imp().focus.set(f);
            self.queue_draw();
        }
    }

    /// True: show the whole photo. False: crop to fill the cell.
    pub fn set_fit(&self, fit: bool) {
        self.imp().fit.set(fit);
        self.queue_draw();
    }

    /// The rectangle where the photo draws in a `w` x `h` area: x, y, width,
    /// height. The rectangle can be larger than the area in crop mode.
    pub fn draw_rect(&self, w: i32, h: i32) -> Option<(f64, f64, f64, f64)> {
        let imp = self.imp();
        let tex = imp.texture.borrow();
        let tex = tex.as_ref()?;
        let (iw, ih) = (tex.width() as f64, tex.height() as f64);
        let (aw, ah) = (w as f64, h as f64);
        if iw <= 0.0 || ih <= 0.0 || aw <= 0.0 || ah <= 0.0 {
            return None;
        }
        if imp.fit.get() || !CROP.with(|c| c.get()) {
            let s = (aw / iw).min(ah / ih);
            let (dw, dh) = (iw * s, ih * s);
            return Some(((aw - dw) / 2.0, (ah - dh) / 2.0, dw, dh));
        }
        let s = (aw / iw).max(ah / ih);
        let (dw, dh) = (iw * s, ih * s);
        let (fx, fy) = match imp.focus.get() {
            Some((x, y, bw, bh)) => (
                (x as f64 + bw as f64 / 2.0) / 1000.0 * dw,
                (y as f64 + bh as f64 / 2.0) / 1000.0 * dh,
            ),
            None => (dw / 2.0, dh / 2.0),
        };
        let x = (aw / 2.0 - fx).clamp(aw - dw, 0.0);
        let y = (ah / 2.0 - fy).clamp(ah - dh, 0.0);
        Some((x, y, dw, dh))
    }
}
