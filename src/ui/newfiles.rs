//! The "New Files" view: recently added photos grouped by their folder.
//!
//! Shown in the center stack when the user selects "New Files" in the Library
//! tab. Each folder that has new files is a header; its new thumbnails are shown
//! in a flow below it. "New" means added to the library after the owning root's
//! first scan finished, within the configured New Files window (default 14
//! days; see `prefs::Prefs::new_max_age_days` and
//! `Library::new_photos_grouped`); older additions fall off automatically.
//!
//! A large set of new files must not freeze the UI, so the view is lazy in
//! three ways:
//!
//! - Widgets are appended in small chunks on the idle loop (one or a few
//!   folders per pass, `BUILD_CELLS_PER_TICK` cells per tick).
//! - A folder's thumbnail jobs are queued only when the folder scrolls near
//!   the viewport (`queue_visible`, driven by the scroll adjustment).
//! - Workers decode each JPEG blob to raw RGBA pixels; the main thread only
//!   wraps the pixels in a `gdk::MemoryTexture`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use gtk4::gdk;
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

/// How many cells are appended to the UI per idle tick during a rebuild.
const BUILD_CELLS_PER_TICK: usize = 100;

/// Estimated height (px) of one folder's header rows above its thumbnail flow.
const FOLDER_HEADER_EST: f64 = 56.0;

/// Spacing between flow cells (matches the FlowBox construction below).
const CELL_SPACING: i32 = 4;

/// A thumbnail job (UI -> worker).
struct Job {
    hash: String,
    path: String,
    orientation: i32,
    generation: u64,
    index: usize,
}

/// A finished thumbnail (worker -> UI): decoded RGBA pixels.
struct Done {
    width: i32,
    height: i32,
    stride: usize,
    pixels: Vec<u8>,
    generation: u64,
    index: usize,
}

/// One built folder group: the flat photo index range its cells occupy and
/// whether its thumbnail jobs were already queued.
#[derive(Clone, Copy)]
struct FolderRange {
    start: usize,
    end: usize,
    queued: bool,
}

