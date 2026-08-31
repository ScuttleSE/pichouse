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
    Align, Button, DrawingArea, GridView, Image, Label, ListItem, MultiSelection, Overlay,
    PolicyType, ScrolledWindow, SignalListItemFactory,
};

use crate::db::Library;
use crate::model::Photo;
use crate::thumb::Generator;

use super::photo_object::PhotoObject;

/// How many thumbnails are generated concurrently.
const THUMB_WORKERS: usize = 4;
/// How long (ms) each landed thumbnail keeps background scan/enrichment paused,
/// so the visible folder always wins the disk while it is still rendering.
const BROWSE_PAUSE_MS: u64 = 2000;
/// Extra spacing (px) added to the thumbnail size to estimate a grid cell's
/// footprint (inter-item gap plus the small caption row). Used only to compute
/// the visible index range, so an approximate value is fine.
const CELL_SPACING: i32 = 14;
/// How many extra rows above and below the visible window still count as
/// "visible" for on-demand work, so work starts a little before a row is
/// scrolled into view.
const VISIBLE_MARGIN_ROWS: usize = 2;
/// When the grid geometry is not yet known (width or page size is 0, e.g. right
/// after a folder opens before the first allocation), treat the first this-many
/// items as visible so the first screen still fills.
const VISIBLE_FALLBACK: usize = 60;
/// A thumbnail job sent from the UI thread to a worker.
struct Job {
    key: String,
    hash: String,
    path: String,
    orientation: i32,
    edit: crate::model::PhotoEdit,
    generation: u64,
}

/// A finished thumbnail sent from a worker back to the UI thread.
struct Done {
    key: String,
    blob: Vec<u8>,
    generation: u64,
}

/// An Immich thumbnail job sent from the UI thread to an Immich worker.
struct ImmichJob {
    key: String,
    server_id: i64,
    asset_id: String,
    generation: u64,
}

/// Parse an `immich://<server_id>/<asset_id>` path into its parts.
fn parse_immich_path(path: &str) -> Option<(i64, String)> {
    let rest = path.strip_prefix("immich://")?;
    let (sid, asset) = rest.split_once('/')?;
    let server_id: i64 = sid.parse().ok()?;
    if asset.is_empty() {
        return None;
    }
    Some((server_id, asset.to_string()))
}

/// Build an `immich://<server_id>/<asset_id>` path for a `Photo`.
pub fn immich_path(server_id: i64, asset_id: &str) -> String {
    format!("immich://{server_id}/{asset_id}")
}

/// The center thumbnail grid.
pub struct Grid {
    root: gtk4::Box,
    header: Label,
    back_btn: Button,
    back_handler: RefCell<Option<gtk4::glib::SignalHandlerId>>,
    store: gio::ListStore,
    grid_view: GridView,
    selection: MultiSelection,
    thumb_size: std::cell::Cell<i32>,
    generation: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
    /// Job channel to the Immich thumbnail worker pool.
    immich_jobs: mpsc::Sender<ImmichJob>,
    /// Maps a cell key to its `PhotoObject` for the current generation, so a
    /// worker result can find the object to update on the UI thread.
    pending: Rc<RefCell<HashMap<String, PhotoObject>>>,
    /// All photos currently loaded (unfiltered), plus the display title and the
    /// active filter, so filtering/rescale can rebuild the view.
    all_photos: RefCell<Vec<Photo>>,
    title: RefCell<String>,
    filter: RefCell<String>,
    lib: Arc<Library>,
    /// LRU cache of decoded textures, keyed by cell key, to skip re-decoding on
    /// scroll/re-entry.
    tex_cache: Rc<RefCell<super::thumbcache::TextureCache>>,
    /// The source the current photos came from, so the grid can re-query the
    /// database/disk (a true "refresh visible").
    source: RefCell<Source>,
    /// The active sort order for the photo grid.
    sort_order: std::cell::Cell<SortOrder>,
    /// Whether each cell shows a filename caption under its thumbnail.
    show_filenames: std::cell::Cell<bool>,
    /// Whether each cell overlays its detected face box(es) on the thumbnail.
    /// Local, transient state (not persisted), only meaningful while
    /// `is_face_source()` is true.
    show_faces: std::cell::Cell<bool>,
    /// Photo id -> every detected face's rect, for the face-box overlay.
    /// Populated once per `show_*` call (one bulk query), not per-cell.
    face_boxes: RefCell<HashMap<i64, Vec<FaceBoxRect>>>,
    /// Every face-box `DrawingArea` created so far (one per recycled grid
    /// cell, registered once in `connect_setup`), so toggling `show_faces` can
    /// queue a redraw on each directly. GTK4 caches a widget's own render node
    /// independently of its ancestors: `grid_view.queue_draw()` alone does not
    /// force an already-bound cell's `DrawingArea` to re-run its draw func —
    /// only queuing that specific widget does. Held weakly since these
    /// widgets are owned by the list view, not the grid.
    face_areas: RefCell<Vec<glib::WeakRef<DrawingArea>>>,
    /// The "show face boxes" toggle button, visible only for a face source.
    faces_btn: Button,
    /// The header dropdown that selects the sort order.
    sort_dropdown: gtk4::DropDown,
    /// Called with (photos, index) when a cell is activated (double-clicked).
    on_activate: RefCell<Option<Box<dyn Fn(Vec<Photo>, usize)>>>,
    /// Called with a photo when the selection changes (single click).
    on_select: RefCell<Option<Box<dyn Fn(Photo)>>>,
    /// Called with the ids of photos that scrolled into view and still lack a
    /// hash (not yet enriched), so the app can enrich just the visible cells.
    /// Ids accumulate in `enrich_buffer` and flush on an idle tick.
    on_enrich_request: RefCell<Option<Box<dyn Fn(Vec<i64>)>>>,
    /// Photo ids collected from `bind` that still need enrichment, pending a
    /// debounced flush. The `bool` guards against scheduling more than one
    /// flush at a time.
    enrich_buffer: RefCell<Vec<i64>>,
    enrich_flush_scheduled: std::cell::Cell<bool>,
    /// Guards against scheduling more than one scroll-settle refresh at a time.
    scroll_settle_scheduled: std::cell::Cell<bool>,
    /// Called with (x, y) in grid coordinates on a right-click, so the app can
    /// show a context menu over the current selection.
    on_context_menu: RefCell<Option<Box<dyn Fn(f64, f64)>>>,
    /// `true` while the grid shows duplicate groups. In this mode a click
    /// marks the clicked cell as the "delete" copy of its group instead of the
    /// normal selection behaviour.
    dup_mode: std::cell::Cell<bool>,
    /// The duplicate-results action bar (label + delete button), shown only in
    /// duplicate mode.
    dup_bar: gtk4::Box,
    dup_label: Label,
    dup_delete_btn: Button,
    /// Called when the user clicks "Delete marked" with the list of marked
    /// photos to hard delete.
    on_dup_delete: RefCell<Option<Box<dyn Fn(Vec<Photo>)>>>,
    /// The "Not duplicates" button in the duplicate action bar.
    dup_ban_btn: Button,
    /// Called when the user clicks "Not duplicates" with the (marked, keep)
    /// pairs to ban from matching again.
    on_dup_ban: RefCell<Option<Box<dyn Fn(Vec<(Photo, Photo)>)>>>,
    /// The scroller that holds the center view. In normal mode its child is the
    /// `GridView`. In duplicate mode it holds the framed group container.
    scroller: ScrolledWindow,
    /// The duplicate-results container: a vertical stack of framed group boxes.
    dup_container: gtk4::Box,
    /// The live duplicate state: for each group, the photos and their marked
    /// flags, plus the per-thumbnail `PhotoObject` and X widget so a click can
    /// update the mark. Empty outside duplicate mode.
    dup_state: RefCell<Vec<DupGroupUi>>,
}

/// One duplicate group's live UI state.
struct DupGroupUi {
    group: i64,
    cells: Vec<DupCellUi>,
}

/// One thumbnail cell inside a duplicate group.
struct DupCellUi {
    photo: Photo,
    marked: std::cell::Cell<bool>,
    x_widget: Label,
}

/// The order the grid sorts its photos in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Capture time, newest first (then filename). Matches the DB query order.
    DateDesc,
    /// Capture time, oldest first (then filename).
    DateAsc,
    /// Filename, A to Z, case-insensitive.
    NameAsc,
    /// Filename, Z to A, case-insensitive.
    NameDesc,
    /// File size, largest first.
    SizeDesc,
    /// File size, smallest first.
    SizeAsc,
}

impl SortOrder {
    /// Parse a stored setting value. Unknown values fall back to `DateDesc`.
    pub fn from_setting(v: &str) -> SortOrder {
        match v {
            "date_asc" => SortOrder::DateAsc,
            "name_asc" | "filename" => SortOrder::NameAsc,
            "name_desc" => SortOrder::NameDesc,
            "size_desc" => SortOrder::SizeDesc,
            "size_asc" => SortOrder::SizeAsc,
            _ => SortOrder::DateDesc,
        }
    }

