//! Non-destructive edit panel.
//!
//! A modal window bound to one photo. It edits the photo's [`PhotoEdit`] record
//! live: every control change writes the record with [`Library::set_photo_edit`],
//! invalidates the photo's thumbnails, and re-renders the viewer. The original
//! file on disk is never changed.
//!
//! Controls: flip H/V, straighten, brightness/contrast, per-channel color
//! levels (black/white/gamma) with an auto-levels button, crop (numeric
//! per-mille), a levels-preset chooser with save/delete/apply-to-folder, a
//! "view original" toggle, revert, and export of a baked copy.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, DrawingArea, DropDown, Label, Orientation, Scale,
    ScrolledWindow, SpinButton, StringList, Window,
};

use crate::model::{Levels, LevelPreset, Photo, PhotoEdit};

use super::state::{show_error, AppState};

/// Per-channel spin buttons for the levels panel, kept so auto/preset changes
/// can push new values back into the widgets.
struct ChannelWidgets {
    black: SpinButton,
    white: SpinButton,
    gamma: SpinButton,
    area: DrawingArea,
}

/// Shared editor state passed to every control callback.
struct Ed {
    state: Rc<AppState>,
    viewer: Rc<super::viewer::Viewer>,
    photo: Photo,
    edit: RefCell<PhotoEdit>,
    /// Guards control callbacks while we programmatically set widget values.
    loading: std::cell::Cell<bool>,
    presets: RefCell<Vec<LevelPreset>>,
    preset_drop: DropDown,
    /// Per-channel histogram (R, G, B), 256 bins each, from the original at a
    /// working resolution. Used to draw the levels histogram behind the markers.
    histogram: [Vec<u32>; 3],
    /// The per-channel levels widgets, filled as the panel is built.
    channels: RefCell<Vec<ChannelWidgets>>,
}

