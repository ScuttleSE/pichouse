//! Full-image viewer that replaces the grid when a photo is opened.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk_pixbuf::{Pixbuf, PixbufRotation};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DrawingArea, GestureDrag, Label, Orientation, Overlay, Picture,
    Separator,
};

use crate::model::Photo;

use super::shortcuts::Action;
use super::state::{show_error, AppState};

/// A crop rectangle in per-mille of the image (x, y, w, h). `w == 0 || h == 0`
/// means "no crop".
type CropPermille = (i32, i32, i32, i32);

/// The full-image viewer.
pub struct Viewer {
    root: GtkBox,
    picture: Picture,
    /// The top control bar, hidden while a slideshow runs fullscreen.
    bar: GtkBox,
    /// Transparent drawing surface stacked on the picture for the crop overlay.
    crop_area: DrawingArea,
    header: Label,
    close_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    rotate_btn: Button,
    edit_btn: Button,
    /// Toggles the face-box overlay.
    faces_btn: Button,
    /// Transparent surface stacked on the picture for the face-box overlay.
    face_area: DrawingArea,
    /// True while the face overlay is shown.
    faces_mode: std::cell::Cell<bool>,
    /// The faces of the current photo, in per-mille of the oriented image.
    faces: RefCell<Vec<crate::model::Face>>,
    /// Person id -> name, for labelling boxes.
    person_names: RefCell<std::collections::HashMap<i64, String>>,
    /// True when the overlay shows stylised character faces, not human faces.
    /// In this mode `faces` holds style faces mapped into `Face` (person_id
    /// carries the character id) and `person_names` maps character ids to names.
    style_mode: std::cell::Cell<bool>,

    photos: RefCell<Vec<Photo>>,
    index: RefCell<usize>,
    state: RefCell<Option<Rc<AppState>>>,
    /// When true, show the untouched original instead of the edited view.
    show_original: std::cell::Cell<bool>,
    /// Bumped on every `show()` so a late async image load for a previous photo
    /// is discarded instead of flashing on screen.
    generation: std::cell::Cell<u64>,
    /// True while the interactive crop overlay is active.
    crop_mode: std::cell::Cell<bool>,
    /// The crop rectangle being edited, in per-mille.
    crop_rect: RefCell<CropPermille>,
    /// Called with a new per-mille crop when the user finishes a drag.
    crop_cb: RefCell<Option<Box<dyn Fn(CropPermille)>>>,
    /// Drag start point in widget coordinates.
    drag_start: std::cell::Cell<(f64, f64)>,

    // --- slideshow ---
    /// The running slideshow timer, if any.
    slideshow_source: RefCell<Option<glib::SourceId>>,
    /// The order to play photos in (indices into `photos`). Identity unless
    /// shuffle is on.
    slideshow_order: RefCell<Vec<usize>>,
    /// Position within `slideshow_order`.
    slideshow_pos: std::cell::Cell<usize>,
    /// Per-image duration in seconds.
    slideshow_secs: std::cell::Cell<u32>,
    /// Loop back to the start after the last image.
    slideshow_loop: std::cell::Cell<bool>,
    /// True while paused.
    slideshow_paused: std::cell::Cell<bool>,
}

impl Viewer {
    /// Build the viewer. `bind_state` must be called once before use.
    pub fn new() -> Rc<Viewer> {
        let close_btn = Button::from_icon_name("go-previous-symbolic");
        let prev_btn = Button::from_icon_name("media-skip-backward-symbolic");
        let next_btn = Button::from_icon_name("media-skip-forward-symbolic");
        let rotate_btn = Button::from_icon_name("object-rotate-right-symbolic");
        let edit_btn = Button::from_icon_name("document-edit-symbolic");
        let faces_btn = Button::from_icon_name("avatar-default-symbolic");

        let header = Label::new(None);
        header.set_xalign(0.0);
        header.set_hexpand(true);
        header.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        let bar = GtkBox::new(Orientation::Horizontal, 6);
        bar.set_margin_top(6);
        bar.set_margin_bottom(6);
        bar.set_margin_start(6);
        bar.set_margin_end(6);
        bar.append(&close_btn);
        bar.append(&prev_btn);
        bar.append(&next_btn);
        bar.append(&rotate_btn);
        bar.append(&edit_btn);
        bar.append(&faces_btn);
        bar.append(&header);

        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_vexpand(true);
        picture.set_hexpand(true);

        // A transparent drawing area is stacked over the picture for the
        // interactive crop overlay. It stays hidden and pass-through until crop
        // mode is turned on.
        let crop_area = DrawingArea::new();
        crop_area.set_visible(false);
        crop_area.set_can_target(true);
        // A second transparent surface draws the face boxes. It stays hidden and
        // targetable only while the face overlay is on.
        let face_area = DrawingArea::new();
        face_area.set_visible(false);
        face_area.set_can_target(true);
        let overlay = Overlay::new();
        overlay.set_child(Some(&picture));
        overlay.add_overlay(&crop_area);
        overlay.add_overlay(&face_area);

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&overlay);