    /// The setting value string for this order.
    fn as_setting(self) -> &'static str {
        match self {
            SortOrder::DateDesc => "date",
            SortOrder::DateAsc => "date_asc",
            SortOrder::NameAsc => "name_asc",
            SortOrder::NameDesc => "name_desc",
            SortOrder::SizeDesc => "size_desc",
            SortOrder::SizeAsc => "size_asc",
        }
    }

    /// The header dropdown row index for this order.
    fn dropdown_index(self) -> u32 {
        match self {
            SortOrder::DateDesc => 0,
            SortOrder::DateAsc => 1,
            SortOrder::NameAsc => 2,
            SortOrder::NameDesc => 3,
            SortOrder::SizeDesc => 4,
            SortOrder::SizeAsc => 5,
        }
    }

    /// The order for a header dropdown row index.
    fn from_dropdown_index(i: u32) -> SortOrder {
        match i {
            1 => SortOrder::DateAsc,
            2 => SortOrder::NameAsc,
            3 => SortOrder::NameDesc,
            4 => SortOrder::SizeDesc,
            5 => SortOrder::SizeAsc,
            _ => SortOrder::DateDesc,
        }
    }
}

/// Where the grid's current photos came from.
#[derive(Clone)]
enum Source {
    /// Nothing loaded yet, or an ad-hoc photo list.
    None,
    /// A scanned library folder (id, display name).
    Folder(i64, String),
    /// A raw filesystem directory path.
    RawDir(String),
    /// A virtual album (id, display name).
    VirtualAlbum(i64, String),
    /// A person from facial recognition (id, display name).
    Person(i64, String),
    /// An unnamed face cluster (id, display name).
    Cluster(i64, String),
    /// A stylised character (id, display name).
    Character(i64, String),
    /// An unnamed stylised face cluster (id, display name).
    StyleCluster(i64, String),
    /// An Immich album (server id, album uuid, display name). Not re-queryable
    /// from the local database; a reload refetches over HTTP through the caller.
    #[allow(dead_code)] // Fields document the album payload.
    Immich(i64, String, String),
}

/// One detected face's rect for the face-box overlay, in per-mille of the
/// oriented image.
struct FaceBoxRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// True when the face already has a person/character, so the overlay can
    /// draw it green (assigned) or yellow (unassigned), matching `Viewer`'s
    /// convention.
    assigned: bool,
    /// The assigned person/character id, or `0` when unassigned.
    person_id: i64,
    /// The automatic cluster id, meaningful only when unassigned.
    cluster_id: i64,
}

