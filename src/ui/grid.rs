//! Center thumbnail grid with an asynchronous thumbnail worker pool.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use gtk4::gdk;
use gtk4::gdk_pixbuf::PixbufLoader;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, GridView, Image, Label, ListItem, Overlay, PolicyType, ScrolledWindow,
    SignalListItemFactory, SingleSelection,
};

use crate::db::Library;
use crate::model::Photo;
use crate::thumb::Generator;

use super::photo_object::PhotoObject;

/// How many thumbnails are generated concurrently.
const THUMB_WORKERS: usize = 4;

/// A thumbnail job sent from the UI thread to a worker.
struct Job {
    key: String,
    hash: String,
    path: String,
    orientation: i32,
    generation: u64,
}

/// A finished thumbnail sent from a worker back to the UI thread.
struct Done {
    key: String,
    blob: Vec<u8>,
    generation: u64,
}

/// The center thumbnail grid.
pub struct Grid {
    root: gtk4::Box,
    header: Label,
    store: gio::ListStore,
    #[allow(dead_code)]
    selection: SingleSelection,
    thumb_size: i32,
    generation: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
    /// Maps a cell key to its `PhotoObject` for the current generation, so a
    /// worker result can find the object to update on the UI thread.
    pending: Rc<RefCell<HashMap<String, PhotoObject>>>,
}

