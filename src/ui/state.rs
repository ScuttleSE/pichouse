//! Shared application state, held in an `Rc` so widgets and callbacks can reach
//! the library, thumbnail generator, preferences, and panels.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Stack};

use crate::ai;
use crate::db::Library;
use crate::thumb::Generator;

use super::controller::Controller;
use super::grid::Grid;
use super::prefs::Prefs;
use super::properties::Properties;
use super::shortcuts::Shortcuts;
use super::status::StatusBar;
use super::viewer::Viewer;

/// Wall-clock milliseconds since the Unix epoch. Used for the enrichment pause
/// deadline, shared with the background workers via an `AtomicU64`.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Everything shared across the UI. Panels are filled in after construction via
/// `OnceCell`-like `RefCell<Option<...>>` slots.
pub struct AppState {
    pub lib: Arc<Library>,
    pub gen: Arc<Generator>,
    pub window: RefCell<Option<ApplicationWindow>>,

    pub prefs: RefCell<Prefs>,
    pub ai_config: RefCell<ai::Config>,
    pub ai_manager: Arc<Mutex<ai::Manager>>,
    pub shortcuts: RefCell<Shortcuts>,

    pub scan: Controller,
    pub ai_job: Controller,
    /// The face-detection worker session (see `super::facescan`).
    pub face_job: Controller,
    /// The loaded face recognition configuration.
    pub face_config: RefCell<crate::face::FaceConfig>,
    /// The face-crop thumbnail cache, opened on first use.
    pub face_thumbs: RefCell<Option<Arc<crate::db::FaceThumbs>>>,
    /// The stylised-face-detection worker session (see `super::stylefacescan`).
    pub style_face_job: Controller,
    /// The loaded stylised face configuration.
    pub style_face_config: RefCell<crate::styleface::StyleFaceConfig>,
    /// The stylised-face-crop thumbnail cache, opened on first use.
    pub style_face_thumbs: RefCell<Option<Arc<crate::db::FaceThumbs>>>,
    /// The Phase 2 enrichment worker session (see `super::enrich`).
    pub enrich_job: Controller,
    /// The library-freshness reconciliation session (see `super::freshness`).
    pub reconcile_job: Controller,
    /// The Immich album upload session (see `super::immich`).
    pub immich_upload: Controller,
    /// The duplicate-finder scan session (see `super::dedup_scan`).
    pub dedup_job: Controller,
    /// Paths waiting to be scanned. A running scan thread drains this, so adding
    /// a folder while a scan runs appends to it instead of cancelling the scan.
    pub scan_queue: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// Photo ids waiting for Phase 2 enrichment. The enrichment worker pool
    /// drains this front-to-back; opening a folder front-loads its ids.
    pub enrich_queue: Arc<Mutex<std::collections::VecDeque<i64>>>,
    /// Wall-clock millis (since the Unix epoch) until which Phase 2 enrichment
    /// is paused. Set when the user interacts with the grid so background
    /// hashing/thumbnailing never competes with on-demand UI work on a slow
    /// disk. Enrichment workers sleep while `now < enrich_pause_until`.
    pub enrich_pause_until: Arc<std::sync::atomic::AtomicU64>,
    /// True while a "nice" bulk enrichment pass (Tools > Generate Thumbnails) is
    /// active. The enrichment worker sleeps a short slice between photos when
    /// this is set, so the full pass does not choke the system. Viewport
    /// enrichment leaves it false for prompt on-screen results.
    pub enrich_nice: Arc<std::sync::atomic::AtomicBool>,

