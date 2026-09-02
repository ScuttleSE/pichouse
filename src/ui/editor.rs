//! Non-destructive edit panel, hosted as the "Edit" tab of the right-hand
//! properties panel.
//!
//! The panel is built once and reused. `load(photo)` binds it to a photo: it
//! reads the photo's [`PhotoEdit`] record, computes a histogram, and refreshes
//! every control. Editing a control writes the record with
//! [`Library::set_photo_edit`], invalidates the photo's thumbnails, and
//! re-renders the viewer live. The original file on disk is never changed.
//!
//! Controls: flip H/V, straighten, brightness/contrast, per-channel color
//! levels with a live draggable histogram and an auto-levels button, crop
//! (numeric per-mille), a levels-preset chooser with save/delete/apply-to-
//! folder, a "view original" toggle, revert, and export of a baked copy.
//!
//! Immich photos are supported: their full-resolution asset is downloaded for
//! the histogram, auto-levels, and export; view-time edits apply to the preview.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DrawingArea, DropDown, Label, Orientation, Scale,
    ScrolledWindow, SpinButton, StringList,
};

use crate::model::{Levels, LevelPreset, Photo, PhotoEdit};

use super::state::{show_error, AppState};

/// Per-channel widgets so auto-levels and presets can push values back.
struct ChannelWidgets {
    black: SpinButton,
    white: SpinButton,
    gamma: SpinButton,
    area: DrawingArea,
}

/// Other controls that must reflect a newly loaded photo's edit record.
struct Controls {
    flip_h: CheckButton,
    flip_v: CheckButton,
    straighten: Scale,
    brightness: Scale,
    contrast: Scale,
    crop_x: SpinButton,
    crop_y: SpinButton,
    crop_w: SpinButton,
    crop_h: SpinButton,
    view_original: CheckButton,
}

/// The Edit tab. Built once, rebound to a photo with [`EditPanel::load`].
pub struct EditPanel {
    root: ScrolledWindow,
    body: GtkBox,
    empty: Label,
    state: RefCell<Option<Rc<AppState>>>,
    photo: RefCell<Option<Photo>>,
    edit: RefCell<PhotoEdit>,
    /// Guards callbacks while widgets are set programmatically.
    loading: std::cell::Cell<bool>,
    presets: RefCell<Vec<LevelPreset>>,
    preset_drop: DropDown,
    histogram: RefCell<[Vec<u32>; 3]>,
    channels: RefCell<Vec<ChannelWidgets>>,
    controls: RefCell<Option<Controls>>,
    /// The interactive "crop by dragging" toggle, so `load` can reset it.
    crop_btn: RefCell<Option<CheckButton>>,
    /// Bumped on every `load` so a late async histogram for a previously shown
    /// photo is discarded instead of overwriting the current one.
    hist_generation: std::cell::Cell<u64>,
}

impl EditPanel {
    /// Build the panel. `bind_state` must be called before use.
    pub fn new() -> Rc<EditPanel> {
        let root = ScrolledWindow::new();
        root.set_hscrollbar_policy(gtk4::PolicyType::Never);
        root.set_size_request(260, -1);
        root.set_vexpand(true);

        let empty = Label::new(Some("Open a photo to edit it."));
        empty.set_xalign(0.5);
        empty.set_vexpand(true);
        empty.add_css_class("dim-label");

        let body = GtkBox::new(Orientation::Vertical, 10);
        body.set_margin_top(10);
        body.set_margin_bottom(10);
        body.set_margin_start(10);
        body.set_margin_end(10);

        let preset_drop =
            DropDown::new(Some(StringList::new(&["— presets —"])), gtk4::Expression::NONE);

        let panel = Rc::new(EditPanel {
            root: root.clone(),
            body: body.clone(),
            empty: empty.clone(),
            state: RefCell::new(None),
            photo: RefCell::new(None),
            edit: RefCell::new(PhotoEdit::default()),
            loading: std::cell::Cell::new(false),
            presets: RefCell::new(Vec::new()),
            preset_drop,
            histogram: RefCell::new([vec![0; 256], vec![0; 256], vec![0; 256]]),
            channels: RefCell::new(Vec::new()),
            controls: RefCell::new(None),
            crop_btn: RefCell::new(None),
            hist_generation: std::cell::Cell::new(0),
        });

        panel.build_body();
        // Show the empty hint until a photo is loaded.
        root.set_child(Some(&empty));
        panel
    }