/// Open the edit panel for `photo`.
pub fn open(state: &Rc<AppState>, viewer: Rc<super::viewer::Viewer>, photo: Photo) {
    let edit = state.lib.photo_edit(photo.id).unwrap_or(PhotoEdit {
        photo_id: photo.id,
        ..Default::default()
    });

    let presets = state.lib.level_presets().unwrap_or_default();
    let preset_names: Vec<String> = std::iter::once("— presets —".to_string())
        .chain(presets.iter().map(|p| p.name.clone()))
        .collect();
    let name_refs: Vec<&str> = preset_names.iter().map(|s| s.as_str()).collect();
    let preset_drop = DropDown::new(Some(StringList::new(&name_refs)), gtk4::Expression::NONE);

    let histogram = compute_histogram(&photo);

    let ed = Rc::new(Ed {
        state: state.clone(),
        viewer: viewer.clone(),
        photo,
        edit: RefCell::new(edit),
        loading: std::cell::Cell::new(false),
        presets: RefCell::new(presets),
        preset_drop: preset_drop.clone(),
        histogram,
        channels: RefCell::new(Vec::new()),
    });

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let heading = Label::new(Some(&format!("Edit \"{}\"", ed.photo.filename)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    root.append(&heading);

    // --- Orientation: flip ---
    let flip_row = GtkBox::new(Orientation::Horizontal, 6);
    let flip_h = CheckButton::with_label("Flip horizontal");
    let flip_v = CheckButton::with_label("Flip vertical");
    flip_h.set_active(ed.edit.borrow().flip_h);
    flip_v.set_active(ed.edit.borrow().flip_v);
    flip_row.append(&flip_h);
    flip_row.append(&flip_v);
    root.append(&flip_row);
    {
        let ed = ed.clone();
        flip_h.connect_toggled(move |b| {
            if ed.loading.get() {
                return;
            }
            ed.edit.borrow_mut().flip_h = b.is_active();
            commit(&ed);
        });
    }
    {
        let ed = ed.clone();
        flip_v.connect_toggled(move |b| {
            if ed.loading.get() {
                return;
            }
            ed.edit.borrow_mut().flip_v = b.is_active();
            commit(&ed);
        });
    }

    // --- Straighten ---
    root.append(&labeled_scale(
        "Straighten (°)",
        -15.0,
        15.0,
        0.1,
        ed.edit.borrow().straighten_mdeg as f64 / 1000.0,
        {
            let ed = ed.clone();
            move |v| {
                ed.edit.borrow_mut().straighten_mdeg = (v * 1000.0).round() as i32;
                commit(&ed);
            }
        },
    ));

    // --- Brightness / Contrast ---
    root.append(&labeled_scale(
        "Brightness",
        -100.0,
        100.0,
        1.0,
        ed.edit.borrow().brightness as f64,
        {
            let ed = ed.clone();
            move |v| {
                ed.edit.borrow_mut().brightness = v.round() as i32;
                commit(&ed);
            }
        },
    ));
    root.append(&labeled_scale(
        "Contrast",
        -100.0,
        100.0,
        1.0,
        ed.edit.borrow().contrast as f64,
        {
            let ed = ed.clone();
            move |v| {
                ed.edit.borrow_mut().contrast = v.round() as i32;
                commit(&ed);
            }
        },
    ));

    // --- Crop (numeric per-mille) ---
    let crop = build_crop(&ed);
    root.append(&crop);

    // --- Color levels ---
    let levels = build_levels(&ed);
    root.append(&levels);

    // --- Presets ---
    let presets_box = build_presets(&ed);
    root.append(&presets_box);

    // --- Bottom actions ---
    let actions = GtkBox::new(Orientation::Horizontal, 6);
    let original = CheckButton::with_label("View original");
    {
        let viewer = viewer.clone();
        original.connect_toggled(move |b| viewer.set_show_original(b.is_active()));
    }
    let revert = Button::with_label("Revert all");
    revert.add_css_class("destructive-action");
    {
        let ed = ed.clone();
        revert.connect_clicked(move |_| {
            *ed.edit.borrow_mut() = PhotoEdit {
                photo_id: ed.photo.id,
                ..Default::default()
            };
            commit(&ed);
        });
    }
    let export = Button::with_label("Export copy…");
    {
        let ed = ed.clone();
        export.connect_clicked(move |_| export_copy(&ed));
    }
    let close = Button::with_label("Close");
    close.add_css_class("suggested-action");
    actions.append(&original);
    actions.append(&revert);
    actions.append(&export);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    actions.append(&spacer);
    actions.append(&close);
    root.append(&actions);

    let scroll = ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroll.set_min_content_height(560);
    scroll.set_child(Some(&root));

    let window = Window::builder()
        .title("Edit Photo")
        .modal(false)
        .default_width(420)
        .default_height(640)
        .child(&scroll)
        .build();
    if let Some(p) = state.window() {
        window.set_transient_for(Some(&p));
    }
    {
        let window = window.clone();
        let viewer = viewer.clone();
        close.connect_clicked(move |_| {
            // Restore the edited view if the user left "view original" on.
            viewer.set_show_original(false);
            window.close();
        });
    }
    window.set_visible(true);
}

/// Persist the current edit, invalidate thumbnails, and refresh viewer + grid.
fn commit(ed: &Rc<Ed>) {
    let mut edit = ed.edit.borrow().clone();
    match ed.state.lib.set_photo_edit(&edit) {
        Ok(rev) => edit.edit_rev = rev,
        Err(e) => {
            show_error(&ed.state, &e.to_string());
            return;
        }
    }
    *ed.edit.borrow_mut() = edit;
    if !ed.photo.hash.is_empty() {
        let _ = ed.state.gen.invalidate(&ed.photo.hash);
    }
    ed.viewer.reload_current();
    ed.state.grid().reload_from_source();
}

/// A titled horizontal slider that reports its value on change.
fn labeled_scale<F: Fn(f64) + 'static>(
    title: &str,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
    on_change: F,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 2);
    let label = Label::new(Some(title));
    label.set_xalign(0.0);
    let scale = Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value(value);
    scale.connect_value_changed(move |s| on_change(s.value()));
    row.append(&label);
    row.append(&scale);
    row
}