    pub status: RefCell<Option<Rc<StatusBar>>>,
    pub grid: RefCell<Option<Rc<Grid>>>,
    pub new_files: RefCell<Option<Rc<super::newfiles::NewFilesView>>>,
    pub faces_view: RefCell<Option<Rc<super::facesview::FacesView>>>,
    pub characters_view: RefCell<Option<Rc<super::charactersview::CharactersView>>>,
    pub properties: RefCell<Option<Rc<Properties>>>,
    pub viewer: RefCell<Option<Rc<Viewer>>>,
    pub sidebar: RefCell<Option<Rc<super::sidebar::Sidebar>>>,
    pub folder_tree: RefCell<Option<Rc<super::foldertree::FolderTree>>>,
    pub center_stack: RefCell<Option<Stack>>,
    /// The Tools > Generate Thumbnails toggle action, so the enrichment worker
    /// can turn it off when the pass finishes on its own.
    pub gen_thumbs_action: RefCell<Option<gtk4::gio::SimpleAction>>,
    /// The id of the folder currently shown in the grid (0 = none / raw view).
    pub current_folder: RefCell<i64>,
    /// Cached Immich albums per server id, filled by a background refresh. The
    /// sidebar reads this cache. HTTP never runs on the GTK main thread.
    pub immich_albums: RefCell<std::collections::HashMap<i64, Vec<crate::model::ImmichAlbum>>>,
    /// The character id chosen in the last merge. Used to pre-select the merge
    /// dropdown next time. None until the first merge. Resets on restart.
    pub last_merged_character: RefCell<Option<i64>>,
    /// How many new photos the most recent face scan added to each existing
    /// People group. Cleared at the start of every scan, then filled in once
    /// clustering finishes, so the People view can show a "+N new" badge.
    pub face_group_new_counts: RefCell<std::collections::HashMap<crate::db::FaceGroup, i64>>,
}

/// One crop-render job. A worker renders the crop, writes it to the cache, and
/// sends the JPEG to the main thread through the job's sender. Shared by every
/// UI spot that shows a stylised face crop as a small image (the Characters
/// view's tiles, and the per-face "Assign to Character" dialog's cards).
pub struct CropJob {
    pub face_id: i64,
    pub path: std::path::PathBuf,
    pub orientation: i32,
    pub bbox: (i32, i32, i32, i32),
    pub thumbs: Option<Arc<crate::db::FaceThumbs>>,
    pub reply: gtk4::glib::Sender<Option<Vec<u8>>>,
}

/// A bounded worker pool for crop rendering. Opening a view with many
/// uncached crops (the Characters view during a scan, or a multi-face "Assign
/// to Character" dialog) can request several at once. A fixed pool of workers
/// reads a shared queue, so callers never spawn one thread per crop.
struct CropPool {
    queue: Mutex<std::collections::VecDeque<CropJob>>,
    cv: std::sync::Condvar,
}

static CROP_POOL: std::sync::OnceLock<Arc<CropPool>> = std::sync::OnceLock::new();

fn crop_pool() -> &'static Arc<CropPool> {
    CROP_POOL.get_or_init(|| {
        let pool = Arc::new(CropPool {
            queue: Mutex::new(std::collections::VecDeque::new()),
            cv: std::sync::Condvar::new(),
        });
        for _ in 0..4 {
            let pool = pool.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let mut q = pool.queue.lock().unwrap();
                    while q.is_empty() {
                        q = pool.cv.wait(q).unwrap();
                    }
                    q.pop_front().unwrap()
                };
                let jpeg =
                    crate::thumb::render_face_crop(&job.path, job.orientation, job.bbox, 320).ok();
                if let (Some(ft), Some(j)) = (job.thumbs.as_ref(), jpeg.as_ref()) {
                    let _ = ft.put(job.face_id, j);
                }
                let _ = job.reply.send(jpeg);
            });
        }
        pool
    })
}

/// Queue a crop-render job on the shared pool.
pub fn queue_crop_job(job: CropJob) {
    let pool = crop_pool();
    pool.queue.lock().unwrap().push_back(job);
    pool.cv.notify_one();
}

