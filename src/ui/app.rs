//! Top-level application: window, full layout, and startup wiring.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Label, Orientation, Paned, Stack, StackSwitcher};

use crate::ai;
use crate::db::Library;
use crate::thumb::Generator;
use crate::version;

use super::controller::Controller;
use super::foldertree::FolderTree;
use super::grid::Grid;
use super::prefs::{load_ai_config, load_face_config, Prefs};
use super::properties::Properties;
use super::shortcuts::Shortcuts;
use super::sidebar::Sidebar;
use super::state::AppState;
use super::status::StatusBar;
use super::viewer::Viewer;

/// The GTK application identifier.
const APP_ID: &str = "se.hemmalab.pichouse";

/// Start the pichouse GUI application. The single entry point called from main.
pub fn run() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    // Our own CLI flags (-v/-vv/-vvv/-q) are parsed in `main` before this call.
    // Pass GTK an empty argument list so its own parser does not see — and
    // reject — those flags (e.g. "Unknown option -vvv").
    let no_args: [&str; 0] = [];
    app.run_with_args(&no_args)
}

fn build_ui(app: &Application) {
    let lib = match Library::open() {
        Ok(l) => Arc::new(l),
        Err(e) => {
            show_fatal(app, &format!("Could not open library database: {e}"));
            return;
        }
    };

    let prefs = Prefs::load(&lib);
    let ai_config = load_ai_config(&lib);
    let face_config = load_face_config(&lib);
    let style_face_config = super::prefs::load_styleface_config(&lib);
    let shortcuts = Shortcuts::load(&lib);

    // Apply the theme choice before building any widgets. Forcing Adwaita
    // avoids broken system themes (e.g. on Kasm/remote desktops) that hide the
    // folder-tree expander.
    apply_theme(prefs.theme_override);

    let gen = Arc::new(Generator::new(prefs.active_size()));

    let state = Rc::new(AppState {
        lib: lib.clone(),
        gen: gen.clone(),
        window: RefCell::new(None),
        prefs: RefCell::new(prefs.clone()),
        ai_config: RefCell::new(ai_config),
        ai_manager: Arc::new(Mutex::new(ai::Manager::default())),
        shortcuts: RefCell::new(shortcuts),
        scan: Controller::default(),
        ai_job: Controller::default(),
        face_job: Controller::default(),
        face_config: RefCell::new(face_config),
        face_thumbs: RefCell::new(None),
        style_face_job: Controller::default(),
        style_face_config: RefCell::new(style_face_config),
        style_face_thumbs: RefCell::new(None),
        enrich_job: Controller::default(),
        reconcile_job: Controller::default(),
        immich_upload: Controller::default(),
        dedup_job: Controller::default(),
        scan_queue: std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        enrich_queue: std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        enrich_pause_until: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        status: RefCell::new(None),
        grid: RefCell::new(None),
        new_files: RefCell::new(None),
        faces_view: RefCell::new(None),
        characters_view: RefCell::new(None),
        properties: RefCell::new(None),
        viewer: RefCell::new(None),
        sidebar: RefCell::new(None),
        folder_tree: RefCell::new(None),
        center_stack: RefCell::new(None),
        current_folder: RefCell::new(0),
        immich_albums: RefCell::new(std::collections::HashMap::new()),
        last_merged_character: RefCell::new(None),
    });
    state.apply_thumb_prefs();

    // Build panels.
    let status = StatusBar::new(&state);
    *state.status.borrow_mut() = Some(status.clone());

    let grid = Grid::new(
        lib.clone(),
        gen.clone(),
        prefs.active_size(),
        state.enrich_pause_until.clone(),
    );
    *state.grid.borrow_mut() = Some(grid.clone());

    let new_files = super::newfiles::NewFilesView::new(gen.clone(), prefs.active_size());
    *state.new_files.borrow_mut() = Some(new_files.clone());
    let faces_view = super::facesview::FacesView::new();
    faces_view.bind_state(state.clone());
    *state.faces_view.borrow_mut() = Some(faces_view.clone());

    let characters_view = super::charactersview::CharactersView::new();
    characters_view.bind_state(state.clone());
    *state.characters_view.borrow_mut() = Some(characters_view.clone());
    let properties = Properties::new();
    properties.bind_state(state.clone());
    *state.properties.borrow_mut() = Some(properties.clone());

    let viewer = Viewer::new();
    *state.viewer.borrow_mut() = Some(viewer.clone());
    viewer.bind_state(state.clone());

    // Grid callbacks: click selects (properties), activate opens the viewer.
    {
        let state = state.clone();
        grid.set_on_select(move |photo| {
            state.properties().show(&photo);
        });
    }
    {
        let state = state.clone();
        grid.set_on_activate(move |photos, index| {
            state.open_viewer(photos, index);
        });
    }
    {
        let state = state.clone();
        new_files.set_on_activate(move |photos, index| {
            state.open_viewer(photos, index);
        });
    }

    // Library sidebar (album tree) and raw Folders tree.
    let sidebar = Sidebar::new();
    sidebar.bind_state(state.clone());
    *state.sidebar.borrow_mut() = Some(sidebar.clone());

    let folder_tree = FolderTree::new(state.clone());
    *state.folder_tree.borrow_mut() = Some(folder_tree.clone());

    // Grid right-click: show a context menu to add the selected photos to a
    // virtual album (or create a new one from the selection).
    super::vmenu::install_grid_context_menu(&state, &grid, &sidebar);

    // Center stack: grid <-> viewer.
    let center_stack = Stack::new();
    center_stack.set_vexpand(true);
    center_stack.set_hexpand(true);
    center_stack.add_named(grid.widget(), Some("grid"));
    center_stack.add_named(new_files.widget(), Some("newfiles"));
    center_stack.add_named(faces_view.widget(), Some("faces"));
    center_stack.add_named(characters_view.widget(), Some("characters"));
    center_stack.add_named(viewer.widget(), Some("viewer"));
    center_stack.set_visible_child_name("grid");
    *state.center_stack.borrow_mut() = Some(center_stack.clone());

    // Left: a stack switcher toggles Library (album tree) and Folders (raw fs).
    let left_stack = Stack::new();
    left_stack.set_vexpand(true);
    left_stack.add_titled(sidebar.widget(), Some("library"), "Library");
    left_stack.add_titled(folder_tree.widget(), Some("folders"), "Folders");
    let switcher = StackSwitcher::new();
    switcher.set_stack(Some(&left_stack));
    let left_box = gtk4::Box::new(Orientation::Vertical, 0);
    left_box.append(&switcher);
    left_box.append(&left_stack);
    left_box.set_size_request(300, -1);

    // sidebar | center split.
    let left_paned = Paned::new(Orientation::Horizontal);
    left_paned.set_start_child(Some(&left_box));
    left_paned.set_end_child(Some(&center_stack));
    left_paned.set_resize_start_child(false);
    left_paned.set_position(300);

    // (sidebar|center) | properties split.
    let main_paned = Paned::new(Orientation::Horizontal);
    main_paned.set_start_child(Some(&left_paned));
    main_paned.set_end_child(Some(properties.widget()));
    main_paned.set_resize_end_child(false);
    main_paned.set_vexpand(true);
    if !prefs.props_visible {
        properties.set_visible(false);
    }

    let toolbar = super::toolbar::build_toolbar(&state);

    let root = gtk4::Box::new(Orientation::Vertical, 0);
    root.append(&toolbar);
    root.append(&main_paned);
    root.append(status.widget());

    let window = ApplicationWindow::builder()
        .application(app)
        .title(format!("pichouse {}", version::VERSION))
        .default_width(1280)
        .default_height(820)
        .child(&root)
        .build();
    *state.window.borrow_mut() = Some(window.clone());

    // Install application CSS (duplicate-finder styling, and future themes).
    install_css();

    // Window-level key handling (capture phase): route keys to the viewer when
    // it is the visible center child.
    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let state = state.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _keycode, _modifier| {
            if state.viewer_active() && state.viewer().handle_key(keyval.into_glib()) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(key_ctrl);

    // Populate the sidebar and select the first folder.
    populate(&state);

    window.present();
}

/// Load the application CSS once for the default display. Provides the
/// duplicate-finder cell styling (group tint and the red X overlay).
fn install_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        "\
        .dup-x { \
            color: #ffffff; \
            background-color: rgba(200, 30, 30, 0.85); \
            border-radius: 999px; \
            font-size: 20px; \
            font-weight: bold; \
            padding: 2px 8px; \
        } \
        .dup-group-frame { \
            border: 2px solid rgba(60, 130, 220, 0.9); \
            border-radius: 6px; \
            background-color: rgba(60, 130, 220, 0.06); \
        } \
        .character-tile { \
            border: 2px solid transparent; \
            border-radius: 6px; \
            padding: 2px; \
        } \
        .character-tile.selected { \
            border-color: rgba(60, 130, 220, 0.95); \
            background-color: rgba(60, 130, 220, 0.18); \
        } \
        ",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Apply the theme preference. When `force_adwaita` is set, override the system
/// theme with the built-in Adwaita theme (which always parses cleanly); this
/// fixes environments whose system GTK theme is broken and hides the tree
/// expander. When unset, reset the override so the system theme is used.
///
/// Takes effect live; a few cached theme resources may only fully update after
/// a restart.
pub fn apply_theme(force_adwaita: bool) {
    if let Some(settings) = gtk4::Settings::default() {
        if force_adwaita {
            settings.set_gtk_theme_name(Some("Adwaita"));
        } else {
            settings.reset_property("gtk-theme-name");
        }
    }
}

/// Reload both sidebars from the current database (after scan/add/remove).
pub fn reload_folders(state: &Rc<AppState>) {
    if let Some(sidebar) = state.sidebar.borrow().clone() {
        sidebar.reload();
    }
    if let Some(ft) = state.folder_tree.borrow().clone() {
        ft.reload();
    }
}

fn populate(state: &Rc<AppState>) {
    reload_folders(state);

    // Load Immich albums in the background so the sidebar section fills in.
    super::immich::refresh_albums(state);

    let folders = state.lib.folders().unwrap_or_default();
    if folders.is_empty() {
        state
            .status()
            .set_message("Library is empty. Add a folder in Settings → Library Folders.");
    } else {
        state
            .status()
            .set_message(&format!("{} folders", folders.len()));
    }

    // Defer all heavy startup work until after the window is presented, so the
    // GUI appears at once. After an interrupted large scan the first-folder
    // load and the whole-library enrichment seed can take a while. Running them
    // on an idle tick lets the window paint first.
    let state = state.clone();
    glib::idle_add_local_once(move || {
        populate_deferred(&state);
    });
}

/// The heavy part of startup. Runs after `window.present()` on an idle tick.
fn populate_deferred(state: &Rc<AppState>) {
    let folders = state.lib.folders().unwrap_or_default();
    if !folders.is_empty() {
        if let Some(sidebar) = state.sidebar.borrow().clone() {
            if let Some(folder) = sidebar.select_first_folder() {
                load_folder_into_grid(state, &folder);
            }
        }
        // Resume Phase 2 enrichment for any photos left structure-only by an
        // interrupted import in a previous session.
        super::enrich::ensure_running(state);
    }
    // Reconcile against disk once at startup (catches files added or removed
    // while the app was closed, including on network drives), then keep a
    // periodic reconcile running as the reliable freshness path.
    super::freshness::reconcile_now(state);
    super::freshness::start_periodic(state);
    super::immich::start_periodic_refresh(state);
    super::immich::sync_all_down(state);
    // inotify fast-path for local folders (optional; periodic reconcile is the
    // reliable path and covers network drives where inotify is silent).
    super::watcher::start(state);
}


/// Load a scanned folder's photos into the grid (called by the sidebar).
pub fn load_folder_into_grid(state: &Rc<AppState>, folder: &crate::model::Folder) {
    *state.current_folder.borrow_mut() = folder.id;
    state.grid().show_folder(folder.id, &folder.name);
    let count = state
        .lib
        .photos_in_folder(folder.id)
        .map(|p| p.len())
        .unwrap_or(0);
    state
        .status()
        .set_message(&format!("{} — {} photos", folder.path, count));
    // On-demand priority: if this folder has un-enriched photos, move them to
    // the front of the Phase 2 worklist so what the user opened fills in first.
    super::enrich::prioritize_folder(state, folder.id);
}

/// Load a raw filesystem directory's images into the grid (Folders tab). Reuses
/// content hashes recorded during scanning so cached thumbnails are found.
pub fn load_raw_folder_into_grid(state: &Rc<AppState>, dir: &str) {
    *state.current_folder.borrow_mut() = 0;
    state.grid().show_raw_folder(dir);
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

// Silence unused import when gdk helpers are only used indirectly.
#[allow(unused_imports)]
use gdk as _gdk;
