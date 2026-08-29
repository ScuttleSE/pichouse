//! The Characters view: browse named stylised characters and unnamed groups.
//!
//! Shown in the center stack when the user selects the Characters header in the
//! Library sidebar. It shows one tile per group: named characters first, then
//! the largest unnamed clusters. The HDBSCAN noise group (cluster -1) shows as
//! "Unclear". A named tile opens that character's photos. An unnamed tile opens
//! the name/merge dialog. The scan refreshes this view as groups appear.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FlowBox, GestureClick, Image, Label, Orientation, PolicyType,
    PopoverMenu, ScrolledWindow, SelectionMode,
};
use gtk4::gio;

use super::state::AppState;
use super::util::texture_from_bytes;

/// One crop-render job. A worker renders the crop, writes it to the cache, and
/// sends the JPEG to the main thread through the job's sender.
struct CropJob {
    face_id: i64,
    path: std::path::PathBuf,
    orientation: i32,
    bbox: (i32, i32, i32, i32),
    thumbs: Option<std::sync::Arc<crate::db::FaceThumbs>>,
    reply: glib::Sender<Option<Vec<u8>>>,
}

/// A bounded worker pool for crop rendering. Opening the Characters view during
/// a scan can request many uncached crops. A fixed pool of workers reads a
/// shared queue, so the view never spawns one thread per tile.
struct CropPool {
    queue: std::sync::Mutex<std::collections::VecDeque<CropJob>>,
    cv: std::sync::Condvar,
}

static CROP_POOL: std::sync::OnceLock<std::sync::Arc<CropPool>> = std::sync::OnceLock::new();