    /// The panel root widget (added as a Notebook tab).
    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }

    /// Give the panel access to shared state.
    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state);
    }

    fn state(&self) -> Option<Rc<AppState>> {
        self.state.borrow().clone()
    }

    /// Bind the panel to `photo`, reading its edit record and refreshing every
    /// control. Pass `None` to clear the panel (show the empty hint).
    ///
    /// Re-binding the *same* photo is a no-op. This matters because the viewer's
    /// `show()` refreshes the properties panel (and thus this editor) on every
    /// render, and this method resets the viewer's "view original" flag; without
    /// the guard, re-showing the same photo would loop (`viewer.show →
    /// properties.show → editor.load → viewer.set_show_original → viewer.show →
    /// …`) and also needlessly re-decode the image for the histogram.
    pub fn load(self: &Rc<Self>, photo: Option<Photo>) {
        let Some(state) = self.state() else { return };

        // Skip if this is the same photo already bound (id, or path for Immich
        // photos whose id is 0).
        {
            let cur = self.photo.borrow();
            match (&*cur, &photo) {
                (Some(a), Some(b)) if a.id == b.id && a.path == b.path => return,
                (None, None) => return,
                _ => {}
            }
        }

        // Leaving crop mode on across photos would be confusing; reset it.
        if let Some(b) = self.crop_btn.borrow().as_ref() {
            if b.is_active() {
                b.set_active(false);
            }
        }
        match photo {
            None => {
                *self.photo.borrow_mut() = None;
                self.root.set_child(Some(&self.empty));
            }
            Some(p) => {
                let edit = state.lib.photo_edit(p.id).unwrap_or(PhotoEdit {
                    photo_id: p.id,
                    ..Default::default()
                });
                *self.edit.borrow_mut() = edit;
                // Clear the histogram and show the panel immediately. Computing
                // the histogram reads and decodes the full image, which is slow
                // on a big file or a busy disk — doing it on the main thread here
                // froze the UI on every photo open during a scan. Compute it on a
                // background thread and fill it in when ready.
                *self.histogram.borrow_mut() = [vec![0; 256], vec![0; 256], vec![0; 256]];
                *self.photo.borrow_mut() = Some(p.clone());
                self.root.set_child(Some(&self.body));
                state.viewer().set_show_original(false);
                self.refresh_presets(None);
                self.refresh_all();
                self.load_histogram_async(&state, &p);
            }
        }
    }

    /// The id of the photo currently bound, or 0.
    fn photo_id(&self) -> i64 {
        self.photo.borrow().as_ref().map(|p| p.id).unwrap_or(0)
    }

    /// Compute the histogram off the main thread and apply it when ready.
    ///
    /// The image load/decode is slow (a big file, or a disk busy with a scan);
    /// running it here instead of in `load` keeps opening a photo instant. A
    /// generation guard discards a result that arrives after the user moved to a
    /// different photo.
    fn load_histogram_async(self: &Rc<Self>, state: &Rc<AppState>, photo: &Photo) {
        let generation = self.hist_generation.get().wrapping_add(1);
        self.hist_generation.set(generation);

        // Resolve the source on the main thread (owns the non-Send AppState).
        let source = if let Some((server_id, asset_id)) = parse_immich(&photo.path) {
            match state.lib.immich_server(server_id) {
                Ok(Some(s)) => HistSource::Immich(s.base_url, s.api_key, asset_id),
                _ => return,
            }
        } else {
            HistSource::Local(photo.path.clone())
        };

        let (tx, rx) = glib::MainContext::channel::<[Vec<u32>; 3]>(glib::Priority::DEFAULT);
        std::thread::spawn(move || {
            if let Some(hist) = histogram_from_source(&source) {
                let _ = tx.send(hist);
            }
        });

        let this = self.clone();
        rx.attach(None, move |hist| {
            // Ignore a late result for a photo the user already navigated away
            // from.
            if this.hist_generation.get() == generation {
                *this.histogram.borrow_mut() = hist;
                // Redraw the per-channel histogram areas.
                for cw in this.channels.borrow().iter() {
                    cw.area.queue_draw();
                }
            }
            glib::ControlFlow::Break
        });
    }

    // --- body construction (once) ---

    fn build_body(self: &Rc<Self>) {
        let head = Label::new(Some("Edit"));
        head.set_xalign(0.0);
        head.add_css_class("title-4");
        self.body.append(&head);

        self.build_flip();
        self.build_scales();
        self.build_crop();
        self.build_levels();
        self.build_presets();
        self.build_actions();
    }

    fn build_flip(self: &Rc<Self>) {
        let row = GtkBox::new(Orientation::Horizontal, 6);
        let flip_h = CheckButton::with_label("Flip H");
        let flip_v = CheckButton::with_label("Flip V");
        row.append(&flip_h);
        row.append(&flip_v);
        self.body.append(&row);

        {
            let this = self.clone();
            let w = flip_h.clone();
            flip_h.connect_toggled(move |_| {
                if this.loading.get() {
                    return;
                }
                this.edit.borrow_mut().flip_h = w.is_active();
                this.commit();
            });
        }
        {
            let this = self.clone();
            let w = flip_v.clone();
            flip_v.connect_toggled(move |_| {
                if this.loading.get() {
                    return;
                }
                this.edit.borrow_mut().flip_v = w.is_active();
                this.commit();
            });
        }
        self.stash_flip(flip_h, flip_v);
    }

    fn build_scales(self: &Rc<Self>) {
        let straighten = labeled_scale(&self.body, "Straighten (°)", -15.0, 15.0, 0.1);
        {
            let this = self.clone();
            let s = straighten.clone();
            straighten.connect_value_changed(move |_| {
                if this.loading.get() {
                    return;
                }
                this.edit.borrow_mut().straighten_mdeg = (s.value() * 1000.0).round() as i32;
                this.commit();
            });
        }
        let brightness = labeled_scale(&self.body, "Brightness", -100.0, 100.0, 1.0);
        {
            let this = self.clone();
            let s = brightness.clone();
            brightness.connect_value_changed(move |_| {
                if this.loading.get() {
                    return;
                }
                this.edit.borrow_mut().brightness = s.value().round() as i32;
                this.commit();
            });
        }
        let contrast = labeled_scale(&self.body, "Contrast", -100.0, 100.0, 1.0);
        {
            let this = self.clone();
            let s = contrast.clone();
            contrast.connect_value_changed(move |_| {
                if this.loading.get() {
                    return;
                }
                this.edit.borrow_mut().contrast = s.value().round() as i32;
                this.commit();
            });
        }
        self.stash_scales(straighten, brightness, contrast);
    }

    fn build_crop(self: &Rc<Self>) {
        let label = Label::new(Some("Crop (per-mille; width/height 0 = no crop)"));
        label.set_xalign(0.0);
        self.body.append(&label);
        let row = GtkBox::new(Orientation::Horizontal, 6);
        let mk = |name: &str| {
            let l = Label::new(Some(name));
            let sb = SpinButton::with_range(0.0, 1000.0, 10.0);
            row.append(&l);
            row.append(&sb);
            sb
        };
        let cx = mk("x");
        let cy = mk("y");
        let cw = mk("w");
        let ch = mk("h");
        self.body.append(&row);

        for (kind, sb) in [
            (0, cx.clone()),
            (1, cy.clone()),
            (2, cw.clone()),
            (3, ch.clone()),
        ] {
            let this = self.clone();
            sb.connect_value_changed(move |s| {
                if this.loading.get() {
                    return;
                }
                let v = s.value().round() as i32;
                {
                    let mut e = this.edit.borrow_mut();
                    match kind {
                        0 => e.crop_x = v,
                        1 => e.crop_y = v,
                        2 => e.crop_w = v,
                        _ => e.crop_h = v,
                    }
                }
                this.commit();
            });
        }
        self.stash_crop(cx.clone(), cy.clone(), cw.clone(), ch.clone());

        // Interactive drag-to-crop toggle. When on, the viewer shows the image
        // uncropped with a draggable rectangle overlay; finishing a drag writes
        // the crop back into the spin buttons (and commits).
        let crop_btn = CheckButton::with_label("Crop by dragging on the image");
        self.body.append(&crop_btn);
        *self.crop_btn.borrow_mut() = Some(crop_btn.clone());
        let this = self.clone();
        crop_btn.connect_toggled(move |b| {
            let Some(state) = this.state() else { return };
            let on = b.is_active();
            if on {
                // Register the callback that receives the new per-mille crop.
                let this2 = this.clone();
                state.viewer().set_crop_callback(move |(x, y, w, h)| {
                    {
                        let mut e = this2.edit.borrow_mut();
                        e.crop_x = x;
                        e.crop_y = y;
                        e.crop_w = w;
                        e.crop_h = h;
                    }
                    // Reflect in the spin buttons without retriggering commit.
                    this2.loading.set(true);
                    if let Some(c) = this2.controls.borrow().as_ref() {
                        c.crop_x.set_value(x as f64);
                        c.crop_y.set_value(y as f64);
                        c.crop_w.set_value(w as f64);
                        c.crop_h.set_value(h as f64);
                    }
                    this2.loading.set(false);
                    this2.commit();
                });
            }
            let e = this.edit.borrow();
            let initial = (e.crop_x, e.crop_y, e.crop_w, e.crop_h);
            drop(e);
            state.viewer().set_crop_mode(on, initial);
        });
    }

    fn build_levels(self: &Rc<Self>) {
        let head = Label::new(Some("Color levels — drag the markers under each histogram"));
        head.set_xalign(0.0);
        head.add_css_class("heading");
        self.body.append(&head);

        for ch in 0..3usize {
            let name = ["Red", "Green", "Blue"][ch];
            let lbl = Label::new(Some(name));
            lbl.set_xalign(0.0);
            self.body.append(&lbl);

            let area = DrawingArea::new();
            area.set_content_height(80);
            area.set_hexpand(true);
            self.attach_histogram_draw(&area, ch);
            self.attach_marker_drag(&area, ch);
            self.body.append(&area);

            let row = GtkBox::new(Orientation::Horizontal, 6);
            row.append(&Label::new(Some("blk")));
            let black = SpinButton::with_range(0.0, 255.0, 1.0);
            row.append(&black);
            row.append(&Label::new(Some("wht")));
            let white = SpinButton::with_range(0.0, 255.0, 1.0);
            row.append(&white);
            row.append(&Label::new(Some("γ×1000")));
            let gamma = SpinButton::with_range(10.0, 5000.0, 10.0);
            row.append(&gamma);
            self.body.append(&row);

            for (kind, sb) in [(0, black.clone()), (1, white.clone()), (2, gamma.clone())] {
                let this = self.clone();
                sb.connect_value_changed(move |s| {
                    if this.loading.get() {
                        return;
                    }
                    let v = s.value().round() as i32;
                    set_channel_val(&mut this.edit.borrow_mut().levels, ch, kind, v);
                    this.commit();
                    if let Some(cw) = this.channels.borrow().get(ch) {
                        cw.area.queue_draw();
                    }
                });
            }

            self.channels.borrow_mut().push(ChannelWidgets {
                black,
                white,
                gamma,
                area,
            });
        }

        let auto = Button::with_label("Auto levels (from histogram)");
        {
            let this = self.clone();
            auto.connect_clicked(move |_| this.auto_levels());
        }
        self.body.append(&auto);
    }

    fn build_presets(self: &Rc<Self>) {
        let head = Label::new(Some("Levels presets"));
        head.set_xalign(0.0);
        head.add_css_class("heading");
        self.body.append(&head);
        self.body.append(&self.preset_drop);
        {
            let this = self.clone();
            self.preset_drop.connect_selected_notify(move |d| {
                if this.loading.get() {
                    return;
                }
                let sel = d.selected();
                if sel == 0 {
                    return;
                }
                let levels = this.presets.borrow().get(sel as usize - 1).map(|p| p.levels);
                if let Some(levels) = levels {
                    this.edit.borrow_mut().levels = levels;
                    this.commit();
                    this.refresh_channels();
                }
            });
        }

        let row = GtkBox::new(Orientation::Horizontal, 6);
        let save = Button::with_label("Save…");
        {
            let this = self.clone();
            save.connect_clicked(move |_| this.save_preset());
        }
        let delete = Button::with_label("Delete");
        {
            let this = self.clone();
            delete.connect_clicked(move |_| this.delete_preset());
        }
        let apply = Button::with_label("Apply to folder");
        {
            let this = self.clone();
            apply.connect_clicked(move |_| this.apply_to_folder());
        }
        row.append(&save);
        row.append(&delete);
        row.append(&apply);
        self.body.append(&row);
    }

    fn build_actions(self: &Rc<Self>) {
        let view_original = CheckButton::with_label("View original");
        {
            let this = self.clone();
            let w = view_original.clone();
            view_original.connect_toggled(move |_| {
                if let Some(s) = this.state() {
                    s.viewer().set_show_original(w.is_active());
                }
            });
        }
        self.body.append(&view_original);

        let row = GtkBox::new(Orientation::Horizontal, 6);
        let revert = Button::with_label("Revert all");
        revert.add_css_class("destructive-action");
        {
            let this = self.clone();
            revert.connect_clicked(move |_| {
                let id = this.photo_id();
                *this.edit.borrow_mut() = PhotoEdit {
                    photo_id: id,
                    ..Default::default()
                };
                this.commit();
                this.refresh_all();
            });
        }
        let export = Button::with_label("Export copy…");
        {
            let this = self.clone();
            export.connect_clicked(move |_| this.export_copy());
        }
        row.append(&revert);
        row.append(&export);
        self.body.append(&row);

        self.stash_view_original(view_original);
    }

    // --- refresh (on load / auto / preset) ---

    fn refresh_all(self: &Rc<Self>) {
        self.loading.set(true);
        let e = self.edit.borrow().clone();
        if let Some(c) = self.controls.borrow().as_ref() {
            c.flip_h.set_active(e.flip_h);
            c.flip_v.set_active(e.flip_v);
            c.straighten.set_value(e.straighten_mdeg as f64 / 1000.0);
            c.brightness.set_value(e.brightness as f64);
            c.contrast.set_value(e.contrast as f64);
            c.crop_x.set_value(e.crop_x as f64);
            c.crop_y.set_value(e.crop_y as f64);
            c.crop_w.set_value(e.crop_w as f64);
            c.crop_h.set_value(e.crop_h as f64);
            c.view_original.set_active(false);
        }
        self.loading.set(false);
        self.refresh_channels();
    }

    /// Push levels values into the channel spin buttons and redraw histograms.
    fn refresh_channels(self: &Rc<Self>) {
        self.loading.set(true);
        let lv = self.edit.borrow().levels;
        for (ch, cw) in self.channels.borrow().iter().enumerate() {
            let (b, w, g) = channel_vals(&lv, ch);
            cw.black.set_value(b as f64);
            cw.white.set_value(w as f64);
            cw.gamma.set_value(g as f64);
            cw.area.queue_draw();
        }
        self.loading.set(false);
    }

    // --- persistence ---

    /// Persist the current edit, invalidate thumbnails, refresh viewer + grid.
    fn commit(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let mut edit = self.edit.borrow().clone();
        match state.lib.set_photo_edit(&edit) {
            Ok(rev) => edit.edit_rev = rev,
            Err(e) => {
                show_error(&state, &e.to_string());
                return;
            }
        }
        let hash = self.photo.borrow().as_ref().map(|p| p.hash.clone());
        *self.edit.borrow_mut() = edit;
        if let Some(h) = hash {
            if !h.is_empty() {
                let _ = state.gen.invalidate(&h);
            }
        }
        state.viewer().reload_current();
        state.grid().reload_from_source();
    }

    // --- levels helpers ---

    fn attach_histogram_draw(self: &Rc<Self>, area: &DrawingArea, ch: usize) {
        let this = self.clone();
        area.set_draw_func(move |_area, cr, w, h| {
            let w = w as f64;
            let h = h as f64;
            let strip = 12.0;
            let hist_h = (h - strip).max(1.0);

            cr.set_source_rgb(0.12, 0.12, 0.12);
            let _ = cr.paint();

            let hist = &this.histogram.borrow()[ch];
            let max = hist.iter().copied().max().unwrap_or(1).max(1) as f64;
            let max_log = (1.0 + max).ln();
            let col = [(0.85, 0.3, 0.3), (0.3, 0.8, 0.3), (0.4, 0.5, 0.9)][ch];
            cr.set_source_rgb(col.0, col.1, col.2);
            for (i, &count) in hist.iter().enumerate() {
                let x = i as f64 / 255.0 * w;
                let bar = (1.0 + count as f64).ln() / max_log * hist_h;
                cr.rectangle(x, hist_h - bar, (w / 256.0).max(1.0), bar);
            }
            let _ = cr.fill();

            let lv = this.edit.borrow().levels;
            let (black, white, gamma_m) = channel_vals(&lv, ch);
            let bx = black as f64 / 255.0 * w;
            let wx = white as f64 / 255.0 * w;
            let gamma = (gamma_m.max(1) as f64) / 1000.0;
            let gx = bx + (wx - bx) * 0.5f64.powf(gamma);
            let y0 = hist_h;

            cr.set_source_rgb(0.0, 0.0, 0.0);
            draw_triangle(cr, bx, y0, strip);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(1.0);
            draw_triangle_outline(cr, bx, y0, strip);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            draw_triangle(cr, wx, y0, strip);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            draw_triangle_outline(cr, wx, y0, strip);
            cr.set_source_rgb(0.6, 0.6, 0.6);
            draw_triangle(cr, gx, y0, strip);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            draw_triangle_outline(cr, gx, y0, strip);
        });
    }

    fn attach_marker_drag(self: &Rc<Self>, area: &DrawingArea, ch: usize) {
        let active = Rc::new(std::cell::Cell::new(-1i32));
        let drag = gtk4::GestureDrag::new();
        {
            let this = self.clone();
            let active = active.clone();
            let area_w = area.clone();
            drag.connect_drag_begin(move |_g, sx, _sy| {
                let w = area_w.width().max(1) as f64;
                let val = (sx / w * 255.0).clamp(0.0, 255.0);
                let lv = this.edit.borrow().levels;
                let (black, white, gamma_m) = channel_vals(&lv, ch);
                let gamma = (gamma_m.max(1) as f64) / 1000.0;
                let gx = black as f64 + (white as f64 - black as f64) * 0.5f64.powf(gamma);
                let db = (val - black as f64).abs();
                let dw = (val - white as f64).abs();
                let dg = (val - gx).abs();
                let pick = if db <= dw && db <= dg {
                    0
                } else if dw <= dg {
                    1
                } else {
                    2
                };
                active.set(pick);
                this.apply_marker(ch, pick, val);
            });
        }
        {
            let this = self.clone();
            let active = active.clone();
            let area_w = area.clone();
            drag.connect_drag_update(move |g, ox, _oy| {
                let pick = active.get();
                if pick < 0 {
                    return;
                }
                let w = area_w.width().max(1) as f64;
                let start = g.start_point().map(|(x, _)| x).unwrap_or(0.0);
                let val = ((start + ox) / w * 255.0).clamp(0.0, 255.0);
                this.apply_marker(ch, pick, val);
            });
        }
        {
            let active = active.clone();
            drag.connect_drag_end(move |_g, _ox, _oy| active.set(-1));
        }
        area.add_controller(drag);
    }

    fn apply_marker(self: &Rc<Self>, ch: usize, marker: i32, val: f64) {
        if self.photo_id() == 0 {
            return;
        }
        {
            let mut edit = self.edit.borrow_mut();
            let lv = &mut edit.levels;
            let (black, white, _g) = channel_vals(lv, ch);
            match marker {
                0 => set_channel_val(lv, ch, 0, (val.round() as i32).min(white - 1).max(0)),
                1 => set_channel_val(lv, ch, 1, (val.round() as i32).max(black + 1).min(255)),
                2 => {
                    let span = (white - black).max(1) as f64;
                    let t = ((val - black as f64) / span).clamp(0.01, 0.99);
                    let gamma = (0.5f64.ln() / t.ln()).clamp(0.01, 5.0);
                    set_channel_val(lv, ch, 2, (gamma * 1000.0).round() as i32);
                }
                _ => {}
            }
        }
        self.commit();
        self.refresh_channels();
    }

    fn auto_levels(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let Some(photo) = self.photo.borrow().clone() else {
            return;
        };
        let img = match load_image_for_edit(&state, &photo) {
            Some(i) => i,
            None => {
                show_error(&state, "Could not read the image for auto levels.");
                return;
            }
        };
        let small = if img.width().max(img.height()) > 1024 {
            image::imageops::thumbnail(&img, 1024, 1024)
        } else {
            img
        };
        let levels = crate::edit::auto_levels(&small, 0.005);
        self.edit.borrow_mut().levels = levels;
        self.commit();
        self.refresh_channels();
    }

    // --- presets ---

    fn refresh_presets(self: &Rc<Self>, select_name: Option<&str>) {
        let Some(state) = self.state() else { return };
        let presets = state.lib.level_presets().unwrap_or_default();
        let names: Vec<String> = std::iter::once("— presets —".to_string())
            .chain(presets.iter().map(|p| p.name.clone()))
            .collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        self.loading.set(true);
        self.preset_drop.set_model(Some(&StringList::new(&refs)));
        if let Some(n) = select_name {
            if let Some(pos) = presets.iter().position(|p| p.name == n) {
                self.preset_drop.set_selected(pos as u32 + 1);
            }
        }
        self.loading.set(false);
        *self.presets.borrow_mut() = presets;
    }

    fn save_preset(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let levels = self.edit.borrow().levels;
        let this = self.clone();
        super::dialogs::prompt_text(
            &state,
            None,
            "Save levels preset",
            "Preset name",
            "",
            move |name| {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Some(state) = this.state() {
                    if let Err(e) = state.lib.save_level_preset(&name, &levels) {
                        show_error(&state, &e.to_string());
                        return;
                    }
                }
                this.refresh_presets(Some(&name));
            },
        );
    }

    fn delete_preset(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let sel = self.preset_drop.selected();
        if sel == 0 {
            return;
        }
        let id = self.presets.borrow().get(sel as usize - 1).map(|p| p.id);
        if let Some(id) = id {
            if let Err(e) = state.lib.delete_level_preset(id) {
                show_error(&state, &e.to_string());
                return;
            }
            self.refresh_presets(None);
        }
    }

    fn apply_to_folder(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let Some(photo) = self.photo.borrow().clone() else {
            return;
        };
        let levels = self.edit.borrow().levels;
        match state.lib.apply_levels_to_folder(photo.folder_id, &levels) {
            Ok(touched) => {
                for (_, hash) in &touched {
                    if !hash.is_empty() {
                        let _ = state.gen.invalidate(hash);
                    }
                }
                state.viewer().reload_current();
                state.grid().reload_from_source();
            }
            Err(e) => show_error(&state, &e.to_string()),
        }
    }

    fn export_copy(self: &Rc<Self>) {
        let Some(state) = self.state() else { return };
        let Some(photo) = self.photo.borrow().clone() else {
            return;
        };
        let edit = self.edit.borrow().clone();
        super::export::export_photos(&state, vec![(photo, edit)]);
    }

    // --- widget stashing (fill Controls once) ---

    fn stash_flip(self: &Rc<Self>, flip_h: CheckButton, flip_v: CheckButton) {
        self.with_controls(|c| {
            c.flip_h = flip_h;
            c.flip_v = flip_v;
        });
    }
    fn stash_scales(self: &Rc<Self>, straighten: Scale, brightness: Scale, contrast: Scale) {
        self.with_controls(|c| {
            c.straighten = straighten;
            c.brightness = brightness;
            c.contrast = contrast;
        });
    }
    fn stash_crop(self: &Rc<Self>, x: SpinButton, y: SpinButton, w: SpinButton, h: SpinButton) {
        self.with_controls(|c| {
            c.crop_x = x;
            c.crop_y = y;
            c.crop_w = w;
            c.crop_h = h;
        });
    }
    fn stash_view_original(self: &Rc<Self>, v: CheckButton) {
        self.with_controls(|c| c.view_original = v);
    }

    /// Ensure a `Controls` exists, then mutate it. Placeholder widgets fill any
    /// not-yet-stashed fields; every field is stashed during `build_body`.
    fn with_controls(self: &Rc<Self>, f: impl FnOnce(&mut Controls)) {
        let mut slot = self.controls.borrow_mut();
        if slot.is_none() {
            *slot = Some(Controls {
                flip_h: CheckButton::new(),
                flip_v: CheckButton::new(),
                straighten: Scale::with_range(Orientation::Horizontal, -15.0, 15.0, 0.1),
                brightness: Scale::with_range(Orientation::Horizontal, -100.0, 100.0, 1.0),
                contrast: Scale::with_range(Orientation::Horizontal, -100.0, 100.0, 1.0),
                crop_x: SpinButton::with_range(0.0, 1000.0, 10.0),
                crop_y: SpinButton::with_range(0.0, 1000.0, 10.0),
                crop_w: SpinButton::with_range(0.0, 1000.0, 10.0),
                crop_h: SpinButton::with_range(0.0, 1000.0, 10.0),
                view_original: CheckButton::new(),
            });
        }
        f(slot.as_mut().unwrap());
    }
}

