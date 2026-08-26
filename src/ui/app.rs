//! Top-level application: window, layout skeleton, and startup wiring.

use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Label, Orientation, Paned, Separator};

use crate::db::Library;
use crate::thumb::Generator;
use crate::version;

use super::grid::Grid;
use super::prefs::Prefs;
use super::sidebar::Sidebar;

/// The GTK application identifier.
const APP_ID: &str = "se.hemmalab.pichouse";

/// Start the pichouse GUI application. The single entry point called from main.
pub fn run() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    // Open the library. On failure, show the error in a bare window.
    let lib = match Library::open() {
        Ok(l) => Arc::new(l),
        Err(e) => {
            show_fatal(app, &format!("Could not open library database: {e}"));
            return;
        }
    };

    let prefs = Prefs::load(&lib);
    let gen = Arc::new(Generator::new(prefs.active_size()));
    if prefs.save_all_sizes {
        gen.set_all_sizes(&prefs.sizes);
    }

    let grid = Rc::new(Grid::new(lib.clone(), gen.clone(), prefs.active_size()));

    // Status bar (placeholder; filled out in a later sub-step).
    let status = Rc::new(Label::new(None));
    status.set_xalign(0.0);
    status.set_margin_start(8);
    status.set_margin_top(4);
    status.set_margin_bottom(4);

    // Sidebar: selecting a folder loads its photos into the grid.
    let sidebar = {
        let lib = lib.clone();
        let grid = grid.clone();
        let status = status.clone();
        Sidebar::new(move |folder_id| {
            load_folder(&lib, &grid, &status, folder_id);
        })
    };

    // Left (sidebar) | center (grid) split.
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(sidebar.widget()));
    paned.set_end_child(Some(grid.widget()));
    paned.set_resize_start_child(false);
    paned.set_position(280);
    paned.set_vexpand(true);

    let root = gtk4::Box::new(Orientation::Vertical, 0);
    root.append(&paned);
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(status.as_ref());

    let window = ApplicationWindow::builder()
        .application(app)
        .title(format!("pichouse {}", version::VERSION))
        .default_width(1280)
        .default_height(820)
        .child(&root)
        .build();

    // Populate the sidebar and auto-select the first folder.
    populate(&lib, &sidebar, &grid, &status);

    window.present();
}

/// Fill the sidebar with folders and select the first one.
fn populate(lib: &Arc<Library>, sidebar: &Sidebar, grid: &Rc<Grid>, status: &Rc<Label>) {
    let folders = match lib.folders() {
        Ok(f) => f,
        Err(e) => {
            status.set_text(&format!("Error reading folders: {e}"));
            return;
        }
    };
    if folders.is_empty() {
        status.set_text(
            "Library is empty. Add and scan a folder (Settings arrives in a later \
             milestone) to populate it.",
        );
        return;
    }
    let counts = lib.folder_photo_counts().unwrap_or_default();
    sidebar.set_folders(&folders, &counts);
    status.set_text(&format!("{} folders", folders.len()));
    // Selecting the first row triggers the on_select callback, which loads it.
    if sidebar.select_first().is_none() {
        // Fallback: load directly if selection did not fire.
        load_folder(lib, grid, status, folders[0].id);
    }
}

/// Load one folder's photos into the grid and update the status bar.
fn load_folder(lib: &Arc<Library>, grid: &Rc<Grid>, status: &Rc<Label>, folder_id: i64) {
    let folder = match lib.folders() {
        Ok(fs) => fs.into_iter().find(|f| f.id == folder_id),
        Err(_) => None,
    };
    let title = folder
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "Folder".to_string());
    match lib.photos_in_folder(folder_id) {
        Ok(photos) => {
            grid.show_photos(&title, &photos);
            let path = folder.map(|f| f.path).unwrap_or_default();
            status.set_text(&format!("{} — {} photos", path, photos.len()));
        }
        Err(e) => status.set_text(&format!("Error loading photos: {e}")),
    }
}

/// Show a fatal error in a minimal window (used when the DB cannot open).
fn show_fatal(app: &Application, msg: &str) {
    let label = Label::new(Some(msg));
    label.set_margin_top(20);
    label.set_margin_bottom(20);
    label.set_margin_start(20);
    label.set_margin_end(20);
    label.set_wrap(true);
    let window = ApplicationWindow::builder()
        .application(app)
        .title("pichouse")
        .default_width(560)
        .default_height(200)
        .child(&label)
        .build();
    window.present();
}