fn crop_pool() -> &'static std::sync::Arc<CropPool> {
    CROP_POOL.get_or_init(|| {
        let pool = std::sync::Arc::new(CropPool {
            queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
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
fn queue_crop_job(job: CropJob) {
    let pool = crop_pool();
    pool.queue.lock().unwrap().push_back(job);
    pool.cv.notify_one();
}

/// The noise cluster id from HDBSCAN. Shown to the user, not hidden.
const NOISE_CLUSTER_ID: i64 = -1;

/// A stable key for one tile. The grid keeps a tile in a fixed position while
/// its key stays the same, so a scan never moves a tile under the pointer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileKey {
    Named(i64),
    Cluster(i64),
}

/// One rendered tile and the data needed to update it in place.
struct TileEntry {
    key: TileKey,
    root: GtkBox,
    count_label: Label,
    name: String,
}

/// The Characters view widget and its rebuild logic.
pub struct CharactersView {
    root: GtkBox,
    flow: FlowBox,
    empty: Label,
    state: RefCell<Option<Rc<AppState>>>,
    tiles: RefCell<Vec<TileEntry>>,
    /// The currently selected groups. Empty when nothing is selected.
    selected: RefCell<Vec<TileKey>>,
    /// The anchor for a shift-click range. Set on a plain single click.
    anchor: RefCell<Option<TileKey>>,
    /// The selection action bar. Visible only when the selection is not empty.
    sel_bar: GtkBox,
    /// The selection count label in the action bar.
    sel_label: Label,
}

impl CharactersView {
    /// Build the view. `bind_state` must be called once before use.
    pub fn new() -> Rc<CharactersView> {
        let root = GtkBox::new(Orientation::Vertical, 0);

        let bar = GtkBox::new(Orientation::Horizontal, 6);
        bar.set_margin_top(8);
        bar.set_margin_bottom(4);
        bar.set_margin_start(8);
        bar.set_margin_end(8);
        let title = Label::new(Some("Characters"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("title-4");
        bar.append(&title);

        // The selection controls live in the title row, so showing them does
        // not add a new row. The bar always reserves its height, so the grid
        // never moves. We hide it by opacity, not visibility, so the row height
        // stays constant whether or not a group is selected.
        let sel_bar = GtkBox::new(Orientation::Horizontal, 6);
        let sel_label = Label::new(None);
        sel_label.set_xalign(1.0);
        sel_bar.append(&sel_label);
        let skip_btn = Button::with_label("Do not scan selected");
        skip_btn.add_css_class("destructive-action");
        sel_bar.append(&skip_btn);
        let clear_btn = Button::with_label("Clear selection");
        sel_bar.append(&clear_btn);
        sel_bar.set_opacity(0.0);
        sel_bar.set_sensitive(false);
        bar.append(&sel_bar);
        root.append(&bar);

        let flow = FlowBox::new();
        flow.set_selection_mode(SelectionMode::None);
        flow.set_max_children_per_line(8);
        flow.set_min_children_per_line(2);
        flow.set_row_spacing(8);
        flow.set_column_spacing(8);
        flow.set_margin_top(8);
        flow.set_margin_bottom(8);
        flow.set_margin_start(8);
        flow.set_margin_end(8);
        flow.set_valign(Align::Start);

        let empty = Label::new(Some(
            "No stylised faces yet. Turn on stylised face detection in \
             Settings → Characters, then scan.",
        ));
        empty.set_wrap(true);
        empty.set_margin_top(16);
        empty.set_margin_start(12);

        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .build();
        let inner = GtkBox::new(Orientation::Vertical, 0);
        inner.append(&empty);
        inner.append(&flow);
        scroll.set_child(Some(&inner));
        root.append(&scroll);

        let view = Rc::new(CharactersView {
            root,
            flow,
            empty,
            state: RefCell::new(None),
            tiles: RefCell::new(Vec::new()),
            selected: RefCell::new(Vec::new()),
            anchor: RefCell::new(None),
            sel_bar,
            sel_label,
        });

        {
            let this = view.clone();
            skip_btn.connect_clicked(move |_| this.skip_selected());
        }
        {
            let this = view.clone();
            clear_btn.connect_clicked(move |_| this.clear_selection());
        }

        view
    }

    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state);
    }

    /// The view root widget.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Rebuild every tile from scratch. Used on open and after a structural
    /// change (rename, remove, skip). It clears the tile cache first.
    pub fn reload(self: &Rc<Self>) {
        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }
        self.tiles.borrow_mut().clear();
        self.refresh();
    }

    /// Update the tiles in place. A new group appends at the end. An existing
    /// group updates its count label. A group that is gone gets removed. The
    /// order of the surviving tiles does not change, so a scan never moves a
    /// tile under the pointer. Safe to call repeatedly during a scan.
    pub fn refresh(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };

        let tile_px = state.prefs.borrow().active_size().clamp(72, 320);

        let characters = state.lib.characters().unwrap_or_default();
        let clusters = state.lib.unnamed_style_clusters().unwrap_or_default();

        if characters.is_empty() && clusters.is_empty() {
            while let Some(child) = self.flow.first_child() {
                self.flow.remove(&child);
            }
            self.tiles.borrow_mut().clear();
            self.empty.set_visible(true);
            self.flow.set_visible(false);
            return;
        }
        self.empty.set_visible(false);
        self.flow.set_visible(true);

        // The wanted set, in stable display order: named characters first, then
        // unnamed clusters by id.
        let mut wanted: Vec<(TileKey, String, i64)> = Vec::new();
        for (character, count) in &characters {
            wanted.push((TileKey::Named(character.id), character.name.clone(), *count));
        }
        for (cluster_id, count) in &clusters {
            let name = if *cluster_id == NOISE_CLUSTER_ID {
                "Unclear"
            } else {
                "Unnamed"
            };
            wanted.push((TileKey::Cluster(*cluster_id), name.to_string(), *count));
        }

        // Remove tiles no longer wanted.
        {
            let mut tiles = self.tiles.borrow_mut();
            let mut i = 0;
            while i < tiles.len() {
                let still = wanted.iter().any(|(k, _, _)| *k == tiles[i].key);
                if still {
                    i += 1;
                } else {
                    self.flow.remove(&tiles[i].root);
                    tiles.remove(i);
                }
            }
        }

        // Add new tiles and update existing ones. New tiles append at the end,
        // so an existing tile never changes position.
        for (key, name, count) in wanted {
            let existing = self
                .tiles
                .borrow()
                .iter()
                .position(|t| t.key == key);
            if let Some(idx) = existing {
                let mut tiles = self.tiles.borrow_mut();
                let entry = &mut tiles[idx];
                if entry.name != name {
                    entry.name = name.clone();
                }
                entry
                    .count_label
                    .set_text(&format!("{name} ({count})"));
            } else {
                let (face_id, named, character_id, cluster_id) = match key {
                    TileKey::Named(cid) => (
                        state.lib.character_representative_face(cid).unwrap_or(0),
                        true,
                        cid,
                        0,
                    ),
                    TileKey::Cluster(clid) => (
                        state.lib.cluster_representative_face(clid).unwrap_or(0),
                        false,
                        0,
                        clid,
                    ),
                };
                let (tile_root, count_label) = self.build_tile(
                    &state,
                    face_id,
                    &name,
                    count,
                    named,
                    character_id,
                    cluster_id,
                    tile_px,
                );
                self.flow.append(&tile_root);
                self.tiles.borrow_mut().push(TileEntry {
                    key,
                    root: tile_root,
                    count_label,
                    name,
                });
            }
        }

        // Drop selection entries whose group is gone, then refresh the
        // highlight and the action bar. Drop the anchor if its group is gone.
        {
            let mut sel = self.selected.borrow_mut();
            sel.retain(|k| self.tiles.borrow().iter().any(|t| t.key == *k));
        }
        {
            let mut anchor = self.anchor.borrow_mut();
            if let Some(k) = *anchor {
                if !self.tiles.borrow().iter().any(|t| t.key == k) {
                    *anchor = None;
                }
            }
        }
        self.update_selection_ui();
    }

    /// Build one tile. Returns the tile root and its count label. The count
    /// label is updated in place by `refresh`.
    #[allow(clippy::too_many_arguments)]
    fn build_tile(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        face_id: i64,
        name: &str,
        count: i64,
        named: bool,
        character_id: i64,
        cluster_id: i64,
        tile_px: i32,
    ) -> (GtkBox, Label) {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);
        tile.add_css_class("character-tile");

        let key = if named {
            TileKey::Named(character_id)
        } else {
            TileKey::Cluster(cluster_id)
        };
        if self.selected.borrow().iter().any(|k| *k == key) {
            tile.add_css_class("selected");
        }

        let image = Image::new();
        image.set_pixel_size(tile_px);
        image.set_size_request(tile_px, tile_px);
        image.set_icon_name(Some("avatar-default-symbolic"));
        if face_id != 0 {
            if let Some(jpeg) = state.style_face_crop_cached(face_id) {
                if let Some(tex) = texture_from_bytes(&jpeg) {
                    image.set_paintable(Some(&tex));
                }
            } else if let Some((path, orientation, bbox)) =
                state.style_face_crop_inputs(face_id)
            {
                // Render the crop off the main thread, then fill the image. This
                // keeps the view fast to open during a scan, when many crops are
                // not yet cached.
                let thumbs = state.style_face_thumbs();
                let (tx, rx) = glib::MainContext::channel::<Option<Vec<u8>>>(
                    glib::Priority::DEFAULT,
                );
                queue_crop_job(CropJob {
                    face_id,
                    path,
                    orientation,
                    bbox,
                    thumbs,
                    reply: tx,
                });
                let image_weak = image.downgrade();
                rx.attach(None, move |jpeg| {
                    if let (Some(image), Some(jpeg)) = (image_weak.upgrade(), jpeg) {
                        if let Some(tex) = texture_from_bytes(&jpeg) {
                            image.set_paintable(Some(&tex));
                        }
                    }
                    glib::ControlFlow::Break
                });
            }
        }

        let label_text = format!("{name} ({count})");
        let count_label = Label::new(Some(&label_text));
        count_label.set_wrap(true);
        count_label.set_max_width_chars(16);
        count_label.set_justify(gtk4::Justification::Center);
        if !named {
            count_label.add_css_class("dim-label");
        }

        // Primary-button clicks. One click toggles the group selection. Two
        // clicks open the group. A single click runs after a short delay. A
        // second click inside the delay cancels the single-click and opens the
        // group. We do not claim the sequence, so GTK still reports the double.
        let primary = GestureClick::new();
        primary.set_button(gtk4::gdk::BUTTON_PRIMARY);
        // A generation counter. Each press bumps it. A pending single-click runs
        // only if its generation still matches, so a double-click cancels it.
        let click_gen: Rc<std::cell::Cell<u64>> = Rc::new(std::cell::Cell::new(0));
        {
            let this = self.clone();
            let state = state.clone();
            let name = name.to_string();
            let click_gen = click_gen.clone();
            primary.connect_pressed(move |g, n_press, _, _| {
                let gen = click_gen.get().wrapping_add(1);
                click_gen.set(gen);
                if n_press >= 2 {
                    // A double-click: open the group. Undo the stray toggle the
                    // first click applied.
                    this.clear_selection();
                    if named {
                        state.show_character(character_id, &name);
                    } else {
                        state.show_style_cluster(cluster_id, "Unnamed character");
                    }
                    return;
                }
                let shift = g
                    .current_event_state()
                    .contains(gtk4::gdk::ModifierType::SHIFT_MASK);
                if shift {
                    this.select_range_to(key);
                    return;
                }
                // A single click. Defer the toggle so a second click can cancel
                // it and open the group instead.
                let this2 = this.clone();
                let click_gen2 = click_gen.clone();
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(250),
                    move || {
                        if click_gen2.get() == gen {
                            this2.toggle_selection(key);
                        }
                    },
                );
            });
        }
        tile.add_controller(primary);

        // A right-click menu on the whole tile. It offers group-level actions.
        let gesture = GestureClick::new();
        gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
        {
            let this = self.clone();
            let state = state.clone();
            let name = name.to_string();
            let tile_ref = tile.clone();
            gesture.connect_pressed(move |g, _, x, y| {
                g.set_state(gtk4::EventSequenceState::Claimed);
                this.show_tile_menu(&state, &tile_ref, named, character_id, cluster_id, &name, x, y);
            });
        }
        tile.add_controller(gesture);

        tile.append(&image);
        tile.append(&count_label);
        (tile, count_label)
    }

    /// Toggle whether a group is in the selection. Updates the highlight and
    /// the action bar. Sets the anchor for a later shift-click range.
    fn toggle_selection(self: &Rc<Self>, key: TileKey) {
        {
            let mut sel = self.selected.borrow_mut();
            if let Some(pos) = sel.iter().position(|k| *k == key) {
                sel.remove(pos);
            } else {
                sel.push(key);
            }
        }
        *self.anchor.borrow_mut() = Some(key);
        self.update_selection_ui();
    }

    /// Select every group from the anchor to `key`, in display order. Adds the
    /// whole range to the selection and keeps the anchor. With no anchor, this
    /// falls back to a plain toggle.
    fn select_range_to(self: &Rc<Self>, key: TileKey) {
        let anchor = *self.anchor.borrow();
        let Some(anchor) = anchor else {
            self.toggle_selection(key);
            return;
        };
        let (from, to) = {
            let tiles = self.tiles.borrow();
            let a = tiles.iter().position(|t| t.key == anchor);
            let b = tiles.iter().position(|t| t.key == key);
            match (a, b) {
                (Some(a), Some(b)) => (a.min(b), a.max(b)),
                _ => {
                    drop(tiles);
                    self.toggle_selection(key);
                    return;
                }
            }
        };
        {
            let tiles = self.tiles.borrow();
            let mut sel = self.selected.borrow_mut();
            for entry in &tiles[from..=to] {
                if !sel.iter().any(|k| *k == entry.key) {
                    sel.push(entry.key);
                }
            }
        }
        self.update_selection_ui();
    }

    /// Clear the whole selection and the anchor.
    fn clear_selection(self: &Rc<Self>) {
        self.selected.borrow_mut().clear();
        *self.anchor.borrow_mut() = None;
        self.update_selection_ui();
    }

    /// Apply the highlight class to each tile and update the action bar text and
    /// visibility from the current selection.
    fn update_selection_ui(self: &Rc<Self>) {
        let sel = self.selected.borrow();
        for entry in self.tiles.borrow().iter() {
            if sel.iter().any(|k| *k == entry.key) {
                entry.root.add_css_class("selected");
            } else {
                entry.root.remove_css_class("selected");
            }
        }
        let n = sel.len();
        if n == 0 {
            self.sel_bar.set_opacity(0.0);
            self.sel_bar.set_sensitive(false);
        } else {
            self.sel_bar.set_opacity(1.0);
            self.sel_bar.set_sensitive(true);
            let word = if n == 1 { "group" } else { "groups" };
            self.sel_label.set_text(&format!("{n} {word} selected"));
        }
    }

    /// Mark every photo in every selected group as "do not scan". This excludes
    /// the photos from every future face scan and removes them from every group.
    fn skip_selected(self: &Rc<Self>) {
        let keys: Vec<TileKey> = self.selected.borrow().clone();
        if keys.is_empty() {
            return;
        }
        self.skip_keys(&keys);
    }

    /// Mark every photo in the given groups as "do not scan", then refresh.
    fn skip_keys(self: &Rc<Self>, keys: &[TileKey]) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        let mut ids: Vec<i64> = Vec::new();
        for key in keys {
            let group_ids = match key {
                TileKey::Named(cid) => state.lib.photo_ids_of_character(*cid),
                TileKey::Cluster(clid) => state.lib.photo_ids_in_style_cluster(*clid),
            }
            .unwrap_or_default();
            ids.extend(group_ids);
        }
        ids.sort_unstable();
        ids.dedup();
        if let Err(e) = state.lib.set_photos_skip_face_scan(&ids, true) {
            super::state::show_error(&state, &e.to_string());
            return;
        }
        self.selected.borrow_mut().clear();
        *self.anchor.borrow_mut() = None;
        self.reload();
        let sb = state.sidebar.borrow().as_ref().cloned();
        if let Some(sb) = sb {
            sb.reload_deferred();
        }
    }

    /// Show the right-click menu for one tile. Named tiles offer rename, clear
    /// name, delete, and "do not scan this group". Unnamed tiles offer name and
    /// "do not scan this group".
    #[allow(clippy::too_many_arguments)]
    fn show_tile_menu(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        tile: &GtkBox,
        named: bool,
        character_id: i64,
        cluster_id: i64,
        name: &str,
        x: f64,
        y: f64,
    ) {
        let group = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();

        if named {
            menu.append(Some("Rename…"), Some("char.rename"));
            menu.append(Some("Clear name (make unnamed)"), Some("char.unname"));
            menu.append(Some("Delete character"), Some("char.delete"));
        } else {
            menu.append(Some("Name this group…"), Some("char.name"));
        }
        menu.append(Some("Do not scan selected"), Some("char.skip"));

        let add = |act_name: &str, cb: Box<dyn Fn()>| {
            let a = gio::SimpleAction::new(act_name, None);
            a.connect_activate(move |_, _| cb());
            group.add_action(&a);
        };

        if named {
            {
                let this = self.clone();
                let state = state.clone();
                add(
                    "rename",
                    Box::new(move || {
                        let this2 = this.clone();
                        let state2 = state.clone();
                        super::dialogs::prompt_text(
                            &state,
                            None,
                            "Rename Character",
                            "Character name:",
                            "",
                            move |new_name| {
                                if new_name.trim().is_empty() {
                                    return;
                                }
                                if let Err(e) = state2.lib.rename_character(character_id, &new_name) {
                                    super::state::show_error(&state2, &e.to_string());
                                    return;
                                }
                                this2.reload();
                                if let Some(sb) = state2.sidebar.borrow().as_ref() {
                                    sb.reload_deferred();
                                }
                            },
                        );
                    }),
                );
            }
            {
                let this = self.clone();
                let state = state.clone();
                add(
                    "unname",
                    Box::new(move || {
                        if let Err(e) = state.lib.unname_character(character_id) {
                            super::state::show_error(&state, &e.to_string());
                            return;
                        }
                        this.reload();
                        if let Some(sb) = state.sidebar.borrow().as_ref() {
                            sb.reload_deferred();
                        }
                    }),
                );
            }
            {
                let this = self.clone();
                let state = state.clone();
                add(
                    "delete",
                    Box::new(move || {
                        if let Err(e) = state.lib.delete_character(character_id) {
                            super::state::show_error(&state, &e.to_string());
                            return;
                        }
                        this.reload();
                        if let Some(sb) = state.sidebar.borrow().as_ref() {
                            sb.reload_deferred();
                        }
                    }),
                );
            }
        } else {
            let this = self.clone();
            let state = state.clone();
            add(
                "name",
                Box::new(move || {
                    let this2 = this.clone();
                    let state2 = state.clone();
                    // Name every selected unnamed cluster into one character. If
                    // the clicked group is not in the selection, name just it.
                    let mut cluster_ids: Vec<i64> = this
                        .selected
                        .borrow()
                        .iter()
                        .filter_map(|k| match k {
                            TileKey::Cluster(c) => Some(*c),
                            TileKey::Named(_) => None,
                        })
                        .collect();
                    if !cluster_ids.contains(&cluster_id) {
                        cluster_ids = vec![cluster_id];
                    }
                    super::characters::name_style_clusters_dialog(&state, cluster_ids, move || {
                        this2.clear_selection();
                        this2.reload();
                        if let Some(sb) = state2.sidebar.borrow().as_ref() {
                            sb.reload_deferred();
                        }
                    });
                }),
            );
        }

        {
            let this = self.clone();
            add(
                "skip",
                Box::new(move || {
                    // Apply to every selected group. If the clicked tile is not
                    // in the selection, apply to just it.
                    let clicked = if named {
                        TileKey::Named(character_id)
                    } else {
                        TileKey::Cluster(cluster_id)
                    };
                    let mut keys: Vec<TileKey> = this.selected.borrow().clone();
                    if !keys.contains(&clicked) {
                        keys = vec![clicked];
                    }
                    this.skip_keys(&keys);
                }),
            );
        }
        let _ = name;

        let popover = PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_parent(tile);
        popover.insert_action_group("char", Some(&group));
        let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    }
}