        let viewer = Rc::new(Viewer {
            root,
            picture,
            bar,
            crop_area,
            header,
            close_btn,
            prev_btn,
            next_btn,
            rotate_btn,
            edit_btn,
            faces_btn,
            face_area,
            faces_mode: std::cell::Cell::new(false),
            faces: RefCell::new(Vec::new()),
            person_names: RefCell::new(std::collections::HashMap::new()),
            style_mode: std::cell::Cell::new(false),
            photos: RefCell::new(Vec::new()),
            index: RefCell::new(0),
            state: RefCell::new(None),
            show_original: std::cell::Cell::new(false),
            generation: std::cell::Cell::new(0),
            crop_mode: std::cell::Cell::new(false),
            crop_rect: RefCell::new((0, 0, 0, 0)),
            crop_cb: RefCell::new(None),
            drag_start: std::cell::Cell::new((0.0, 0.0)),
            slideshow_source: RefCell::new(None),
            slideshow_order: RefCell::new(Vec::new()),
            slideshow_pos: std::cell::Cell::new(0),
            slideshow_secs: std::cell::Cell::new(4),
            slideshow_loop: std::cell::Cell::new(true),
            slideshow_paused: std::cell::Cell::new(false),
        });
        viewer.setup_crop_overlay();
        viewer.setup_face_overlay();
        viewer
    }

    /// Give the viewer access to shared state and wire the buttons.
    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state.clone());

        let this = self.clone();
        self.close_btn.connect_clicked(move |_| {
            if let Some(s) = this.state.borrow().clone() {
                s.close_viewer();
            }
        });
        let this = self.clone();
        self.prev_btn.connect_clicked(move |_| this.navigate(-1));
        let this = self.clone();
        self.next_btn.connect_clicked(move |_| this.navigate(1));
        let this = self.clone();
        self.rotate_btn.connect_clicked(move |_| this.rotate());
        let this = self.clone();
        self.edit_btn.connect_clicked(move |_| this.open_editor());

        let this = self.clone();
        self.faces_btn.connect_clicked(move |_| this.toggle_faces());

        self.refresh_tooltips();
    }

    /// The viewer root widget.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Update button tooltips to reflect the current key bindings.
    pub fn refresh_tooltips(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let sc = state.shortcuts.borrow();
        let lbl = |a: Action| super::shortcuts::keyval_label(sc.keyval(a));
        self.close_btn
            .set_tooltip_text(Some(&format!("Back to grid ({})", lbl(Action::Close))));
        self.prev_btn
            .set_tooltip_text(Some(&format!("Previous ({})", lbl(Action::Prev))));
        self.next_btn
            .set_tooltip_text(Some(&format!("Next ({})", lbl(Action::Next))));
        self.rotate_btn
            .set_tooltip_text(Some(&format!("Rotate 90° ({})", lbl(Action::Rotate))));
        self.faces_btn
            .set_tooltip_text(Some("Show faces"));
    }

    /// Handle a key press while the viewer is active. Returns true if consumed.
    pub fn handle_key(self: &Rc<Self>, keyval: u32) -> bool {
        // Slideshow controls take priority while a show runs.
        if self.slideshow_active() {
            // Space (0x20) toggles pause; Escape stops the slideshow.
            if keyval == 0x20 {
                self.toggle_slideshow_pause();
                return true;
            }
            if keyval == glib::translate::IntoGlib::into_glib(gtk4::gdk::Key::Escape) {
                self.stop_slideshow();
                return true;
            }
        }
        let Some(state) = self.state.borrow().clone() else {
            return false;
        };
        let action = { state.shortcuts.borrow().action(keyval) };
        match action {
            Some(Action::Prev) => {
                self.navigate(-1);
                true
            }
            Some(Action::Next) => {
                self.navigate(1);
                true
            }
            Some(Action::Rotate) => {
                self.rotate();
                true
            }
            Some(Action::Close) => {
                self.stop_slideshow();
                state.close_viewer();
                true
            }
            None => false,
        }
    }

    /// Display the given photos with the initial index selected.
    pub fn open(self: &Rc<Self>, photos: Vec<Photo>, index: usize) {
        *self.photos.borrow_mut() = photos;
        *self.index.borrow_mut() = index;
        self.show();
    }

    fn navigate(self: &Rc<Self>, delta: i32) {
        let len = self.photos.borrow().len();
        if len == 0 {
            return;
        }
        let mut idx = *self.index.borrow() as i32 + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx as usize >= len {
            idx = len as i32 - 1;
        }
        *self.index.borrow_mut() = idx as usize;
        self.show();
    }

    /// Switch the right-hand panel to the Edit tab for the current photo.
    fn open_editor(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let idx = *self.index.borrow();
        let photo = match self.photos.borrow().get(idx) {
            Some(p) => p.clone(),
            None => return,
        };
        if photo.id == 0 {
            return;
        }
        // The properties panel already shows this photo; just reveal Edit.
        state.properties().open_edit_tab();
    }

    /// The photo currently shown, if any.
    #[allow(dead_code)] // Public accessor for future callers.
    pub fn current_photo(self: &Rc<Self>) -> Option<Photo> {
        let idx = *self.index.borrow();
        self.photos.borrow().get(idx).cloned()
    }

    /// Show the untouched original (true) versus the edited view (false), then
    /// re-render the current photo.
    ///
    /// Re-rendering only happens when the flag actually changes. This is
    /// important because `show()` refreshes the properties panel, whose Edit tab
    /// calls back into `set_show_original(false)` on load; without this guard
    /// that path recurses (`show → properties → editor → set_show_original →
    /// show → …`) and hangs the UI.
    pub fn set_show_original(self: &Rc<Self>, original: bool) {
        if self.show_original.get() == original {
            return;
        }
        self.show_original.set(original);
        self.show();
    }

    /// Re-render the current photo (for example after edits change).
    pub fn reload_current(self: &Rc<Self>) {
        self.show();
    }

    /// Set up the crop overlay's draw function and drag gesture. Called once at
    /// construction.
    fn setup_crop_overlay(self: &Rc<Self>) {
        // Draw the dimmed outside region and the crop rectangle.
        let this = self.clone();
        self.crop_area.set_draw_func(move |_, cr, w, h| {
            if !this.crop_mode.get() {
                return;
            }
            let Some((ix, iy, iw, ih)) = this.image_rect(w, h) else {
                return;
            };
            let (px, py, pw, ph) = *this.crop_rect.borrow();
            let (rx, ry, rw, rh) = if pw > 0 && ph > 0 {
                (
                    ix + iw * px as f64 / 1000.0,
                    iy + ih * py as f64 / 1000.0,
                    iw * pw as f64 / 1000.0,
                    ih * ph as f64 / 1000.0,
                )
            } else {
                (ix, iy, iw, ih)
            };
            // Dim the whole image, then clear the crop rectangle.
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.5);
            let _ = cr.rectangle(ix, iy, iw, ih);
            let _ = cr.fill();
            cr.set_operator(gtk4::cairo::Operator::Clear);
            let _ = cr.rectangle(rx, ry, rw, rh);
            let _ = cr.fill();
            cr.set_operator(gtk4::cairo::Operator::Over);
            // Outline the crop rectangle.
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
            cr.set_line_width(1.5);
            let _ = cr.rectangle(rx, ry, rw, rh);
            let _ = cr.stroke();
        });

        let drag = GestureDrag::new();
        let this = self.clone();
        drag.connect_drag_begin(move |_, x, y| {
            if this.crop_mode.get() {
                this.drag_start.set((x, y));
            }
        });
        let this = self.clone();
        drag.connect_drag_update(move |_, ox, oy| {
            if !this.crop_mode.get() {
                return;
            }
            let (sx, sy) = this.drag_start.get();
            this.update_crop_from_drag(sx, sy, sx + ox, sy + oy);
        });
        let this = self.clone();
        drag.connect_drag_end(move |_, ox, oy| {
            if !this.crop_mode.get() {
                return;
            }
            let (sx, sy) = this.drag_start.get();
            this.update_crop_from_drag(sx, sy, sx + ox, sy + oy);
            let rect = *this.crop_rect.borrow();
            if let Some(cb) = this.crop_cb.borrow().as_ref() {
                cb(rect);
            }
        });
        self.crop_area.add_controller(drag);
    }

    /// Set up the face overlay's draw function and click gesture. Called once.
    fn setup_face_overlay(self: &Rc<Self>) {
        let this = self.clone();
        self.face_area.set_draw_func(move |_, cr, w, h| {
            if !this.faces_mode.get() {
                return;
            }
            let Some((ix, iy, iw, ih)) = this.image_rect(w, h) else {
                return;
            };
            let faces = this.faces.borrow();
            let names = this.person_names.borrow();
            for f in faces.iter() {
                let rx = ix + iw * f.bbox_x as f64 / 1000.0;
                let ry = iy + ih * f.bbox_y as f64 / 1000.0;
                let rw = iw * f.bbox_w as f64 / 1000.0;
                let rh = ih * f.bbox_h as f64 / 1000.0;
                // A named face draws green, an unnamed face draws yellow.
                if f.person_id != 0 {
                    cr.set_source_rgba(0.3, 0.9, 0.4, 0.95);
                } else {
                    cr.set_source_rgba(1.0, 0.85, 0.2, 0.95);
                }
                cr.set_line_width(2.0);
                let _ = cr.rectangle(rx, ry, rw, rh);
                let _ = cr.stroke();
                // Draw the person name under the box, when known.
                if let Some(name) = names.get(&f.person_id) {
                    cr.move_to(rx, ry + rh + 14.0);
                    cr.set_font_size(13.0);
                    // A dark shadow, then the label.
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.8);
                    let _ = cr.show_text(name);
                    cr.move_to(rx - 1.0, ry + rh + 13.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                    let _ = cr.show_text(name);
                }
            }
        });

        // Click a box to assign or change the person.
        let click = gtk4::GestureClick::new();
        let this = self.clone();
        click.connect_released(move |_, _, x, y| {
            if !this.faces_mode.get() {
                return;
            }
            this.face_clicked(x, y);
        });
        self.face_area.add_controller(click);
    }

    /// Toggle the face overlay on the current photo.
    fn toggle_faces(self: &Rc<Self>) {
        let on = !self.faces_mode.get();
        self.faces_mode.set(on);
        self.face_area.set_visible(on);
        if on {
            self.load_faces();
        }
        self.face_area.queue_draw();
    }

    /// Load the current photo's faces and the name map. In stylised mode this
    /// loads style faces and character names. Otherwise it loads human faces and
    /// person names.
    fn load_faces(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let Some(photo) = self.current_photo() else {
            return;
        };
        let style = state.grid().is_style_source();
        self.style_mode.set(style);
        if style {
            // Map each StyleFace into a Face so the draw code stays the same.
            // person_id carries the character id.
            let sfaces = state
                .lib
                .style_faces_for_photo(photo.id)
                .unwrap_or_default();
            let faces = sfaces
                .into_iter()
                .map(|s| crate::model::Face {
                    id: s.id,
                    photo_id: s.photo_id,
                    person_id: s.character_id,
                    cluster_id: s.cluster_id,
                    bbox_x: s.bbox_x,
                    bbox_y: s.bbox_y,
                    bbox_w: s.bbox_w,
                    bbox_h: s.bbox_h,
                    det_score: s.det_score,
                    confirmed: s.confirmed,
                    source: s.source,
                    ..Default::default()
                })
                .collect();
            let mut names = std::collections::HashMap::new();
            for (c, _) in state.lib.characters().unwrap_or_default() {
                names.insert(c.id, c.name);
            }
            *self.faces.borrow_mut() = faces;
            *self.person_names.borrow_mut() = names;
        } else {
            let faces = state.lib.faces_for_photo(photo.id).unwrap_or_default();
            let mut names = std::collections::HashMap::new();
            for (p, _) in state.lib.persons().unwrap_or_default() {
                names.insert(p.id, p.name);
            }
            *self.faces.borrow_mut() = faces;
            *self.person_names.borrow_mut() = names;
        }
    }

    /// Handle a click at widget `(x,y)`: find the face box under it and offer to
    /// assign it to a person.
    fn face_clicked(self: &Rc<Self>, x: f64, y: f64) {
        let w = self.face_area.width();
        let h = self.face_area.height();
        let Some((ix, iy, iw, ih)) = self.image_rect(w, h) else {
            return;
        };
        let hit = {
            let faces = self.faces.borrow();
            faces.iter().find(|f| {
                let rx = ix + iw * f.bbox_x as f64 / 1000.0;
                let ry = iy + ih * f.bbox_y as f64 / 1000.0;
                let rw = iw * f.bbox_w as f64 / 1000.0;
                let rh = ih * f.bbox_h as f64 / 1000.0;
                x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
            })
            .cloned()
        };
        let Some(face) = hit else { return };
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let on_done = {
            let this = self.clone();
            let state = state.clone();
            let style = self.style_mode.get();
            move || {
                this.load_faces();
                this.face_area.queue_draw();
                // A reassignment or rejection changes grouping; re-cluster so
                // the People/Characters view and sidebar reflect it.
                if style {
                    super::stylefacescan::recluster_now(&state);
                } else {
                    super::facescan::recluster_now(&state);
                }
            }
        };
        if self.style_mode.get() {
            super::characters::assign_style_face_dialog(&state, face.id, on_done);
        } else {
            super::people::assign_face_dialog(&state, face.id, on_done);
        }
    }

    /// The displayed image rectangle inside the crop_area, honouring
    /// `ContentFit::Contain` letterboxing. Returns `(x, y, w, h)` in widget
    /// pixels, or `None` when no image is shown.
    fn image_rect(&self, area_w: i32, area_h: i32) -> Option<(f64, f64, f64, f64)> {
        let paintable = self.picture.paintable()?;
        let iw = paintable.intrinsic_width() as f64;
        let ih = paintable.intrinsic_height() as f64;
        if iw <= 0.0 || ih <= 0.0 {
            return None;
        }
        let (aw, ah) = (area_w as f64, area_h as f64);
        let scale = (aw / iw).min(ah / ih);
        let dw = iw * scale;
        let dh = ih * scale;
        Some(((aw - dw) / 2.0, (ah - dh) / 2.0, dw, dh))
    }

    /// Convert a drag from `(x0,y0)` to `(x1,y1)` (widget pixels) into a
    /// per-mille crop rectangle and redraw.
    fn update_crop_from_drag(&self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let w = self.crop_area.width();
        let h = self.crop_area.height();
        let Some((ix, iy, iw, ih)) = self.image_rect(w, h) else {
            return;
        };
        let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi);
        let ax = clamp(x0.min(x1), ix, ix + iw);
        let ay = clamp(y0.min(y1), iy, iy + ih);
        let bx = clamp(x0.max(x1), ix, ix + iw);
        let by = clamp(y0.max(y1), iy, iy + ih);
        let to_mille = |v: f64, origin: f64, span: f64| {
            (((v - origin) / span) * 1000.0).round().clamp(0.0, 1000.0) as i32
        };
        let px = to_mille(ax, ix, iw);
        let py = to_mille(ay, iy, ih);
        let pw = to_mille(bx, ix, iw) - px;
        let ph = to_mille(by, iy, ih) - py;
        // Ignore a too-small drag (treat as a click, no change).
        if pw < 10 || ph < 10 {
            return;
        }
        *self.crop_rect.borrow_mut() = (px, py, pw, ph);
        self.crop_area.queue_draw();
    }

    /// Turn the interactive crop overlay on or off. When turning on, seed it
    /// with the current per-mille crop so the existing rectangle shows. While
    /// crop mode is active the picture renders with crop suppressed, so the
    /// user drags the rectangle over the whole (uncropped) image.
    pub fn set_crop_mode(self: &Rc<Self>, on: bool, initial: CropPermille) {
        self.crop_mode.set(on);
        *self.crop_rect.borrow_mut() = initial;
        self.crop_area.set_visible(on);
        // Re-render so the picture shows with/without the crop applied.
        self.show();
        self.crop_area.queue_draw();
    }

    /// Whether the crop overlay is active.
    #[allow(dead_code)]
    pub fn crop_mode_active(&self) -> bool {
        self.crop_mode.get()
    }

    /// Set the callback invoked with the new per-mille crop after a drag.
    pub fn set_crop_callback(self: &Rc<Self>, f: impl Fn(CropPermille) + 'static) {
        *self.crop_cb.borrow_mut() = Some(Box::new(f));
    }

    // --- slideshow ---

    /// Whether a slideshow is currently running.
    pub fn slideshow_active(&self) -> bool {
        self.slideshow_source.borrow().is_some()
    }

    /// Start a full-screen slideshow of the current photo set.
    ///
    /// `secs` is the per-image duration, `shuffle` randomises the order, and
    /// `do_loop` restarts after the last image. The viewer must already hold the
    /// photo set (via `open`).
    pub fn start_slideshow(self: &Rc<Self>, secs: u32, shuffle: bool, do_loop: bool) {
        let len = self.photos.borrow().len();
        if len == 0 {
            return;
        }
        self.stop_slideshow();
        self.slideshow_secs.set(secs.max(1));
        self.slideshow_loop.set(do_loop);
        self.slideshow_paused.set(false);

        // Build the play order, starting from the currently shown photo.
        let mut order: Vec<usize> = (0..len).collect();
        if shuffle {
            shuffle_indices(&mut order);
        }
        let cur = *self.index.borrow();
        if let Some(p) = order.iter().position(|&i| i == cur) {
            order.swap(0, p);
        }
        *self.slideshow_order.borrow_mut() = order;
        self.slideshow_pos.set(0);
        self.goto_slideshow_pos();

        // Enter fullscreen and hide the control bar for an immersive view.
        if let Some(state) = self.state.borrow().clone() {
            if let Some(w) = state.window() {
                w.fullscreen();
            }
        }
        self.bar.set_visible(false);

        self.arm_slideshow_timer();
    }

    /// (Re)arm the per-image advance timer.
    fn arm_slideshow_timer(self: &Rc<Self>) {
        let secs = self.slideshow_secs.get();
        let this = self.clone();
        let id = glib::timeout_add_seconds_local(secs, move || {
            if this.slideshow_paused.get() {
                return glib::ControlFlow::Continue;
            }
            if this.advance_slideshow() {
                glib::ControlFlow::Continue
            } else {
                // Reached the end with loop off: stop.
                this.stop_slideshow();
                glib::ControlFlow::Break
            }
        });
        *self.slideshow_source.borrow_mut() = Some(id);
    }

    /// Advance to the next slideshow image. Returns false when the show should
    /// end (last image reached with loop off).
    fn advance_slideshow(self: &Rc<Self>) -> bool {
        let len = self.slideshow_order.borrow().len();
        if len == 0 {
            return false;
        }
        let mut pos = self.slideshow_pos.get() + 1;
        if pos >= len {
            if self.slideshow_loop.get() {
                pos = 0;
            } else {
                return false;
            }
        }
        self.slideshow_pos.set(pos);
        self.goto_slideshow_pos();
        true
    }

    /// Show the photo at the current slideshow position.
    fn goto_slideshow_pos(self: &Rc<Self>) {
        let idx = self
            .slideshow_order
            .borrow()
            .get(self.slideshow_pos.get())
            .copied();
        if let Some(idx) = idx {
            *self.index.borrow_mut() = idx;
            self.show();
        }
    }

    /// Pause or resume a running slideshow.
    pub fn toggle_slideshow_pause(self: &Rc<Self>) {
        if !self.slideshow_active() {
            return;
        }
        let paused = !self.slideshow_paused.get();
        self.slideshow_paused.set(paused);
        if let Some(state) = self.state.borrow().clone() {
            state
                .status()
                .set_message(if paused { "Slideshow paused" } else { "Slideshow" });
        }
    }

    /// Stop the slideshow, leave fullscreen, and restore the control bar.
    pub fn stop_slideshow(self: &Rc<Self>) {
        if let Some(id) = self.slideshow_source.borrow_mut().take() {
            id.remove();
        }
        self.slideshow_paused.set(false);
        self.bar.set_visible(true);
        if let Some(state) = self.state.borrow().clone() {
            if let Some(w) = state.window() {
                w.unfullscreen();
            }
        }
    }

    fn rotate(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let idx = *self.index.borrow();
        let (id, hash, new_orient) = {            let mut photos = self.photos.borrow_mut();
            let Some(p) = photos.get_mut(idx) else {
                return;
            };
            p.orientation = (p.orientation + 90) % 360;
            (p.id, p.hash.clone(), p.orientation)
        };
        if id != 0 {
            if let Err(e) = state.lib.set_orientation(id, new_orient) {
                show_error(&state, &e.to_string());
            }
        }
        if !hash.is_empty() {
            let _ = state.gen.invalidate(&hash);
        }
        self.show();
        // Re-query the grid's source so the rotated thumbnail regenerates when
        // the user returns to the grid.
        state.grid().reload_from_source();
    }

    fn show(self: &Rc<Self>) {
        let idx = *self.index.borrow();
        let photo = {
            let photos = self.photos.borrow();
            match photos.get(idx) {
                Some(p) => p.clone(),
                None => return,
            }
        };
        self.header.set_text(&photo.filename);
        if let Some(state) = self.state.borrow().clone() {
            state.properties().show(&photo);
        }

        // Clear the previous image immediately so it is not left on screen while
        // the new file is read and decoded.
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);

        // Bump the generation so a late result for a previously shown photo is
        // ignored (e.g. opening a second photo before the first finished loading).
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);

        // Read the image bytes off-thread, then decode + rotate on the UI
        // thread (Pixbuf is not Send). Local photos read from disk; Immich
        // photos download the preview over HTTP.
        let (tx, rx) = glib::MainContext::channel::<Option<Vec<u8>>>(glib::Priority::DEFAULT);
        let path = photo.path.clone();
        let server = immich_server_for(&self.state.borrow().clone(), &path);
        std::thread::spawn(move || {
            let bytes = match server {
                Some((server, asset_id)) => {
                    let client = crate::immich::Client::new(&server.base_url, &server.api_key);
                    client.asset_preview(&asset_id).ok()
                }
                None => std::fs::read(&path).ok(),
            };
            let _ = tx.send(bytes);
        });
        let picture = self.picture.clone();
        let rot = photo.orientation;
        let this = self.clone();
        // The non-destructive edit to apply on the decoded pixels, unless the
        // user asked to see the original.
        let mut edit = if self.show_original.get() {
            crate::model::PhotoEdit::default()
        } else {
            self.state
                .borrow()
                .as_ref()
                .and_then(|s| s.lib.photo_edit(photo.id).ok())
                .unwrap_or_default()
        };
        // While the crop or face overlay is active, show the image uncropped so
        // the overlay rectangles map to the whole oriented frame.
        if self.crop_mode.get() || self.faces_mode.get() {
            edit.crop_x = 0;
            edit.crop_y = 0;
            edit.crop_w = 0;
            edit.crop_h = 0;
        }
        rx.attach(None, move |bytes| {
            // Drop stale results from an earlier show().
            if this.generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            match bytes.and_then(|b| decode_edited(&b, rot, &edit)) {
                Some(pb) => picture.set_pixbuf(Some(&pb)),
                None => picture.set_paintable(gtk4::gdk::Paintable::NONE),
            }
            this.crop_area.queue_draw();
            if this.faces_mode.get() {
                this.load_faces();
                this.face_area.queue_draw();
            }
            glib::ControlFlow::Break
        });
    }
}

