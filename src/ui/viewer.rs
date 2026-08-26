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

    photos: RefCell<Vec<Photo>>,
    index: RefCell<usize>,
    state: RefCell<Option<Rc<AppState>>>,
}

impl Viewer {
    /// Build the viewer. `bind_state` must be called once before use.
    pub fn new() -> Rc<Viewer> {
        let close_btn = Button::from_icon_name("go-previous-symbolic");
        let prev_btn = Button::from_icon_name("media-skip-backward-symbolic");
        let next_btn = Button::from_icon_name("media-skip-forward-symbolic");
        let rotate_btn = Button::from_icon_name("object-rotate-right-symbolic");

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
            photos: RefCell::new(Vec::new()),
            index: RefCell::new(0),
            state: RefCell::new(None),
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

    fn rotate(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let idx = *self.index.borrow();
        let (id, hash, new_orient) = {
            let mut photos = self.photos.borrow_mut();
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

        // Read the file bytes off-thread (I/O), then decode + rotate on the UI
        // thread (Pixbuf is not Send).
        let (tx, rx) = glib::MainContext::channel::<Option<Vec<u8>>>(glib::Priority::DEFAULT);
        let path = photo.path.clone();
        std::thread::spawn(move || {
            let bytes = std::fs::read(&path).ok();
            let _ = tx.send(bytes);
        });
        let picture = self.picture.clone();
        let rot = photo.orientation;
        rx.attach(None, move |bytes| {
            match bytes.and_then(|b| decode_rotated(&b, rot)) {
                Some(pb) => picture.set_pixbuf(Some(&pb)),
                None => picture.set_paintable(gtk4::gdk::Paintable::NONE),
            }
            glib::ControlFlow::Break
        });
    }
}

/// Decode image bytes and apply the given clockwise rotation for display.
fn decode_rotated(bytes: &[u8], degrees: i32) -> Option<Pixbuf> {
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;
    let degrees = ((degrees % 360) + 360) % 360;
    match degrees {
        90 => pb.rotate_simple(PixbufRotation::Clockwise),
        180 => pb.rotate_simple(PixbufRotation::Upsidedown),
        270 => pb.rotate_simple(PixbufRotation::Counterclockwise),
        _ => Some(pb),
    }
}
