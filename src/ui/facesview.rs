//! The Faces view: browse detected people and unnamed face groups.
//!
//! Shown in the center stack when the user selects the People header in the
//! Library sidebar. It shows one tile per group: named people first, then the
//! largest unnamed clusters. A named tile opens that person's photos. An
//! unnamed tile opens the name/assign dialog. The scan refreshes this view as
//! groups appear. A tile whose group gained photos in the most recent scan
//! shows a "+N new" badge; the badge clears at the start of the next scan.
//!
//! The view is also scoped by a person group (e.g. "Disney"): selecting a
//! group in the sidebar opens this same view narrowed to that group's direct
//! sub-groups (folder tiles) and member persons (face tiles), the same way
//! opening an Album shows its folders rather than a merged photo grid. A
//! group tile drills further in; the back button returns to the parent scope.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FlowBox, GestureClick, Image, Label, Orientation, PolicyType,
    PopoverMenu, ScrolledWindow, SelectionMode,
};

use super::state::{show_error, AppState};
use super::util::texture_from_bytes;

/// The Faces view widget and its rebuild logic.
pub struct FacesView {
    root: GtkBox,
    title: Label,
    back_btn: Button,
    flow: FlowBox,
    empty: Label,
    state: RefCell<Option<Rc<AppState>>>,
    /// The person group currently browsed, or `0` for the top-level People
    /// page (every top-level group, every ungrouped person, and unnamed
    /// clusters).
    scope: RefCell<i64>,
}

impl FacesView {
    /// Build the view. `bind_state` must be called once before use.
    pub fn new() -> Rc<FacesView> {
        let root = GtkBox::new(Orientation::Vertical, 0);

        // A small header bar with a back button (for a group scope) and title.
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
        inner.append(&flow);
        scroll.set_child(Some(&inner));
        root.append(&scroll);

        let view = Rc::new(FacesView {
            root,
            title,
            back_btn: back_btn.clone(),
            flow,
            empty,
            state: RefCell::new(None),
            scope: RefCell::new(0),
        });
        {
            let this = view.clone();
            back_btn.connect_clicked(move |_| this.go_back());
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

    /// Show the top-level People page: top-level groups, every ungrouped
    /// person, and unnamed clusters.
    pub fn show_top(self: &Rc<Self>) {
        *self.scope.borrow_mut() = 0;
        self.reload();
    }

    /// Show one group's page: its direct sub-groups and direct member
    /// persons, the same way opening an Album shows its folders.
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

    /// Rebuild the group tiles from the database, at the current scope. Safe
    /// to call repeatedly, so the scan can refresh this view as new groups
    /// appear.
    pub fn reload(self: &Rc<Self>) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        // Clear existing tiles.
        while let Some(child) = self.flow.first_child() {
            self.flow.remove(&child);
        }

        // Match the general thumbnail slider size, clamped to a sane range for
        // face crops.
        let tile = state.prefs.borrow().active_size().clamp(72, 320);

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
                .filter(|(p, _)| !grouped.contains(&p.id))
                .collect();
            (people, state.lib.unnamed_clusters().unwrap_or_default())
        } else {
            let member_ids = members.get(&scope).cloned().unwrap_or_default();
            let people: Vec<_> = all_people
                .into_iter()
                .filter(|(p, _)| member_ids.contains(&p.id))
                .collect();
            (people, Vec::new())
        };

        if subgroups.is_empty() && people.is_empty() && clusters.is_empty() {
            self.empty.set_visible(true);
            self.flow.set_visible(false);
            return;
        }
        self.empty.set_visible(false);
        self.flow.set_visible(true);

        // Sub-groups first, like folders in a file browser.
        for g in &subgroups {
            let count = members.get(&g.id).map(|m| m.len() as i64).unwrap_or(0);
            let t = self.build_group_tile(&state, &g.name, count, g.id, g.cover_face_id, tile);
            self.flow.append(&t);
        }

        // Named people next.
        for (person, count) in people {
            let face_id = state
                .lib
                .person_representative_face(person.id)
                .unwrap_or(0);
            let t = self.build_tile(
                &state,
                face_id,
                &person.name,
                count,
                true,
                person.id,
                0,
                tile,
            );
            self.flow.append(&t);
        }

        // Unnamed clusters, largest first.
        for (cluster_id, count) in clusters {
            let face_id = state
                .lib
                .unassigned_faces_in_cluster(cluster_id)
                .ok()
                .and_then(|v| v.first().map(|f| f.id))
                .unwrap_or(0);
            let t = self.build_tile(
                &state,
                face_id,
                "Unnamed",
                count,
                false,
                0,
                cluster_id,
                tile,
            );
            self.flow.append(&t);
        }
    }

    /// Build one sub-group ("folder") tile. Clicking either the icon or the
    /// label drills into that group's own scope. Shows the group's chosen
    /// cover face when set (via "Set face as thumbnail" on a member's tile),
    /// else a plain folder icon.
    fn build_group_tile(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        name: &str,
        count: i64,
        group_id: i64,
        cover_face_id: i64,
        tile_px: i32,
    ) -> GtkBox {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);

