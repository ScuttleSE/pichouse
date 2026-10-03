//! Alt-hover preview for the Faces and Characters tiles.
//!
//! When the pointer is on a tile and the user holds Alt, the tile image shows a
//! random full photo thumbnail from the group. A new photo shows every second.
//! When the user releases Alt or the pointer leaves the tile, the face crop
//! comes back.

use crate::model::Photo;
use crate::thumb::Generator;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::Image;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The poll interval of the Alt state while the pointer is on a tile.
const POLL_MS: u64 = 100;
/// The interval between two random photos.
const STEP: Duration = Duration::from_secs(1);

struct TileState {
    photos: RefCell<Option<Vec<Photo>>>,
    saved: RefCell<Option<gdk::Paintable>>,
    saved_icon: RefCell<Option<glib::GString>>,
    active: Cell<bool>,
    last: Cell<Option<Instant>>,
    timer: RefCell<Option<glib::SourceId>>,
    /// Bumped on each new request, so a late result does not show.
    gen: Cell<u64>,
    rng: Cell<u64>,
}

fn alt_held(w: &impl IsA<gtk4::Widget>) -> bool {
    w.display()
        .default_seat()
        .and_then(|s| s.keyboard())
        .map(|k| k.modifier_state().contains(gdk::ModifierType::ALT_MASK))
        .unwrap_or(false)
}

fn next_rand(st: &TileState) -> u64 {
    let mut x = st.rng.get();
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    st.rng.set(x);
    x
}

fn restore(image: &Image, st: &TileState) {
    st.active.set(false);
    st.last.set(None);
    st.gen.set(st.gen.get().wrapping_add(1));
    if let Some(p) = st.saved.borrow_mut().take() {
        image.set_paintable(Some(&p));
    } else if let Some(icon) = st.saved_icon.borrow_mut().take() {
        image.set_icon_name(Some(&icon));
    }
}

fn show_random(image: &Image, st: &Rc<TileState>, gen: &Arc<Generator>) {
    let pick = {
        let photos = st.photos.borrow();
        let Some(list) = photos.as_ref() else { return };
        let live: Vec<&Photo> = list.iter().filter(|p| !p.missing).collect();
        if live.is_empty() {
            return;
        }
        live[(next_rand(st) % live.len() as u64) as usize].clone()
    };
    let token = st.gen.get().wrapping_add(1);
    st.gen.set(token);
    let gen = gen.clone();
    let image = image.downgrade();
    let st = st.clone();
    glib::MainContext::default().spawn_local(async move {
        let bytes = gtk4::gio::spawn_blocking(move || {
            gen.get(&pick.hash, std::path::Path::new(&pick.path), pick.orientation)
                .ok()
        })
        .await
        .ok()
        .flatten();
        if st.gen.get() != token || !st.active.get() {
            return;
        }
        let (Some(image), Some(bytes)) = (image.upgrade(), bytes) else { return };
        if let Some(tex) = super::util::texture_from_bytes(&bytes) {
            image.set_paintable(Some(&tex));
        }
    });
}

/// Wire the Alt preview on one tile. `hover` is the widget that tracks the
/// pointer. `image` is the tile image. `load` returns the group photos. The
/// code calls `load` one time, the first time the user holds Alt.
pub fn attach(
    hover: &impl IsA<gtk4::Widget>,
    image: &Image,
    gen: Arc<Generator>,
    load: impl Fn() -> Vec<Photo> + 'static,
) {
    let seed = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        ^ (image.as_ptr() as u64))
        | 1;
    let st = Rc::new(TileState {
        photos: RefCell::new(None),
        saved: RefCell::new(None),
        saved_icon: RefCell::new(None),
        active: Cell::new(false),
        last: Cell::new(None),
        timer: RefCell::new(None),
        gen: Cell::new(0),
        rng: Cell::new(seed),
    });
    let load = Rc::new(load);

    let motion = gtk4::EventControllerMotion::new();
    {
        let st = st.clone();
        let image = image.downgrade();
        motion.connect_enter(move |ctrl, _, _| {
            if let Some(id) = st.timer.borrow_mut().take() {
                id.remove();
            }
            let widget = ctrl.widget();
            let st2 = st.clone();
            let image = image.clone();
            let gen = gen.clone();
            let load = load.clone();
            let tick = move || {
                let Some(image) = image.upgrade() else {
                    st2.timer.borrow_mut().take();
                    return glib::ControlFlow::Break;
                };
                if alt_held(&widget) {
                    if !st2.active.get() {
                        st2.active.set(true);
                        *st2.saved.borrow_mut() = image.paintable();
                        *st2.saved_icon.borrow_mut() = image.icon_name();
                        if st2.photos.borrow().is_none() {
                            *st2.photos.borrow_mut() = Some(load());
                        }
                    }
                    let due = st2.last.get().map(|t| t.elapsed() >= STEP).unwrap_or(true);
                    if due {
                        st2.last.set(Some(Instant::now()));
                        show_random(&image, &st2, &gen);
                    }
                } else if st2.active.get() {
                    restore(&image, &st2);
                }
                glib::ControlFlow::Continue
            };
            let id = glib::timeout_add_local(Duration::from_millis(POLL_MS), tick);
            *st.timer.borrow_mut() = Some(id);
        });
    }
    {
        let st = st.clone();
        let image = image.downgrade();
        motion.connect_leave(move |_| {
            if let Some(id) = st.timer.borrow_mut().take() {
                id.remove();
            }
            if st.active.get() {
                if let Some(image) = image.upgrade() {
                    restore(&image, &st);
                }
            }
        });
    }
    hover.add_controller(motion);
}
