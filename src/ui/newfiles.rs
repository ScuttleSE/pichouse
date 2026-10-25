//! The "New Files" view: recently added photos grouped by their folder.
//!
//! Shown in the center stack when the user selects "New Files" in the Library
//! tab. Each folder that has new files is a header; its new thumbnails are shown
//! in a flow below it. "New" means added to the library after the owning root's
//! first scan finished, within the configured New Files window (default 14
//! days; see `prefs::Prefs::new_max_age_days` and
//! `Library::new_photos_grouped`); older additions fall off automatically.
//!
//! The window can hold tens of thousands of photos, so the view is lazy:
//!
//! - Widgets are appended in small chunks on the idle loop
//!   (`BUILD_CELLS_PER_TICK` cells per tick).
//! - Thumbnail jobs are queued only for folder slices near the viewport
//!   (`queue_visible`, driven by the scroll adjustment). Huge folders are
//!   recorded as slices of `RANGE_SLICE` photos, so one giant folder cannot
//!   fire one giant job burst.
//! - Thumbnails that scroll far above the viewport are evicted, so a full
//!   scroll-through cannot accumulate every texture in memory.
//! - Workers decode each JPEG blob to raw RGBA pixels; the main thread only
//!   wraps the pixels in a `gdk::MemoryTexture`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// How many photos one `FolderRange` slice covers. Slices keep the job queue
/// windowed when a single folder holds thousands of new files.
const RANGE_SLICE: usize = 500;

/// Estimated height (px) of one folder's header rows above its thumbnail flow.
const FOLDER_HEADER_EST: f64 = 56.0;

/// Spacing between flow cells (matches the FlowBox construction below).
const CELL_SPACING: i32 = 4;

/// How many viewport pages below the scroll position get thumbnails.
const PAGES_BELOW: f64 = 2.0;

/// A folder evicts its textures once it sits this many pages above the
/// viewport. Workers re-decode cheaply when it scrolls back.
const PAGES_ABOVE_EVICT: f64 = 3.0;

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

/// A job queue. The workers take the newest job first. `queue_visible`
/// removes all queued jobs before it adds the jobs for the new window, so the
/// photos on screen never wait behind photos that the user scrolled past.
struct JobQueue {
    q: std::sync::Mutex<Vec<Job>>,
    cv: std::sync::Condvar,
}

impl JobQueue {
    fn pop(&self) -> Job {
        let mut q = self.q.lock().unwrap();
        loop {
            if let Some(j) = q.pop() {
                return j;
            }
            q = self.cv.wait(q).unwrap();
        }
    }
}

/// The load state of one photo cell.
const ST_NONE: u8 = 0;
const ST_QUEUED: u8 = 1;
const ST_LOADED: u8 = 2;

/// One slice of a built folder: the flat photo index range its cells occupy,
/// whether its thumbnail jobs were already queued, and whether the slice is
/// the folder's first (its height estimate includes the folder header).
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct FolderRange {
    start: usize,
    end: usize,
    queued: bool,
    first: bool,
}

/// State of an in-progress chunked rebuild.
struct BuildState {
    /// Folder metadata in display order (the photos live in the view's flat
    /// list, moved out of the grouped query result).
    folders: Vec<Folder>,
    /// Photo count per folder, same order as `folders`.
    counts: Vec<usize>,
    /// Index of the folder currently being appended.
    folder: usize,
    /// Offset of the next cell within the current folder.
    cell: usize,
    /// Flat index of the current folder's first photo.
    base: usize,
    /// Flat offset (relative to the folder start) already covered by recorded
    /// slices. The folder's first chunk records its slice early so the first
    /// screen fills while the rest of the folder is still being built.
    recorded: usize,
    /// The current folder's flow box, created with its first chunk.
    flow: Option<FlowBox>,
}

/// The New Files grouped view.
pub struct NewFilesView {
    root: ScrolledWindow,
    content: GtkBox,
    thumb_size: std::cell::Cell<i32>,
    generation: Arc<AtomicU64>,
    jobs: Arc<JobQueue>,
    /// The load state of each photo (`ST_*`), by flat photo index.
    state: Rc<RefCell<Vec<u8>>>,
    /// Images awaiting a texture for the current generation, indexed by flat
    /// photo index.
    pending: Rc<RefCell<Vec<Image>>>,
    /// Flat list of the currently shown photos. Filled once at `show_groups`
    /// time (moved, not cloned) so activation and lazy job dispatch resolve
    /// any index while the widgets are still being built.
    photos: RefCell<Rc<Vec<Photo>>>,
    /// Flat index range of each built folder slice, in display order.
    ranges: RefCell<Vec<FolderRange>>,
    on_activate: RefCell<Option<Box<dyn Fn(Vec<Photo>, usize)>>>,
    /// True while a `queue_visible` run waits on the idle loop.
    visible_scheduled: std::cell::Cell<bool>,
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
        super::smoothscroll::attach(&root);