        let image = Image::new();
        image.set_pixel_size(tile_px);
        image.set_size_request(tile_px, tile_px);
        if cover_face_id != 0 {
            if let Some(jpeg) = state.face_crop_jpeg(cover_face_id) {
                if let Some(tex) = texture_from_bytes(&jpeg) {
                    image.set_paintable(Some(&tex));
                }
            }
        }
        if image.paintable().is_none() {
            image.set_icon_name(Some("folder-new-symbolic"));
        }

        let img_btn = Button::new();
        img_btn.set_child(Some(&image));
        img_btn.add_css_class("flat");
        {
            let this = self.clone();
            img_btn.connect_clicked(move |_| this.show_group(group_id));
        }

        let lbl_btn = Button::with_label(&format!("{name} ({count})"));
        lbl_btn.add_css_class("flat");
        if let Some(child) = lbl_btn.child() {
            if let Ok(l) = child.downcast::<Label>() {
                l.set_wrap(true);
                l.set_max_width_chars(16);
                l.set_justify(gtk4::Justification::Center);
            }
        }
        {
            let this = self.clone();
            lbl_btn.connect_clicked(move |_| this.show_group(group_id));
        }

        tile.append(&img_btn);
        tile.append(&lbl_btn);
        tile
    }

    /// Build one group tile: a clickable face crop over a clickable label.
    ///
    /// Image click opens the group's photos. Label click, for an unnamed group,
    /// opens the name dialog. For a named group the label also opens the photos.
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
    ) -> GtkBox {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);

        let image = Image::new();
        image.set_pixel_size(tile_px);
        image.set_size_request(tile_px, tile_px);
        if face_id != 0 {
            if let Some(jpeg) = state.face_crop_jpeg(face_id) {
                if let Some(tex) = texture_from_bytes(&jpeg) {
                    image.set_paintable(Some(&tex));
                }
            }
        }
        if image.paintable().is_none() {
            image.set_icon_name(Some("avatar-default-symbolic"));
        }

        // The image opens the group's photos.
        let img_btn = Button::new();
        img_btn.set_child(Some(&image));
        img_btn.add_css_class("flat");
        {
            let state = state.clone();
            let name = name.to_string();
            img_btn.connect_clicked(move |_| {
                if named {
                    state.show_person(person_id, &name);
                } else {
                    state.show_cluster(cluster_id, "Unnamed person");
                }
            });
        }

        // The label. For an unnamed group it opens the name dialog; for a named
        // group it opens the photos.
        let group_key = if named {
            crate::db::FaceGroup::Person(person_id)
        } else {
            crate::db::FaceGroup::Cluster(cluster_id)
        };
        let new_count = state
            .face_group_new_counts
            .borrow()
            .get(&group_key)
            .copied()
            .unwrap_or(0);
        let label_text = if new_count > 0 {
            format!("{name} ({count}) +{new_count} new")
        } else {
            format!("{name} ({count})")
        };
        let lbl_btn = Button::with_label(&label_text);
        lbl_btn.add_css_class("flat");
        if let Some(child) = lbl_btn.child() {
            if let Ok(l) = child.downcast::<Label>() {
                l.set_wrap(true);
                l.set_max_width_chars(16);
                l.set_justify(gtk4::Justification::Center);
                if !named {
                    l.add_css_class("dim-label");
                }
            }
        }
        {
            let state = state.clone();
            let this = self.clone();
            let name = name.to_string();
            lbl_btn.connect_clicked(move |_| {
                if named {
                    state.show_person(person_id, &name);
                } else {
                    let this2 = this.clone();
                    let state2 = state.clone();
                    super::people::name_cluster_dialog(&state, cluster_id, move || {
                        this2.reload();
                        if let Some(sb) = state2.sidebar.borrow().as_ref() {
                            sb.reload_deferred();
                        }
                    });
                }
            });
        }

        // Right-click a named person's tile, while browsing inside a group, to
        // use their face as that group's own tile icon.
        if named {
            let scope = *self.scope.borrow();
            if scope != 0 && face_id != 0 {
                let gesture = GestureClick::new();
                gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
                let this = self.clone();
                let tile_ref = tile.clone();
                gesture.connect_pressed(move |g, _, x, y| {
                    g.set_state(gtk4::EventSequenceState::Claimed);
                    this.show_face_tile_menu(&tile_ref, face_id, scope, x, y);
                });
                tile.add_controller(gesture);
            }
        }

        tile.append(&img_btn);
        tile.append(&lbl_btn);
        tile
    }

    /// Right-click menu for a named person's tile inside a group's page:
    /// "Set face as thumbnail" makes that face the enclosing group's cover.
    fn show_face_tile_menu(self: &Rc<Self>, tile: &GtkBox, face_id: i64, group_id: i64, x: f64, y: f64) {
        let Some(state) = self.state.borrow().clone() else {
            return;
        };
        let group = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        menu.append(Some("Set face as thumbnail"), Some("facetile.set-thumb"));

        let action = gio::SimpleAction::new("set-thumb", None);
        let this = self.clone();
        action.connect_activate(move |_, _| {
            if let Err(e) = state.lib.set_person_group_cover(group_id, face_id) {
                show_error(&state, &e.to_string());
                return;
            }
            this.reload();
        });
        group.add_action(&action);

        let popover = PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_parent(tile);
        popover.insert_action_group("facetile", Some(&group));
        let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    }
}
