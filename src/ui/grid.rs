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
    Align, GridView, Image, Label, ListItem, MultiSelection, Overlay, PolicyType, ScrolledWindow,
    SignalListItemFactory,
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
    /// Called with (photos, index) when a cell is activated (double-clicked).
    on_activate: RefCell<Option<Box<dyn Fn(Vec<Photo>, usize)>>>,
    /// Called with a photo when the selection changes (single click).
    on_select: RefCell<Option<Box<dyn Fn(Photo)>>>,
    /// Called with (x, y) in grid coordinates on a right-click, so the app can
    /// show a context menu over the current selection.
    on_context_menu: RefCell<Option<Box<dyn Fn(f64, f64)>>>,
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
    /// An Immich album (server id, album uuid, display name). Not re-queryable
    /// from the local database; a reload refetches over HTTP through the caller.
    Immich(i64, String, String),
}

impl Grid {
    /// Build the grid, starting the worker pool. `lib` supplies photo data;
    /// `gen` renders thumbnails. Both are shared with the workers.
    pub fn new(lib: Arc<Library>, gen: Arc<Generator>, thumb_size: i32) -> Rc<Grid> {
        let header = Label::new(None);
        header.set_xalign(0.0);
        header.set_margin_start(8);
        header.set_margin_top(6);
        header.set_margin_bottom(6);

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
                            eprintln!(
                                "[immich] thumb ok server={} asset={} bytes={}",
                                job.server_id,
                                job.asset_id,
                                blob.len()
                            );
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
                        Ok(_) => {
                            eprintln!(
                                "[immich] thumb EMPTY server={} asset={}",
                                job.server_id, job.asset_id
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "[immich] thumb ERROR server={} asset={}: {e}",
                                job.server_id, job.asset_id
                            );
                        }
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

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&scroller);

        // Apply finished thumbnails on the UI thread by setting the texture on
        // the matching PhotoObject; the bound Image updates automatically. The
        // decoded texture is also cached so re-entry skips decoding.
        let gen_for_apply = generation.clone();
        let pending_for_apply = pending.clone();
        let cache_for_apply = tex_cache.clone();
        done_rx.attach(None, move |done: Done| {
            let is_immich = done.key.starts_with("immich://");
            if done.generation == gen_for_apply.load(Ordering::Relaxed) {
                if let Some(obj) = pending_for_apply.borrow_mut().remove(&done.key) {
                    match decode_texture(&done.blob) {
                        Some(texture) => {
                            cache_for_apply
                                .borrow_mut()
                                .put(done.key.clone(), texture.clone());
                            obj.set_texture(Some(texture));
                        }
                        None => {
                            if is_immich {
                                eprintln!(
                                    "[immich] decode FAILED key={} bytes={}",
                                    done.key,
                                    done.blob.len()
                                );
                            }
                        }
                    }
                } else if is_immich {
                    eprintln!("[immich] no pending object for key={}", done.key);
                }
            } else if is_immich {
                eprintln!(
                    "[immich] dropped stale result key={} gen={} current={}",
                    done.key,
                    done.generation,
                    gen_for_apply.load(Ordering::Relaxed)
                );
            }
            glib::ControlFlow::Continue
        });

        Grid {
            root,
            header,
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
            on_activate: RefCell::new(None),
            on_select: RefCell::new(None),
            on_context_menu: RefCell::new(None),
        }
        .into_rc()
    }

    fn into_rc(self) -> Rc<Grid> {
        let rc = Rc::new(self);
        // Activation (double-click / Enter) opens the viewer.
        {
            let rc2 = rc.clone();
            rc.grid_view.connect_activate(move |_, pos| {
                let photos = rc2.filtered_photos();
                eprintln!(
                    "[immich] activate pos={pos} n={} path={:?}",
                    photos.len(),
                    photos.get(pos as usize).map(|p| p.path.clone())
                );
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

    /// The `GridView` widget, used as a menu anchor.
    pub fn grid_view(&self) -> &GridView {
        &self.grid_view
    }

    /// The grid's root widget.
    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// The active thumbnail size in pixels.
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

    /// Show an Immich album's assets. The caller passes the already-fetched
    /// photos (each with an `immich://<server_id>/<asset_id>` path). The grid
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

    /// The photos currently selected in the grid (multi-selection), in view
    /// order. Empty when nothing is selected.
    pub fn selected_photos(&self) -> Vec<Photo> {
        let photos = self.filtered_photos();
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

    /// Re-query the current source (folder or raw dir) from the database/disk
    /// and rebuild the view. A true "refresh visible": picks up newly scanned
    /// photos and updated orientations. No-op for ad-hoc lists.
    pub fn reload_from_source(&self) {
        let source = self.source.borrow().clone();
        match source {
            Source::Folder(id, name) => {
                let photos = self.lib.photos_in_folder(id).unwrap_or_default();
                self.set_photos(&name, photos);
            }
            Source::RawDir(dir) => {
                let (title, photos) = self.load_raw_dir(&dir);
                self.set_photos(&title, photos);
            }
            Source::VirtualAlbum(id, name) => {
                let photos = self.lib.photos_in_virtual_album(id).unwrap_or_default();
                self.set_photos(&name, photos);
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
    fn set_photos(&self, title: &str, photos: Vec<Photo>) {
        *self.all_photos.borrow_mut() = photos;
        *self.title.borrow_mut() = title.to_string();
        self.rebuild();
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
    fn rebuild(&self) {
        let photos = self.filtered_photos();
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.store.remove_all();
        self.pending.borrow_mut().clear();

        let size = self.thumb_size.get();
        let mut objs = Vec::with_capacity(photos.len());
        for p in &photos {
            let obj = PhotoObject::from_photo(p);
            self.store.append(&obj);
            objs.push(obj);
        }
        self.header
            .set_text(&format!("{}  ({})", self.title.borrow(), photos.len()));

        for (p, obj) in photos.iter().zip(objs) {
            let key = cell_key(p, size);
            // Serve from the in-memory texture cache when available, skipping a
            // worker job and JPEG decode entirely.
            if let Some(texture) = self.tex_cache.borrow_mut().get(&key) {
                obj.set_texture(Some(texture));
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