impl Grid {
    /// Build the grid, starting the worker pool. `lib` supplies photo data;
    /// `gen` renders thumbnails. Both are shared with the workers.
    pub fn new(lib: Arc<Library>, gen: Arc<Generator>, thumb_size: i32) -> Grid {
        let _ = lib; // reserved for later (raw-folder hash lookups)
        let header = Label::new(None);
        header.set_xalign(0.0);
        header.set_margin_start(8);
        header.set_margin_top(6);
        header.set_margin_bottom(6);

        let store = gio::ListStore::new::<PhotoObject>();
        let selection = SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let generation = Arc::new(AtomicU64::new(0));
        let pending: Rc<RefCell<HashMap<String, PhotoObject>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Result channel: workers -> UI thread.
        let (done_tx, done_rx) = glib::MainContext::channel::<Done>(glib::Priority::DEFAULT);
        // Job channel: UI thread -> workers.
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let job_rx = Arc::new(std::sync::Mutex::new(job_rx));

        for _ in 0..THUMB_WORKERS {
            let job_rx = job_rx.clone();
            let done_tx = done_tx.clone();
            let gen = gen.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let rx = job_rx.lock().unwrap();
                    match rx.recv() {
                        Ok(j) => j,
                        Err(_) => return, // senders dropped; exit
                    }
                };
                match gen.get(&job.hash, std::path::Path::new(&job.path), job.orientation) {
                    Ok(blob) if !blob.is_empty() => {
                        let _ = done_tx.send(Done {
                            key: job.key,
                            blob,
                            generation: job.generation,
                        });
                    }
                    _ => {}
                }
            });
        }

        let factory = build_factory(thumb_size);
        let grid_view = GridView::new(Some(selection.clone()), Some(factory));
        grid_view.set_min_columns(1);
        grid_view.set_max_columns(20);

        let scroller = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vexpand(true)
            .child(&grid_view)
            .build();

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&scroller);

        // Apply finished thumbnails on the UI thread by setting the texture on
        // the matching PhotoObject; the bound Image updates automatically.
        let gen_for_apply = generation.clone();
        let pending_for_apply = pending.clone();
        done_rx.attach(None, move |done: Done| {
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(obj) = pending_for_apply.borrow_mut().remove(&done.key) {
                    if let Some(texture) = decode_texture(&done.blob) {
                        obj.set_texture(Some(texture));
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        Grid {
            root,
            header,
            store,
            selection,
            thumb_size,
            generation,
            jobs: job_tx,
            pending,
        }
    }

    /// The grid's root widget.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// The active thumbnail size in pixels.
    pub fn thumb_size(&self) -> i32 {
        self.thumb_size
    }

    /// Replace the shown photos. Bumps the generation so stale thumbnail results
    /// are discarded, then enqueues a job per photo.
    pub fn show_photos(&self, title: &str, photos: &[Photo]) {
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.store.remove_all();
        self.pending.borrow_mut().clear();

        let mut objs = Vec::with_capacity(photos.len());
        for p in photos {
            let obj = PhotoObject::from_photo(p);
            self.store.append(&obj);
            objs.push(obj);
        }
        self.header.set_text(&format!("{}  ({})", title, photos.len()));

        for (p, obj) in photos.iter().zip(objs) {
            let key = cell_key(p, self.thumb_size);
            self.pending.borrow_mut().insert(key.clone(), obj);
            let _ = self.jobs.send(Job {
                key,
                hash: p.hash.clone(),
                path: p.path.clone(),
                orientation: p.orientation,
                generation: gen,
            });
        }
    }
}

/// The cache/recycle key for a photo at a given size. Encodes hash (or path),
/// size, and orientation so a size or rotation change never matches a stale
/// cell.
fn cell_key(p: &Photo, size: i32) -> String {
    let base = if p.hash.is_empty() { &p.path } else { &p.hash };
    format!("{base}|{size}|{}", p.orientation)
}

/// Build the recycled cell factory: an `Overlay` of a fallback `Label` under an
/// `Image`. The image observes the bound `PhotoObject.texture` property; the
/// label shows the filename until a texture arrives.
fn build_factory(thumb_size: i32) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let overlay = Overlay::new();
        // Reserve a square cell so thumbnails are shown at full size and the
        // grid lays out evenly before textures arrive.
        overlay.set_size_request(thumb_size, thumb_size);

        let label = Label::new(None);
        label.set_wrap(true);
        label.set_justify(gtk4::Justification::Center);
        label.set_halign(Align::Center);
        label.set_valign(Align::Center);
        label.add_css_class("dim-label");
        overlay.set_child(Some(&label));

        let image = Image::new();
        image.set_pixel_size(thumb_size);
        overlay.add_overlay(&image);

        item.set_child(Some(&overlay));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let Some(photo) = item.item().and_downcast::<PhotoObject>() else {
            return;
        };
        let Some(overlay) = item.child().and_downcast::<Overlay>() else {
            return;
        };
        let (image, label) = overlay_parts(&overlay);
        label.set_text(&photo.filename());

        // Show the current texture (if already decoded) and update the label.
        apply_texture(&image, &label, photo.texture());

        // Observe future texture changes for this bound object.
        let image_weak = image.downgrade();
        let label_weak = label.downgrade();
        let handler = photo.connect_notify_local(
            Some("texture"),
            move |obj: &PhotoObject, _pspec| {
                if let (Some(image), Some(label)) =
                    (image_weak.upgrade(), label_weak.upgrade())
                {
                    apply_texture(&image, &label, obj.texture());
                }
            },
        );
        // Store the handler id so unbind can disconnect it.
        unsafe {
            item.set_data("texture-handler", handler);
        }
    });
    factory.connect_unbind(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        if let Some(photo) = item.item().and_downcast::<PhotoObject>() {
            unsafe {
                if let Some(handler) =
                    item.steal_data::<glib::SignalHandlerId>("texture-handler")
                {
                    photo.disconnect(handler);
                }
            }
        }
    });
    factory
}

/// Set the image from a texture (or clear it and show the label if `None`).
fn apply_texture(image: &Image, label: &Label, texture: Option<gdk::Texture>) {
    match texture {
        Some(t) => {
            image.set_paintable(Some(&t));
            label.set_visible(false);
        }
        None => {
            image.set_paintable(gdk::Paintable::NONE);
            label.set_visible(true);
        }
    }
}

/// Extract the `Image` (overlay child) and fallback `Label` from a cell.
fn overlay_parts(overlay: &Overlay) -> (Image, Label) {
    let label = overlay.first_child().and_downcast::<Label>().unwrap();
    let image = overlay
        .first_child()
        .and_then(|c| c.next_sibling())
        .and_downcast::<Image>()
        .unwrap();
    (image, label)
}

/// Decode a JPEG blob into a `gdk::Texture`.
fn decode_texture(blob: &[u8]) -> Option<gdk::Texture> {
    let loader = PixbufLoader::new();
    loader.write(blob).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}