        let generation = Arc::new(AtomicU64::new(0));
        let pending: Rc<RefCell<Vec<Image>>> = Rc::new(RefCell::new(Vec::new()));

        let (done_tx, done_rx) = glib::MainContext::channel::<Done>(glib::Priority::DEFAULT);
        let job_tx = Arc::new(JobQueue {
            q: std::sync::Mutex::new(Vec::new()),
            cv: std::sync::Condvar::new(),
        });
        let state: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let cur_gen = generation.clone();

        for _ in 0..WORKERS {
            let job_rx = job_tx.clone();
            let done_tx = done_tx.clone();
            let gen = gen.clone();
            let cur_gen = cur_gen.clone();
            std::thread::spawn(move || loop {
                let job = job_rx.pop();
                if job.generation != cur_gen.load(Ordering::Relaxed) {
                    continue;
                }
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
        let state_for_apply = state.clone();
        done_rx.attach(None, move |done: Done| {
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(st) = state_for_apply.borrow_mut().get_mut(done.index) {
                    // The cell was evicted while the job ran: do not show it.
                    if *st != ST_QUEUED {
                        return glib::ControlFlow::Continue;
                    }
                    *st = ST_LOADED;
                }
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
            state,
            pending,
            photos: RefCell::new(Rc::new(Vec::new())),
            ranges: RefCell::new(Vec::new()),
            on_activate: RefCell::new(None),
            visible_scheduled: std::cell::Cell::new(false),
        });

        // Queue thumbnail work as the user scrolls (value changes) and when the
        // geometry first arrives or changes (page size / upper change).
        let weak = Rc::downgrade(&view);
        view.root.vadjustment().connect_value_notify(move |_| {
            if let Some(v) = weak.upgrade() {
                v.schedule_visible();
            }
        });
        let weak = Rc::downgrade(&view);
        view.root.vadjustment().connect_changed(move |_| {
            if let Some(v) = weak.upgrade() {
                v.schedule_visible();
            }
        });

        view
    }

    /// Run `queue_visible` one time after the current burst of scroll events.
    fn schedule_visible(self: &Rc<Self>) {
        if self.visible_scheduled.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
            if let Some(v) = weak.upgrade() {
                v.visible_scheduled.set(false);
                v.queue_visible();
            }
        });
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
        self.jobs.q.lock().unwrap().clear();

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
            *self.photos.borrow_mut() = Rc::new(Vec::new());
            return;
        }

        // Move the photos into the flat list (no per-photo clones) and keep
        // only folder metadata for the incremental builder.
        let mut folders = Vec::with_capacity(groups.len());
        let mut counts = Vec::with_capacity(groups.len());
        let mut flat = Vec::with_capacity(total);
        for (folder, photos) in groups {
            counts.push(photos.len());
            folders.push(folder);
            flat.extend(photos);
        }
        *self.state.borrow_mut() = vec![ST_NONE; flat.len()];
        *self.photos.borrow_mut() = Rc::new(flat);

