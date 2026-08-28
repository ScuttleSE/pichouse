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
    /// The Phase 2 enrichment worker session (see `super::enrich`).
    pub enrich_job: Controller,
    /// The library-freshness reconciliation session (see `super::freshness`).
    pub reconcile_job: Controller,
    /// The Immich album upload session (see `super::immich`).
    pub immich_upload: Controller,
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

    pub status: RefCell<Option<Rc<StatusBar>>>,
    pub grid: RefCell<Option<Rc<Grid>>>,
    pub new_files: RefCell<Option<Rc<super::newfiles::NewFilesView>>>,
    pub properties: RefCell<Option<Rc<Properties>>>,
    pub viewer: RefCell<Option<Rc<Viewer>>>,
    pub sidebar: RefCell<Option<Rc<super::sidebar::Sidebar>>>,
    pub folder_tree: RefCell<Option<Rc<super::foldertree::FolderTree>>>,
    pub center_stack: RefCell<Option<Stack>>,
    /// The id of the folder currently shown in the grid (0 = none / raw view).
    pub current_folder: RefCell<i64>,
    /// Cached Immich albums per server id, filled by a background refresh. The
    /// sidebar reads this cache. HTTP never runs on the GTK main thread.
    pub immich_albums: RefCell<std::collections::HashMap<i64, Vec<crate::model::ImmichAlbum>>>,
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

    /// Pause background Phase 2 enrichment for `secs` seconds from now, so the
    /// UI (folder open, scrolling, thumbnail fetches) gets the disk to itself.
    /// Called on grid interaction. Extending an existing pause simply pushes the
    /// resume time further out.
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
