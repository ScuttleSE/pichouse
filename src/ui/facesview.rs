//! The Faces view: browse detected people and unnamed face groups.
//!
//! Shown in the center stack when the user selects the People header in the
//! Library sidebar. It shows one tile per group: named people first, then the
//! unnamed clusters. One click selects a tile. Two clicks open the group. A
//! right-click opens the tile menu (rename, clear name, delete, name, ignore).
//! A tile whose group gained photos in the most recent scan shows a "+N new"
//! badge.
//!
//! The view is also scoped by a person group. Selecting a group in the
//! sidebar opens this view narrowed to the direct sub-groups and member
//! persons of that group. The back button returns to the parent scope.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, FlowBox, GestureClick, Image, Label, Orientation, PolicyType,
    PopoverMenu, ScrolledWindow,
};
use gtk4::gio;

use super::groupsort::{sort_groups, tile_flow, GroupSort, SectionHeader, UnnamedHeader};
use super::prefs::{KEY_FACES_IDENTIFIED_COLLAPSED, KEY_FACES_UNNAMED_COLLAPSED, KEY_FACES_UNNAMED_SORT};
use super::state::AppState;
use super::util::texture_from_bytes;

/// The noise cluster id from HDBSCAN. Shown to the user, not hidden.
const NOISE_CLUSTER_ID: i64 = -1;

/// A stable key for one tile. The grid keeps a tile in a fixed position while
/// its key stays the same, so a scan never moves a tile under the pointer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TileKey {
    Named(i64),
    Cluster(i64),
    /// A sub-group ("folder") tile, drilling into that group's own scope.
    Group(i64),
}

/// One rendered tile and the data needed to update it in place.
struct TileEntry {
    key: TileKey,
    root: GtkBox,
    count_label: Label,
    name: String,
    /// The face ids that the tile shows. A change rebuilds the tile.
    faces: Vec<i64>,
}

