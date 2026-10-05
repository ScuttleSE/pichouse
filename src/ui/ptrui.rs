//! The PTR tag database in the app: settings tab, refresh, move, lookup,
//! and import.
//!
//! The database is optional. All PTR UI hides when it is off or missing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, Entry, Label, LinkButton, Orientation, PasswordEntry, ProgressBar, Separator,
    SpinButton, Window,
};

use crate::db::Library;
use crate::model::TagSource;
use crate::ptr;
use crate::ptr::place::{self, FsVerdict};

use super::controller::Controller;
use super::prefs;
use super::state::{show_error, show_message, AppState};

const DEFAULT_EXCLUDE: &str = "filename, title, source, url, page, booru";

/// The PTR settings, read from the `settings` table.
#[derive(Clone, Debug)]
pub struct PtrSettings {
    pub enabled: bool,
    pub path: PathBuf,
    pub url: String,
    pub key: String,
    pub max_gb_per_day: f64,
    pub exclude: Vec<String>,
    pub add_parents: bool,
}

pub fn default_path() -> PathBuf {
    crate::db::data_dir().unwrap_or_else(|_| PathBuf::from(".")).join("ptr.db")
}

pub fn load(lib: &Library) -> PtrSettings {
    let get = |k, d: &str| lib.get_setting(k, d).unwrap_or_else(|_| d.to_string());
    let path = get(prefs::KEY_PTR_PATH, "");
    PtrSettings {
        enabled: get(prefs::KEY_PTR_ENABLED, "0") == "1",
        path: if path.trim().is_empty() { default_path() } else { PathBuf::from(path) },
        url: get(prefs::KEY_PTR_URL, ptr::client::DEFAULT_BASE_URL),
        key: get(prefs::KEY_PTR_KEY, ""),
        max_gb_per_day: get(prefs::KEY_PTR_MAX_GB, "2").parse().unwrap_or(2.0),
        exclude: get(prefs::KEY_PTR_EXCLUDE_NS, DEFAULT_EXCLUDE)
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        add_parents: get(prefs::KEY_PTR_PARENTS, "0") == "1",
    }
}

/// True while a move runs. Lookups stop during a move.
static MOVING: AtomicBool = AtomicBool::new(false);

/// The refresh job.
fn job() -> &'static Controller {
    static J: OnceLock<Controller> = OnceLock::new();
    J.get_or_init(Controller::default)
}

/// The PTR tags of each photo, normalized to the pichouse tag style. Returns
/// `None` when the feature is off, a move runs, or the database cannot open.
pub fn lookup(lib: &Library, ids: &[i64]) -> Option<HashMap<i64, Vec<String>>> {
    let s = load(lib);
    if !s.enabled || MOVING.load(Ordering::Relaxed) {
        return None;
    }
    let reader = ptr::lookup::PtrReader::open(&s.path).ok()?;
    let opts = ptr::lookup::LookupOptions { exclude_namespaces: s.exclude, add_parents: s.add_parents };
    let mut out = HashMap::new();
    for id in ids {
        let Ok(Some(p)) = lib.photo_by_id(*id) else { continue };
        let mut tags: Vec<String> = reader
            .tags_for_sha256(&p.hash, &opts)
            .unwrap_or_default()
            .iter()
            .map(|t| ptr::lookup::normalize(t))
            .collect();
        tags.sort();
        tags.dedup();
        out.insert(*id, tags);
    }
    Some(out)
}

/// The `ptr.db` path and the lookup options for a background lookup.
/// Returns `None` when the feature is off or a move runs. It does not open
/// the database, so it is safe on the GTK main thread.
pub fn lookup_params(lib: &Library) -> Option<(PathBuf, ptr::lookup::LookupOptions)> {
    let s = load(lib);
    if !s.enabled || MOVING.load(Ordering::Relaxed) {
        return None;
    }
    Some((s.path, ptr::lookup::LookupOptions { exclude_namespaces: s.exclude, add_parents: s.add_parents }))
}

/// True when PTR lookups are possible now.
pub fn available(lib: &Library) -> bool {
    let s = load(lib);
    s.enabled && !MOVING.load(Ordering::Relaxed) && ptr::lookup::PtrReader::open(&s.path).is_ok()
}

