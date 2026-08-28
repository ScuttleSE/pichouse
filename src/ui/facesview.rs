//! The Faces view: browse detected people and unnamed face groups.
//!
//! Shown in the center stack when the user selects the People header in the
//! Library sidebar. It shows one tile per group: named people first, then the
//! largest unnamed clusters. A named tile opens that person's photos. An
//! unnamed tile opens the name/assign dialog. The scan refreshes this view as
//! groups appear.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FlowBox, Image, Label, Orientation, PolicyType, ScrolledWindow,
    SelectionMode,
};

use super::state::AppState;
use super::util::texture_from_bytes;

/// The Faces view widget and its rebuild logic.
pub struct FacesView {
    root: GtkBox,
    flow: FlowBox,
    empty: Label,
    state: RefCell<Option<Rc<AppState>>>,
}

impl FacesView {
    /// Build the view. `bind_state` must be called once before use.
    pub fn new() -> Rc<FacesView> {
        let root = GtkBox::new(Orientation::Vertical, 0);

        // A small header bar with a manage action.
        let bar = GtkBox::new(Orientation::Horizontal, 6);
        bar.set_margin_top(8);
        bar.set_margin_bottom(4);
        bar.set_margin_start(8);
        bar.set_margin_end(8);
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

        Rc::new(FacesView {
            root,
            flow,
            empty,
            state: RefCell::new(None),
        })
    }

    pub fn bind_state(self: &Rc<Self>, state: Rc<AppState>) {
        *self.state.borrow_mut() = Some(state);
    }

    /// The view root widget.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Rebuild the group tiles from the database. Safe to call repeatedly, so
    /// the scan can refresh this view as new groups appear.
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

        let people = state.lib.persons().unwrap_or_default();
        let clusters = state.lib.unnamed_clusters().unwrap_or_default();

        if people.is_empty() && clusters.is_empty() {
            self.empty.set_visible(true);
            self.flow.set_visible(false);
            return;
        }
        self.empty.set_visible(false);
        self.flow.set_visible(true);

        // Named people first.
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
        let label_text = format!("{name} ({count})");
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