/// The Characters view widget and its rebuild logic.
pub struct FacesView {
    root: GtkBox,
    title: Label,
    back_btn: Button,
    flow: FlowBox,
    /// The tiles of the unidentified groups, below the line.
    uflow: FlowBox,
    /// The line, title, and sort menu above `uflow`.
    uheader: UnnamedHeader,
    /// The collapsible "Identified" header above `flow`.
    iheader: SectionHeader,
    empty: Label,
    state: RefCell<Option<Rc<AppState>>>,
    /// The person group currently browsed, or `0` for the top-level
    /// Characters page.
    scope: RefCell<i64>,
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

impl FacesView {
    /// Build the view. `bind_state` must be called once before use.
    pub fn new() -> Rc<FacesView> {
        let root = GtkBox::new(Orientation::Vertical, 0);

        let bar = GtkBox::new(Orientation::Horizontal, 6);
        bar.set_margin_top(8);
        bar.set_margin_bottom(4);
        bar.set_margin_start(8);
        bar.set_margin_end(8);
        let back_btn = Button::from_icon_name("go-previous-symbolic");
        back_btn.add_css_class("flat");
        back_btn.set_visible(false);
        bar.append(&back_btn);
        let title = Label::new(Some("People"));
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
        let skip_btn = Button::with_label("Ignore selected");
                sel_bar.append(&skip_btn);
        let clear_btn = Button::with_label("Clear selection");
        sel_bar.append(&clear_btn);
        sel_bar.set_opacity(0.0);
        sel_bar.set_sensitive(false);
        bar.append(&sel_bar);
        root.append(&bar);

        let flow = tile_flow();
        let uflow = tile_flow();
        // The sort menu needs the view, so the view is set after it is built.
        let weak: Rc<RefCell<std::rc::Weak<FacesView>>> =
            Rc::new(RefCell::new(std::rc::Weak::new()));
        let uheader = {
            let weak = weak.clone();
            let weak2 = weak.clone();
            UnnamedHeader::new(
                &uflow,
                GroupSort::MostImages,
                move |s| {
                    if let Some(v) = weak.borrow().upgrade() {
                        if let Some(st) = v.state.borrow().clone() {
                            let _ = st.lib.set_setting(KEY_FACES_UNNAMED_SORT, s.key());
                        }
                        v.reload();
                    }
                },
                move |c| save_collapsed(&weak2, KEY_FACES_UNNAMED_COLLAPSED, c),
            )
        };
        uheader.header.root.set_visible(false);
        let iheader = {
            let weak = weak.clone();
            SectionHeader::new("Identified", &flow, None, move |c| {
                save_collapsed(&weak, KEY_FACES_IDENTIFIED_COLLAPSED, c)
            })
        };
        iheader.root.set_visible(false);

        let empty = Label::new(Some(
            "No faces yet. Turn on face detection in Settings → Faces, then scan.",
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
        inner.append(&iheader.root);
        inner.append(&flow);
        inner.append(&uheader.header.root);
        inner.append(&uflow);
        scroll.set_child(Some(&inner));
        root.append(&scroll);

        let view = Rc::new(FacesView {
            root,
            title,
            back_btn: back_btn.clone(),
            flow,
            uflow,
            uheader,
            iheader,
            empty,
            state: RefCell::new(None),
            scope: RefCell::new(0),
            tiles: RefCell::new(Vec::new()),
            selected: RefCell::new(Vec::new()),
            anchor: RefCell::new(None),
            sel_bar,
            sel_label,
        });

        *weak.borrow_mut() = Rc::downgrade(&view);
        {
            let this = view.clone();
            skip_btn.connect_clicked(move |_| this.skip_selected());
        }
        {
            let this = view.clone();
            clear_btn.connect_clicked(move |_| this.clear_selection());
        }
        {
            let this = view.clone();
            back_btn.connect_clicked(move |_| this.go_back());
        }

        view
    }

    /// Show the top-level Characters page: top-level groups, every ungrouped
    /// person, and unnamed clusters.
    pub fn show_top(self: &Rc<Self>) {
        *self.scope.borrow_mut() = 0;
        self.reload();
    }

    /// Show one group's page: its direct sub-groups and direct member
    /// people.
    pub fn show_group(self: &Rc<Self>, group_id: i64) {
        *self.scope.borrow_mut() = group_id;
        self.reload();
    }

    /// Return to the current group's parent scope (or the top-level page).
    fn go_back(self: &Rc<Self>) {
        let scope = *self.scope.borrow();
        if scope == 0 {
            return;
        }
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let parent = state
            .lib
            .person_groups()
            .unwrap_or_default()
            .into_iter()
            .find(|g| g.id == scope)
            .map(|g| g.parent_id)
            .unwrap_or(0);
        self.show_group(parent);
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
        while let Some(child) = self.uflow.first_child() {
            self.uflow.remove(&child);
        }
        self.tiles.borrow_mut().clear();
        let t = std::time::Instant::now();
        self.refresh();
        log::debug!("peopleview.reload {:.2?}", t.elapsed());
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

        let scope = *self.scope.borrow();
        let all_groups = state.lib.person_groups().unwrap_or_default();
        self.title.set_text(
            &all_groups
                .iter()
                .find(|g| g.id == scope)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| "People".to_string()),
        );
        self.back_btn.set_visible(scope != 0);

        let subgroups: Vec<crate::model::PersonGroup> = all_groups
            .iter()
            .filter(|g| g.parent_id == scope)
            .cloned()
            .collect();
        let members = state.lib.person_group_members().unwrap_or_default();
        let all_people = state.lib.persons().unwrap_or_default();
        let (people, clusters) = if scope == 0 {
            let grouped: HashSet<i64> = members.values().flatten().copied().collect();
            let people: Vec<_> = all_people
                .into_iter()
                .filter(|(c, _)| !grouped.contains(&c.id))
                .collect();
            let sort = GroupSort::from_key(
                &state.lib.get_setting(KEY_FACES_UNNAMED_SORT, "").unwrap_or_default(),
            );
            self.uheader.set_sort(sort);
            let on = |k: &str| state.lib.get_setting(k, "0").unwrap_or_default() == "1";
            self.iheader.set_collapsed(on(KEY_FACES_IDENTIFIED_COLLAPSED));
            self.uheader.header.set_collapsed(on(KEY_FACES_UNNAMED_COLLAPSED));
            // Compute the sort only on a full reload (no tiles yet). A
            // refresh keeps existing tiles in place, so it needs no order.
            let raw = state.lib.unnamed_clusters().unwrap_or_default();
            let clusters = if self.tiles.borrow().is_empty() {
                let info = state.lib.unnamed_group_info(false).unwrap_or_default();
                sort_groups(raw, &info, sort)
            } else {
                raw
            };
            (people, clusters)
        } else {
            let member_ids = members.get(&scope).cloned().unwrap_or_default();
            let people: Vec<_> = all_people
                .into_iter()
                .filter(|(c, _)| member_ids.contains(&c.id))
                .collect();
            (people, Vec::new())
        };

        self.uheader.set_count(clusters.len());
        if subgroups.is_empty() && people.is_empty() && clusters.is_empty() {
            while let Some(child) = self.flow.first_child() {
                self.flow.remove(&child);
            }
            while let Some(child) = self.uflow.first_child() {
                self.uflow.remove(&child);
            }
            self.tiles.borrow_mut().clear();
            self.empty.set_visible(true);
            self.iheader.apply(0, false);
            return;
        }
        self.empty.set_visible(false);
        self.iheader
            .apply(subgroups.len() + people.len(), scope == 0);

        // The wanted set, in stable display order: sub-groups first (like
        // folders in a file browser), then named people, then unnamed
        // clusters by id.
        let mut wanted: Vec<(TileKey, String, i64)> = Vec::new();
        for g in &subgroups {
            let count = members.get(&g.id).map(|m| m.len() as i64).unwrap_or(0);
            wanted.push((TileKey::Group(g.id), g.name.clone(), count));
        }
        for (person, count) in &people {
            wanted.push((TileKey::Named(person.id), person.name.clone(), *count));
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
                    self.flow_for(tiles[i].key).remove(&tiles[i].root);
                    tiles.remove(i);
                }
            }
        }

