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

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, FlowBox, Image, Label, Orientation, PolicyType,
    ScrolledWindow,
};

use super::groupsort::{sort_groups, tile_flow, GroupSort, SectionHeader, UnnamedHeader};
use super::prefs::{KEY_FACES_IDENTIFIED_COLLAPSED, KEY_FACES_UNNAMED_COLLAPSED, KEY_FACES_UNNAMED_SORT};
use super::state::AppState;
use super::util::texture_from_bytes;

/// The Faces view widget and its rebuild logic.
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

        let flow = tile_flow();
        let uflow = tile_flow();
        // The sort menu needs the view, so the view is set after it is built.
        let weak: Rc<RefCell<std::rc::Weak<FacesView>>> = Rc::new(RefCell::new(std::rc::Weak::new()));
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
        });
        *weak.borrow_mut() = Rc::downgrade(&view);
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
        while let Some(child) = self.uflow.first_child() {
            self.uflow.remove(&child);
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
            let sort = GroupSort::from_key(
                &state.lib.get_setting(KEY_FACES_UNNAMED_SORT, "").unwrap_or_default(),
            );
            self.uheader.set_sort(sort);
            let on = |k: &str| state.lib.get_setting(k, "0").unwrap_or_default() == "1";
            self.iheader.set_collapsed(on(KEY_FACES_IDENTIFIED_COLLAPSED));
            self.uheader.header.set_collapsed(on(KEY_FACES_UNNAMED_COLLAPSED));
            let info = state.lib.unnamed_group_info(false).unwrap_or_default();
            let clusters = sort_groups(state.lib.unnamed_clusters().unwrap_or_default(), &info, sort);
            (people, clusters)
        } else {
            let member_ids = members.get(&scope).cloned().unwrap_or_default();
            let people: Vec<_> = all_people
                .into_iter()
                .filter(|(p, _)| member_ids.contains(&p.id))
                .collect();
            (people, Vec::new())
        };

        self.uheader.set_count(clusters.len());
        if subgroups.is_empty() && people.is_empty() && clusters.is_empty() {
            self.empty.set_visible(true);
            self.iheader.apply(0, false);
            return;
        }
        self.empty.set_visible(false);
        self.iheader
            .apply(subgroups.len() + people.len(), scope == 0);

        // One query each for all representative faces.
        let reps = state.lib.representative_faces(false).unwrap_or_default();
        let cluster_reps = state.lib.cluster_representative_faces(false).unwrap_or_default();

        // Sub-groups first, like folders in a file browser.
        for g in &subgroups {
            let count = members.get(&g.id).map(|m| m.len() as i64).unwrap_or(0);
            let faces = person_folder_faces(&state, g.id, &reps);
            let t = self.build_group_tile(&state, &g.name, count, g.id, &faces, tile);
            self.flow.append(&t);
        }

        // Named people next.
        for (person, count) in people {
            let face_id = reps.get(&person.id).copied().unwrap_or(0);
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

        // Unnamed clusters below the line, in the chosen sort order.
        for (cluster_id, count) in clusters {
            let face_id = cluster_reps.get(&cluster_id).copied().unwrap_or(0);
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
            self.uflow.append(&t);
        }
    }

    /// Build one sub-group ("folder") tile. Clicking either the icon or the
    /// label drills into that group's own scope. It shows a 3x3 mosaic of
    /// `faces`, else a plain folder icon.
    fn build_group_tile(
        self: &Rc<Self>,
        state: &Rc<AppState>,
        name: &str,
        count: i64,
        group_id: i64,
        faces: &[i64],
        tile_px: i32,
    ) -> GtkBox {
        let tile = GtkBox::new(Orientation::Vertical, 4);
        tile.set_width_request(tile_px + 12);

        let img_btn = Button::new();
        if faces.is_empty() {
            let image = Image::new();
            image.set_pixel_size(tile_px);
            image.set_size_request(tile_px, tile_px);
            image.set_icon_name(Some("folder-new-symbolic"));
            img_btn.set_child(Some(&image));
        } else {
            let grid = super::mosaic::build(tile_px, faces, |img, f| {
                if let Some(tex) = state.face_crop_jpeg(f).and_then(|j| texture_from_bytes(&j)) {
                    img.set_paintable(Some(&tex));
                }
            });
            img_btn.set_child(Some(&grid));
        }
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
            // Hold Alt while hovering to preview random group photos.
            let lib = state.lib.clone();
            super::altpreview::attach(&img_btn, &image, state.gen.clone(), move || {
                if named {
                    lib.photos_of_person(person_id).unwrap_or_default()
                } else {
                    lib.photos_in_cluster(cluster_id).unwrap_or_default()
                }
            });
        }
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

        tile.append(&img_btn);
        tile.append(&lbl_btn);
        tile
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
        .map(|p| (*p, all_reps.get(p).copied().unwrap_or(0)))
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