impl AppState {
    pub fn window(&self) -> Option<ApplicationWindow> {
        self.window.borrow().clone()
    }
    pub fn status(&self) -> Rc<StatusBar> {
        self.status.borrow().clone().expect("status set")
    }
    pub fn grid(&self) -> Rc<Grid> {
        self.grid.borrow().clone().expect("grid set")
    }
    pub fn new_files(&self) -> Rc<super::newfiles::NewFilesView> {
        self.new_files.borrow().clone().expect("new_files set")
    }
    pub fn faces_view(&self) -> Rc<super::facesview::FacesView> {
        self.faces_view.borrow().clone().expect("faces_view set")
    }
    pub fn characters_view(&self) -> Rc<super::charactersview::CharactersView> {
        self.characters_view
            .borrow()
            .clone()
            .expect("characters_view set")
    }
    pub fn properties(&self) -> Rc<Properties> {
        self.properties.borrow().clone().expect("properties set")
    }
    pub fn viewer(&self) -> Rc<Viewer> {
        self.viewer.borrow().clone().expect("viewer set")
    }

    /// A clone of the shared AI manager handle for background workers.
    pub fn ai_manager_arc(&self) -> Arc<Mutex<ai::Manager>> {
        self.ai_manager.clone()
    }

    /// The face-crop cache, opened on first use.
    pub fn face_thumbs(&self) -> Option<Arc<crate::db::FaceThumbs>> {
        if let Some(ft) = self.face_thumbs.borrow().as_ref() {
            return Some(ft.clone());
        }
        match crate::db::FaceThumbs::open() {
            Ok(ft) => {
                let ft = Arc::new(ft);
                *self.face_thumbs.borrow_mut() = Some(ft.clone());
                Some(ft)
            }
            Err(e) => {
                log::warn!("open face thumbs: {e}");
                None
            }
        }
    }

    /// The cached (or freshly rendered) JPEG crop for a face. It renders on a
    /// cache miss and stores the result. Returns `None` on any error.
    pub fn face_crop_jpeg(&self, face_id: i64) -> Option<Vec<u8>> {
        let ft = self.face_thumbs()?;
        if let Ok(Some(jpeg)) = ft.get(face_id) {
            return Some(jpeg);
        }
        // Load the face and its photo, then render the crop.
        let face = self.lib.face_by_id(face_id).ok().flatten()?;
        let photo = self.lib.photo_by_id(face.photo_id).ok().flatten()?;
        let jpeg = crate::thumb::render_face_crop(
            std::path::Path::new(&photo.path),
            photo.orientation,
            (face.bbox_x, face.bbox_y, face.bbox_w, face.bbox_h),
            // Render at a generous size so tiles stay crisp up to the largest
            // thumbnail-slider size. The Image widget scales down as needed.
            320,
        )
        .ok()?;
        let _ = ft.put(face_id, &jpeg);
        Some(jpeg)
    }

    /// The stylised-face-crop cache, opened on first use.
    pub fn style_face_thumbs(&self) -> Option<Arc<crate::db::FaceThumbs>> {
        if let Some(ft) = self.style_face_thumbs.borrow().as_ref() {
            return Some(ft.clone());
        }
        match crate::db::open_style_face_thumbs() {
            Ok(ft) => {
                let ft = Arc::new(ft);
                *self.style_face_thumbs.borrow_mut() = Some(ft.clone());
                Some(ft)
            }
            Err(e) => {
                log::warn!("open style face thumbs: {e}");
                None
            }
        }
    }

    /// The cached (or freshly rendered) JPEG crop for a stylised face.
    #[allow(dead_code)]
    pub fn style_face_crop_jpeg(&self, face_id: i64) -> Option<Vec<u8>> {
        let ft = self.style_face_thumbs()?;
        if let Ok(Some(jpeg)) = ft.get(face_id) {
            return Some(jpeg);
        }
        let face = self.lib.style_face_by_id(face_id).ok().flatten()?;
        let photo = self.lib.photo_by_id(face.photo_id).ok().flatten()?;
        let jpeg = crate::thumb::render_face_crop(
            std::path::Path::new(&photo.path),
            photo.orientation,
            (face.bbox_x, face.bbox_y, face.bbox_w, face.bbox_h),
            320,
        )
        .ok()?;
        let _ = ft.put(face_id, &jpeg);
        Some(jpeg)
    }