impl Grid {
    /// Build the grid, starting the worker pool. `lib` supplies photo data;
    /// `gen` renders thumbnails. Both are shared with the workers.
    pub fn new(
        lib: Arc<Library>,
        gen: Arc<Generator>,
        thumb_size: i32,
        pause_until: Arc<AtomicU64>,
    ) -> Rc<Grid> {
        let header = Label::new(None);
        header.set_xalign(0.0);
        header.set_hexpand(true);
        header.set_margin_top(6);
        header.set_margin_bottom(6);

        // A back button, shown only for the person/cluster views. It sits left
        // of the title. Its action is set per view with `set_back`.
        let back_btn = Button::from_icon_name("go-previous-symbolic");
        back_btn.add_css_class("flat");
        back_btn.set_visible(false);
        back_btn.set_margin_start(6);

        let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        header_box.append(&back_btn);
        header_box.append(&header);

        // The sort-order dropdown, at the right of the header. "Date" sorts by
        // capture time then filename. "Name" sorts by filename only.
        let sort_setting = lib
            .get_setting(super::prefs::KEY_SORT_ORDER, "date")
            .unwrap_or_else(|_| "date".to_string());
        let sort_order = SortOrder::from_setting(&sort_setting);
        let show_filenames = lib
            .get_setting(super::prefs::KEY_SHOW_FILENAMES, "0")
            .map(|v| v == "1")
            .unwrap_or(false);
        let sort_dropdown =
            gtk4::DropDown::from_strings(&[
                "Date \u{2193}",
                "Date \u{2191}",
                "Name \u{2193}",
                "Name \u{2191}",
                "Size \u{2193}",
                "Size \u{2191}",
            ]);
        sort_dropdown.set_selected(sort_order.dropdown_index());
        sort_dropdown.set_margin_end(6);
        let sort_label = Label::new(Some("Sort:"));
        sort_label.set_margin_start(6);
        header_box.append(&sort_label);
        header_box.append(&sort_dropdown);

        // The "show face boxes" toggle, visible only for a face source
        // (person/cluster/character/style-cluster). Wired in `into_rc`, once
        // an `Rc<Grid>` exists to call back into.
        let faces_btn = Button::from_icon_name("avatar-default-symbolic");
        faces_btn.add_css_class("flat");
        faces_btn.set_visible(false);
        faces_btn.set_tooltip_text(Some("Show face boxes"));
        header_box.append(&faces_btn);

        // The duplicate-results action bar. Hidden unless a duplicate view is
        // shown. It holds a hint label and a "Delete marked" button.
        let dup_label = Label::new(None);
        dup_label.set_xalign(0.0);
        dup_label.set_hexpand(true);
        let dup_delete_btn = Button::with_label("Delete marked");
        dup_delete_btn.add_css_class("destructive-action");
        let dup_ban_btn = Button::with_label("Not duplicates");
        let dup_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        dup_bar.add_css_class("toolbar");
        dup_bar.set_margin_start(6);
        dup_bar.set_margin_end(6);
        dup_bar.set_margin_top(4);
        dup_bar.set_margin_bottom(4);
        dup_bar.append(&dup_label);
        dup_bar.append(&dup_ban_btn);
        dup_bar.append(&dup_delete_btn);
        dup_bar.set_visible(false);

        let store = gio::ListStore::new::<PhotoObject>();
        let selection = MultiSelection::new(Some(store.clone()));

        let generation = Arc::new(AtomicU64::new(0));
        let pending: Rc<RefCell<HashMap<String, PhotoObject>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let tex_cache = Rc::new(RefCell::new(super::thumbcache::TextureCache::new(512)));

        // Result channel: workers -> UI thread.
        let (done_tx, done_rx) = glib::MainContext::channel::<Done>(glib::Priority::DEFAULT);
        // Job channel: UI thread -> workers.
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let job_rx = Arc::new(std::sync::Mutex::new(job_rx));

        for _ in 0..THUMB_WORKERS {
            let job_rx = job_rx.clone();
            let done_tx = done_tx.clone();
            let gen = gen.clone();
            let cur_gen = generation.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let rx = job_rx.lock().unwrap();
                    match rx.recv() {
                        Ok(j) => j,
                        Err(_) => return, // senders dropped; exit
                    }
                };
                // Drop a stale job without decoding it. Fast folder switches
                // leave old jobs in the FIFO channel. Skip them so the current
                // folder's thumbnails render without a decode backlog first.
                if job.generation != cur_gen.load(Ordering::Relaxed) {
                    continue;
                }
                match gen.get_edited(
                    &job.hash,
                    std::path::Path::new(&job.path),
                    job.orientation,
                    &job.edit,
                ) {
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

        // Immich thumbnail worker pool: download asset thumbnails over HTTP and
        // feed the decoded bytes back through the same `done_tx` channel. Each
        // worker caches one `immich::Client` per server id it has seen.
        let (immich_tx, immich_rx) = mpsc::channel::<ImmichJob>();
        let immich_rx = Arc::new(std::sync::Mutex::new(immich_rx));
        for _ in 0..THUMB_WORKERS {
            let immich_rx = immich_rx.clone();
            let done_tx = done_tx.clone();
            let lib = lib.clone();
            std::thread::spawn(move || {
                let mut clients: HashMap<i64, crate::immich::Client> = HashMap::new();
                let mut thumbs: HashMap<i64, Option<crate::db::ImmichThumbs>> = HashMap::new();
                loop {
                    let job = {
                        let rx = immich_rx.lock().unwrap();
                        match rx.recv() {
                            Ok(j) => j,
                            Err(_) => return,
                        }
                    };
                    // Disk cache first: a stored thumbnail skips the HTTP call.
                    let cache = thumbs.entry(job.server_id).or_insert_with(|| {
                        crate::db::ImmichThumbs::open_for_server(job.server_id).ok()
                    });
                    if let Some(cache) = cache.as_ref() {
                        if let Ok(Some(blob)) = cache.get(&job.asset_id) {
                            if !blob.is_empty() {
                                let _ = done_tx.send(Done {
                                    key: job.key,
                                    blob,
                                    generation: job.generation,
                                });
                                continue;
                            }
                        }
                    }
                    let client = match clients.get(&job.server_id) {
                        Some(c) => c,
                        None => {
                            let Ok(Some(s)) = lib.immich_server(job.server_id) else {
                                continue;
                            };
                            clients.insert(
                                job.server_id,
                                crate::immich::Client::new(&s.base_url, &s.api_key),
                            );
                            clients.get(&job.server_id).unwrap()
                        }
                    };
                    match client.asset_thumbnail(&job.asset_id) {
                        Ok(blob) if !blob.is_empty() => {
                            // Store for next time, then hand the bytes to the UI.
                            if let Some(cache) = thumbs.get(&job.server_id).and_then(|c| c.as_ref())
                            {
                                let _ = cache.put(&job.asset_id, &blob);
                            }
                            let _ = done_tx.send(Done {
                                key: job.key,
                                blob,
                                generation: job.generation,
                            });
                        }
                        _ => {}
                    }
                }
            });
        }

        let factory = build_factory(thumb_size, std::rc::Weak::new());
        let grid_view = GridView::new(Some(selection.clone()), Some(factory));
        grid_view.set_min_columns(1);
        grid_view.set_max_columns(20);

        let scroller = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vexpand(true)
            .child(&grid_view)
            .build();

        // The duplicate-results container. Filled and swapped into the scroller
        // in duplicate mode; empty otherwise.
        let dup_container = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        dup_container.set_margin_top(8);
        dup_container.set_margin_bottom(8);
        dup_container.set_margin_start(8);
        dup_container.set_margin_end(8);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header_box);
        root.append(&dup_bar);
        root.append(&scroller);

        // Apply finished thumbnails on the UI thread by setting the texture on
        // the matching PhotoObject; the bound Image updates automatically. The
        // decoded texture is also cached so re-entry skips decoding.
        let gen_for_apply = generation.clone();
        let pending_for_apply = pending.clone();
        let cache_for_apply = tex_cache.clone();
        let pause_for_apply = pause_until.clone();
        done_rx.attach(None, move |done: Done| {
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(obj) = pending_for_apply.borrow_mut().remove(&done.key) {
                    if let Some(texture) = decode_texture(&done.blob) {
                        cache_for_apply
                            .borrow_mut()
                            .put(done.key.clone(), texture.clone());
                        obj.set_texture(Some(texture));
                        // A thumbnail for the current view just landed: keep the
                        // background scan/enrichment paused so the UI keeps the
                        // disk while the visible folder is still rendering.
                        let now = super::state::now_millis();
                        let until = now.saturating_add(BROWSE_PAUSE_MS);
                        if until > pause_for_apply.load(Ordering::Relaxed) {
                            pause_for_apply.store(until, Ordering::Relaxed);
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });

        Grid {
            root,
            header,
            back_btn,
            back_handler: RefCell::new(None),
            store,
            grid_view,
            selection,
            thumb_size: std::cell::Cell::new(thumb_size),
            generation,
            jobs: job_tx,
            immich_jobs: immich_tx,
            pending,
            all_photos: RefCell::new(Vec::new()),
            title: RefCell::new(String::new()),
            filter: RefCell::new(String::new()),
            lib,
            tex_cache,
            source: RefCell::new(Source::None),
            sort_order: std::cell::Cell::new(sort_order),
            show_filenames: std::cell::Cell::new(show_filenames),
            show_faces: std::cell::Cell::new(false),
            face_boxes: RefCell::new(HashMap::new()),
            face_areas: RefCell::new(Vec::new()),
            faces_btn,
            sort_dropdown,
            on_activate: RefCell::new(None),
            on_select: RefCell::new(None),
            on_enrich_request: RefCell::new(None),
            enrich_buffer: RefCell::new(Vec::new()),
            enrich_flush_scheduled: std::cell::Cell::new(false),
            scroll_settle_scheduled: std::cell::Cell::new(false),
            on_context_menu: RefCell::new(None),
            dup_mode: std::cell::Cell::new(false),
            dup_bar,
            dup_label,
            dup_delete_btn,
            on_dup_delete: RefCell::new(None),
            dup_ban_btn,
            on_dup_ban: RefCell::new(None),
            scroller,
            dup_container,
            dup_state: RefCell::new(Vec::new()),
        }
        .into_rc()
    }

    fn into_rc(self) -> Rc<Grid> {
        let rc = Rc::new(self);
        // Install the demand-driven factory now that the `Rc<Grid>` exists, so
        // the cell `bind` can reach the grid to enqueue a thumbnail only when a
        // cell scrolls into view.
        {
            let factory = build_factory(rc.thumb_size.get(), Rc::downgrade(&rc));
            rc.grid_view.set_factory(Some(&factory));
        }
        // The sort dropdown re-orders the current photos and persists the choice.
        {
            let rc2 = rc.clone();
            rc.sort_dropdown.connect_selected_notify(move |dd| {
                let order = SortOrder::from_dropdown_index(dd.selected());
                rc2.set_sort_order(order);
            });
        }
        // The face-box toggle: flips `show_faces` and repaints the grid so
        // every realised cell's `DrawingArea` re-runs its draw func.
        {
            let rc2 = rc.clone();
            rc.faces_btn.connect_clicked(move |btn| {
                let on = !rc2.show_faces.get();
                rc2.show_faces.set(on);
                if on {
                    btn.add_css_class("suggested-action");
                } else {
                    btn.remove_css_class("suggested-action");
                }
                // Queue a redraw on every live face-box DrawingArea directly:
                // GTK4 caches each widget's own render node, so queuing the
                // grid_view alone would not re-run an already-bound cell's
                // draw func (only a cell that gets rebound, e.g. by
                // scrolling, would pick up the new state).
                rc2.face_areas.borrow_mut().retain(|w| match w.upgrade() {
                    Some(area) => {
                        area.queue_draw();
                        true
                    }
                    None => false,
                });
            });
        }
        // Activation (double-click / Enter) opens the viewer.
        {
            let rc2 = rc.clone();
            rc.grid_view.connect_activate(move |_, pos| {
                let photos = rc2.filtered_photos();
                if (pos as usize) < photos.len() {
                    if let Some(cb) = rc2.on_activate.borrow().as_ref() {
                        cb(photos, pos as usize);
                    }
                }
            });
        }
        // Selection change updates the properties panel with the first
        // selected photo.
        {
            let rc2 = rc.clone();
            rc.selection.connect_selection_changed(move |sel, _, _| {
                let bitset = sel.selection();
                if bitset.size() == 0 {
                    return;
                }
                let pos = bitset.nth(0);
                let photos = rc2.filtered_photos();
                if let Some(p) = photos.get(pos as usize) {
                    if let Some(cb) = rc2.on_select.borrow().as_ref() {
                        cb(p.clone());
                    }
                }
            });
        }
        // Duplicate mode: the "Delete marked" button hands the marked photos to
        // the app for a hard delete.
        {
            let rc2 = rc.clone();
            rc.dup_delete_btn.connect_clicked(move |_| {
                let marked = rc2.marked_photos();
                if marked.is_empty() {
                    return;
                }
                if let Some(cb) = rc2.on_dup_delete.borrow().as_ref() {
                    cb(marked);
                }
            });
        }
        // Duplicate mode: the "Not duplicates" button bans each marked photo
        // from grouping with its group's kept copy again.
        {
            let rc2 = rc.clone();
            rc.dup_ban_btn.connect_clicked(move |_| {
                let pairs = rc2.marked_ban_pairs();
                if pairs.is_empty() {
                    return;
                }
                if let Some(cb) = rc2.on_dup_ban.borrow().as_ref() {
                    cb(pairs);
                }
            });
        }
        // Right-click anywhere in the grid raises the context menu over the
        // current selection.
        {
            let rc2 = rc.clone();
            let gesture = gtk4::GestureClick::new();
            gesture.set_button(gdk::BUTTON_SECONDARY);
            gesture.connect_pressed(move |_, _, x, y| {
                if let Some(cb) = rc2.on_context_menu.borrow().as_ref() {
                    cb(x, y);
                }
            });
            rc.grid_view.add_controller(gesture);
        }
        // Drag source: dragging thumbnails carries the selected photo ids as a
        // string payload `photos:<id>,<id>,...` so a virtual-album sidebar row
        // can accept them. GridView selects the pressed cell before the drag
        // begins, so a drag over an unselected cell carries just that cell.
        {
            let rc2 = rc.clone();
            let src = gtk4::DragSource::new();
            src.set_actions(gdk::DragAction::COPY);
            src.connect_prepare(move |_, _, _| {
                let ids: Vec<String> = rc2
                    .selected_photos()
                    .iter()
                    .filter(|p| p.id != 0)
                    .map(|p| p.id.to_string())
                    .collect();
                if ids.is_empty() {
                    return None;
                }
                let payload = format!("photos:{}", ids.join(","));
                Some(gdk::ContentProvider::for_value(&payload.to_value()))
            });
            rc.grid_view.add_controller(src);
        }
        // Scroll-settle: when the user stops scrolling, fill on-demand work for
        // the new visible window. Because GTK's bind is not a reliable "visible"
        // signal, this is the authoritative trigger for enriching/thumbnailing
        // the window the user actually stopped on.
        {
            let rc2 = rc.clone();
            rc.scroller
                .vadjustment()
                .connect_value_changed(move |_| {
                    rc2.schedule_visible_refresh();
                });
        }
        rc
    }

    /// Schedule a debounced refresh of on-demand work for the visible window.
    /// Called on scroll. Coalesces a burst of scroll events into one refresh
    /// ~200 ms after scrolling stops.
    fn schedule_visible_refresh(self: &Rc<Self>) {
        if self.scroll_settle_scheduled.replace(true) {
            return;
        }
        let this = self.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            this.scroll_settle_scheduled.set(false);
            this.refresh_visible_window();
        });
    }

    /// Request thumbnails and enrichment for the photos in the current visible
    /// window that still need them. Idempotent: cached/enriched cells are
    /// skipped inside `ensure_thumb_for` and by the empty-hash check.
    fn refresh_visible_window(self: &Rc<Self>) {
        if self.dup_mode.get() {
            return;
        }
        let (start, end) = self.visible_index_range();
        let mut enrich_ids: Vec<i64> = Vec::new();
        for i in start..end {
            let Some(obj) = self.store.item(i as u32).and_downcast::<PhotoObject>() else {
                continue;
            };
            self.ensure_thumb_for(&obj);
            if obj.hash().is_empty() && !obj.path().is_empty() && obj.id() != 0 {
                enrich_ids.push(obj.id());
            }
        }
        if !enrich_ids.is_empty() {
            if let Some(cb) = self.on_enrich_request.borrow().as_ref() {
                cb(enrich_ids);
            }
        }
    }

    /// Register the activation callback (opens the viewer).
    pub fn set_on_activate<F: Fn(Vec<Photo>, usize) + 'static>(&self, f: F) {
        *self.on_activate.borrow_mut() = Some(Box::new(f));
    }