/// A titled horizontal slider appended to `parent`; returns it for wiring.
fn labeled_scale(parent: &GtkBox, title: &str, min: f64, max: f64, step: f64) -> Scale {
    let row = GtkBox::new(Orientation::Vertical, 2);
    let label = Label::new(Some(title));
    label.set_xalign(0.0);
    let scale = Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    row.append(&label);
    row.append(&scale);
    parent.append(&row);
    scale
}

fn draw_triangle(cr: &gtk4::cairo::Context, x: f64, y0: f64, h: f64) {
    let half = h * 0.5;
    cr.move_to(x, y0);
    cr.line_to(x - half, y0 + h);
    cr.line_to(x + half, y0 + h);
    cr.close_path();
    let _ = cr.fill();
}

fn draw_triangle_outline(cr: &gtk4::cairo::Context, x: f64, y0: f64, h: f64) {
    let half = h * 0.5;
    cr.move_to(x, y0);
    cr.line_to(x - half, y0 + h);
    cr.line_to(x + half, y0 + h);
    cr.close_path();
    let _ = cr.stroke();
}

fn channel_vals(l: &Levels, ch: usize) -> (i32, i32, i32) {
    match ch {
        0 => (l.r_black, l.r_white, l.r_gamma_mille),
        1 => (l.g_black, l.g_white, l.g_gamma_mille),
        _ => (l.b_black, l.b_white, l.b_gamma_mille),
    }
}