    /// Return a cached stylised-face crop only. Does not render a missing crop.
    /// This is cheap and safe to call for many tiles on the main thread.
    pub fn style_face_crop_cached(&self, face_id: i64) -> Option<Vec<u8>> {
        let ft = self.style_face_thumbs()?;
        ft.get(face_id).ok().flatten()
    }

    /// The inputs needed to render a stylised-face crop off the main thread:
    /// the source path, the orientation, and the bounding box. Cheap DB reads.
    pub fn style_face_crop_inputs(
        &self,
        face_id: i64,
    ) -> Option<(std::path::PathBuf, i32, (i32, i32, i32, i32))> {
        let face = self.lib.style_face_by_id(face_id).ok().flatten()?;
        let photo = self.lib.photo_by_id(face.photo_id).ok().flatten()?;
        Some((
            std::path::PathBuf::from(photo.path),
            photo.orientation,
            (face.bbox_x, face.bbox_y, face.bbox_w, face.bbox_h),
        ))
    }

    /// Pause background Phase 2 enrichment for `secs` seconds from now, so the
    /// UI (folder open, scrolling, thumbnail fetches) gets the disk to itself.
    /// Called on grid interaction. Extending an existing pause simply pushes the
    /// resume time further out.
    #[allow(dead_code)] // Kept for callers that pause enrichment explicitly.
    pub fn pause_enrichment(&self, secs: u64) {
        use std::sync::atomic::Ordering;
        let until = now_millis().saturating_add(secs.saturating_mul(1000));
        // Only ever push the deadline later, never earlier.
        let cur = self.enrich_pause_until.load(Ordering::Relaxed);
        if until > cur {
            self.enrich_pause_until.store(until, Ordering::Relaxed);
            log::debug!("enrichment/scan paused for {secs}s (browsing)");
        }
    }

    /// A clone of the shared scan queue handle for the scan worker.
    pub fn scan_queue_arc(&self) -> Arc<Mutex<std::collections::VecDeque<String>>> {
        self.scan_queue.clone()
    }

    /// Push the active thumbnail preferences into the generator.
    pub fn apply_thumb_prefs(&self) {
        let prefs = self.prefs.borrow();
        self.gen.set_size(prefs.active_size());
        if prefs.save_all_sizes {
            self.gen.set_all_sizes(&prefs.sizes);
        } else {
            self.gen.set_all_sizes(&[]);
        }
    }