    /// Register the selection callback (updates properties).
    pub fn set_on_select<F: Fn(Photo) + 'static>(&self, f: F) {
        *self.on_select.borrow_mut() = Some(Box::new(f));
    }

    /// Register the viewport-enrichment callback. Called with the ids of photos
    /// that scrolled into view and still lack a hash.
    pub fn set_on_enrich_request<F: Fn(Vec<i64>) + 'static>(&self, f: F) {
        *self.on_enrich_request.borrow_mut() = Some(Box::new(f));
    }

    /// Record that a bound cell's photo needs enrichment, and schedule one
    /// debounced flush. Batching turns the burst of `bind` calls on a folder
    /// open into a single small enrich request for just the realized cells.
    fn request_enrich(self: &Rc<Self>, id: i64) {
        if id == 0 {
            return;
        }
        {
            let mut buf = self.enrich_buffer.borrow_mut();
            if buf.contains(&id) {
                return;
            }
            buf.push(id);
        }
        if self.enrich_flush_scheduled.replace(true) {
            return; // a flush is already pending
        }
        let this = self.clone();
        // ~180 ms after the last realize burst, hand the collected ids to the
        // app for enrichment. New binds inside the window join the same batch.
        // The batch is filtered to the current visible window, because GTK may
        // have bound the whole model, not only the on-screen cells.
        glib::timeout_add_local_once(std::time::Duration::from_millis(180), move || {
            this.enrich_flush_scheduled.set(false);
            let ids: Vec<i64> = this.enrich_buffer.borrow_mut().drain(..).collect();
            if ids.is_empty() {
                return;
            }
            let ids = this.filter_ids_to_visible(ids);
            if ids.is_empty() {
                return;
            }
            if let Some(cb) = this.on_enrich_request.borrow().as_ref() {
                cb(ids);
            }
        });
    }

    /// Keep only the ids whose photo is within the current visible index window.
    /// Used to discard the off-screen ids GTK bound during a full-model measure
    /// pass, so enrichment follows the viewport.
    fn filter_ids_to_visible(&self, ids: Vec<i64>) -> Vec<i64> {
        let (start, end) = self.visible_index_range();
        // Build the set of visible ids once, then keep the requested ids that
        // fall in it, preserving order.
        let mut visible: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for i in start..end {
            if let Some(obj) = self.store.item(i as u32).and_downcast::<PhotoObject>() {
                visible.insert(obj.id());
            }
        }
        ids.into_iter().filter(|id| visible.contains(id)).collect()
    }

    /// Drop a photo id from the pending enrich batch when its cell scrolled out
    /// of view before the batch flushed.
    fn drop_enrich_request(&self, id: i64) {
        self.enrich_buffer.borrow_mut().retain(|&x| x != id);
    }

    /// Register the right-click context-menu callback.
    pub fn set_on_context_menu<F: Fn(f64, f64) + 'static>(&self, f: F) {
        *self.on_context_menu.borrow_mut() = Some(Box::new(f));
    }

    /// Register the "Delete marked" callback for duplicate mode.
    pub fn set_on_dup_delete<F: Fn(Vec<Photo>) + 'static>(&self, f: F) {
        *self.on_dup_delete.borrow_mut() = Some(Box::new(f));
    }

    /// Register the "Not duplicates" ban callback for duplicate mode. The
    /// callback receives `(marked, keep)` pairs to ban from matching again.
    pub fn set_on_dup_ban<F: Fn(Vec<(Photo, Photo)>) + 'static>(&self, f: F) {
        *self.on_dup_ban.borrow_mut() = Some(Box::new(f));
    }

    /// Show the duplicate groups. Each group is drawn as its own framed box
    /// (a single bounding box around all its photos). The marked (worst) photo
    /// per group starts with the red X. A click marks a photo for deletion, or
    /// clears the mark when the marked photo is clicked. The "Delete marked"
    /// action bar appears above the view.
    pub fn show_duplicates(self: &Rc<Self>, title: &str, photos: &[(Photo, i64, bool)]) {
        *self.source.borrow_mut() = Source::None;
        self.hide_back();
        self.dup_mode.set(true);
        self.dup_bar.set_visible(true);
        *self.title.borrow_mut() = title.to_string();
        self.header.set_text(title);

        // Bump the generation so any in-flight normal-grid thumbnail results are
        // ignored, and clear the pending map for this fresh view.
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending.borrow_mut().clear();

        // Rebuild the group container.
        let container = &self.dup_container;
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        self.dup_state.borrow_mut().clear();

        let size = self.thumb_size.get();

        // Group the incoming entries by group id, preserving order.
        let mut groups: Vec<(i64, Vec<(Photo, bool)>)> = Vec::new();
        for (p, group, mark) in photos {
            match groups.last_mut() {
                Some((g, v)) if *g == *group => v.push((p.clone(), *mark)),
                _ => groups.push((*group, vec![(p.clone(), *mark)])),
            }
        }

        for (group, members) in groups {
            let frame = gtk4::Frame::new(None);
            frame.add_css_class("dup-group-frame");

            let flow = gtk4::FlowBox::new();
            flow.set_selection_mode(gtk4::SelectionMode::None);
            flow.set_homogeneous(false);
            flow.set_column_spacing(6);
            flow.set_row_spacing(6);
            flow.set_margin_top(6);
            flow.set_margin_bottom(6);
            flow.set_margin_start(6);
            flow.set_margin_end(6);
            flow.set_max_children_per_line(20);
            frame.set_child(Some(&flow));

            let mut cells: Vec<DupCellUi> = Vec::new();
            for (photo, mark) in members {
                let overlay = Overlay::new();
                overlay.set_size_request(size, size);

                let image = Image::new();
                image.set_pixel_size(size);
                overlay.set_child(Some(&image));

                let x = Label::new(Some("\u{2715}"));
                x.set_halign(Align::Center);
                x.set_valign(Align::Center);
                x.add_css_class("dup-x");
                x.set_visible(mark);
                overlay.add_overlay(&x);

                // A per-thumbnail PhotoObject drives the async thumbnail load.
                let obj = PhotoObject::from_photo(&photo);
                {
                    let image_weak = image.downgrade();
                    obj.connect_notify_local(Some("texture"), move |o: &PhotoObject, _| {
                        if let Some(image) = image_weak.upgrade() {
                            if let Some(t) = o.texture() {
                                image.set_paintable(Some(&t));
                            }
                        }
                    });
                }

                // Click handling: mark/unmark within the group.
                {
                    let this = self.clone();
                    let pid = photo.id;
                    let gid = group;
                    let gesture = gtk4::GestureClick::new();
                    gesture.set_button(gdk::BUTTON_PRIMARY);
                    gesture.connect_pressed(move |_, _, _, _| {
                        this.toggle_dup_mark(gid, pid);
                    });
                    overlay.add_controller(gesture);
                }

                flow.append(&overlay);

                // Queue the thumbnail through the shared worker pool.
                let edit = self.lib.photo_edit(photo.id).unwrap_or_default();
                let key = cell_key(&photo, size, &edit);
                if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
                    image.set_paintable(Some(&texture));
                } else {
                    self.enqueue_thumb(&photo, &obj, &key, edit, gen);
                }

                cells.push(DupCellUi {
                    photo,
                    marked: std::cell::Cell::new(mark),
                    x_widget: x,
                });
            }

            container.append(&frame);
            self.dup_state.borrow_mut().push(DupGroupUi { group, cells });
        }

        // Swap the scroller to show the group container.
        self.scroller.set_child(Some(&self.dup_container));
        self.update_dup_label();
    }

    /// Leave duplicate mode: hide the action bar, clear the group container, and
    /// restore the normal grid view.
    pub fn exit_dup_mode(&self) {
        if !self.dup_mode.get() {
            return;
        }
        self.dup_mode.set(false);
        self.dup_bar.set_visible(false);
        let container = &self.dup_container;
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        self.dup_state.borrow_mut().clear();
        self.scroller.set_child(Some(&self.grid_view));
    }

    /// Toggle the red X for photo `pid` inside group `gid`. Clicking the marked
    /// photo clears it; clicking any other photo moves the mark to it.
    fn toggle_dup_mark(&self, gid: i64, pid: i64) {
        let state = self.dup_state.borrow();
        let Some(grp) = state.iter().find(|g| g.group == gid) else {
            return;
        };
        // Was the clicked photo already marked?
        let was_marked = grp
            .cells
            .iter()
            .find(|c| c.photo.id == pid)
            .map(|c| c.marked.get())
            .unwrap_or(false);
        // Clear every mark in the group.
        for c in &grp.cells {
            c.marked.set(false);
            c.x_widget.set_visible(false);
        }
        // Clicking the marked photo unmarks the group; any other click moves it.
        if !was_marked {
            if let Some(c) = grp.cells.iter().find(|c| c.photo.id == pid) {
                c.marked.set(true);
                c.x_widget.set_visible(true);
            }
        }
        drop(state);
        self.update_dup_label();
    }

    /// The photos currently marked for deletion in duplicate mode.
    fn marked_photos(&self) -> Vec<Photo> {
        let mut out = Vec::new();
        for g in self.dup_state.borrow().iter() {
            for c in &g.cells {
                if c.marked.get() {
                    out.push(c.photo.clone());
                }
            }
        }
        out
    }

    /// For each marked photo, the pair `(marked, keep)` where keep is the
    /// group's kept copy (the first cell). Backs the "Not duplicates" ban
    /// action: banning each pair stops the marked copy grouping with the keep.
    fn marked_ban_pairs(&self) -> Vec<(Photo, Photo)> {
        let mut out = Vec::new();
        for g in self.dup_state.borrow().iter() {
            let Some(keep) = g.cells.first().map(|c| c.photo.clone()) else {
                continue;
            };
            for c in &g.cells {
                if c.marked.get() && c.photo.id != keep.id {
                    out.push((c.photo.clone(), keep.clone()));
                }
            }
        }
        out
    }

    /// Update the duplicate action-bar label with the current marked count.
    fn update_dup_label(&self) {
        let mut marked = 0i64;
        let mut reclaim = 0i64;
        for g in self.dup_state.borrow().iter() {
            for c in &g.cells {
                if c.marked.get() {
                    marked += 1;
                    reclaim += c.photo.size;
                }
            }
        }
        self.dup_label.set_text(&format!(
            "Click a photo to mark it for deletion (red X). Click the X to unmark. \
             {marked} marked, {} to free.",
            human_size(reclaim)
        ));
        self.dup_delete_btn.set_sensitive(marked > 0);
    }


    /// The `GridView` widget, used as a menu anchor.
    pub fn grid_view(&self) -> &GridView {
        &self.grid_view
    }

    /// The grid's root widget.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Show the back button and set its action. Used by the person and cluster
    /// views to return to the Faces view.
    pub fn set_back<F: Fn() + 'static>(&self, cb: F) {
        // Drop any previous handler by disconnecting via a fresh closure. GTK
        // keeps one handler here; connecting again adds another, so we guard by
        // clearing first through a stored handler id.
        self.back_btn.set_visible(true);
        // Remove old handlers by re-creating the click connection. Simplest is
        // to disconnect all by setting a new signal; GTK4 lacks a direct clear,
        // so we track a single handler id.
        if let Some(id) = self.back_handler.borrow_mut().take() {
            self.back_btn.disconnect(id);
        }
        let id = self.back_btn.connect_clicked(move |_| cb());
        *self.back_handler.borrow_mut() = Some(id);
    }

    /// Hide the back button (non-person views).
    pub fn hide_back(&self) {
        self.back_btn.set_visible(false);
        if let Some(id) = self.back_handler.borrow_mut().take() {
            self.back_btn.disconnect(id);
        }
    }

    /// The active thumbnail size in pixels.
    #[allow(dead_code)] // Kept API accessor.
    pub fn thumb_size(&self) -> i32 {
        self.thumb_size.get()
    }

    /// The currently displayed (filtered) photos. Matches on filename OR on
    /// tags (via the FTS index), mirroring the toolbar search behaviour.
    fn filtered_photos(&self) -> Vec<Photo> {
        let filter = self.filter.borrow().to_lowercase();
        let all = self.all_photos.borrow();
        if filter.is_empty() {
            return all.clone();
        }
        let tag_matches = self
            .lib
            .search_photo_ids_by_tag(&filter)
            .unwrap_or_default();
        all.iter()
            .filter(|p| {
                p.filename.to_lowercase().contains(&filter)
                    || (p.id != 0 && tag_matches.contains(&p.id))
            })
            .cloned()
            .collect()
    }

    /// Replace the shown photos with an ad-hoc list (no re-queryable source).
    /// Bumps the generation so stale thumbnail results are discarded.
    pub fn show_photos(&self, title: &str, photos: &[Photo]) {
        *self.source.borrow_mut() = Source::None;
        self.set_photos(title, photos.to_vec());
    }

    /// True when the current source is a stylised character or style cluster.
    /// The viewer uses this to draw stylised face boxes instead of human faces.
    pub fn is_style_source(&self) -> bool {
        matches!(
            *self.source.borrow(),
            Source::Character(..) | Source::StyleCluster(..)
        )
    }

    /// True when the current source has per-photo face data to overlay: a
    /// named person, an unnamed human cluster, a named character, or an
    /// unnamed style cluster. Gates the "show face boxes" toggle's
    /// visibility.
    pub fn is_face_source(&self) -> bool {
        matches!(
            *self.source.borrow(),
            Source::Person(..) | Source::Cluster(..) | Source::Character(..) | Source::StyleCluster(..)
        )
    }

    /// Bulk-load every face detected in `photos` (any person/character, not
    /// just the one this view is scoped to) into `face_boxes`, keyed by photo
    /// id, for the face-box overlay. `style` selects the stylised vs. human
    /// face table.
    fn load_face_boxes(&self, photos: &[Photo], style: bool) {
        let ids: Vec<i64> = photos.iter().map(|p| p.id).collect();
        let mut map: HashMap<i64, Vec<FaceBoxRect>> = HashMap::new();
        if style {
            for f in self.lib.style_faces_for_photos(&ids).unwrap_or_default() {
                map.entry(f.photo_id).or_default().push(FaceBoxRect {
                    x: f.bbox_x,
                    y: f.bbox_y,
                    w: f.bbox_w,
                    h: f.bbox_h,
                    assigned: f.character_id != 0,
                    person_id: f.character_id,
                    cluster_id: f.cluster_id,
                });
            }
        } else {
            for f in self.lib.faces_for_photos(&ids).unwrap_or_default() {
                map.entry(f.photo_id).or_default().push(FaceBoxRect {
                    x: f.bbox_x,
                    y: f.bbox_y,
                    w: f.bbox_w,
                    h: f.bbox_h,
                    assigned: f.person_id != 0,
                    person_id: f.person_id,
                    cluster_id: f.cluster_id,
                });
            }
        }
        *self.face_boxes.borrow_mut() = map;
    }

    /// Show a scanned library folder, remembering it as the source so the grid
    /// can re-query the database later (e.g. after a scan or rotation).
    pub fn show_folder(&self, folder_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Folder(folder_id, name.to_string());
        let photos = self.lib.photos_in_folder(folder_id).unwrap_or_default();
        self.set_photos(name, photos);
    }

    /// Show a raw filesystem directory, remembering it as the source.
    pub fn show_raw_folder(&self, dir: &str) {
        *self.source.borrow_mut() = Source::RawDir(dir.to_string());
        let (title, photos) = self.load_raw_dir(dir);
        self.set_photos(&title, photos);
    }

    /// Show a virtual album, remembering it as the source so the grid can
    /// re-query after membership or rule changes.
    pub fn show_virtual_album(&self, album_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::VirtualAlbum(album_id, name.to_string());
        let photos = self
            .lib
            .photos_in_virtual_album(album_id)
            .unwrap_or_default();
        self.set_photos(name, photos);
    }

    /// Show every photo that contains a given person, remembering the person as
    /// the source so the grid can re-query after a new scan.
    pub fn show_person(&self, person_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Person(person_id, name.to_string());
        let photos = self.lib.photos_of_person(person_id).unwrap_or_default();
        self.load_face_boxes(&photos, false);
        self.set_photos(name, photos);
    }

    /// Show every photo in an unnamed face cluster, remembering the cluster as
    /// the source so the grid can re-query after a new scan.
    pub fn show_cluster(&self, cluster_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Cluster(cluster_id, name.to_string());
        let photos = self.lib.photos_in_cluster(cluster_id).unwrap_or_default();
        self.load_face_boxes(&photos, false);
        self.set_photos(name, photos);
    }

    /// Show every photo that contains a given stylised character.
    pub fn show_character(&self, character_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Character(character_id, name.to_string());
        let photos = self.lib.photos_of_character(character_id).unwrap_or_default();
        self.load_face_boxes(&photos, true);
        self.set_photos(name, photos);
    }

    /// Show every photo in an unnamed stylised face cluster.
    pub fn show_style_cluster(&self, cluster_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::StyleCluster(cluster_id, name.to_string());
        let photos = self
            .lib
            .photos_in_style_cluster(cluster_id)
            .unwrap_or_default();
        self.load_face_boxes(&photos, true);
        self.set_photos(name, photos);
    }

    /// Show an Immich album's assets. The caller passes the already-fetched    /// photos (each with an `immich://<server_id>/<asset_id>` path). The grid
    /// downloads each thumbnail over HTTP through the Immich worker pool.
    pub fn show_immich_album(&self, server_id: i64, album_id: &str, name: &str, photos: Vec<Photo>) {
        *self.source.borrow_mut() =
            Source::Immich(server_id, album_id.to_string(), name.to_string());
        self.set_photos(name, photos);
    }

    /// The virtual album id the grid is currently showing, if any.
    pub fn current_virtual_album(&self) -> Option<i64> {        match &*self.source.borrow() {
            Source::VirtualAlbum(id, _) => Some(*id),
            _ => None,
        }
    }

    /// The character id the grid is currently showing, if any.
    pub fn current_character(&self) -> Option<i64> {
        match &*self.source.borrow() {
            Source::Character(id, _) => Some(*id),
            _ => None,
        }
    }

    /// The person id the grid is currently showing, if any.
    pub fn current_person(&self) -> Option<i64> {
        match &*self.source.borrow() {
            Source::Person(id, _) => Some(*id),
            _ => None,
        }
    }

    /// The unnamed face cluster id the grid is currently showing, if any.
    pub fn current_cluster(&self) -> Option<i64> {
        match &*self.source.borrow() {
            Source::Cluster(id, _) => Some(*id),
            _ => None,
        }
    }

    /// The stylised cluster id the grid is currently showing, if any.
    pub fn current_style_cluster(&self) -> Option<i64> {
        match &*self.source.borrow() {
            Source::StyleCluster(id, _) => Some(*id),
            _ => None,
        }
    }

    /// The photos currently selected in the grid (multi-selection), in view
    /// order. Empty when nothing is selected.
    pub fn selected_photos(&self) -> Vec<Photo> {        let photos = self.filtered_photos();
        let bitset = self.selection.selection();
        let mut out = Vec::new();
        for i in 0..bitset.size() {
            let pos = bitset.nth(i as u32) as usize;
            if let Some(p) = photos.get(pos) {
                out.push(p.clone());
            }
        }
        out
    }

    /// All photos currently visible in the grid (after any search filter), in
    /// view order. Used to play a slideshow of the whole current view.
    pub fn visible_photos(&self) -> Vec<Photo> {
        self.filtered_photos()
    }

    /// Re-query the current source (folder or raw dir) from the database/disk
    /// and rebuild the view. A true "refresh visible": picks up newly scanned
    /// photos and updated orientations. No-op for ad-hoc lists.
    pub fn reload_from_source(&self) {
        let source = self.source.borrow().clone();
        match source {
            Source::Folder(id, name) => {
                let photos = self.lib.photos_in_folder(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::RawDir(dir) => {
                let (title, photos) = self.load_raw_dir(&dir);
                self.set_photos_preserving(&title, photos);
            }
            Source::VirtualAlbum(id, name) => {
                let photos = self.lib.photos_in_virtual_album(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::Person(id, name) => {
                let photos = self.lib.photos_of_person(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::Cluster(id, name) => {
                let photos = self.lib.photos_in_cluster(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::Character(id, name) => {
                let photos = self.lib.photos_of_character(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::StyleCluster(id, name) => {
                let photos = self.lib.photos_in_style_cluster(id).unwrap_or_default();
                self.set_photos_preserving(&name, photos);
            }
            Source::None => {}
            // Immich albums refetch over HTTP. The grid keeps the last-shown
            // photos; the caller re-drives the fetch when it needs fresh data.
            Source::Immich(..) => {}
        }
    }

    /// Load a raw filesystem directory's images, reusing scanned content hashes
    /// so cached thumbnails are found. Returns (title, photos).
    fn load_raw_dir(&self, dir: &str) -> (String, Vec<Photo>) {
        let hashes = self.lib.hashes_by_dir(dir).unwrap_or_default();
        let mut photos: Vec<Photo> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(crate::scan::is_image)
                        .unwrap_or(false)
                })
                .collect();
            paths.sort();
            for path in paths {
                let path_str = path.to_string_lossy().into_owned();
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let hash = hashes.get(&path_str).cloned().unwrap_or_default();
                photos.push(Photo {
                    path: path_str,
                    filename,
                    hash,
                    ..Default::default()
                });
            }
        }
        let title = std::path::Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string());
        (title, photos)
    }

    /// Store a photo set and rebuild the view.
    /// Order a photo list by the active sort order. `Date` sorts by capture
    /// time first, then filename, to match the database query order. Photos
    /// with no capture time come first. `Filename` sorts by filename only,
    /// case-insensitive.
    fn sort_photos(&self, photos: &mut [Photo]) {
        match self.sort_order.get() {
            SortOrder::DateDesc => {
                photos.sort_by(|a, b| {
                    b.taken_at
                        .cmp(&a.taken_at)
                        .then_with(|| a.filename.cmp(&b.filename))
                });
            }
            SortOrder::DateAsc => {
                photos.sort_by(|a, b| {
                    a.taken_at
                        .cmp(&b.taken_at)
                        .then_with(|| a.filename.cmp(&b.filename))
                });
            }
            SortOrder::NameAsc => {
                photos.sort_by(|a, b| {
                    a.filename.to_lowercase().cmp(&b.filename.to_lowercase())
                });
            }
            SortOrder::NameDesc => {
                photos.sort_by(|a, b| {
                    b.filename.to_lowercase().cmp(&a.filename.to_lowercase())
                });
            }
            SortOrder::SizeDesc => {
                photos.sort_by(|a, b| {
                    b.size.cmp(&a.size).then_with(|| a.filename.cmp(&b.filename))
                });
            }
            SortOrder::SizeAsc => {
                photos.sort_by(|a, b| {
                    a.size.cmp(&b.size).then_with(|| a.filename.cmp(&b.filename))
                });
            }
        }
    }

    fn set_photos(&self, title: &str, photos: Vec<Photo>) {
        // Default to no back button. The person and cluster views re-enable it
        // right after, via `set_back`.
        self.hide_back();
        self.exit_dup_mode();
        // The face-box toggle only makes sense for a face source; leaving one
        // clears its state so a later face view doesn't inherit a stray
        // "on"-looking button or stale boxes.
        self.faces_btn.set_visible(self.is_face_source());
        if !self.is_face_source() {
            self.show_faces.set(false);
            self.faces_btn.remove_css_class("suggested-action");
            self.face_boxes.borrow_mut().clear();
        }
        let mut photos = photos;
        self.sort_photos(&mut photos);
        *self.all_photos.borrow_mut() = photos;
        *self.title.borrow_mut() = title.to_string();
        self.rebuild();
    }

    /// Update the view from a background reload (scan/enrich) **without**
    /// destroying and recreating the store when the set of photos is unchanged.
    ///
    /// A full `rebuild` (`store.remove_all()` + re-append) resets the GridView's
    /// selection and focus, which — fired every few seconds by a running scan —
    /// makes it nearly impossible to click a thumbnail. This path instead diffs
    /// the incoming set against the current store by file path (a stable id that
    /// does not change as a photo gains its hash during enrichment). When the
    /// paths match in order, it updates only the changed fields on the existing
    /// `PhotoObject`s in place, so the user's selection is preserved. When the
    /// set differs (folder switch, files added/removed), it falls back to a full
    /// rebuild.
    fn set_photos_preserving(&self, title: &str, photos: Vec<Photo>) {
        let mut photos = photos;
        self.sort_photos(&mut photos);
        *self.all_photos.borrow_mut() = photos;
        *self.title.borrow_mut() = title.to_string();

        let filtered = self.filtered_photos();
        // Compare the incoming (filtered) set to the current store by path/order.
        let same = {
            let n = self.store.n_items() as usize;
            if n != filtered.len() {
                false
            } else {
                let mut ok = true;
                for (i, p) in filtered.iter().enumerate() {
                    let cur = self
                        .store
                        .item(i as u32)
                        .and_downcast::<PhotoObject>()
                        .map(|o| o.path())
                        .unwrap_or_default();
                    if cur != p.path {
                        ok = false;
                        break;
                    }
                }
                ok
            }
        };

        if !same {
            // Structure changed: a full rebuild is required (and a selection
            // reset here is expected — the view genuinely changed).
            self.rebuild();
            return;
        }

        // In-place update: refresh each existing object's mutable fields and
        // enqueue a thumbnail only when the cell has none yet or its key changed
        // (e.g. the photo just gained its hash). The store items — and thus the
        // selection — are never removed.
        let size = self.thumb_size.get();
        self.header
            .set_text(&format!("{}  ({})", self.title.borrow(), filtered.len()));
        for (i, p) in filtered.iter().enumerate() {
            let Some(obj) = self.store.item(i as u32).and_downcast::<PhotoObject>() else {
                continue;
            };
            // Update fields that enrichment may have filled in.
            if obj.hash() != p.hash {
                obj.set_hash(p.hash.clone());
            }
            if obj.id() != p.id {
                obj.set_id(p.id);
            }
            if obj.missing() != p.missing {
                obj.set_missing(p.missing);
            }
            // If the cell already shows a thumbnail, leave it; the worker result
            // for a re-keyed job will replace it when ready.
            if obj.texture().is_some() {
                continue;
            }
            let edit = self.lib.photo_edit(p.id).unwrap_or_default();
            let key = cell_key(p, size, &edit);
            if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
                obj.set_texture(Some(texture));
                continue;
            }
            // Not cached: leave it to the factory `bind` to enqueue on demand
            // when the cell scrolls into view. This keeps a huge folder reload
            // from queuing a job for every photo at once.
        }
    }

    /// Enqueue a thumbnail job for one cell (local or Immich).
    /// The store index window that is currently on screen, plus a margin of
    /// `VISIBLE_MARGIN_ROWS` rows above and below. On-demand work (thumbnails,
    /// enrichment) is limited to this window.
    ///
    /// GTK's `GridView` can bind far more cells than are visible (it realises
    /// the whole model while it first measures a freshly populated grid whose
    /// viewport height is not yet allocated). So the `bind` signal is not a
    /// reliable "is visible" test. This computes the true window from the
    /// scroller's vertical adjustment and the cell geometry instead.
    ///
    /// Returns a half-open `[start, end)` range of store indices. When the
    /// geometry is not ready (width or page size is 0), returns
    /// `[0, VISIBLE_FALLBACK)` so a just-opened folder still fills its first
    /// screen.
    fn visible_index_range(&self) -> (usize, usize) {
        let n = self.store.n_items() as usize;
        if n == 0 {
            return (0, 0);
        }
        let size = self.thumb_size.get();
        let width = self.grid_view.allocated_width();
        let vadj = self.scroller.vadjustment();
        let page = vadj.page_size();
        // Geometry not ready yet: fall back to the first screenful.
        if width <= 0 || page <= 0.0 {
            return (0, VISIBLE_FALLBACK.min(n));
        }
        let cell_w = (size + CELL_SPACING).max(1);
        let cols = ((width / cell_w).max(1) as usize).min(20);
        let row_h = (size + CELL_SPACING).max(1) as f64;
        let first_row = (vadj.value() / row_h).floor() as i64;
        let rows_visible = (page / row_h).ceil() as i64 + 1;
        let margin = VISIBLE_MARGIN_ROWS as i64;
        let start_row = (first_row - margin).max(0) as usize;
        let end_row = (first_row + rows_visible + margin).max(0) as usize;
        let start = (start_row * cols).min(n);
        let end = (end_row * cols).min(n);
        (start, end)
    }

    /// Whether a photo object is within the current visible index window. When
    /// the geometry is not ready, treats the fallback first-screen as visible.
    fn is_object_visible(&self, obj: &PhotoObject) -> bool {
        let (start, end) = self.visible_index_range();
        if let Some(i) = self.store.find(obj) {
            let i = i as usize;
            i >= start && i < end
        } else {
            false
        }
    }

    /// Ensure a thumbnail exists for a cell that just scrolled into view.
    ///
    /// This is the demand-driven path: the factory `bind` calls it for every
    /// cell GTK realises. Because GTK can bind the whole model, the work is
    /// gated by `is_object_visible` so only the on-screen window (plus a small
    /// margin) is decoded. It serves the in-memory texture cache first, and only
    /// sends a worker job when the cell has no texture yet.
    fn ensure_thumb_for(&self, obj: &PhotoObject) {
        // Nothing to do in duplicate mode (that path renders its own cells).
        if self.dup_mode.get() {
            return;
        }
        // Gate to the visible window so a full-model bind pass does not queue a
        // decode job for every photo in a large folder.
        if !self.is_object_visible(obj) {
            return;
        }
        // Rebuild the minimal Photo fields the job needs from the object.
        let path = obj.path();
        let hash = obj.hash();
        let orientation = obj.orientation();
        let id = obj.id();
        let size = self.thumb_size.get();
        let edit = self.lib.photo_edit(id).unwrap_or_default();
        let base = if hash.is_empty() { &path } else { &hash };
        let key = format!("{base}|{size}|{orientation}|{}", edit.edit_rev);

        // Already decoded and cached: set it and skip the worker.
        if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
            obj.set_texture(Some(texture));
            return;
        }
        // Already showing a texture for this exact key: nothing to do.
        if obj.texture().is_some() {
            return;
        }
        // Already queued for this key: do not double-enqueue.
        if self.pending.borrow().contains_key(&key) {
            return;
        }
        let gen = self.generation.load(Ordering::Relaxed);
        self.pending.borrow_mut().insert(key.clone(), obj.clone());
        if let Some((server_id, asset_id)) = parse_immich_path(&path) {
            let _ = self.immich_jobs.send(ImmichJob {
                key,
                server_id,
                asset_id,
                generation: gen,
            });
            return;
        }
        let _ = self.jobs.send(Job {
            key,
            hash,
            path,
            orientation,
            edit,
            generation: gen,
        });
    }

    /// Forget any in-flight job for a cell that scrolled out of view, so a
    /// landed worker result is discarded instead of applied to a recycled cell.
    fn drop_pending_for(&self, obj: &PhotoObject) {
        let path = obj.path();
        let hash = obj.hash();
        let orientation = obj.orientation();
        let id = obj.id();
        let size = self.thumb_size.get();
        let edit = self.lib.photo_edit(id).unwrap_or_default();
        let base = if hash.is_empty() { &path } else { &hash };
        let key = format!("{base}|{size}|{orientation}|{}", edit.edit_rev);
        self.pending.borrow_mut().remove(&key);
    }

    fn enqueue_thumb(
        &self,
        p: &Photo,
        obj: &PhotoObject,
        key: &str,
        edit: crate::model::PhotoEdit,
        gen: u64,
    ) {
        self.pending.borrow_mut().insert(key.to_string(), obj.clone());
        if let Some((server_id, asset_id)) = parse_immich_path(&p.path) {
            let _ = self.immich_jobs.send(ImmichJob {
                key: key.to_string(),
                server_id,
                asset_id,
                generation: gen,
            });
            return;
        }
        let _ = self.jobs.send(Job {
            key: key.to_string(),
            hash: p.hash.clone(),
            path: p.path.clone(),
            orientation: p.orientation,
            edit,
            generation: gen,
        });
    }

    /// Set the filename/tag filter and rebuild the view.
    pub fn set_filter(&self, filter: &str) {
        *self.filter.borrow_mut() = filter.to_string();
        self.rebuild();
    }

    /// Set the sort order, persist it, sync the header dropdown, and re-sort.
    /// Callable from both the header dropdown handler and the Tools menu.
    pub fn set_sort_order(self: &Rc<Grid>, order: SortOrder) {
        if self.sort_order.get() == order {
            return;
        }
        self.sort_order.set(order);
        let _ = self
            .lib
            .set_setting(super::prefs::KEY_SORT_ORDER, order.as_setting());
        // Keep the header dropdown in sync without re-entering this handler.
        if self.sort_dropdown.selected() != order.dropdown_index() {
            self.sort_dropdown.set_selected(order.dropdown_index());
        }
        self.reload_from_source();
    }

    /// The current sort order (so the Tools menu can show a check mark).
    pub fn sort_order_setting(&self) -> &'static str {
        self.sort_order.get().as_setting()
    }

    /// Toggle the filename caption under each thumbnail. Persists the choice and
    /// rebuilds so every cell re-binds with the new visibility.
    pub fn set_show_filenames(&self, show: bool) {
        self.show_filenames.set(show);
        let _ = self.lib.set_setting(
            super::prefs::KEY_SHOW_FILENAMES,
            if show { "1" } else { "0" },
        );
        self.rebuild();
    }

    /// Whether the filename caption is currently shown.
    pub fn show_filenames(&self) -> bool {
        self.show_filenames.get()
    }

    /// Change the active thumbnail size and rebuild (new factory + jobs).
    pub fn set_thumb_size(self: &Rc<Grid>, size: i32) {
        self.thumb_size.set(size);
        let factory = build_factory(size, Rc::downgrade(self));
        self.grid_view.set_factory(Some(&factory));
        self.rebuild();
    }

    /// Rebuild the current folder's view (e.g. after a rotation invalidation).
    pub fn refresh_current(&self) {
        self.rebuild();
    }

    /// Drop the in-memory texture cache (after clearing the on-disk cache).
    pub fn clear_texture_cache(&self) {
        self.tex_cache.borrow_mut().clear();
    }

    /// Rebuild the store and re-enqueue thumbnail jobs for the filtered set.
    ///
    /// To avoid the "thumbnails flash away and come back" seen when a background
    /// scan/enrich reloads the same folder every few seconds, this preserves
    /// already-shown thumbnails across the rebuild: it snapshots each visible
    /// cell's texture by the file path (a stable key that does not change when a
    /// photo gains its content hash during enrichment) and re-seeds the new
    /// objects from that snapshot. A worker job is only enqueued for cells that
    /// have no texture yet, so a folder that is already thumbnailed does not
    /// re-decode on every scan tick.
    fn rebuild(&self) {
        let photos = self.filtered_photos();
        // Bump the generation so any in-flight jobs from the previous view are
        // dropped by the workers. New jobs are enqueued on demand by the factory
        // `bind` as cells scroll into view.
        let _gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;

        // Snapshot currently-shown textures by stable path key before wiping.
        let mut prev_tex: HashMap<String, gdk::Texture> = HashMap::new();
        for i in 0..self.store.n_items() {
            if let Some(obj) = self.store.item(i).and_downcast::<PhotoObject>() {
                if let Some(tex) = obj.texture() {
                    prev_tex.insert(obj.path(), tex);
                }
            }
        }

        self.store.remove_all();
        self.pending.borrow_mut().clear();

        let size = self.thumb_size.get();
        let mut objs = Vec::with_capacity(photos.len());
        for p in &photos {
            let obj = PhotoObject::from_photo(p);
            // Carry the previously shown thumbnail over so the cell never blanks
            // during a background reload.
            if let Some(tex) = prev_tex.get(&p.path) {
                obj.set_texture(Some(tex.clone()));
            }
            self.store.append(&obj);
            objs.push(obj);
        }
        self.header
            .set_text(&format!("{}  ({})", self.title.borrow(), photos.len()));

        for (p, obj) in photos.iter().zip(objs) {
            let edit = self.lib.photo_edit(p.id).unwrap_or_default();
            let key = cell_key(p, size, &edit);
            // Serve from the in-memory texture cache when available, skipping a
            // worker job and JPEG decode entirely. Cells that are not cached are
            // rendered on demand by the factory `bind` when they scroll into
            // view, so a huge folder no longer enqueues every photo up front.
            if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
                obj.set_texture(Some(texture));
            }
        }
    }
}

/// The cache/recycle key for a photo at a given size. Encodes hash (or path),
/// size, orientation, and edit revision so a size, rotation, or edit change
/// never matches a stale cell.
fn cell_key(p: &Photo, size: i32, edit: &crate::model::PhotoEdit) -> String {
    let base = if p.hash.is_empty() { &p.path } else { &p.hash };
    format!("{base}|{size}|{}|{}", p.orientation, edit.edit_rev)
}

/// Build the recycled cell factory: an `Overlay` of a fallback `Label` under an
/// `Image`. The image observes the bound `PhotoObject.texture` property; the
/// label shows the filename until a texture arrives.
fn build_factory(thumb_size: i32, grid: std::rc::Weak<Grid>) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let grid_unbind = grid.clone();
    let grid_setup = grid.clone();
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

        // The face-box overlay: a transparent DrawingArea stacked on top of the
        // image. Purely visual (no click handling), drawn only when
        // `show_faces` is on. Reads the bound photo's boxes from the cell's
        // "photo-id" data (set in `connect_bind`) via the grid's `face_boxes`
        // map, so no per-cell signal wiring is needed.
        let face_area = DrawingArea::new();
        face_area.set_size_request(thumb_size, thumb_size);
        face_area.set_can_target(false);
        overlay.add_overlay(&face_area);
        // Register this cell's DrawingArea so a later toggle can queue a
        // redraw on it directly (see `face_areas`'s doc comment).
        if let Some(grid) = grid_setup.upgrade() {
            grid.face_areas.borrow_mut().push(face_area.downgrade());
        }
        let image_weak = image.downgrade();
        let grid_for_draw = grid_setup.clone();
        face_area.set_draw_func(move |area, cr, w, h| {
            let Some(grid) = grid_for_draw.upgrade() else {
                return;
            };
            if !grid.show_faces.get() {
                return;
            }
            let Some(image) = image_weak.upgrade() else {
                return;
            };
            let photo_id: i64 =
                unsafe { area.data::<i64>("photo-id").map(|p| *p.as_ref()).unwrap_or(0) };
            let boxes = grid.face_boxes.borrow();
            let Some(rects) = boxes.get(&photo_id) else {
                return;
            };
            let Some((ix, iy, iw, ih)) = image_rect(&image, w, h) else {
                return;
            };
            // The face this view is scoped to (if any) draws thicker, so it
            // stands out from any other, unrelated face also in the shot.
            let (hl_person, hl_cluster) = if grid.is_style_source() {
                (grid.current_character().unwrap_or(0), grid.current_style_cluster().unwrap_or(0))
            } else {
                (grid.current_person().unwrap_or(0), grid.current_cluster().unwrap_or(0))
            };
            for r in rects.iter() {
                if r.assigned {
                    cr.set_source_rgba(0.3, 0.9, 0.4, 0.95);
                } else {
                    cr.set_source_rgba(1.0, 0.85, 0.2, 0.95);
                }
                let is_active = (hl_person != 0 && r.person_id == hl_person)
                    || (hl_cluster != 0 && r.person_id == 0 && r.cluster_id == hl_cluster);
                cr.set_line_width(if is_active { 4.0 } else { 2.0 });
                let rx = ix + iw * r.x as f64 / 1000.0;
                let ry = iy + ih * r.y as f64 / 1000.0;
                let rw = iw * r.w as f64 / 1000.0;
                let rh = ih * r.h as f64 / 1000.0;
                let _ = cr.rectangle(rx, ry, rw, rh);
                let _ = cr.stroke();
            }
        });

        // A vertical cell: the thumbnail overlay on top and an optional filename
        // caption below. The caption is hidden unless "Show filenames" is on.
        let cell = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        cell.append(&overlay);
        let caption = Label::new(None);
        caption.set_wrap(true);
        caption.set_max_width_chars(1);
        caption.set_justify(gtk4::Justification::Center);
        caption.set_halign(Align::Center);
        caption.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        caption.add_css_class("caption");
        caption.set_visible(false);
        cell.append(&caption);

        item.set_child(Some(&cell));
    });
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        let Some(photo) = item.item().and_downcast::<PhotoObject>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(overlay) = cell.first_child().and_downcast::<Overlay>() else {
            return;
        };
        let caption = cell
            .first_child()
            .and_then(|c| c.next_sibling())
            .and_downcast::<Label>();
        let (image, label) = overlay_parts(&overlay);
        label.set_text(&photo.filename());

        // The face-box overlay (third overlay child, added after the image in
        // `connect_setup`): tag it with this cell's photo id and repaint, so a
        // rebind (recycled cell scrolled to a new photo) never shows the
        // previous photo's boxes.
        if let Some(face_area) = image.next_sibling().and_downcast::<DrawingArea>() {
            unsafe {
                face_area.set_data("photo-id", photo.id());
            }
            face_area.queue_draw();
        }

        // Filename caption (shown only when the setting is on) and a filename
        // tooltip on every cell.
        let show_names = grid.upgrade().map(|g| g.show_filenames.get()).unwrap_or(false);
        if let Some(caption) = &caption {
            caption.set_text(&photo.filename());
            caption.set_visible(show_names);
        }
        overlay.set_tooltip_text(Some(&photo.filename()));

        // Dim the cell when the underlying file is missing from disk.
        if photo.missing() {
            overlay.add_css_class("dim-label");
            overlay.set_tooltip_text(Some("File missing from disk"));
        } else {
            overlay.remove_css_class("dim-label");
        }

        // Show the current texture (if already decoded) and update the label.
        apply_texture(&image, &label, photo.texture());

        // Demand-driven thumbnail: enqueue a job only now that this cell is
        // realised (visible range + GridView overscan). When the cell scrolls
        // away, unbind marks the job stale so the worker pool drops it.
        if let Some(grid) = grid.upgrade() {
            grid.ensure_thumb_for(&photo);
            // Viewport enrichment: an on-screen photo with no hash is not yet
            // enriched, so request enrichment for just this cell. Enrichment
            // never runs library-wide on its own; it follows what is viewed.
            if photo.hash().is_empty() && !photo.path().is_empty() {
                grid.request_enrich(photo.id());
            }
        }

        // Observe future texture changes for this bound object.
        let image_weak = image.downgrade();
        let label_weak = label.downgrade();
        let handler =
            photo.connect_notify_local(Some("texture"), move |obj: &PhotoObject, _pspec| {
                if let (Some(image), Some(label)) = (image_weak.upgrade(), label_weak.upgrade()) {
                    apply_texture(&image, &label, obj.texture());
                }
            });
        // Store the handler id so unbind can disconnect it.
        unsafe {
            item.set_data("texture-handler", handler);
        }
    });
    factory.connect_unbind(move |_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        if let Some(photo) = item.item().and_downcast::<PhotoObject>() {
            // Cell scrolled out of view: drop any in-flight job for it so the
            // worker result is discarded and the viewport keeps priority. A
            // re-bind re-enqueues if still needed. Cells that already hold a
            // texture keep it (the object retains the texture across unbind).
            if let Some(grid) = grid_unbind.upgrade() {
                if photo.texture().is_none() {
                    grid.drop_pending_for(&photo);
                }
                // Also drop a not-yet-flushed enrich request for this cell.
                grid.drop_enrich_request(photo.id());
            }
            unsafe {
                if let Some(handler) = item.steal_data::<glib::SignalHandlerId>("texture-handler") {
                    photo.disconnect(handler);
                }
            }
        }
    });
    factory
}

