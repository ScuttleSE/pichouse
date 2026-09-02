//! The "New Files" view: recently added photos grouped by their folder.
//!
//! Shown in the center stack when the user selects "New Files" in the Library
//! tab. Each folder that has new files is a header; its new thumbnails are shown
//! in a flow below it. "New" means added to the library after the owning root's
//! first scan finished, within the configured New Files window (default 14
//! days; see `prefs::Prefs::new_max_age_days` and
//! `Library::new_photos_grouped`); older additions fall off automatically.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use gtk4::gdk;
use gtk4::gdk_pixbuf::PixbufLoader;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, FlowBox, Image, Label, Orientation, PolicyType, ScrolledWindow,
    SelectionMode,
};

use crate::model::{Folder, Photo};
use crate::thumb::Generator;

/// How many thumbnails are generated concurrently for this view.
const WORKERS: usize = 3;

/// A thumbnail job (UI -> worker).
struct Job {
    hash: String,
    path: String,
    orientation: i32,
    generation: u64,
    index: usize,
}

/// A finished thumbnail (worker -> UI).
struct Done {
    blob: Vec<u8>,
    generation: u64,
    index: usize,
}

/// The New Files grouped view.
pub struct NewFilesView {
    root: ScrolledWindow,
    content: GtkBox,
    thumb_size: std::cell::Cell<i32>,
    generation: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
    /// Images awaiting a texture for the current generation, indexed by job idx.
    pending: Rc<RefCell<Vec<Image>>>,
    /// Flat list of the currently shown photos (for activation → viewer).
    photos: RefCell<Vec<Photo>>,
    on_activate: RefCell<Option<Box<dyn Fn(Vec<Photo>, usize)>>>,
}

