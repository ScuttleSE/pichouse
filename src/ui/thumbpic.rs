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

thread_local! {
    /// True: fade between the crop and the whole photo on mouseover.
    static FADE: Cell<bool> = Cell::new(true);
}

/// The fade time in microseconds.
const FADE_US: f64 = 150_000.0;

/// Turn the mouseover fade on or off for all thumbnails.
pub fn set_fade_enabled(on: bool) {
    FADE.with(|c| c.set(on));
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
        /// The fade state: 0.0 is the crop, 1.0 is the whole photo.
        pub progress: Cell<f64>,
        /// True while a fade tick callback runs.
        pub ticking: Cell<bool>,
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
            let t = self.progress.get();
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            // Draw the crop, then the whole photo on top with opacity `t`.
            for (fit, alpha) in [(false, 1.0 - t), (true, t)] {
                if alpha <= 0.0 {
                    continue;
                }
                let Some((x, y, dw, dh)) = obj.rect_for(w, h, fit) else {
                    continue;
                };
                snapshot.push_opacity(alpha);
                snapshot.append_texture(
                    &tex,
                    &graphene::Rect::new(x as f32, y as f32, dw as f32, dh as f32),
                );
                snapshot.pop();
            }
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

    /// Show the crop at once, with no fade. Use this on a cell rebind.
    pub fn reset_fit(&self) {
        let imp = self.imp();
        imp.fit.set(false);
        imp.progress.set(0.0);
        self.queue_draw();
    }

    /// True: show the whole photo. False: crop to fill the cell.
    pub fn set_fit(&self, fit: bool) {
        let imp = self.imp();
        imp.fit.set(fit);
        let target = if fit { 1.0 } else { 0.0 };
        if !FADE.with(|c| c.get()) || !self.is_mapped() {
            imp.progress.set(target);
            self.queue_draw();
            return;
        }
        if imp.ticking.get() {
            return;
        }
        imp.ticking.set(true);
        let last: Cell<Option<i64>> = Cell::new(None);
        self.add_tick_callback(move |w, clock| {
            let imp = w.imp();
            let now = clock.frame_time();
            let dt = last.get().map(|l| (now - l) as f64).unwrap_or(0.0);
            last.set(Some(now));
            let target = if imp.fit.get() { 1.0 } else { 0.0 };
            let step = dt / FADE_US;
            let p = imp.progress.get();
            let np = if target > p { (p + step).min(target) } else { (p - step).max(target) };
            imp.progress.set(np);
            w.queue_draw();
            // Redraw the face-box layer above, so the boxes follow.
            if let Some(n) = w.next_sibling() {
                n.queue_draw();
            }
            if np == target {
                imp.ticking.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// The rectangle where the photo draws in a `w` x `h` area: x, y, width,
    /// height. The rectangle can be larger than the area in crop mode.
    /// The face boxes follow the view that is more visible.
    pub fn draw_rect(&self, w: i32, h: i32) -> Option<(f64, f64, f64, f64)> {
        self.rect_for(w, h, self.imp().progress.get() >= 0.5)
    }

    /// The photo rectangle in crop mode (`fit` false) or fit mode.
    fn rect_for(&self, w: i32, h: i32, fit: bool) -> Option<(f64, f64, f64, f64)> {
        let imp = self.imp();
        let tex = imp.texture.borrow();
        let tex = tex.as_ref()?;
        let (iw, ih) = (tex.width() as f64, tex.height() as f64);
        let (aw, ah) = (w as f64, h as f64);
        if iw <= 0.0 || ih <= 0.0 || aw <= 0.0 || ah <= 0.0 {
            return None;
        }
        if fit || !CROP.with(|c| c.get()) {
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
        // Float rounding can make `aw - dw` a tiny positive value.
        // `clamp` panics when min > max, so cap the min at 0.
        let x = (aw / 2.0 - fx).clamp((aw - dw).min(0.0), 0.0);
        let y = (ah / 2.0 - fy).clamp((ah - dh).min(0.0), 0.0);
        Some((x, y, dw, dh))
    }
}