/// Crop controls: four per-mille spin buttons (x, y, w, h). w/h = 0 disables.
fn build_crop(ed: &Rc<Ed>) -> GtkBox {
    let outer = GtkBox::new(Orientation::Vertical, 2);
    let label = Label::new(Some("Crop (per-mille of image; width/height 0 = no crop)"));
    label.set_xalign(0.0);
    outer.append(&label);
    let row = GtkBox::new(Orientation::Horizontal, 6);
    let e = ed.edit.borrow();
    let spins = [
        ("x", e.crop_x),
        ("y", e.crop_y),
        ("w", e.crop_w),
        ("h", e.crop_h),
    ];
    drop(e);
    let mut widgets = Vec::new();
    for (name, val) in spins {
        let lbl = Label::new(Some(name));
        let sb = SpinButton::with_range(0.0, 1000.0, 10.0);
        sb.set_value(val as f64);
        row.append(&lbl);
        row.append(&sb);
        widgets.push((name, sb));
    }
    for (name, sb) in widgets {
        let ed = ed.clone();
        sb.connect_value_changed(move |s| {
            if ed.loading.get() {
                return;
            }
            let v = s.value().round() as i32;
            let mut edit = ed.edit.borrow_mut();
            match name {
                "x" => edit.crop_x = v,
                "y" => edit.crop_y = v,
                "w" => edit.crop_w = v,
                "h" => edit.crop_h = v,
                _ => {}
            }
            drop(edit);
            commit(&ed);
        });
    }
    outer.append(&row);
    outer
}

/// Per-channel color-levels controls: a live histogram with draggable black,
/// white, and gamma markers, plus spin buttons, and an auto-levels button.
fn build_levels(ed: &Rc<Ed>) -> GtkBox {
    let outer = GtkBox::new(Orientation::Vertical, 4);
    let head = Label::new(Some("Color levels — drag the markers under each histogram"));
    head.set_xalign(0.0);
    head.add_css_class("heading");
    outer.append(&head);

    for ch in 0..3usize {
        let name = ["Red", "Green", "Blue"][ch];
        let lbl = Label::new(Some(name));
        lbl.set_xalign(0.0);
        outer.append(&lbl);

        // Histogram + markers strip.
        let area = DrawingArea::new();
        area.set_content_height(80);
        area.set_hexpand(true);
        attach_histogram_draw(ed, &area, ch);
        attach_marker_drag(ed, &area, ch);
        outer.append(&area);

        // Numeric spin buttons under the strip.
        let row = GtkBox::new(Orientation::Horizontal, 6);
        let (b, w, g) = channel_vals(&ed.edit.borrow().levels, ch);
        row.append(&Label::new(Some("black")));
        let black = SpinButton::with_range(0.0, 255.0, 1.0);
        black.set_value(b as f64);
        row.append(&black);
        row.append(&Label::new(Some("white")));
        let white = SpinButton::with_range(0.0, 255.0, 1.0);
        white.set_value(w as f64);
        row.append(&white);
        row.append(&Label::new(Some("gamma×1000")));
        let gamma = SpinButton::with_range(10.0, 5000.0, 10.0);
        gamma.set_value(g as f64);
        row.append(&gamma);
        outer.append(&row);

        for (kind, sb) in [(0, black.clone()), (1, white.clone()), (2, gamma.clone())] {
            let ed = ed.clone();
            sb.connect_value_changed(move |s| {
                if ed.loading.get() {
                    return;
                }
                let v = s.value().round() as i32;
                set_channel_val(&mut ed.edit.borrow_mut().levels, ch, kind, v);
                commit(&ed);
                if let Some(cw) = ed.channels.borrow().get(ch) {
                    cw.area.queue_draw();
                }
            });
        }

        ed.channels.borrow_mut().push(ChannelWidgets {
            black,
            white,
            gamma,
            area,
        });
    }

    let auto = Button::with_label("Auto levels (from histogram)");
    {
        let ed = ed.clone();
        auto.connect_clicked(move |_| auto_levels(&ed));
    }
    outer.append(&auto);
    outer
}

/// Draw the channel histogram plus the black/white/gamma marker positions.
fn attach_histogram_draw(ed: &Rc<Ed>, area: &DrawingArea, ch: usize) {
    let ed = ed.clone();
    area.set_draw_func(move |_area, cr, w, h| {
        let w = w as f64;
        let h = h as f64;
        let strip = 12.0; // marker strip height at the bottom
        let hist_h = (h - strip).max(1.0);

        // Background.
        cr.set_source_rgb(0.12, 0.12, 0.12);
        let _ = cr.paint();

        // Histogram bars, log-scaled so small counts remain visible.
        let hist = &ed.histogram[ch];
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

        // Markers along the bottom strip.
        let lv = ed.edit.borrow().levels;
        let (black, white, gamma_m) = channel_vals(&lv, ch);
        let bx = black as f64 / 255.0 * w;
        let wx = white as f64 / 255.0 * w;
        // Gamma marker sits between black and white; midpoint shifted by gamma.
        let gamma = (gamma_m.max(1) as f64) / 1000.0;
        let t = 0.5f64.powf(gamma); // input position mapping to mid output
        let gx = bx + (wx - bx) * t;

        let y0 = hist_h;
        // Black marker (filled left triangle).
        cr.set_source_rgb(0.0, 0.0, 0.0);
        draw_triangle(cr, bx, y0, strip);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_line_width(1.0);
        draw_triangle_outline(cr, bx, y0, strip);
        // White marker.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        draw_triangle(cr, wx, y0, strip);
        cr.set_source_rgb(0.0, 0.0, 0.0);
        draw_triangle_outline(cr, wx, y0, strip);
        // Gamma marker (gray).
        cr.set_source_rgb(0.6, 0.6, 0.6);
        draw_triangle(cr, gx, y0, strip);
        cr.set_source_rgb(0.0, 0.0, 0.0);
        draw_triangle_outline(cr, gx, y0, strip);
    });
}

