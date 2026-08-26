//! Top-level application: window, layout skeleton, and startup wiring.

use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Label, Orientation, Separator};

use crate::db::Library;
use crate::thumb::Generator;
use crate::version;

use super::grid::Grid;
use super::prefs::Prefs;

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

    let grid = Grid::new(lib.clone(), gen.clone(), prefs.active_size());

    // Status bar (placeholder; filled out in a later sub-step).
    let status = Label::new(None);
    status.set_xalign(0.0);
    status.set_margin_start(8);
    status.set_margin_top(4);
    status.set_margin_bottom(4);

    let root = gtk4::Box::new(Orientation::Vertical, 0);
    root.append(grid.widget());
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&status);

    let window = ApplicationWindow::builder()
        .application(app)
        .title(format!("pichouse {}", version::VERSION))
        .default_width(1280)
        .default_height(820)
        .child(&root)
        .build();

    // Checkpoint behaviour: show the first scanned folder's photos, or a hint
    // if the library is empty. Sidebar-driven navigation arrives in M6.3.
    load_initial(&lib, &grid, &status);

    window.present();
}

/// Load the first folder's photos into the grid, or show a hint when empty.
fn load_initial(lib: &Arc<Library>, grid: &Grid, status: &Label) {
    match lib.folders() {
        Ok(folders) if !folders.is_empty() => {
            let f = &folders[0];
            match lib.photos_in_folder(f.id) {
                Ok(photos) => {
                    grid.show_photos(&f.name, &photos);
                    status.set_text(&format!(
                        "{} photos in {} (first of {} folders)",
                        photos.len(),
                        f.path,
                        folders.len()
                    ));
                }
                Err(e) => status.set_text(&format!("Error loading photos: {e}")),
            }
        }
        Ok(_) => {
            status.set_text(
                "Library is empty. Add and scan a folder with the Go app (or a later \
                 milestone's Settings) to populate it.",
            );
        }
        Err(e) => status.set_text(&format!("Error reading folders: {e}")),
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