    /// Show the full-image viewer for a photo set at the given index.
    pub fn open_viewer(&self, photos: Vec<crate::model::Photo>, index: usize) {
        self.viewer().open(photos, index);
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("viewer");
        }
    }

    /// Return from the viewer to the grid.
    pub fn close_viewer(&self) {
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("grid");
        }
    }

    /// Show the grouped "New Files" view in the center, rebuilding it from the
    /// current database state.
    pub fn show_new_files(self: &Rc<Self>) {
        *self.current_folder.borrow_mut() = 0;
        let groups = self
            .lib
            .new_photos_grouped(self.prefs.borrow().new_max_age_secs())
            .unwrap_or_default();
        let count: usize = groups.iter().map(|(_, ps)| ps.len()).sum();
        self.new_files().show_groups(groups);
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("newfiles");
        }
        self.status()
            .set_message(&format!("New Files — {count} recently added"));
    }

    /// Show every photo currently marked missing (gone from disk) in the grid.
    pub fn show_missing_files(self: &Rc<Self>) {
        *self.current_folder.borrow_mut() = 0;
        let photos = self.lib.photos_missing().unwrap_or_default();
        let n = photos.len();
        self.grid().show_photos("Missing Files", &photos);
        self.show_grid();
        self.status()
            .set_message(&format!("Missing Files — {n} gone from disk"));
    }

    /// Show the photos involved in banned duplicate matches. Each banned pair
    /// contributes both photos. The user un-bans everything with the section's
    /// right-click "Clear Banned Matches…" action.
    pub fn show_banned_matches(self: &Rc<Self>) {
        *self.current_folder.borrow_mut() = 0;
        let pairs = self.lib.banned_dup_photo_pairs().unwrap_or_default();
        // Flatten to a de-duplicated photo list, keeping pair order.
        let mut seen = std::collections::HashSet::new();
        let mut photos = Vec::new();
        for (a, b) in &pairs {
            for p in [a, b] {
                if seen.insert(p.id) {
                    photos.push(p.clone());
                }
            }
        }
        let n = pairs.len();
        self.grid().show_photos("Banned Matches", &photos);
        self.show_grid();
        self.status()
            .set_message(&format!("Banned Matches — {n} pairs will not group again"));
    }

    /// Show the top-level Faces view in the center, rebuilding its group tiles.
    pub fn show_faces(self: &Rc<Self>) {
        *self.current_folder.borrow_mut() = 0;
        self.faces_view().show_top();
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("faces");
        }
        self.status().set_message("People");
    }

    /// Return to the Faces view without resetting its scope, so leaving a
    /// person's photos lands back on the group page (if any) they were opened
    /// from, not always the top-level People page.
    fn back_to_faces(self: &Rc<Self>) {
        self.faces_view().reload();
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("faces");
        }
        self.status().set_message("People");
    }

    /// Rebuild the Faces view if it is the visible center child. Used by the
    /// scan to show new groups progressively.
    pub fn refresh_faces_if_active(self: &Rc<Self>) {
        let active = self
            .center_stack
            .borrow()
            .as_ref()
            .and_then(|s| s.visible_child_name())
            .map(|n| n == "faces")
            .unwrap_or(false);
        if active {
            self.faces_view().reload();
        }
    }

    /// Show the normal thumbnail grid in the center.
    pub fn show_grid(&self) {
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("grid");
        }
    }

    /// Load a virtual album's photos into the grid and show it.
    pub fn show_virtual_album(self: &Rc<Self>, album_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.grid().show_virtual_album(album_id, name);
        let count = self.lib.virtual_album_photo_count(album_id).unwrap_or(0);
        self.show_grid();
        self.status()
            .set_message(&format!("{name} — {count} photos"));
    }

    /// Show every photo that contains a given person.
    pub fn show_person(self: &Rc<Self>, person_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.grid().show_person(person_id, name);
        let count = self.lib.person_face_count(person_id).unwrap_or(0);
        {
            let this = self.clone();
            self.grid().set_back(move || this.back_to_faces());
        }
        self.show_grid();
        self.status()
            .set_message(&format!("{name} — {count} faces"));
    }

    /// Show a person group's page in the Faces view: its sub-groups and
    /// member persons, the same way opening an Album shows its folders rather
    /// than a merged photo grid.
    pub fn show_person_group(self: &Rc<Self>, group_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.faces_view().show_group(group_id);
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("faces");
        }
        self.status().set_message(name);
    }

    /// Show every photo in an unnamed face cluster, with a back button to the
    /// Faces view.
    pub fn show_cluster(self: &Rc<Self>, cluster_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.grid().show_cluster(cluster_id, name);
        {
            let this = self.clone();
            self.grid().set_back(move || this.back_to_faces());
        }
        self.show_grid();
        self.status().set_message(name);
    }

    /// Show the Characters view in the center, rebuilding its group tiles.
    pub fn show_characters(self: &Rc<Self>) {
        *self.current_folder.borrow_mut() = 0;
        self.characters_view().show_top();
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("characters");
        }
        self.status().set_message("Characters");
    }

    /// Return to the Characters view without resetting its scope, so leaving
    /// a character's photos lands back on the group page (if any) they were
    /// opened from, not always the top-level Characters page.
    fn back_to_characters(self: &Rc<Self>) {
        self.characters_view().refresh();
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("characters");
        }
        self.status().set_message("Characters");
    }

    /// Rebuild the Characters view if it is the visible center child.
    pub fn refresh_characters_if_active(self: &Rc<Self>) {
        let active = self
            .center_stack
            .borrow()
            .as_ref()
            .and_then(|s| s.visible_child_name())
            .map(|n| n == "characters")
            .unwrap_or(false);
        if active {
            self.characters_view().refresh();
        }
    }

    /// Show every photo that contains a given character.
    pub fn show_character(self: &Rc<Self>, character_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.grid().show_character(character_id, name);
        let count = self.lib.character_face_count(character_id).unwrap_or(0);
        {
            let this = self.clone();
            self.grid().set_back(move || this.back_to_characters());
        }
        self.show_grid();
        self.status()
            .set_message(&format!("{name} — {count} faces"));
    }

    /// Show a character group's page in the Characters view: its sub-groups
    /// and member characters, the same way opening an Album shows its
    /// folders rather than a merged photo grid.
    pub fn show_character_group(self: &Rc<Self>, group_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.characters_view().show_group(group_id);
        if let Some(stack) = self.center_stack.borrow().as_ref() {
            stack.set_visible_child_name("characters");
        }
        self.status().set_message(name);
    }

    /// Show every photo in an unnamed stylised cluster, with a back button.
    pub fn show_style_cluster(self: &Rc<Self>, cluster_id: i64, name: &str) {
        *self.current_folder.borrow_mut() = 0;
        self.grid().show_style_cluster(cluster_id, name);
        {
            let this = self.clone();
            self.grid().set_back(move || this.back_to_characters());
        }
        self.show_grid();
        self.status().set_message(name);
    }

    /// Clear the grid if the folder it is showing no longer exists (e.g. after
    /// the owning library folder was removed). Resets the current folder and
    /// empties the grid so stale, now-deleted thumbnails cannot be opened.
    pub fn clear_grid_if_folder_gone(&self) {
        let current = *self.current_folder.borrow();
        if current == 0 {
            return;
        }
        let exists = self
            .lib
            .folders()
            .map(|fs| fs.iter().any(|f| f.id == current))
            .unwrap_or(false);
        if !exists {
            *self.current_folder.borrow_mut() = 0;
            self.grid().show_photos("", &[]);
            self.show_grid();
        }
    }

    /// Whether the viewer is the visible center child.
    pub fn viewer_active(&self) -> bool {
        self.center_stack
            .borrow()
            .as_ref()
            .map(|s| {
                s.visible_child_name()
                    .map(|n| n == "viewer")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Whether the New Files view is the visible center child.
    pub fn new_files_active(&self) -> bool {
        self.center_stack
            .borrow()
            .as_ref()
            .map(|s| {
                s.visible_child_name()
                    .map(|n| n == "newfiles")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// If the New Files view is showing, rebuild it from the database (used
    /// after a reconcile/enrichment lands new files).
    pub fn refresh_new_files_if_active(self: &Rc<Self>) {
        if self.new_files_active() {
            let groups = self
                .lib
                .new_photos_grouped(self.prefs.borrow().new_max_age_secs())
                .unwrap_or_default();
            self.new_files().show_groups(groups);
        }
    }
}

/// A message dialog helper (error/info) parented on the main window.
pub fn show_message(state: &Rc<AppState>, title: &str, detail: &str) {
    use super::util::escape_markup;
    let dialog = gtk4::MessageDialog::builder()
        .modal(true)
        .message_type(gtk4::MessageType::Info)
        .buttons(gtk4::ButtonsType::Close)
        .build();
    if let Some(win) = state.window() {
        dialog.set_transient_for(Some(&win));
    }
    dialog.set_title(Some(title));
    dialog.set_markup(&format!(
        "<b>{}</b>\n{}",
        escape_markup(title),
        escape_markup(detail)
    ));
    dialog.connect_response(|d, _| d.destroy());
    dialog.set_visible(true);
}

/// Show an error dialog.
pub fn show_error(state: &Rc<AppState>, msg: &str) {
    show_message(state, "Error", msg);
}