/// A filled up-pointing triangle marker centered at `x`, sitting below `y0`.
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

/// Wire click-drag on the histogram strip to move the nearest marker (black,
/// white, or gamma) and commit live.
fn attach_marker_drag(ed: &Rc<Ed>, area: &DrawingArea, ch: usize) {
    // Which marker is being dragged: 0=black, 1=white, 2=gamma, -1=none.
    let active = Rc::new(std::cell::Cell::new(-1i32));
    let drag = gtk4::GestureDrag::new();

    {
        let ed = ed.clone();
        let active = active.clone();
        let area_w = area.clone();
        drag.connect_drag_begin(move |_g, sx, _sy| {
            let w = area_w.width().max(1) as f64;
            let val = (sx / w * 255.0).clamp(0.0, 255.0);
            let lv = ed.edit.borrow().levels;
            let (black, white, gamma_m) = channel_vals(&lv, ch);
            let gamma = (gamma_m.max(1) as f64) / 1000.0;
            let gx = black as f64 + (white as f64 - black as f64) * 0.5f64.powf(gamma);
            // Pick the closest of the three markers.
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
            apply_marker(&ed, ch, pick, val);
        });
    }
    {
        let ed = ed.clone();
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
            apply_marker(&ed, ch, pick, val);
        });
    }
    {
        let active = active.clone();
        drag.connect_drag_end(move |_g, _ox, _oy| active.set(-1));
    }
    area.add_controller(drag);
}

/// Apply a marker drag: set black, white, or gamma for `ch` to reflect the
/// input value `val` (0..255), then commit and refresh widgets.
fn apply_marker(ed: &Rc<Ed>, ch: usize, marker: i32, val: f64) {
    {
        let mut edit = ed.edit.borrow_mut();
        let lv = &mut edit.levels;
        let (black, white, _g) = channel_vals(lv, ch);
        match marker {
            0 => {
                // Black must stay below white.
                let v = (val.round() as i32).min(white - 1).max(0);
                set_channel_val(lv, ch, 0, v);
            }
            1 => {
                let v = (val.round() as i32).max(black + 1).min(255);
                set_channel_val(lv, ch, 1, v);
            }
            2 => {
                // Convert the dragged input position into a gamma value:
                // t = (val-black)/(white-black); gamma = ln(0.5)/ln(t).
                let span = (white - black).max(1) as f64;
                let t = ((val - black as f64) / span).clamp(0.01, 0.99);
                let gamma = (0.5f64.ln() / t.ln()).clamp(0.01, 5.0);
                set_channel_val(lv, ch, 2, (gamma * 1000.0).round() as i32);
            }
            _ => {}
        }
    }
    commit(ed);
    refresh_channel_widgets(ed);
}

/// Push the current levels values back into every channel's spin buttons and
/// redraw its histogram strip, without firing the change callbacks.
fn refresh_channel_widgets(ed: &Rc<Ed>) {
    ed.loading.set(true);
    let lv = ed.edit.borrow().levels;
    for (ch, cw) in ed.channels.borrow().iter().enumerate() {
        let (b, w, g) = channel_vals(&lv, ch);
        cw.black.set_value(b as f64);
        cw.white.set_value(w as f64);
        cw.gamma.set_value(g as f64);
        cw.area.queue_draw();
    }
    ed.loading.set(false);
}

/// Compute a per-channel 256-bin histogram from the original at a working
/// resolution. Returns empty bins if the image cannot be read.
fn compute_histogram(photo: &Photo) -> [Vec<u32>; 3] {
    let mut hist = [vec![0u32; 256], vec![0u32; 256], vec![0u32; 256]];
    if let Some(img) = load_original(photo) {
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
    }
    hist
}

