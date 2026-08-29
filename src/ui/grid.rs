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
    Align, Button, GridView, Image, Label, ListItem, MultiSelection, Overlay, PolicyType,
    ScrolledWindow, SignalListItemFactory,
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
    /// The header dropdown that selects the sort order.
    sort_dropdown: gtk4::DropDown,
    /// Called with (photos, index) when a cell is activated (double-clicked).
    on_activate: RefCell<Option<Box<dyn Fn(Vec<Photo>, usize)>>>,
    /// Called with a photo when the selection changes (single click).
    on_select: RefCell<Option<Box<dyn Fn(Photo)>>>,
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
enum SortOrder {
    /// Capture time first, then filename. Matches the database query order.
    Date,
    /// Filename only, case-insensitive.
    Filename,
}

impl SortOrder {
    /// Parse a stored setting value. Unknown values fall back to `Date`.
    fn from_setting(v: &str) -> SortOrder {
        match v {
            "filename" => SortOrder::Filename,
            _ => SortOrder::Date,
        }
    }

    /// The setting value string for this order.
    fn as_setting(self) -> &'static str {
        match self {
            SortOrder::Date => "date",
            SortOrder::Filename => "filename",
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
        let sort_dropdown =
            gtk4::DropDown::from_strings(&["Date", "Name"]);
        sort_dropdown.set_selected(match sort_order {
            SortOrder::Date => 0,
            SortOrder::Filename => 1,
        });
        sort_dropdown.set_margin_end(6);
        let sort_label = Label::new(Some("Sort:"));
        sort_label.set_margin_start(6);
        header_box.append(&sort_label);
        header_box.append(&sort_dropdown);

        // The duplicate-results action bar. Hidden unless a duplicate view is
        // shown. It holds a hint label and a "Delete marked" button.
        let dup_label = Label::new(None);
        dup_label.set_xalign(0.0);
        dup_label.set_hexpand(true);
        let dup_delete_btn = Button::with_label("Delete marked");
        dup_delete_btn.add_css_class("destructive-action");
        let dup_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        dup_bar.add_css_class("toolbar");
        dup_bar.set_margin_start(6);
        dup_bar.set_margin_end(6);
        dup_bar.set_margin_top(4);
        dup_bar.set_margin_bottom(4);
        dup_bar.append(&dup_label);
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

        let factory = build_factory(thumb_size);
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
            sort_dropdown,
            on_activate: RefCell::new(None),
            on_select: RefCell::new(None),
            on_context_menu: RefCell::new(None),
            dup_mode: std::cell::Cell::new(false),
            dup_bar,
            dup_label,
            dup_delete_btn,
            on_dup_delete: RefCell::new(None),
            scroller,
            dup_container,
            dup_state: RefCell::new(Vec::new()),
        }
        .into_rc()
    }

    fn into_rc(self) -> Rc<Grid> {
        let rc = Rc::new(self);
        // The sort dropdown re-orders the current photos and persists the choice.
        {
            let rc2 = rc.clone();
            rc.sort_dropdown.connect_selected_notify(move |dd| {
                let order = match dd.selected() {
                    1 => SortOrder::Filename,
                    _ => SortOrder::Date,
                };
                rc2.sort_order.set(order);
                let _ = rc2
                    .lib
                    .set_setting(super::prefs::KEY_SORT_ORDER, order.as_setting());
                rc2.reload_from_source();
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
        rc
    }

    /// Register the activation callback (opens the viewer).
    pub fn set_on_activate<F: Fn(Vec<Photo>, usize) + 'static>(&self, f: F) {
        *self.on_activate.borrow_mut() = Some(Box::new(f));
    }

    /// Register the selection callback (updates properties).
    pub fn set_on_select<F: Fn(Photo) + 'static>(&self, f: F) {
        *self.on_select.borrow_mut() = Some(Box::new(f));
    }

    /// Register the right-click context-menu callback.
    pub fn set_on_context_menu<F: Fn(f64, f64) + 'static>(&self, f: F) {
        *self.on_context_menu.borrow_mut() = Some(Box::new(f));
    }

    /// Register the "Delete marked" callback for duplicate mode.
    pub fn set_on_dup_delete<F: Fn(Vec<Photo>) + 'static>(&self, f: F) {
        *self.on_dup_delete.borrow_mut() = Some(Box::new(f));
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
        self.set_photos(name, photos);
    }

    /// Show every photo in an unnamed face cluster, remembering the cluster as
    /// the source so the grid can re-query after a new scan.
    pub fn show_cluster(&self, cluster_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Cluster(cluster_id, name.to_string());
        let photos = self.lib.photos_in_cluster(cluster_id).unwrap_or_default();
        self.set_photos(name, photos);
    }

    /// Show every photo that contains a given stylised character.
    pub fn show_character(&self, character_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::Character(character_id, name.to_string());
        let photos = self.lib.photos_of_character(character_id).unwrap_or_default();
        self.set_photos(name, photos);
    }

    /// Show every photo in an unnamed stylised face cluster.
    pub fn show_style_cluster(&self, cluster_id: i64, name: &str) {
        *self.source.borrow_mut() = Source::StyleCluster(cluster_id, name.to_string());
        let photos = self
            .lib
            .photos_in_style_cluster(cluster_id)
            .unwrap_or_default();
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
            SortOrder::Date => {
                photos.sort_by(|a, b| {
                    a.taken_at
                        .cmp(&b.taken_at)
                        .then_with(|| a.filename.cmp(&b.filename))
                });
            }
            SortOrder::Filename => {
                photos.sort_by(|a, b| {
                    a.filename
                        .to_lowercase()
                        .cmp(&b.filename.to_lowercase())
                });
            }
        }
    }

    fn set_photos(&self, title: &str, photos: Vec<Photo>) {
        // Default to no back button. The person and cluster views re-enable it
        // right after, via `set_back`.
        self.hide_back();
        self.exit_dup_mode();
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
        let gen = self.generation.load(Ordering::Relaxed);
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
            if self.pending.borrow().contains_key(&key) {
                continue; // already queued
            }
            self.enqueue_thumb(p, &obj, &key, edit, gen);
        }
    }

    /// Enqueue a thumbnail job for one cell (local or Immich).
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

    /// Change the active thumbnail size and rebuild (new factory + jobs).
    pub fn set_thumb_size(&self, size: i32) {
        self.thumb_size.set(size);
        let factory = build_factory(size);
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
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;

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
            // worker job and JPEG decode entirely.
            if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
                obj.set_texture(Some(texture));
                continue;
            }
            // Already showing a carried-over thumbnail for this exact cell key
            // (nothing changed): no need to re-render.
            if obj.texture().is_some() && prev_tex.contains_key(&p.path) {
                continue;
            }
            self.pending.borrow_mut().insert(key.clone(), obj);
            if let Some((server_id, asset_id)) = parse_immich_path(&p.path) {
                let _ = self.immich_jobs.send(ImmichJob {
                    key,
                    server_id,
                    asset_id,
                    generation: gen,
                });
                continue;
            }
            let _ = self.jobs.send(Job {
                key,
                hash: p.hash.clone(),
                path: p.path.clone(),
                orientation: p.orientation,
                edit,
                generation: gen,
            });
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

        // Dim the cell when the underlying file is missing from disk.
        if photo.missing() {
            overlay.add_css_class("dim-label");
            overlay.set_tooltip_text(Some("File missing from disk"));
        } else {
            overlay.remove_css_class("dim-label");
            overlay.set_tooltip_text(None);
        }

        // Show the current texture (if already decoded) and update the label.
        apply_texture(&image, &label, photo.texture());

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
    factory.connect_unbind(|_, item| {
        let item = item.downcast_ref::<ListItem>().unwrap();
        if let Some(photo) = item.item().and_downcast::<PhotoObject>() {
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
