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
use super::grid::Grid;
use super::prefs::{load_ai_config, Prefs};
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
    app.run()
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
    let shortcuts = Shortcuts::load(&lib);

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
        status: RefCell::new(None),
        grid: RefCell::new(None),
        properties: RefCell::new(None),
        viewer: RefCell::new(None),
        center_stack: RefCell::new(None),
        current_folder: RefCell::new(0),
    });
    state.apply_thumb_prefs();

    // Build panels.
    let status = StatusBar::new(&state);
    *state.status.borrow_mut() = Some(status.clone());

    let grid = Grid::new(lib.clone(), gen.clone(), prefs.active_size());
    *state.grid.borrow_mut() = Some(grid.clone());

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

    // Sidebar: selecting a folder loads it.
    let sidebar = {
        let state = state.clone();
        Sidebar::new(move |folder_id| {
            load_folder(&state, folder_id);
        })
    };
    let sidebar = Rc::new(sidebar);

    // Center stack: grid <-> viewer.
    let center_stack = Stack::new();
    center_stack.set_vexpand(true);
    center_stack.set_hexpand(true);
    center_stack.add_named(grid.widget(), Some("grid"));
    center_stack.add_named(viewer.widget(), Some("viewer"));
    center_stack.set_visible_child_name("grid");
    *state.center_stack.borrow_mut() = Some(center_stack.clone());

    // Left: sidebar in a stack (Library only for now) with a switcher header.
    let left_stack = Stack::new();
    left_stack.set_vexpand(true);
    left_stack.add_titled(sidebar.widget(), Some("library"), "Library");
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
    populate(&state, &sidebar);

    window.present();
}

/// Reload the folder list into the sidebar (after scan/add/remove).
pub fn reload_folders(state: &Rc<AppState>) {
    // The sidebar is not stored in AppState; the app rebuilds its list by
    // reading folders and updating via a stored closure is unnecessary here.
    // Instead, the grid header stays; the sidebar is refreshed by re-selecting.
    // For simplicity we refresh through the stored sidebar reference.
    if let Some(reload) = SIDEBAR_RELOAD.with(|c| c.borrow().clone()) {
        reload(state);
    }
}

thread_local! {
    /// A stored closure that reloads the sidebar folder list. Set during build.
    static SIDEBAR_RELOAD: RefCell<Option<Rc<dyn Fn(&Rc<AppState>)>>> = const { RefCell::new(None) };
}

fn populate(state: &Rc<AppState>, sidebar: &Rc<Sidebar>) {
    // Store a reload closure for later (scan/add/remove).
    {
        let sidebar = sidebar.clone();
        SIDEBAR_RELOAD.with(|c| {
            *c.borrow_mut() = Some(Rc::new(move |state: &Rc<AppState>| {
                fill_sidebar(state, &sidebar);
            }));
        });
    }
    fill_sidebar(state, sidebar);

    let folders = state.lib.folders().unwrap_or_default();
    if folders.is_empty() {
        state
            .status()
            .set_message("Library is empty. Add a folder in Settings → Library Folders.");
    } else {
        state.status().set_message(&format!("{} folders", folders.len()));
        if let Some(id) = sidebar.select_first() {
            load_folder(state, id);
        }
    }
}

fn fill_sidebar(state: &Rc<AppState>, sidebar: &Rc<Sidebar>) {
    let folders = state.lib.folders().unwrap_or_default();
    let counts = state.lib.folder_photo_counts().unwrap_or_default();
    sidebar.set_folders(&folders, &counts);
}

/// Load one folder's photos into the grid and update the status bar.
fn load_folder(state: &Rc<AppState>, folder_id: i64) {
    *state.current_folder.borrow_mut() = folder_id;
    let folder = state
        .lib
        .folders()
        .ok()
        .and_then(|fs| fs.into_iter().find(|f| f.id == folder_id));
    let title = folder
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "Folder".to_string());
    match state.lib.photos_in_folder(folder_id) {
        Ok(photos) => {
            state.grid().show_photos(&title, &photos);
            let path = folder.map(|f| f.path).unwrap_or_default();
            state
                .status()
                .set_message(&format!("{} — {} photos", path, photos.len()));
        }
        Err(e) => state.status().set_message(&format!("Error: {e}")),
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

// Silence unused import when gdk helpers are only used indirectly.
#[allow(unused_imports)]
use gdk as _gdk;