impl NewFilesView {
    /// Build the view and start its worker pool.
    pub fn new(gen: Arc<Generator>, thumb_size: i32) -> Rc<NewFilesView> {
        let content = GtkBox::new(Orientation::Vertical, 8);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let root = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        let generation = Arc::new(AtomicU64::new(0));
        let pending: Rc<RefCell<Vec<Image>>> = Rc::new(RefCell::new(Vec::new()));

        let (done_tx, done_rx) = glib::MainContext::channel::<Done>(glib::Priority::DEFAULT);
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let job_rx = Arc::new(std::sync::Mutex::new(job_rx));

        for _ in 0..WORKERS {
            let job_rx = job_rx.clone();
            let done_tx = done_tx.clone();
            let gen = gen.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let rx = job_rx.lock().unwrap();
                    match rx.recv() {
                        Ok(j) => j,
                        Err(_) => return,
                    }
                };
                if let Ok(blob) =
                    gen.get(&job.hash, std::path::Path::new(&job.path), job.orientation)
                {
                    if !blob.is_empty() {
                        let _ = done_tx.send(Done {
                            blob,
                            generation: job.generation,
                            index: job.index,
                        });
                    }
                }
            });
        }

        let gen_for_apply = generation.clone();
        let pending_for_apply = pending.clone();
        done_rx.attach(None, move |done: Done| {
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(image) = pending_for_apply.borrow().get(done.index) {
                    if let Some(texture) = decode_texture(&done.blob) {
                        image.set_paintable(Some(&texture));
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        Rc::new(NewFilesView {
            root,
            content,
            thumb_size: std::cell::Cell::new(thumb_size),
            generation,
            jobs: job_tx,
            pending,
            photos: RefCell::new(Vec::new()),
            on_activate: RefCell::new(None),
        })
    }

    /// The view's root widget.
    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }

    /// Register the activation callback (opens the viewer).
    pub fn set_on_activate<F: Fn(Vec<Photo>, usize) + 'static>(&self, f: F) {
        *self.on_activate.borrow_mut() = Some(Box::new(f));
    }

    /// Update the active thumbnail size for subsequent rebuilds.
    #[allow(dead_code)] // Kept API; the New Files view uses a fixed size today.
    pub fn set_thumb_size(&self, size: i32) {
        self.thumb_size.set(size);
    }

    /// Rebuild the view from grouped photos. An empty input shows a friendly
    /// "nothing new" message.
    pub fn show_groups(self: &Rc<Self>, groups: Vec<(Folder, Vec<Photo>)>) {
        // Bump the generation so stale worker results are ignored.
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending.borrow_mut().clear();

        // Clear existing content.
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }

        let size = self.thumb_size.get();
        let total: usize = groups.iter().map(|(_, ps)| ps.len()).sum();

        let header = Label::new(None);
        header.set_xalign(0.0);
        header.set_markup(&format!(
            "<b>New Files</b>  ({total} in {} folder{})",
            groups.len(),
            if groups.len() == 1 { "" } else { "s" }
        ));
        self.content.append(&header);

        if groups.is_empty() {
            let empty = Label::new(Some(
                "No new files. Files added to your library folders after the \
                 first scan appear here.",
            ));
            empty.add_css_class("dim-label");
            empty.set_xalign(0.0);
            empty.set_margin_top(8);
            self.content.append(&empty);
            self.photos.borrow_mut().clear();
            return;
        }

        // Build a flat photo list (for activation indices) as we lay out groups.
        let mut flat: Vec<Photo> = Vec::new();

        for (folder, photos) in &groups {
            // Folder header row.
            let folder_header = GtkBox::new(Orientation::Horizontal, 6);
            folder_header.set_margin_top(8);
            let icon = Image::from_icon_name("folder-symbolic");
            let title = Label::new(None);
            title.set_xalign(0.0);
            title.set_markup(&format!(
                "<b>{}</b>  ({})",
                super::util::escape_markup(&folder.name),
                photos.len()
            ));
            let path = Label::new(Some(&folder.path));
            path.add_css_class("dim-label");
            path.set_xalign(0.0);
            folder_header.append(&icon);
            folder_header.append(&title);
            self.content.append(&folder_header);
            self.content.append(&path);

            let flow = FlowBox::new();
            flow.set_selection_mode(SelectionMode::None);
            flow.set_homogeneous(true);
            flow.set_column_spacing(4);
            flow.set_row_spacing(4);
            flow.set_min_children_per_line(1);
            flow.set_max_children_per_line(20);

            for photo in photos {
                let index = flat.len();
                flat.push(photo.clone());

                let cell = GtkBox::new(Orientation::Vertical, 2);
                cell.set_size_request(size, size);

                let image = Image::new();
                image.set_pixel_size(size);
                image.set_valign(Align::Center);
                image.set_halign(Align::Center);
                cell.append(&image);

                // Track the image so its texture can be set when the job returns.
                {
                    let mut pending = self.pending.borrow_mut();
                    debug_assert_eq!(pending.len(), index);
                    pending.push(image.clone());
                }

                // Double-click opens the viewer at this photo.
                {
                    let this = self.clone();
                    let gesture = gtk4::GestureClick::new();
                    gesture.set_button(gdk::BUTTON_PRIMARY);
                    gesture.connect_pressed(move |g, n, _, _| {
                        if n == 2 {
                            g.set_state(gtk4::EventSequenceState::Claimed);
                            this.activate(index);
                        }
                    });
                    cell.add_controller(gesture);
                }

                flow.append(&cell);

                // Queue the thumbnail job.
                let _ = self.jobs.send(Job {
                    hash: photo.hash.clone(),
                    path: photo.path.clone(),
                    orientation: photo.orientation,
                    generation,
                    index,
                });
            }

            self.content.append(&flow);
        }

        *self.photos.borrow_mut() = flat;
    }

    fn activate(&self, index: usize) {
        let photos = self.photos.borrow().clone();
        if index < photos.len() {
            if let Some(cb) = self.on_activate.borrow().as_ref() {
                cb(photos, index);
            }
        }
    }
}

/// Decode a JPEG blob into a `gdk::Texture`.
fn decode_texture(blob: &[u8]) -> Option<gdk::Texture> {
    let loader = PixbufLoader::new();
    loader.write(blob).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}