/// Preset chooser: apply on selection, save current, delete, apply to folder.
fn build_presets(ed: &Rc<Ed>) -> GtkBox {
    let outer = GtkBox::new(Orientation::Vertical, 4);
    let head = Label::new(Some("Levels presets"));
    head.set_xalign(0.0);
    head.add_css_class("heading");
    outer.append(&head);

    outer.append(&ed.preset_drop);
    {
        let ed = ed.clone();
        ed.preset_drop.clone().connect_selected_notify(move |d| {
            if ed.loading.get() {
                return;
            }
            let sel = d.selected();
            if sel == 0 {
                return; // placeholder row
            }
            let levels = ed
                .presets
                .borrow()
                .get(sel as usize - 1)
                .map(|p| p.levels);
            if let Some(levels) = levels {
                ed.edit.borrow_mut().levels = levels;
                commit(&ed);
                reload_widgets(&ed);
            }
        });
    }

    let row = GtkBox::new(Orientation::Horizontal, 6);
    let save = Button::with_label("Save preset…");
    {
        let ed = ed.clone();
        save.connect_clicked(move |_| save_preset(&ed));
    }
    let delete = Button::with_label("Delete preset");
    {
        let ed = ed.clone();
        delete.connect_clicked(move |_| delete_preset(&ed));
    }
    let apply_folder = Button::with_label("Apply to folder");
    {
        let ed = ed.clone();
        apply_folder.connect_clicked(move |_| apply_to_folder(&ed));
    }
    row.append(&save);
    row.append(&delete);
    row.append(&apply_folder);
    outer.append(&row);
    outer
}

/// Compute auto levels from the full-resolution original and apply them.
fn auto_levels(ed: &Rc<Ed>) {
    let img = match load_original(&ed.photo) {
        Some(i) => i,
        None => {
            show_error(&ed.state, "Could not read the original image for auto levels.");
            return;
        }
    };
    // Downscale big images before histogramming for speed.
    let small = if img.width().max(img.height()) > 1024 {
        image::imageops::thumbnail(&img, 1024, 1024)
    } else {
        img
    };
    let levels = crate::edit::auto_levels(&small, 0.005);
    ed.edit.borrow_mut().levels = levels;
    commit(ed);
    reload_widgets(ed);
}

/// Prompt for a name and save the current levels as a preset.
fn save_preset(ed: &Rc<Ed>) {
    let levels = ed.edit.borrow().levels;
    prompt(ed, "Save levels preset", "Preset name", "", {
        let ed = ed.clone();
        move |name| {
            if name.trim().is_empty() {
                return;
            }
            if let Err(e) = ed.state.lib.save_level_preset(name.trim(), &levels) {
                show_error(&ed.state, &e.to_string());
                return;
            }
            refresh_presets(&ed, Some(name.trim()));
        }
    });
}

/// Delete the currently selected preset.
fn delete_preset(ed: &Rc<Ed>) {
    let sel = ed.preset_drop.selected();
    if sel == 0 {
        return;
    }
    let id = ed.presets.borrow().get(sel as usize - 1).map(|p| p.id);
    if let Some(id) = id {
        if let Err(e) = ed.state.lib.delete_level_preset(id) {
            show_error(&ed.state, &e.to_string());
            return;
        }
        refresh_presets(ed, None);
    }
}

/// Apply the current levels to every photo in the viewed photo's folder.
fn apply_to_folder(ed: &Rc<Ed>) {
    let levels = ed.edit.borrow().levels;
    let folder_id = ed.photo.folder_id;
    match ed.state.lib.apply_levels_to_folder(folder_id, &levels) {
        Ok(touched) => {
            for (_, hash) in &touched {
                if !hash.is_empty() {
                    let _ = ed.state.gen.invalidate(hash);
                }
            }
            ed.viewer.reload_current();
            ed.state.grid().reload_from_source();
        }
        Err(e) => show_error(&ed.state, &e.to_string()),
    }
}