        // Add new tiles and update existing ones. New tiles append at the end,
        // so an existing tile never changes position.
        // One query each for all representative faces.
        let reps = state.lib.representative_faces(false).unwrap_or_default();
        let cluster_reps = state.lib.cluster_representative_faces(false).unwrap_or_default();
        for (key, name, count) in wanted {
            let faces: Vec<i64> = match key {
                TileKey::Group(gid) => person_folder_faces(&state, gid, &reps),
                TileKey::Named(cid) => vec![reps.get(&cid).copied().unwrap_or(0)],
                TileKey::Cluster(clid) => vec![cluster_reps.get(&clid).copied().unwrap_or(0)],
            };
            let face_id = faces.first().copied().unwrap_or(0);
            let existing = self
                .tiles
                .borrow()
                .iter()
                .position(|t| t.key == key);
            // A tile whose faces changed is built again at the same index.
            let rebuild_at = match existing {
                Some(idx) if self.tiles.borrow()[idx].faces != faces => Some(idx),
                _ => None,
            };
            if let (Some(idx), None) = (existing, rebuild_at) {
                let mut tiles = self.tiles.borrow_mut();
                let entry = &mut tiles[idx];
                if entry.name != name {
                    entry.name = name.clone();
                }
                entry
                    .count_label
                    .set_text(&tile_label(&state, key, &name, count));
            } else {
                let (tile_root, count_label) = match key {
                    TileKey::Group(gid) => {
                        self.build_group_tile(&state, &name, count, gid, &faces, tile_px)
                    }
                    TileKey::Named(cid) => {
                        self.build_tile(&state, face_id, &name, count, true, cid, 0, tile_px)
                    }
                    TileKey::Cluster(clid) => {
                        self.build_tile(&state, face_id, &name, count, false, 0, clid, tile_px)
                    }
                };
                let entry = TileEntry {
                    key,
                    root: tile_root,
                    count_label,
                    name,
                    faces,
                };
                // `tiles` holds the top tiles first, then the cluster tiles.
                // A cluster tile index in `uflow` is its index minus the
                // number of top tiles.
                let top = self
                    .tiles
                    .borrow()
                    .iter()
                    .filter(|t| !matches!(t.key, TileKey::Cluster(_)))
                    .count();
                let is_cluster = matches!(key, TileKey::Cluster(_));
                let flow = self.flow_for(key);
                if let Some(idx) = rebuild_at {
                    let mut tiles = self.tiles.borrow_mut();
                    let pos = if is_cluster { idx - top } else { idx };
                    flow.remove(&tiles[idx].root);
                    flow.insert(&entry.root, pos as i32);
                    tiles[idx] = entry;
                } else if is_cluster {
                    flow.append(&entry.root);
                    self.tiles.borrow_mut().push(entry);
                } else {
                    flow.append(&entry.root);
                    self.tiles.borrow_mut().insert(top, entry);
                }
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

    /// The FlowBox that holds the tile with `key`.
    fn flow_for(&self, key: TileKey) -> &FlowBox {
        match key {
            TileKey::Cluster(_) => &self.uflow,
            _ => &self.flow,
        }
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
        person_id: i64,
        cluster_id: i64,
        tile_px: i32,
    ) -> (GtkBox, Label) {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);
        tile.add_css_class("person-tile");

        let key = if named {
            TileKey::Named(person_id)
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
        fill_crop(state, &image, face_id);
        {
            // Hold Alt while hovering to preview random group photos.
            let lib = state.lib.clone();
            super::altpreview::attach(&tile, &image, state.gen.clone(), move || {
                if named {
                    lib.photos_of_person(person_id).unwrap_or_default()
                } else {
                    lib.photos_in_cluster(cluster_id).unwrap_or_default()
                }
            });
        }

        let label_text = tile_label(state, key, name, count);
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
                        state.show_person(person_id, &name);
                    } else {
                        state.show_cluster(cluster_id, "Unnamed person");
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
                this.show_tile_menu(
                    &state, &tile_ref, named, person_id, cluster_id, face_id, &name, x, y,
                );
            });
        }
        tile.add_controller(gesture);