fn set_channel_val(l: &mut Levels, ch: usize, kind: i32, v: i32) {
    match (ch, kind) {
        (0, 0) => l.r_black = v,
        (0, 1) => l.r_white = v,
        (0, 2) => l.r_gamma_mille = v,
        (1, 0) => l.g_black = v,
        (1, 1) => l.g_white = v,
        (1, 2) => l.g_gamma_mille = v,
        (2, 0) => l.b_black = v,
        (2, 1) => l.b_white = v,
        (2, 2) => l.b_gamma_mille = v,
        _ => {}
    }
}

/// A histogram image source, resolved on the main thread so the worker thread
/// needs no access to the non-`Send` `AppState`.
enum HistSource {
    /// A local file at this path.
    Local(String),
    /// An Immich asset: server base URL, API key, asset id.
    Immich(String, String, String),
}

/// Compute a per-channel 256-bin histogram at a working resolution from a
/// resolved source. Runs on a background thread.
fn histogram_from_source(source: &HistSource) -> Option<[Vec<u32>; 3]> {
    let img = match source {
        HistSource::Local(path) => image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?
            .to_rgba8(),
        HistSource::Immich(base_url, api_key, asset_id) => {
            let client = crate::immich::Client::new(base_url, api_key);
            let bytes = client.asset_original(asset_id).ok()?;
            image::load_from_memory(&bytes).ok()?.to_rgba8()
        }
    };
    let mut hist = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
    let small = if img.width().max(img.height()) > 1024 {
        image::imageops::thumbnail(&img, 1024, 1024)
    } else {
        img
    };
    for px in small.pixels() {
        hist[0][px.0[0] as usize] += 1;
        hist[1][px.0[1] as usize] += 1;
        hist[2][px.0[2] as usize] += 1;
    }
    Some(hist)
}

/// Load the full image for editing: a local file, or the Immich original.
pub fn load_image_for_edit(state: &Rc<AppState>, photo: &Photo) -> Option<image::RgbaImage> {
    if let Some((server_id, asset_id)) = parse_immich(&photo.path) {
        let server = state.lib.immich_server(server_id).ok()??;
        let client = crate::immich::Client::new(&server.base_url, &server.api_key);
        let bytes = client.asset_original(&asset_id).ok()?;
        let img = image::load_from_memory(&bytes).ok()?;
        return Some(img.to_rgba8());
    }
    let img = image::ImageReader::open(&photo.path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(img.to_rgba8())
}

/// Parse an `immich://<server_id>/<asset_id>` path.
fn parse_immich(path: &str) -> Option<(i64, String)> {
    let rest = path.strip_prefix("immich://")?;
    let (sid, asset) = rest.split_once('/')?;
    let sid: i64 = sid.parse().ok()?;
    if asset.is_empty() {
        return None;
    }
    Some((sid, asset.to_string()))
}