/// Decode image bytes, apply the stored 90-degree rotation, then apply the
/// non-destructive `edit` (flip, straighten, crop, levels, brightness/contrast)
/// for display.
///
/// Tries GTK's `PixbufLoader` first. Immich previews may be WebP, which some
/// GTK builds cannot load, so on failure the `image` crate decodes the bytes
/// and the pixels are copied into a `Pixbuf`.
fn decode_edited(bytes: &[u8], degrees: i32, edit: &crate::model::PhotoEdit) -> Option<Pixbuf> {
    let pb = decode_pixbuf(bytes)?;
    let degrees = ((degrees % 360) + 360) % 360;
    let pb = match degrees {
        90 => pb.rotate_simple(PixbufRotation::Clockwise)?,
        180 => pb.rotate_simple(PixbufRotation::Upsidedown)?,
        270 => pb.rotate_simple(PixbufRotation::Counterclockwise)?,
        _ => pb,
    };
    if edit.is_identity() {
        return Some(pb);
    }
    // Convert the rotated Pixbuf to an RgbaImage, run the edit pipeline, and
    // convert back.
    let rgba = pixbuf_to_rgba(&pb)?;
    let out = crate::edit::apply_edits(rgba, edit);
    let (w, h) = (out.width() as i32, out.height() as i32);
    let data = glib::Bytes::from_owned(out.into_raw());
    Some(Pixbuf::from_bytes(
        &data,
        gtk4::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        w,
        h,
        w * 4,
    ))
}

