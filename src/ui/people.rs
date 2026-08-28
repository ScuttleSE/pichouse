//! People review dialog: name unnamed face clusters and merge people.
//!
//! It lists the largest unnamed clusters. Each row shows a few face crops and a
//! Name button. Naming a cluster creates a person and assigns every face in the
//! cluster to that person. A dropdown also merges the cluster into an existing
//! person.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Label, Orientation, Picture, ScrolledWindow, Separator,
    StringList, Window,
};

use super::dialogs::prompt_text;
use super::state::{show_error, AppState};
use super::util::texture_from_bytes;

/// How many face crops to show per cluster row.
const CROPS_PER_ROW: usize = 6;

/// Open the People review dialog.
pub fn open_people_review(state: &Rc<AppState>) {
    let win = Window::builder()
        .title("Review People")
        .modal(true)
        .default_width(560)
        .default_height(520)
        .build();
    if let Some(w) = state.window() {
        win.set_transient_for(Some(&w));
    }

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let heading = Label::new(Some("Unnamed people found by face grouping"));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    root.append(&heading);

    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    let list = GtkBox::new(Orientation::Vertical, 12);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    populate(state, &list, &win);

    let close = Button::with_label("Close");
    close.set_halign(gtk4::Align::End);
    {
        let win = win.clone();
        close.connect_clicked(move |_| win.close());
    }
    root.append(&close);

    win.set_child(Some(&root));
    win.present();
}

/// Fill the list with one row per unnamed cluster.
fn populate(state: &Rc<AppState>, list: &GtkBox, win: &Window) {
    // Clear existing rows.
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let clusters = state.lib.unnamed_clusters().unwrap_or_default();
    if clusters.is_empty() {
        let empty = Label::new(Some(
            "No unnamed groups. Run a face scan, or every group is already named.",
        ));
        empty.set_xalign(0.0);
        list.append(&empty);
        return;
    }

    // Existing people for the merge dropdown.
    let people: Vec<crate::model::Person> = state
        .lib
        .persons()
        .unwrap_or_default()
        .into_iter()
        .map(|(p, _)| p)
        .collect();

    for (cluster_id, count) in clusters {
        let row = build_cluster_row(state, cluster_id, count, &people, list, win);
        list.append(&row);
        list.append(&Separator::new(Orientation::Horizontal));
    }
}

/// Build one cluster row: face crops plus Name and Merge controls.
fn build_cluster_row(
    state: &Rc<AppState>,
    cluster_id: i64,
    count: i64,
    people: &[crate::model::Person],
    list: &GtkBox,
    win: &Window,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 6);

    let title = Label::new(Some(&format!("{count} faces")));
    title.set_xalign(0.0);
    row.append(&title);

    // Face crops.
    let crops = GtkBox::new(Orientation::Horizontal, 6);
    let faces = state
        .lib
        .unassigned_faces_in_cluster(cluster_id)
        .unwrap_or_default();
    for face in faces.iter().take(CROPS_PER_ROW) {
        if let Some(jpeg) = state.face_crop_jpeg(face.id) {
            if let Some(tex) = texture_from_bytes(&jpeg) {
                let pic = Picture::for_paintable(&tex);
                pic.set_size_request(72, 72);
                pic.set_content_fit(gtk4::ContentFit::Cover);
                crops.append(&pic);
            }
        }
    }
    row.append(&crops);

    // Actions: Name, and Merge into an existing person.
    let actions = GtkBox::new(Orientation::Horizontal, 6);
    let name_btn = Button::with_label("Name this person…");
    actions.append(&name_btn);

    {
        let state = state.clone();
        let list = list.clone();
        let win = win.clone();
        name_btn.connect_clicked(move |_| {
            let state2 = state.clone();
            let list2 = list.clone();
            let win2 = win.clone();
            prompt_text(
                &state,
                Some(&win),
                "Name Person",
                "Person name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    if let Err(e) = name_cluster(&state2, cluster_id, &name) {
                        show_error(&state2, &e);
                        return;
                    }
                    refresh_after_change(&state2, &list2, &win2);
                },
            );
        });
    }

    if !people.is_empty() {
        let labels: Vec<String> = people.iter().map(|p| p.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        let merge_btn = Button::with_label("Merge into");
        actions.append(&drop);
        actions.append(&merge_btn);

        let state = state.clone();
        let people = people.to_vec();
        let list = list.clone();
        let win = win.clone();
        merge_btn.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            let Some(person) = people.get(idx) else { return };
            if let Err(e) = assign_cluster_to_person(&state, cluster_id, person.id) {
                show_error(&state, &e);
                return;
            }
            refresh_after_change(&state, &list, &win);
        });
    }

    row.append(&actions);
    row
}

/// Create a person and assign every face in the cluster to it.
fn name_cluster(state: &Rc<AppState>, cluster_id: i64, name: &str) -> Result<(), String> {
    let person_id = state
        .lib
        .create_person(name)
        .map_err(|e| e.to_string())?;
    assign_cluster_to_person(state, cluster_id, person_id)
}

/// Assign every unassigned face in a cluster to a person.
fn assign_cluster_to_person(
    state: &Rc<AppState>,
    cluster_id: i64,
    person_id: i64,
) -> Result<(), String> {
    let faces = state
        .lib
        .unassigned_faces_in_cluster(cluster_id)
        .map_err(|e| e.to_string())?;
    for f in &faces {
        state
            .lib
            .set_face_person(f.id, person_id)
            .map_err(|e| e.to_string())?;
    }
    // Give the person a cover face from the cluster.
    if let Some(first) = faces.first() {
        let _ = state.lib.set_person_cover(person_id, first.id);
    }
    Ok(())
}

/// Rebuild the dialog list and refresh the sidebar after a change.
fn refresh_after_change(state: &Rc<AppState>, list: &GtkBox, win: &Window) {
    populate(state, list, win);
    if let Some(sb) = state.sidebar.borrow().as_ref() {
        sb.reload_deferred();
    }
}