/// Add the PTR tags to the photos as user tags. Returns the number of
/// photos that got tags.
pub fn import(lib: &Library, ids: &[i64]) -> Result<usize, String> {
    let Some(map) = lookup(lib, ids) else {
        return Err("The PTR tag database is off or not available.".into());
    };
    let mut n = 0;
    for (id, tags) in map {
        if tags.is_empty() {
            continue;
        }
        lib.add_photo_tags(id, &tags, TagSource::User).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

/// Import from the grid menu, with a status message.
pub fn import_with_status(state: &Rc<AppState>, ids: &[i64]) {
    match import(&state.lib, ids) {
        Ok(n) => state.status().set_message_transient(&format!("PTR: added tags to {n} of {} photos.", ids.len()), 6),
        Err(e) => show_error(state, &e),
    }
}

enum Msg {
    Progress { text: String, frac: f64 },
    Done(Result<String, String>),
}

/// The widgets that show a running job.
#[derive(Clone)]
struct JobUi {
    bar: ProgressBar,
    status: Label,
    refresh: Button,
    cancel: Button,
    location: GtkBox,
}

impl JobUi {
    fn running(&self, on: bool) {
        self.bar.set_visible(on);
        self.cancel.set_visible(on);
        self.refresh.set_sensitive(!on);
        self.location.set_sensitive(!on);
    }
}

/// Run `work` on a thread. It sends `Msg` values. The UI shows them.
fn run_job(
    ui: &JobUi,
    state: &Rc<AppState>,
    after: impl Fn(&Rc<AppState>, &Result<String, String>) + 'static,
    work: impl FnOnce(glib::Sender<Msg>, std::sync::Arc<AtomicBool>) + Send + 'static,
) {
    let cancel = job().begin();
    ui.running(true);
    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    let (ui2, state2, flag) = (ui.clone(), state.clone(), cancel.clone());
    rx.attach(None, move |m| match m {
        Msg::Progress { text, frac } => {
            ui2.status.set_text(&text);
            ui2.bar.set_fraction(frac.clamp(0.0, 1.0));
            glib::ControlFlow::Continue
        }
        Msg::Done(r) => {
            if job().is_current(&flag) {
                job().finish();
            }
            ui2.running(false);
            match &r {
                Ok(t) => ui2.status.set_text(t),
                Err(e) => ui2.status.set_text(&format!("Error: {e}")),
            }
            after(&state2, &r);
            glib::ControlFlow::Break
        }
    });
    std::thread::spawn(move || work(tx, cancel));
}

/// The text of the status line: file size and sync state.
fn status_text(s: &PtrSettings) -> String {
    if !s.path.exists() {
        return "No database at this location. Run ptr-sync for the first sync (see README).".into();
    }
    let size = place::db_size(&s.path) as f64 / 1e9;
    match ptr::lookup::PtrReader::open(&s.path) {
        Ok(r) => match r.sync_state() {
            Some((idx, at)) => {
                let when = glib::DateTime::from_unix_local(at)
                    .and_then(|d| d.format("%Y-%m-%d %H:%M"))
                    .map(|g| g.to_string())
                    .unwrap_or_default();
                format!("{size:.1} GB. Last update index {idx}. Last sync {when}.")
            }
            None => format!("{size:.1} GB."),
        },
        Err(e) => format!("{size:.1} GB. Not ready: {e}"),
    }
}

fn label(text: &str, width: i32) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_width_request(width);
    l
}

fn note(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.add_css_class("dim-label");
    l
}

fn setting_entry(state: &Rc<AppState>, entry: &impl IsA<gtk4::Editable>, key: &'static str) {
    let state = state.clone();
    entry.connect_changed(move |e| {
        let _ = state.lib.set_setting(key, &e.text());
    });
}

/// The "Tag Database (PTR)" tab of the Tagging settings.
pub fn ptr_tab(state: &Rc<AppState>) -> gtk4::ScrolledWindow {
    let s = load(&state.lib);
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    root.append(&note(
        "A local copy of the Hydrus Public Tag Repository (PTR). pichouse shows PTR tags for photos that are \
         byte-identical to a file in the PTR. A re-saved or resized copy does not match. The database is large \
         (about 75 GB). Put it on an SSD with enough free space.",
    ));

    let enabled = CheckButton::with_label("Use the PTR tag database");
    enabled.set_active(s.enabled);
    {
        let state = state.clone();
        enabled.connect_toggled(move |b| {
            let _ = state.lib.set_setting(prefs::KEY_PTR_ENABLED, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&enabled);

    // Location.
    root.append(&Separator::new(Orientation::Horizontal));
    let location = GtkBox::new(Orientation::Vertical, 4);
    let loc_row = GtkBox::new(Orientation::Horizontal, 6);
    loc_row.append(&label("Location", 110));
    let path_label = Label::new(Some(&s.path.to_string_lossy()));
    path_label.set_xalign(0.0);
    path_label.set_hexpand(true);
    path_label.set_selectable(true);
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    loc_row.append(&path_label);
    let choose = Button::with_label("Change…");
    let reset = Button::with_label("Default");
    loc_row.append(&choose);
    loc_row.append(&reset);
    location.append(&loc_row);
    let fs_note = note("");
    location.append(&fs_note);
    root.append(&location);

    // Server and key.
    let url_row = GtkBox::new(Orientation::Horizontal, 6);
    url_row.append(&label("Server", 110));
    let url = Entry::new();
    url.set_text(&s.url);
    url.set_hexpand(true);
    setting_entry(state, &url, prefs::KEY_PTR_URL);
    url_row.append(&url);
    root.append(&url_row);

    let key_row = GtkBox::new(Orientation::Horizontal, 6);
    key_row.append(&label("Access key", 110));
    let key = PasswordEntry::new();
    key.set_show_peek_icon(true);
    key.set_text(&s.key);
    key.set_hexpand(true);
    setting_entry(state, &key, prefs::KEY_PTR_KEY);
    key_row.append(&key);
    key_row.append(&LinkButton::with_label(ptr::client::ACCESS_KEY_HELP_URL, "Get the key"));
    root.append(&key_row);

    let gb_row = GtkBox::new(Orientation::Horizontal, 6);
    gb_row.append(&label("Max GB per day", 110));
    let gb = SpinButton::with_range(0.0, 1000.0, 0.5);
    gb.set_digits(1);
    gb.set_value(s.max_gb_per_day);
    {
        let state = state.clone();
        gb.connect_value_changed(move |b| {
            let _ = state.lib.set_setting(prefs::KEY_PTR_MAX_GB, &b.value().to_string());
        });
    }
    gb_row.append(&gb);
    gb_row.append(&note("The PTR limits a client to about 2 GB per day. 0 means no limit."));
    root.append(&gb_row);

    // Lookup options.
    root.append(&Separator::new(Orientation::Horizontal));
    let ns_row = GtkBox::new(Orientation::Horizontal, 6);
    ns_row.append(&label("Hide namespaces", 110));
    let ns = Entry::new();
    ns.set_text(&s.exclude.join(", "));
    ns.set_hexpand(true);
    ns.set_tooltip_text(Some("Comma-separated. Tags in these namespaces do not show and do not import."));
    setting_entry(state, &ns, prefs::KEY_PTR_EXCLUDE_NS);
    ns_row.append(&ns);
    root.append(&ns_row);

    let parents = CheckButton::with_label("Add the parent tags (for example \"cat\" adds \"animal\")");
    parents.set_active(s.add_parents);
    {
        let state = state.clone();
        parents.connect_toggled(move |b| {
            let _ = state.lib.set_setting(prefs::KEY_PTR_PARENTS, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&parents);

    // Status and refresh.
    root.append(&Separator::new(Orientation::Horizontal));
    let status = note(&status_text(&s));
    root.append(&status);
    let bar = ProgressBar::new();
    bar.set_show_text(false);
    bar.set_visible(false);
    root.append(&bar);
    let btn_row = GtkBox::new(Orientation::Horizontal, 6);
    let refresh = Button::with_label("Refresh now");
    refresh.set_tooltip_text(Some("Download and apply the new PTR updates. Use ptr-sync for the first sync."));
    let cancel = Button::with_label("Cancel");
    cancel.set_visible(false);
    btn_row.append(&refresh);
    btn_row.append(&cancel);
    root.append(&btn_row);

    let ui = JobUi { bar, status: status.clone(), refresh: refresh.clone(), cancel: cancel.clone(), location: location.clone() };
    if job().running() {
        ui.running(true);
        ui.status.set_text("A PTR job runs.");
    }
    cancel.connect_clicked(|_| job().stop());

    let update_fs_note = {
        let fs_note = fs_note.clone();
        move |p: &Path| {
            let dir = p.parent().unwrap_or(Path::new("/"));
            let text = match place::fs_info(dir) {
                Some((ty, free)) => {
                    let extra = match place::check_fs(&ty) {
                        FsVerdict::Ok => String::new(),
                        FsVerdict::Warn(m) | FsVerdict::Refuse(m) => format!(" {m}"),
                    };
                    format!("Filesystem {ty}, {} GB free.{extra}", free / 1_000_000_000)
                }
                None => String::new(),
            };
            fs_note.set_text(&text);
        }
    };
    update_fs_note(&s.path);

    {
        let (state, ui) = (state.clone(), ui.clone());
        refresh.connect_clicked(move |_| start_refresh(&state, &ui));
    }

    // Location changes.
    let on_new_path: Rc<dyn Fn(PathBuf)> = {
        let (state, ui, path_label, root) = (state.clone(), ui.clone(), path_label.clone(), root.clone());
        let update_fs_note = Rc::new(update_fs_note);
        Rc::new(move |new: PathBuf| {
            let (state2, ui2, pl, ufn) = (state.clone(), ui.clone(), path_label.clone(), update_fs_note.clone());
            let parent = root.root().and_downcast::<Window>();
            change_location(&state, parent.as_ref(), new, move |p| {
                pl.set_text(&p.to_string_lossy());
                ufn(&p);
                ui2.status.set_text(&status_text(&load(&state2.lib)));
            }, ui.clone());
        })
    };
    {
        let (root, on_new_path) = (root.clone(), on_new_path.clone());
        choose.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::new();
            dialog.set_title("Folder for ptr.db");
            let parent = root.root().and_downcast::<Window>();
            let on_new_path = on_new_path.clone();
            dialog.select_folder(parent.as_ref(), gio::Cancellable::NONE, move |res| {
                if let Some(dir) = res.ok().and_then(|f| f.path()) {
                    on_new_path(dir.join("ptr.db"));
                }
            });
        });
    }
    reset.connect_clicked(move |_| on_new_path(default_path()));

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_child(Some(&root));
    scroll
}

/// Check the new location. Then point to it, or move the database to it.
/// `done` gets the path in use after the change.
fn change_location(
    state: &Rc<AppState>,
    parent: Option<&Window>,
    new: PathBuf,
    done: impl Fn(PathBuf) + 'static,
    ui: JobUi,
) {
    let old = load(&state.lib).path;
    if new == old {
        return;
    }
    let dir = new.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"));
    if let Some((ty, _)) = place::fs_info(&dir) {
        match place::check_fs(&ty) {
            FsVerdict::Refuse(m) => {
                show_message(state, "PTR location", &format!("pichouse cannot use this location. {m}"));
                return;
            }
            FsVerdict::Warn(m) => show_message(state, "PTR location", &m),
            FsVerdict::Ok => {}
        }
    }
    let set_path = {
        let state = state.clone();
        move |p: &Path| {
            let v = if p == default_path() { String::new() } else { p.to_string_lossy().into_owned() };
            let _ = state.lib.set_setting(prefs::KEY_PTR_PATH, &v);
        }
    };
    if !old.exists() {
        set_path(&new);
        done(new);
        return;
    }
    if new.exists() {
        show_message(state, "PTR location", &format!("{} exists already. Delete it or select a different folder.", new.display()));
        return;
    }

    // Ask: move, point only, or cancel.
    let size = place::db_size(&old);
    let win = Window::builder().title("Move the PTR database?").modal(true).default_width(420).build();
    if let Some(p) = parent {
        win.set_transient_for(Some(p));
    }
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    let msg = Label::new(Some(&format!(
        "A database ({:.1} GB) exists at the old location:\n{}\n\nMove it to the new location?",
        size as f64 / 1e9,
        old.display()
    )));
    msg.set_wrap(true);
    msg.set_xalign(0.0);
    root.append(&msg);
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.set_halign(gtk4::Align::End);
    let b_cancel = Button::with_label("Cancel");
    let b_point = Button::with_label("Use the new path only");
    let b_move = Button::with_label("Move");
    b_move.add_css_class("suggested-action");
    row.append(&b_cancel);
    row.append(&b_point);
    row.append(&b_move);
    root.append(&row);
    win.set_child(Some(&root));

    let done = Rc::new(done);
    {
        let w = win.clone();
        b_cancel.connect_clicked(move |_| w.close());
    }
    {
        let (w, set_path, done, new) = (win.clone(), set_path.clone(), done.clone(), new.clone());
        b_point.connect_clicked(move |_| {
            set_path(&new);
            done(new.clone());
            w.close();
        });
    }
    {
        let (w, state) = (win.clone(), state.clone());
        b_move.connect_clicked(move |_| {
            w.close();
            if job().running() {
                show_message(&state, "PTR", "A PTR job runs. Wait for it or cancel it first.");
                return;
            }
            let (old, new2) = (old.clone(), new.clone());
            let (set_path, done) = (set_path.clone(), done.clone());
            let new_after = new.clone();
            run_job(
                &ui,
                &state,
                move |_, r| {
                    if r.is_ok() {
                        set_path(&new_after);
                        done(new_after.clone());
                    }
                },
                move |tx, cancel| {
                    MOVING.store(true, Ordering::Relaxed);
                    let mut last = std::time::Instant::now();
                    let r = place::move_db(&old, &new2, &cancel, |d, t| {
                        if last.elapsed().as_millis() > 200 {
                            last = std::time::Instant::now();
                            let _ = tx.send(Msg::Progress {
                                text: format!("Moving: {:.1} of {:.1} GB", d as f64 / 1e9, t as f64 / 1e9),
                                frac: d as f64 / t.max(1) as f64,
                            });
                        }
                    });
                    MOVING.store(false, Ordering::Relaxed);
                    let _ = tx.send(Msg::Done(r.map(|_| "The database moved.".into())));
                },
            );
        });
    }
    win.present();
}

/// Download and apply the new PTR updates in the background.
fn start_refresh(state: &Rc<AppState>, ui: &JobUi) {
    let s = load(&state.lib);
    if s.key.trim().is_empty() {
        show_message(state, "PTR", "Enter the access key first.");
        return;
    }
    if !s.path.exists() {
        show_message(
            state,
            "PTR",
            "No database at this location. Run ptr-sync for the first sync. The first sync is too large for the app.",
        );
        return;
    }
    if job().running() {
        return;
    }
    run_job(ui, state, |_, _| {}, move |tx, cancel| {
        let r = (|| -> Result<String, String> {
            let mut db = ptr::db::PtrDb::open(&s.path).map_err(|e| e.to_string())?;
            let client = ptr::client::Client::new(&s.url, &s.key)?;
            let limit = (s.max_gb_per_day > 0.0).then(|| (s.max_gb_per_day * 1e9) as u64);
            let n = ptr::sync::sync(client, &mut db, &cancel, None, limit, |p| {
                let _ = tx.send(Msg::Progress {
                    text: format!("Applied index {} of {}", p.index, p.last_index),
                    frac: p.index as f64 / p.last_index.max(1) as f64,
                });
            })?;
            if cancel.load(Ordering::Relaxed) {
                return Ok(format!("Cancelled after {n} indexes."));
            }
            db.build_lookup_indexes().map_err(|e| e.to_string())?;
            db.checkpoint().map_err(|e| e.to_string())?;
            Ok(format!("Up to date. Applied {n} new indexes. {}", status_text(&s)))
        })();
        let _ = tx.send(Msg::Done(r));
    });
}