/// State of an in-progress chunked rebuild.
struct BuildState {
    groups: Vec<(Folder, Vec<Photo>)>,
    /// Index of the folder currently being appended.
    folder: usize,
    /// Offset of the next cell within the current folder's photos.
    cell: usize,
    /// Flat index of the current folder's first photo.
    base: usize,
    /// The current folder's flow box, created with its first chunk.
    flow: Option<FlowBox>,
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
    /// Flat list of the currently shown photos. Filled once at `show_groups`
    /// time so activation and lazy job dispatch resolve any index while the
    /// widgets are still being built.
    photos: RefCell<Vec<Photo>>,
    /// Flat index range of each built folder group, in display order.
    ranges: RefCell<Vec<FolderRange>>,
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
                    // Decode off the main thread so finished jobs cost the UI
                    // only a cheap texture upload.
                    if let Some(done) = decode_pixels(&blob, job.generation, job.index) {
                        let _ = done_tx.send(done);
                    }
                }
            });
        }

        let gen_for_apply = generation.clone();
        let pending_for_apply = pending.clone();
        done_rx.attach(None, move |done: Done| {
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(image) = pending_for_apply.borrow().get(done.index) {
                    let bytes = glib::Bytes::from_owned(done.pixels);
                    let texture = gdk::MemoryTexture::new(
                        done.width,
                        done.height,
                        gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        done.stride,
                    );
                    image.set_paintable(Some(&texture));
                }
            }
            glib::ControlFlow::Continue
        });

        let view = Rc::new(NewFilesView {
            root,
            content,
            thumb_size: std::cell::Cell::new(thumb_size),
            generation,
            jobs: job_tx,
            pending,
            photos: RefCell::new(Vec::new()),
            ranges: RefCell::new(Vec::new()),
            on_activate: RefCell::new(None),
        });

        // Queue thumbnail work as the user scrolls (value changes) and when the
        // geometry first arrives or changes (page size / upper change).
        let weak = Rc::downgrade(&view);
        view.root.vadjustment().connect_value_notify(move |_| {
            if let Some(v) = weak.upgrade() {
                v.queue_visible();
            }
        });
        let weak = Rc::downgrade(&view);
        view.root.vadjustment().connect_changed(move |_| {
            if let Some(v) = weak.upgrade() {
                v.queue_visible();
            }
        });

        view
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
    ///
    /// The rebuild is chunked: the header appears at once, then the idle loop
    /// appends folder groups in small batches so the main thread stays free.
    pub fn show_groups(self: &Rc<Self>, groups: Vec<(Folder, Vec<Photo>)>) {
        // Bump the generation so stale worker results and a stale rebuild are
        // ignored.
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending.borrow_mut().clear();
        self.ranges.borrow_mut().clear();

        // Clear existing content.
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }

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

        // Fill the flat photo list up front so activation and lazy job
        // dispatch resolve any index while widgets are still being built.
        let mut flat = Vec::with_capacity(total);
        for (_, ps) in &groups {
            flat.extend(ps.iter().cloned());
        }
        *self.photos.borrow_mut() = flat;

        let state = RefCell::new(BuildState {
            groups,
            folder: 0,
            cell: 0,
            base: 0,
            flow: None,
        });
        let weak = Rc::downgrade(self);
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let mut st = state.borrow_mut();
            this.clone().build_chunk(&mut st, generation)
        });
    }

    /// Append one chunk of the rebuild. Runs on the idle loop; returns Break
    /// when the rebuild finishes or a newer rebuild supersedes it.
    fn build_chunk(self: Rc<Self>, st: &mut BuildState, gen: u64) -> glib::ControlFlow {
        if self.generation.load(Ordering::Relaxed) != gen {
            return glib::ControlFlow::Break;
        }
        let Some((folder, photos)) = st.groups.get(st.folder) else {
            // Rebuild complete: fill the first screen's thumbnails.
            self.queue_visible();
            return glib::ControlFlow::Break;
        };

        let size = self.thumb_size.get();

        // First chunk of this folder: header widgets and the flow box.
        if st.cell == 0 {
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
            flow.set_column_spacing(CELL_SPACING as u32);
            flow.set_row_spacing(CELL_SPACING as u32);
            flow.set_min_children_per_line(1);
            flow.set_max_children_per_line(20);
            st.flow = Some(flow);
        }
        let flow = st.flow.clone().expect("flow exists after folder start");

        let folder_len = photos.len();
        let start = st.cell;
        let end = (start + BUILD_CELLS_PER_TICK).min(folder_len);

        for (i, photo) in photos[start..end].iter().enumerate() {
            let index = st.base + start + i;

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
        }
        st.cell = end;

        if st.cell == folder_len {
            // Folder complete: add the flow, record its range, advance.
            self.content.append(&flow);
            self.ranges.borrow_mut().push(FolderRange {
                start: st.base,
                end: st.base + folder_len,
                queued: false,
            });
            st.base += folder_len;
            st.folder += 1;
            st.cell = 0;
            st.flow = None;
        }
        glib::ControlFlow::Continue
    }

    /// Queue thumbnail jobs for every built folder group at or above the
    /// scroll horizon: the viewport plus two pages below it. Groups scrolled
    /// past are queued too, so cells never stay blank. Called on scroll, on
    /// geometry change, and when a rebuild finishes.
    fn queue_visible(&self) {
        let gen = self.generation.load(Ordering::Relaxed);
        let vadj = self.root.vadjustment();
        let value = vadj.value();
        let page = vadj.page_size();
        if page <= 0.0 {
            // Not allocated yet; the `changed` handler runs once it is.
            return;
        }
        let win_bottom = value + 2.0 * page;

        // Homogeneous cells: the per-cell pitch and the column count follow
        // from the allocated width and the thumbnail size.
        let pitch = (self.thumb_size.get() + CELL_SPACING) as f64;
        let width = self.content.allocated_width() as f64 - 16.0; // content margins
        let cols = if width > 0.0 { ((width / pitch).floor() as usize).max(1) } else { 6 };
        let cols = cols.min(20);

        let mut ranges = self.ranges.borrow_mut();
        let photos = self.photos.borrow();
        let mut top = 0.0f64;
        for range in ranges.iter_mut() {
            let count = (range.end - range.start) as f64;
            let rows = (count / cols as f64).ceil().max(1.0);
            let height = rows * pitch + FOLDER_HEADER_EST;
            let bottom = top + height;
            top = bottom;

            if range.queued {
                continue;
            }
            if top >= win_bottom {
                // Below the window; every later group is further down.
                break;
            }
            // At or above the horizon: queue it so cells never stay blank.
            range.queued = true;
            for i in range.start..range.end {
                let photo = &photos[i];
                let _ = self.jobs.send(Job {
                    hash: photo.hash.clone(),
                    path: photo.path.clone(),
                    orientation: photo.orientation,
                    generation: gen,
                    index: i,
                });
            }
        }
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

/// Decode a JPEG blob into RGBA pixels off the main thread. Returns `None`
/// when the blob does not decode.
fn decode_pixels(blob: &[u8], generation: u64, index: usize) -> Option<Done> {
    let img = image::load_from_memory(blob).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    Some(Done {
        width: w,
        height: h,
        stride: (w * 4) as usize,
        pixels: rgba.into_raw(),
        generation,
        index,
    })
}