/// Copy a `Pixbuf` (RGB or RGBA) into an `image::RgbaImage`.
fn pixbuf_to_rgba(pb: &Pixbuf) -> Option<image::RgbaImage> {
    let (w, h) = (pb.width() as u32, pb.height() as u32);
    let channels = pb.n_channels();
    let rowstride = pb.rowstride() as usize;
    let pixels = pb.read_pixel_bytes();
    let src = pixels.as_ref();
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = y * rowstride + x * channels as usize;
            let r = *src.get(i)?;
            let g = *src.get(i + 1)?;
            let b = *src.get(i + 2)?;
            let a = if channels >= 4 { *src.get(i + 3)? } else { 255 };
            out.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, a]));
        }
    }
    Some(out)
}

/// Decode image bytes into a `Pixbuf`, with an `image`-crate fallback for
/// formats GTK cannot load (for example WebP).
fn decode_pixbuf(bytes: &[u8]) -> Option<Pixbuf> {
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    if loader.write(bytes).is_ok() && loader.close().is_ok() {
        if let Some(pb) = loader.pixbuf() {
            return Some(pb);
        }
    }
    // Fallback: decode with the `image` crate and copy RGBA into a Pixbuf.
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let data = glib::Bytes::from_owned(rgba.into_raw());
    Some(Pixbuf::from_bytes(
        &data,
        gtk4::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        w,
        h,
        w * 4,
    ))
}

/// Shuffle a slice in place with a Fisher–Yates pass seeded from the wall
/// clock. This needs no extra dependency and is good enough for a slideshow.
fn shuffle_indices(v: &mut [usize]) {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b9)
        | 1;
    let mut next = || {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        seed.wrapping_mul(0x2545F4914F6CDD1D)
    };
    let n = v.len();
    for i in (1..n).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

/// If `path` is an `immich://<server_id>/<asset_id>` URL and the server exists,
/// return the server record and the asset id. Otherwise return `None`.
fn immich_server_for(
    state: &Option<Rc<AppState>>,
    path: &str,
) -> Option<(crate::model::ImmichServer, String)> {
    let rest = path.strip_prefix("immich://")?;
    let (sid, asset_id) = rest.split_once('/')?;
    let server_id: i64 = sid.parse().ok()?;
    if asset_id.is_empty() {
        return None;
    }
    let state = state.as_ref()?;
    let server = state.lib.immich_server(server_id).ok()??;
    Some((server, asset_id.to_string()))
}