        let state = RefCell::new(BuildState {
            folders,
            counts,
            folder: 0,
            cell: 0,
            base: 0,
            recorded: 0,
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
        let Some(folder) = st.folders.get(st.folder) else {
            // Rebuild complete: fill the first screen's thumbnails.
            self.queue_visible();
            return glib::ControlFlow::Break;
        };
        let folder_len = st.counts[st.folder];
        let photos = self.photos.borrow().clone();

        let size = self.thumb_size.get();

        // First chunk of this folder: header widgets, the flow box, and the
        // one double-click gesture that serves every cell in the folder.
        if st.cell == 0 {
            let folder_header = GtkBox::new(Orientation::Horizontal, 6);
            folder_header.set_margin_top(8);
            let icon = Image::from_icon_name("folder-symbolic");
            let title = Label::new(None);
            title.set_xalign(0.0);
            title.set_markup(&format!(
                "<b>{}</b>  ({})",
                super::util::escape_markup(&folder.name),
                folder_len
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

            // Double-click opens the viewer at the clicked cell. Cell order in
            // the flow matches flat photo order, so the child index plus the
            // folder's flat base gives the photo index.
            {
                let this = self.clone();
                let base = st.base;
                let flow_for_gesture = flow.clone();
                let gesture = gtk4::GestureClick::new();
                gesture.set_button(gdk::BUTTON_PRIMARY);
                gesture.connect_pressed(move |g, n, x, y| {
                    if n == 2 {
                        g.set_state(gtk4::EventSequenceState::Claimed);
                        if let Some(child) = flow_for_gesture.child_at_pos(x as i32, y as i32) {
                            this.activate(base + child.index() as usize);
                        }
                    }
                });
                flow.add_controller(gesture);
            }

            st.flow = Some(flow);
        }
        let flow = st.flow.clone().expect("flow exists after folder start");

        let start = st.cell;
        let end = (start + BUILD_CELLS_PER_TICK).min(folder_len);

        for i in start..end {
            let index = st.base + i;
            let photo = &photos[index];

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

            flow.append(&cell);
        }
        st.cell = end;

        // Record the first slice as soon as its cells exist, so the first
        // screen can fill while the rest of the folder is still being built.
        if st.recorded == 0 && end > 0 {
            self.ranges.borrow_mut().push(FolderRange {
                start: st.base,
                end: st.base + end,
                queued: false,
                first: true,
            });
            st.recorded = end;
            // Geometry may already be live (this is not the first folder).
            self.queue_visible();
        }

        if st.cell == folder_len {
            // Folder complete: add the flow, record the remaining slices,
            // advance.
            self.content.append(&flow);
            {
                let mut ranges = self.ranges.borrow_mut();
                let mut s = st.recorded;
                while s < folder_len {
                    let e = (s + RANGE_SLICE).min(folder_len);
                    ranges.push(FolderRange {
                        start: st.base + s,
                        end: st.base + e,
                        queued: false,
                        first: false,
                    });
                    s = e;
                }
            }
            st.base += folder_len;
            st.folder += 1;
            st.cell = 0;
            st.recorded = 0;
            st.flow = None;
        }
        glib::ControlFlow::Continue
    }

    /// Load the thumbnails near the viewport and drop the ones far from it.
    ///
    /// The positions come from the real widget layout (`compute_bounds` of
    /// the first and last cell of each slice), not from an estimate. An
    /// estimate drifts in a list of 100,000+ photos. It then loaded photos off
    /// screen and dropped photos on screen.
    ///
    /// Each call removes all queued jobs first. Then it queues the photos in
    /// the window that have no thumbnail, with the top row last, because the
    /// workers take the newest job first.
    fn queue_visible(&self) {
        let gen = self.generation.load(Ordering::Relaxed);
        let vadj = self.root.vadjustment();
        let value = vadj.value();
        let page = vadj.page_size();
        if page <= 0.0 {
            // Not allocated yet; the `changed` handler runs once it is.
            return;
        }
        let win_top = value - page * 0.5;
        let win_bottom = value + PAGES_BELOW * page;
        let evict_top = value - PAGES_ABOVE_EVICT * page;
        let evict_bottom = value + (PAGES_BELOW + PAGES_ABOVE_EVICT) * page;

        let ranges = self.ranges.borrow();
        let photos = self.photos.borrow();
        let pending = self.pending.borrow();
        let mut state = self.state.borrow_mut();

        // Forget the queued jobs. Their cells go back to "none".
        for j in self.jobs.q.lock().unwrap().drain(..) {
            if let Some(st) = state.get_mut(j.index) {
                if *st == ST_QUEUED {
                    *st = ST_NONE;
                }
            }
        }

        let bounds_y = |img: &Image| -> Option<(f64, f64)> {
            let b = img.compute_bounds(&self.content)?;
            Some((b.y() as f64, (b.y() + b.height()) as f64))
        };

        let mut want: Vec<usize> = Vec::new();
        for range in ranges.iter() {
            if range.end <= range.start || range.end > pending.len() {
                continue;
            }
            // A slice whose flow is not in the tree yet has no bounds.
            let (Some((top, _)), Some((_, bottom))) =
                (bounds_y(&pending[range.start]), bounds_y(&pending[range.end - 1]))
            else {
                continue;
            };
            if bottom < evict_top || top > evict_bottom {
                // Far from the viewport: drop the textures.
                for i in range.start..range.end {
                    if state[i] != ST_NONE {
                        pending[i].set_paintable(None::<&gdk::Paintable>);
                        state[i] = ST_NONE;
                    }
                }
                continue;
            }
            if bottom < win_top || top > win_bottom {
                continue;
            }
            // The slice meets the window. Check each cell that needs work.
            for i in range.start..range.end {
                if state[i] != ST_NONE {
                    continue;
                }
                match bounds_y(&pending[i]) {
                    Some((t, b)) if b >= win_top && t <= win_bottom => want.push(i),
                    _ => {}
                }
            }
        }

        let mut q = self.jobs.q.lock().unwrap();
        // Push the bottom cells first, so a worker takes the top cells first.
        for &i in want.iter().rev() {
            let photo = &photos[i];
            state[i] = ST_QUEUED;
            q.push(Job {
                hash: photo.hash.clone(),
                path: photo.path.clone(),
                orientation: photo.orientation,
                generation: gen,
                index: i,
            });
        }
        drop(q);
        self.jobs.cv.notify_all();
    }

    fn activate(&self, index: usize) {
        // Clone the Rc (cheap); the Vec itself is cloned once for the viewer,
        // which takes ownership.
        let photos = self.photos.borrow().clone();
        if index < photos.len() {
            if let Some(cb) = self.on_activate.borrow().as_ref() {
                cb((*photos).clone(), index);
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