        tile.append(&image);
        tile.append(&count_label);
        (tile, count_label)
    }

    /// Build one sub-group ("folder") tile. A single click drills into that
    /// group's own scope. Unlike a person/cluster tile it is never
    /// selectable and has no right-click menu (group management lives in the
    /// sidebar). It shows a 3x3 mosaic of `faces`, else a plain folder icon.
    fn build_group_tile(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        name: &str,
        count: i64,
        group_id: i64,
        faces: &[i64],
        tile_px: i32,
    ) -> (GtkBox, Label) {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);
        tile.add_css_class("person-tile");

        if faces.is_empty() {
            let image = Image::new();
            image.set_pixel_size(tile_px);
            image.set_size_request(tile_px, tile_px);
            image.set_icon_name(Some("folder-new-symbolic"));
            tile.append(&image);
        } else {
            let grid = super::mosaic::build(tile_px, faces, |img, f| fill_crop(state, img, f));
            tile.append(&grid);
        }

        let label_text = format!("{name} ({count})");
        let count_label = Label::new(Some(&label_text));
        count_label.set_wrap(true);
        count_label.set_max_width_chars(16);
        count_label.set_justify(gtk4::Justification::Center);

        let click = GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        {
            let this = self.clone();
            click.connect_pressed(move |_, _, _, _| this.show_group(group_id));
        }
        tile.add_controller(click);

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
            // Skip the tiles in a collapsed section.
            for entry in tiles[from..=to].iter().filter(|e| e.root.is_mapped()) {
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

    /// Ignore the faces of every selected group. The photos stay face-scanned.
    fn skip_selected(self: &Rc<Self>) {
        let keys: Vec<TileKey> = self.selected.borrow().clone();
        if keys.is_empty() {
            return;
        }
        self.skip_keys(&keys);
    }

    /// Ignore every face of the given groups, then refresh. The photos stay
    /// face-scanned. The faces show in the "Ignored People" view.
    fn skip_keys(self: &Rc<Self>, keys: &[TileKey]) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        for key in keys {
            let res = match key {
                TileKey::Named(cid) => state.lib.ignore_faces(Some(*cid), None, None),
                TileKey::Cluster(clid) => state.lib.ignore_faces(None, Some(*clid), None),
                // A sub-group tile is a navigation folder, never selectable.
                TileKey::Group(_) => Ok(()),
            };
            if let Err(e) = res {
                super::state::show_error(&state, &e.to_string());
                return;
            }
        }
        self.selected.borrow_mut().clear();
        *self.anchor.borrow_mut() = None;
        // Update in place and keep the scroll position. A full reload removes
        // the focused tile, and GTK then scrolls back to the top.
        let adj = self
            .flow
            .ancestor(ScrolledWindow::static_type())
            .and_downcast::<ScrolledWindow>()
            .map(|s| s.vadjustment());
        let pos = adj.as_ref().map(|a| a.value());
        self.refresh();
        if let (Some(adj), Some(pos)) = (adj, pos) {
            adj.set_value(pos);
            glib::idle_add_local_once(move || adj.set_value(pos));
        }
        let sb = state.sidebar.borrow().as_ref().cloned();
        if let Some(sb) = sb {
            sb.reload_deferred();
        }
    }

