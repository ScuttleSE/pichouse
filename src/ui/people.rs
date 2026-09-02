//! People dialogs: name a face cluster, assign one face, or merge into a
//! person. The Faces view (`facesview.rs`) drives these.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Label, Orientation, Separator, StringList, Window,
};

use super::dialogs::prompt_text;
use super::state::{show_error, AppState};
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
    // Give the person a default cover face from the cluster, but only if
    // they don't already have one — merging into an existing person must
    // not clobber a cover the user already chose.
    if let Some(first) = faces.first() {
        let _ = state.lib.set_person_cover_if_unset(person_id, first.id);
    }
    Ok(())
}

/// Open a small dialog to assign one face to a person. It lists existing people
/// and offers a New person option. `on_done` runs after a change.
pub fn assign_face_dialog<F: Fn() + 'static>(state: &Rc<AppState>, face_id: i64, on_done: F) {
    let win = Window::builder()
        .title("Assign Face")
        .modal(true)
        .default_width(320)
        .build();
    if let Some(w) = state.window() {
        win.set_transient_for(Some(&w));
    }
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let people: Vec<crate::model::Person> = state
        .lib
        .persons()
        .unwrap_or_default()
        .into_iter()
        .map(|(p, _)| p)
        .collect();

    let on_done = Rc::new(on_done);

    // If this face is currently assigned, offer to remove it from that person.
    // Removing records a rejection so a re-scan never re-attaches it there.
    let current = state.lib.face_by_id(face_id).ok().flatten();
    if let Some(face) = &current {
        if face.person_id != 0 {
            let pname = state
                .lib
                .persons()
                .unwrap_or_default()
                .into_iter()
                .find(|(p, _)| p.id == face.person_id)
                .map(|(p, _)| p.name)
                .unwrap_or_default();
            let remove = Button::with_label(&format!("Not {pname} — remove from this person"));
            remove.add_css_class("destructive-action");
            root.append(&remove);
            root.append(&Separator::new(Orientation::Horizontal));
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            let person_id = face.person_id;
            remove.connect_clicked(move |_| {
                if let Err(e) = state2.lib.reject_face_from_person(face_id, person_id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                if let Some(sb) = state2.sidebar.borrow().as_ref() {
                    sb.reload_deferred();
                }
                on_done2();
                win2.close();
            });
        }
    }

    if !people.is_empty() {
        root.append(&Label::new(Some("Assign to an existing person:")));
        let labels: Vec<String> = people.iter().map(|p| p.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        root.append(&drop);
        let assign = Button::with_label("Assign");
        assign.add_css_class("suggested-action");
        root.append(&assign);
        let state2 = state.clone();
        let people2 = people.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        assign.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(p) = people2.get(idx) {
                if let Err(e) = state2.lib.set_face_person(face_id, p.id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                if let Some(sb) = state2.sidebar.borrow().as_ref() {
                    sb.reload_deferred();
                }
                on_done2();
            }
            win2.close();
        });
        root.append(&Separator::new(Orientation::Horizontal));
    }

    let new_btn = Button::with_label("New person…");
    root.append(&new_btn);
    {
        let state = state.clone();
        let win = win.clone();
        let on_done = on_done.clone();
        new_btn.connect_clicked(move |_| {
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            prompt_text(
                &state,
                Some(&win),
                "New Person",
                "Person name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    match state2.lib.create_person(&name) {
                        Ok(pid) => {
                            let _ = state2.lib.set_face_person(face_id, pid);
                            let _ = state2.lib.set_person_cover(pid, face_id);
                            if let Some(sb) = state2.sidebar.borrow().as_ref() {
                                sb.reload_deferred();
                            }
                            on_done2();
                        }
                        Err(e) => show_error(&state2, &e.to_string()),
                    }
                    win2.close();
                },
            );
        });
    }

    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    root.append(&cancel);

    win.set_child(Some(&root));
    win.present();
}

/// A dialog to name an unnamed cluster or merge it into an existing person.
/// Runs `on_done` on success.
pub fn name_cluster_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    cluster_id: i64,
    on_done: F,
) {
    let win = Window::builder()
        .title("Name Person")
        .modal(true)
        .default_width(340)
        .build();
    if let Some(w) = state.window() {
        win.set_transient_for(Some(&w));
    }
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let people: Vec<crate::model::Person> = state
        .lib
        .persons()
        .unwrap_or_default()
        .into_iter()
        .map(|(p, _)| p)
        .collect();

    let on_done = Rc::new(on_done);

    // New person.
    root.append(&Label::new(Some("Name this group as a new person:")));
    let name_btn = Button::with_label("New person…");
    name_btn.add_css_class("suggested-action");
    root.append(&name_btn);
    {
        let state = state.clone();
        let win = win.clone();
        let on_done = on_done.clone();
        name_btn.connect_clicked(move |_| {
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            prompt_text(
                &state,
                Some(&win),
                "New Person",
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
                    on_done2();
                    win2.close();
                },
            );
        });
    }

    // Merge into an existing person.
    if !people.is_empty() {
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&Label::new(Some("Or merge into an existing person:")));
        let labels: Vec<String> = people.iter().map(|p| p.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        let merge = Button::with_label("Merge");
        root.append(&drop);
        root.append(&merge);
        let state = state.clone();
        let people2 = people.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        merge.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(p) = people2.get(idx) {
                if let Err(e) = assign_cluster_to_person(&state, cluster_id, p.id) {
                    show_error(&state, &e);
                    return;
                }
                on_done2();
            }
            win2.close();
        });
    }

    let cancel = Button::with_label("Cancel");
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }
    root.append(&cancel);

    win.set_child(Some(&root));
    win.present();
}