/// Format a byte count as a short human string.
fn human_size(bytes: i64) -> String {
    let b = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
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

/// The displayed rect of `image`'s texture inside a `(w, h)` area, honouring
/// aspect-preserving centering (mirrors `Viewer::image_rect`). Used to map a
/// face box's per-mille coordinates onto the letterboxed thumbnail.
fn image_rect(image: &Image, w: i32, h: i32) -> Option<(f64, f64, f64, f64)> {
    let paintable = image.paintable()?;
    let iw = paintable.intrinsic_width() as f64;
    let ih = paintable.intrinsic_height() as f64;
    if iw <= 0.0 || ih <= 0.0 {
        return None;
    }
    let (aw, ah) = (w as f64, h as f64);
    let scale = (aw / iw).min(ah / ih);
    let (dw, dh) = (iw * scale, ih * scale);
    Some(((aw - dw) / 2.0, (ah - dh) / 2.0, dw, dh))
}

/// Decode an image blob into a `gdk::Texture`.
///
/// Tries GTK's `PixbufLoader` first (fast for JPEG/PNG). Immich thumbnails are
/// often WebP, which many GTK builds cannot load, so on failure the `image`
/// crate decodes the bytes and the result is uploaded as an RGBA memory
/// texture.
fn decode_texture(blob: &[u8]) -> Option<gdk::Texture> {
    let loader = PixbufLoader::new();
    if loader.write(blob).is_ok() && loader.close().is_ok() {
        if let Some(pixbuf) = loader.pixbuf() {
            return Some(gdk::Texture::for_pixbuf(&pixbuf));
        }
    }
    // Fallback: decode with the `image` crate (supports WebP) and build a
    // memory texture from raw RGBA bytes.
    let img = image::load_from_memory(blob).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let bytes = glib::Bytes::from_owned(rgba.into_raw());
    let texture = gdk::MemoryTexture::new(
        w,
        h,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        (w * 4) as usize,
    );
    Some(texture.upcast())
}
