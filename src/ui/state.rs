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
    /// The Phase 2 enrichment worker session (see `super::enrich`).
    pub enrich_job: Controller,
    /// The library-freshness reconciliation session (see `super::freshness`).
    pub reconcile_job: Controller,
    /// Paths waiting to be scanned. A running scan thread drains this, so adding
    /// a folder while a scan runs appends to it instead of cancelling the scan.
    pub scan_queue: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// Photo ids waiting for Phase 2 enrichment. The enrichment worker pool
    /// drains this front-to-back; opening a folder front-loads its ids.
    pub enrich_queue: Arc<Mutex<std::collections::VecDeque<i64>>>,

    pub status: RefCell<Option<Rc<StatusBar>>>,
    pub grid: RefCell<Option<Rc<Grid>>>,
    pub properties: RefCell<Option<Rc<Properties>>>,
    pub viewer: RefCell<Option<Rc<Viewer>>>,
    pub sidebar: RefCell<Option<Rc<super::sidebar::Sidebar>>>,
    pub folder_tree: RefCell<Option<Rc<super::foldertree::FolderTree>>>,
    pub center_stack: RefCell<Option<Stack>>,
    /// The id of the folder currently shown in the grid (0 = none / raw view).
    pub current_folder: RefCell<i64>,
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

    /// Whether the viewer is the visible center child.
    pub fn viewer_active(&self) -> bool {
        self.center_stack
            .borrow()
            .as_ref()
            .map(|s| s.visible_child_name().map(|n| n == "viewer").unwrap_or(false))
            .unwrap_or(false)
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