/// Export a baked copy of the edited image to a user-chosen path.
fn export_copy(ed: &Rc<Ed>) {
    let img = match load_original(&ed.photo) {
        Some(i) => i,
        None => {
            show_error(&ed.state, "Could not read the original image to export.");
            return;
        }
    };
    // Apply the stored 90-degree rotation, then the edits, at full resolution.
    let img = rotate_full(img, ed.photo.orientation);
    let out = crate::edit::apply_edits(img, &ed.edit.borrow());

    let dialog = gtk4::FileChooserDialog::new(
        Some("Export edited copy"),
        ed.state.window().as_ref(),
        gtk4::FileChooserAction::Save,
        &[
            ("Cancel", gtk4::ResponseType::Cancel),
            ("Save", gtk4::ResponseType::Accept),
        ],
    );
    let default_name = format!("{}-edited.jpg", stem(&ed.photo.filename));
    dialog.set_current_name(&default_name);
    let ed2 = ed.clone();
    dialog.connect_response(move |d, resp| {
        if resp == gtk4::ResponseType::Accept {
            if let Some(path) = d.file().and_then(|f| f.path()) {
                if let Err(e) = save_image(&out, &path) {
                    show_error(&ed2.state, &e);
                }
            }
        }
        d.close();
    });
    dialog.set_modal(true);
    dialog.set_visible(true);
}

/// Save `img` to `path`, choosing PNG for a `.png` extension, else JPEG q90.
fn save_image(img: &image::RgbaImage, path: &std::path::Path) -> Result<(), String> {
    let is_png = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("png"))
        .unwrap_or(false);
    if is_png {
        img.save(path).map_err(|e| e.to_string())
    } else {
        let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut w = std::io::BufWriter::new(file);
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 90);
        use image::ImageEncoder;
        enc.write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())
    }
}

// --- helpers ---

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

/// Reload the preset list and reselect `select_name` if given.
fn refresh_presets(ed: &Rc<Ed>, select_name: Option<&str>) {
    let presets = ed.state.lib.level_presets().unwrap_or_default();
    let names: Vec<String> = std::iter::once("— presets —".to_string())
        .chain(presets.iter().map(|p| p.name.clone()))
        .collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    ed.loading.set(true);
    ed.preset_drop.set_model(Some(&StringList::new(&refs)));
    if let Some(n) = select_name {
        if let Some(pos) = presets.iter().position(|p| p.name == n) {
            ed.preset_drop.set_selected(pos as u32 + 1);
        }
    }
    ed.loading.set(false);
    *ed.presets.borrow_mut() = presets;
}

/// Push the in-memory edit values back into the levels widgets (spin buttons and
/// histogram strips) after auto-levels or a preset changed them.
fn reload_widgets(ed: &Rc<Ed>) {
    refresh_channel_widgets(ed);
}

/// Load and decode the original file into RGBA. Immich paths are not exported.
fn load_original(photo: &Photo) -> Option<image::RgbaImage> {
    if photo.path.starts_with("immich://") {
        return None;
    }
    let img = image::ImageReader::open(&photo.path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(img.to_rgba8())
}

/// Rotate a full-resolution image clockwise by 0/90/180/270 degrees.
fn rotate_full(img: image::RgbaImage, degrees: i32) -> image::RgbaImage {
    let d = ((degrees % 360) + 360) % 360;
    match d {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

fn stem(filename: &str) -> String {
    match filename.rsplit_once('.') {
        Some((s, _)) => s.to_string(),
        None => filename.to_string(),
    }
}

/// A tiny modal text prompt. Calls `on_ok` with the entered text.
fn prompt<F: Fn(&str) + 'static>(
    ed: &Rc<Ed>,
    title: &str,
    label: &str,
    initial: &str,
    on_ok: F,
) {
    let entry = gtk4::Entry::new();
    entry.set_text(initial);
    entry.set_hexpand(true);
    let lbl = Label::new(Some(label));
    lbl.set_xalign(0.0);
    let ok = Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = Button::with_label("Cancel");
    let btns = GtkBox::new(Orientation::Horizontal, 6);
    btns.set_halign(gtk4::Align::End);
    btns.append(&cancel);
    btns.append(&ok);
    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&lbl);
    root.append(&entry);
    root.append(&btns);
    let win = Window::builder()
        .title(title)
        .modal(true)
        .default_width(320)
        .child(&root)
        .build();
    if let Some(p) = ed.state.window() {
        win.set_transient_for(Some(&p));
    }
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    {
        let win = win.clone();
        let entry = entry.clone();
        ok.connect_clicked(move |_| {
            on_ok(&entry.text());
            win.close();
        });
    }
    win.set_visible(true);
}
