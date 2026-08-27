//! Full-image viewer that replaces the grid when a photo is opened.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk_pixbuf::{Pixbuf, PixbufRotation};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Orientation, Picture, Separator};

use crate::model::Photo;

use super::shortcuts::Action;
use super::state::{show_error, AppState};

/// The full-image viewer.
pub struct Viewer {
    root: GtkBox,
    picture: Picture,
    header: Label,
    close_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    rotate_btn: Button,
    edit_btn: Button,

    photos: RefCell<Vec<Photo>>,
    index: RefCell<usize>,
    state: RefCell<Option<Rc<AppState>>>,
    /// When true, show the untouched original instead of the edited view.
    show_original: std::cell::Cell<bool>,
    /// Bumped on every `show()` so a late async image load for a previous photo
    /// is discarded instead of flashing on screen.
    generation: std::cell::Cell<u64>,
}

impl Viewer {
    /// Build the viewer. `bind_state` must be called once before use.
    pub fn new() -> Rc<Viewer> {
        let close_btn = Button::from_icon_name("go-previous-symbolic");
        let prev_btn = Button::from_icon_name("media-skip-backward-symbolic");
        let next_btn = Button::from_icon_name("media-skip-forward-symbolic");
        let rotate_btn = Button::from_icon_name("object-rotate-right-symbolic");
        let edit_btn = Button::from_icon_name("document-edit-symbolic");

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
        bar.append(&header);

        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_vexpand(true);
        picture.set_hexpand(true);

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&picture);

        Rc::new(Viewer {
            root,
            picture,
            header,
            close_btn,
            prev_btn,
            next_btn,
            rotate_btn,
            edit_btn,
            photos: RefCell::new(Vec::new()),
            index: RefCell::new(0),
            state: RefCell::new(None),
            show_original: std::cell::Cell::new(false),
            generation: std::cell::Cell::new(0),
        })
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
    }

    /// Handle a key press while the viewer is active. Returns true if consumed.
    pub fn handle_key(self: &Rc<Self>, keyval: u32) -> bool {
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
    pub fn set_show_original(self: &Rc<Self>, original: bool) {
        self.show_original.set(original);
        self.show();
    }

    /// Re-render the current photo (for example after edits change).
    pub fn reload_current(self: &Rc<Self>) {
        self.show();
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
        let edit = if self.show_original.get() {
            crate::model::PhotoEdit::default()
        } else {
            self.state
                .borrow()
                .as_ref()
                .and_then(|s| s.lib.photo_edit(photo.id).ok())
                .unwrap_or_default()
        };
        rx.attach(None, move |bytes| {
            // Drop stale results from an earlier show().
            if this.generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            match bytes.and_then(|b| decode_edited(&b, rot, &edit)) {
                Some(pb) => picture.set_pixbuf(Some(&pb)),
                None => picture.set_paintable(gtk4::gdk::Paintable::NONE),
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