    /// Show the right-click menu for one tile. Named tiles offer rename, clear
    /// name, delete, and "Ignore". Unnamed tiles offer name and "Ignore". While browsing inside a person group, a
    /// tile with a face also offers "Set face as thumbnail" for that group.
    #[allow(clippy::too_many_arguments)]
    fn show_tile_menu(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        tile: &GtkBox,
        named: bool,
        person_id: i64,
        cluster_id: i64,
        _face_id: i64,
        name: &str,
        x: f64,
        y: f64,
    ) {
        let group = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();

        if named {
            menu.append(Some("Rename…"), Some("person.rename"));
            menu.append(Some("Clear name (make unnamed)"), Some("person.unname"));
            menu.append(Some("Delete person"), Some("person.delete"));
        } else {
            menu.append(Some("Name this group…"), Some("person.name"));
        }
        let ignore_label = if self.selected.borrow().len() > 1 {
            "Ignore selected"
        } else {
            "Ignore this group"
        };
        menu.append(Some(ignore_label), Some("person.skip"));

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
                            "Rename Person",
                            "Person name:",
                            "",
                            move |new_name| {
                                if new_name.trim().is_empty() {
                                    return;
                                }
                                if let Err(e) = state2.lib.rename_person(person_id, &new_name) {
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
                        if let Err(e) = state.lib.unname_person(person_id) {
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
                        if let Err(e) = state.lib.delete_person(person_id) {
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
                    // Name every selected unnamed cluster into one person. If
                    // the clicked group is not in the selection, name just it.
                    let mut cluster_ids: Vec<i64> = this
                        .selected
                        .borrow()
                        .iter()
                        .filter_map(|k| match k {
                            TileKey::Cluster(c) => Some(*c),
                            TileKey::Named(_) | TileKey::Group(_) => None,
                        })
                        .collect();
                    if !cluster_ids.contains(&cluster_id) {
                        cluster_ids = vec![cluster_id];
                    }
                    super::people::name_cluster_dialog(&state, cluster_ids, move || {
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
                        TileKey::Named(person_id)
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
        popover.insert_action_group("person", Some(&group));
        let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    }
}

/// Load the crop of one face into an image.
fn fill_crop(state: &Rc<AppState>, image: &Image, face_id: i64) {
    if face_id == 0 {
        return;
    }
    if let Some(tex) = state.face_crop_jpeg(face_id).and_then(|j| texture_from_bytes(&j)) {
        image.set_paintable(Some(&tex));
    }
}

/// The mosaic face ids for one person folder.
fn person_folder_faces(
    state: &Rc<AppState>,
    group_id: i64,
    all_reps: &std::collections::HashMap<i64, i64>,
) -> Vec<i64> {
    let owners = state.lib.persons_under_group(group_id).unwrap_or_default();
    let pool = state.lib.faces_of_persons(&owners).unwrap_or_default();
    let reps: std::collections::HashMap<i64, i64> = owners
        .iter()
        .map(|c| (*c, all_reps.get(c).copied().unwrap_or(0)))
        .collect();
    super::mosaic::pick_faces(group_id, &owners, &reps, &pool)
}

/// Save the collapsed state of a section under `key`.
fn save_collapsed(weak: &Rc<RefCell<std::rc::Weak<FacesView>>>, key: &str, collapsed: bool) {
    if let Some(v) = weak.borrow().upgrade() {
        if let Some(st) = v.state.borrow().clone() {
            let _ = st.lib.set_setting(key, if collapsed { "1" } else { "0" });
        }
    }
}

/// The label text of a tile. A group that gained photos in the most recent
/// scan shows a "+N new" suffix.
fn tile_label(state: &Rc<AppState>, key: TileKey, name: &str, count: i64) -> String {
    let group = match key {
        TileKey::Named(id) => crate::db::FaceGroup::Person(id),
        TileKey::Cluster(id) => crate::db::FaceGroup::Cluster(id),
        TileKey::Group(_) => return format!("{name} ({count})"),
    };
    let n = state.face_group_new_counts.borrow().get(&group).copied().unwrap_or(0);
    if n > 0 {
        format!("{name} ({count}) +{n} new")
    } else {
        format!("{name} ({count})")
    }
}
