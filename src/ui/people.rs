//! People dialogs: name a face cluster, assign one face, or merge into a
//! person. The Faces view (`facesview.rs`) drives these.

use std::rc::Rc;

use gtk4::prelude::*;
use std::cell::RefCell;

use gtk4::{
    Box as GtkBox, Button, DropDown, Frame, Image, Label, Orientation, PolicyType,
    ScrolledWindow, Separator, StringList, Window,
};

use super::dialogs::prompt_text;
use super::state::{show_error, AppState};
use super::util::texture_from_bytes;
/// Create a person and assign every face in every given cluster to it.
fn name_clusters(state: &Rc<AppState>, cluster_ids: &[i64], name: &str) -> Result<(), String> {
    let person_id = state
        .lib
        .create_person(name)
        .map_err(|e| e.to_string())?;
    for &c in cluster_ids {
        assign_cluster_to_person(state, c, person_id)?;
    }
    Ok(())
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
    reload_sidebar(state);
    Ok(())
}

/// Reload the sidebar so the unassigned-faces badges follow a change.
fn reload_sidebar(state: &Rc<AppState>) {
    if let Some(sb) = state.sidebar.borrow().as_ref() {
        sb.reload_deferred();
    }
}

/// Assign a list of faces to one person. Give the person a cover face when
/// the person has none.
fn assign_faces_to_person(
    state: &Rc<AppState>,
    face_ids: &[i64],
    person_id: i64,
) -> Result<(), String> {
    for &fid in face_ids {
        state
            .lib
            .set_face_person(fid, person_id)
            .map_err(|e| e.to_string())?;
    }
    if let Some(&first) = face_ids.first() {
        let _ = state.lib.set_person_cover_if_unset(person_id, first);
    }
    reload_sidebar(state);
    Ok(())
}

/// A dialog to assign a list of faces to a new person or to an existing
/// person. The album grid uses it for photos with exactly one face.
/// `photo_count` is for the label text only. `on_done` runs after a change.
pub fn assign_photos_to_person_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    face_ids: Vec<i64>,
    photo_count: usize,
    exclude_person_id: Option<i64>,
    on_done: F,
) {
    if face_ids.is_empty() {
        return;
    }
    let win = Window::builder()
        .title("Assign to Person")
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
        .filter(|p| Some(p.id) != exclude_person_id)
        .collect();
    let on_done = Rc::new(on_done);
    let face_ids = Rc::new(face_ids);

    let new_label = if photo_count > 1 {
        format!("Name these {photo_count} pictures as a new person:")
    } else {
        "Name this picture as a new person:".to_string()
    };
    root.append(&Label::new(Some(&new_label)));
    let name_btn = Button::with_label("New person…");
    name_btn.add_css_class("suggested-action");
    root.append(&name_btn);
    {
        let state = state.clone();
        let win = win.clone();
        let on_done = on_done.clone();
        let face_ids = face_ids.clone();
        name_btn.connect_clicked(move |_| {
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            let face_ids2 = face_ids.clone();
            prompt_text(&state, Some(&win), "New Person", "Person name:", "", move |name| {
                if name.trim().is_empty() {
                    return;
                }
                let res = state2
                    .lib
                    .create_person(&name)
                    .map_err(|e| e.to_string())
                    .and_then(|pid| assign_faces_to_person(&state2, &face_ids2, pid));
                if let Err(e) = res {
                    show_error(&state2, &e);
                    return;
                }
                on_done2();
                win2.close();
            });
        });
    }

    if !people.is_empty() {
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&Label::new(Some("Or assign to an existing person:")));
        let labels: Vec<String> = people.iter().map(|p| p.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let drop = DropDown::new(Some(StringList::new(&label_refs)), gtk4::Expression::NONE);
        // Pre-select the person from the last assignment, if it still exists.
        if let Some(&last) = state.lib.recent_ids(crate::db::RECENT_PERSONS_KEY).first() {
            if let Some(pos) = people.iter().position(|p| p.id == last) {
                drop.set_selected(pos as u32);
            }
        }
        let assign = Button::with_label("Assign");
        assign.add_css_class("suggested-action");
        root.append(&drop);
        root.append(&assign);
        let state = state.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        let face_ids = face_ids.clone();
        assign.connect_clicked(move |_| {
            if let Some(p) = people.get(drop.selected() as usize) {
                if let Err(e) = assign_faces_to_person(&state, &face_ids, p.id) {
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

/// A dialog to name one or more unnamed clusters as one new person, or to
/// merge them into an existing person.
/// Runs `on_done` on success.
pub fn name_cluster_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    cluster_ids: Vec<i64>,
    on_done: F,
) {
    if cluster_ids.is_empty() {
        return;
    }
    let cluster_ids = Rc::new(cluster_ids);
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
    let new_label = if cluster_ids.len() > 1 {
        format!("Name these {} groups as one new person:", cluster_ids.len())
    } else {
        "Name this group as a new person:".to_string()
    };
    root.append(&Label::new(Some(&new_label)));
    let name_btn = Button::with_label("New person…");
    name_btn.add_css_class("suggested-action");
    root.append(&name_btn);
    {
        let state = state.clone();
        let win = win.clone();
        let on_done = on_done.clone();
        let cluster_ids = cluster_ids.clone();
        name_btn.connect_clicked(move |_| {
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            let cluster_ids2 = cluster_ids.clone();
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
                    if let Err(e) = name_clusters(&state2, &cluster_ids2, &name) {
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
        let cluster_ids = cluster_ids.clone();
        merge.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(p) = people2.get(idx) {
                let res = cluster_ids
                    .iter()
                    .try_for_each(|&c| assign_cluster_to_person(&state, c, p.id));
                if let Err(e) = res {
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

/// A dialog to resolve every unidentified face in one photo at once:
/// one card per face (its crop as the title image, its own
/// assign-to-existing/new-person controls below), laid out side by side.
/// Each card resolves independently — assigning one face does not close the
/// dialog or affect the others, so a photo with several different unnamed
/// faces can be fully resolved in one sitting. `on_assigned` runs after each
/// successful per-face assignment (not once for the whole dialog), so the
/// caller's view/sidebar stays live as faces are resolved one at a time.
/// `on_closed` runs exactly once, when the dialog window closes (via "Done"
/// or the window's own close control) regardless of how many faces were
/// actually resolved, so a caller showing one of these per photo can chain to
/// the next photo's dialog. It receives the person each face ended up
/// assigned to (`None` for a face left unresolved), in the same order as
/// `face_ids`, so the caller can carry it forward as the next call's
/// `preselect`.
///
/// `preselect` pre-selects each face's existing-person dropdown, by
/// position (same order as `face_ids`) — typically the people the
/// previous photo in a batch was resolved to, so assigning the same people
/// across a run of photos needs no re-picking each time. A position past the
/// end of `face_ids`, or naming a person no longer in the list, is
/// ignored. Pass an empty `Vec` for none.
pub fn assign_faces_per_face_dialog<F: Fn() + 'static, G: Fn(Vec<Option<i64>>) + 'static>(
    state: &Rc<AppState>,
    face_ids: Vec<i64>,
    preselect: Vec<Option<i64>>,
    on_assigned: F,
    on_closed: G,
) {
    if face_ids.is_empty() {
        return;
    }
    let win = Window::builder()
        .title("Assign to Person")
        .modal(true)
        .default_width(420)
        .build();
    if let Some(w) = state.window() {
        win.set_transient_for(Some(&w));
    }
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let n = face_ids.len();
    let label_text = if n > 1 {
        format!("{n} unidentified faces in this photo — assign each:")
    } else {
        "1 unidentified face in this photo:".to_string()
    };
    root.append(&Label::new(Some(&label_text)));

    let people: Rc<Vec<crate::model::Person>> = Rc::new(
        state
            .lib
            .persons()
            .unwrap_or_default()
            .into_iter()
            .map(|(c, _)| c)
            .collect(),
    );

    let on_assigned = Rc::new(on_assigned);

    let cards = GtkBox::new(Orientation::Horizontal, 10);
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .child(&cards)
        .build();
    root.append(&scroller);

    // Per-card state the "Done" and "Swap" buttons need after the loop: each
    // face's dropdown (`None` when there are no people to pick from) and
    // its resolved person, so both can drive an assignment the same way
    // the per-card "Assign" button does.
    let mut drops: Vec<Option<DropDown>> = Vec::with_capacity(n);
    let mut resolved: Vec<Rc<RefCell<Option<i64>>>> = Vec::with_capacity(n);
    let mut ignored: Vec<Rc<std::cell::Cell<bool>>> = Vec::with_capacity(n);

    for (i, &face_id) in face_ids.iter().enumerate() {
        let frame = Frame::new(None);
        frame.add_css_class("dup-group-frame");
        let card = GtkBox::new(Orientation::Vertical, 6);
        card.set_margin_top(8);
        card.set_margin_bottom(8);
        card.set_margin_start(8);
        card.set_margin_end(8);
        card.set_width_request(150);
        frame.set_child(Some(&card));
        cards.append(&frame);

        // The face crop.
        let image = Image::new();
        image.set_pixel_size(110);
        image.set_size_request(110, 110);
        image.set_icon_name(Some("avatar-default-symbolic"));
        card.append(&image);
        if let Some(tex) = state.face_crop_jpeg(face_id).and_then(|j| texture_from_bytes(&j)) {
            image.set_paintable(Some(&tex));
        }

        // The controls: existing-person picker + "New person…". Both
        // are replaced by the confirmation label below once this face is
        // resolved.
        let controls = GtkBox::new(Orientation::Vertical, 6);
        card.append(&controls);
        let confirm = Label::new(None);
        confirm.set_wrap(true);
        confirm.set_visible(false);
        card.append(&confirm);

        let card_resolved: Rc<RefCell<Option<i64>>> = Rc::new(RefCell::new(None));
        let card_ignored: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(false));

        if !people.is_empty() {
            let labels: Vec<String> = people.iter().map(|c| c.name.clone()).collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let sl = StringList::new(&label_refs);
            let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
            if let Some(Some(cid)) = preselect.get(i) {
                if let Some(pos) = people.iter().position(|c| c.id == *cid) {
                    drop.set_selected(pos as u32);
                }
            }
            controls.append(&drop);
            let assign = Button::with_label("Assign");
            assign.add_css_class("suggested-action");
            controls.append(&assign);

            let state2 = state.clone();
            let people2 = people.clone();
            let controls2 = controls.clone();
            let confirm2 = confirm.clone();
            let on_assigned2 = on_assigned.clone();
            let drop2 = drop.clone();
            let resolved2 = card_resolved.clone();
            assign.connect_clicked(move |_| {
                let idx = drop2.selected() as usize;
                let Some(c) = people2.get(idx) else {
                    return;
                };
                if let Err(e) = state2.lib.set_face_person(face_id, c.id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                if let Some(sb) = state2.sidebar.borrow().as_ref() {
                    sb.reload_deferred();
                }
                controls2.set_visible(false);
                confirm2.set_text(&format!("Assigned to {}", c.name));
                confirm2.set_visible(true);
                *resolved2.borrow_mut() = Some(c.id);
                on_assigned2();
            });

            drops.push(Some(drop));
        } else {
            drops.push(None);
        }

        let new_btn = Button::with_label("New person…");
        controls.append(&new_btn);
        let state2 = state.clone();
        let win2 = win.clone();
        let controls2 = controls.clone();
        let confirm2 = confirm.clone();
        let on_assigned2 = on_assigned.clone();
        let resolved2 = card_resolved.clone();
        new_btn.connect_clicked(move |_| {
            let state3 = state2.clone();
            let controls3 = controls2.clone();
            let confirm3 = confirm2.clone();
            let on_assigned3 = on_assigned2.clone();
            let resolved3 = resolved2.clone();
            prompt_text(
                &state2,
                Some(&win2),
                "New Person",
                "Person name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    let cid = match state3.lib.create_person(&name) {
                        Ok(cid) => cid,
                        Err(e) => {
                            show_error(&state3, &e.to_string());
                            return;
                        }
                    };
                    if let Err(e) = state3.lib.set_face_person(face_id, cid) {
                        show_error(&state3, &e.to_string());
                        return;
                    }
                    let _ = state3.lib.set_person_cover(cid, face_id);
                    if let Some(sb) = state3.sidebar.borrow().as_ref() {
                        sb.reload_deferred();
                    }
                    controls3.set_visible(false);
                    confirm3.set_text(&format!("Assigned to {name}"));
                    confirm3.set_visible(true);
                    *resolved3.borrow_mut() = Some(cid);
                    on_assigned3();
                },
            );
        });

        // "Ignore" marks the face as ignored. The face then has no owner and
        // no group, and "Done" does not assign it.
        let ignore_btn = Button::with_label("Ignore");
        controls.append(&ignore_btn);
        {
            let state2 = state.clone();
            let controls2 = controls.clone();
            let confirm2 = confirm.clone();
            let on_assigned2 = on_assigned.clone();
            let ignored2 = card_ignored.clone();
            ignore_btn.connect_clicked(move |_| {
                if let Err(e) = state2.lib.set_face_ignored(face_id) {
                    show_error(&state2, &e.to_string());
                    return;
                }
                if let Some(sb) = state2.sidebar.borrow().as_ref() {
                    sb.reload_deferred();
                }
                controls2.set_visible(false);
                confirm2.set_text("Ignored");
                confirm2.set_visible(true);
                ignored2.set(true);
                on_assigned2();
            });
        }

        resolved.push(card_resolved);
        ignored.push(card_ignored);
    }

    // "Swap" trades the two faces' currently-selected people — handy for
    // a batch of photos with the same two people, sometimes on the left and
    // sometimes on the right, so a mis-ordered preselect can be fixed in one
    // click instead of re-picking both dropdowns.
    if n == 2 {
        if let (Some(d0), Some(d1)) = (drops[0].clone(), drops[1].clone()) {
            let swap = Button::with_label("Swap");
            root.append(&swap);
            swap.connect_clicked(move |_| {
                let a = d0.selected();
                let b = d1.selected();
                d0.set_selected(b);
                d1.set_selected(a);
            });
        }
    }

    let done = Button::with_label("Done");
    root.append(&Separator::new(Orientation::Horizontal));
    root.append(&done);
    {
        let win = win.clone();
        let state = state.clone();
        let people = people.clone();
        let on_assigned = on_assigned.clone();
        let face_ids = face_ids.clone();
        let drops = drops.clone();
        let resolved = resolved.clone();
        done.connect_clicked(move |_| {
            // Clicking "Done" with a person still selected in a card's
            // dropdown assigns it first, so the user doesn't have to click
            // "Assign" then "Done" for every face.
            for i in 0..face_ids.len() {
                if resolved[i].borrow().is_some() || ignored[i].get() {
                    continue;
                }
                let Some(drop) = &drops[i] else { continue };
                let idx = drop.selected() as usize;
                let Some(c) = people.get(idx) else { continue };
                let face_id = face_ids[i];
                if let Err(e) = state.lib.set_face_person(face_id, c.id) {
                    show_error(&state, &e.to_string());
                    continue;
                }
                if let Some(sb) = state.sidebar.borrow().as_ref() {
                    sb.reload_deferred();
                }
                *resolved[i].borrow_mut() = Some(c.id);
                on_assigned();
            }
            win.close();
        });
    }

    // Fires exactly once, however the window closes (the "Done" button just
    // calls `win.close()` above, which triggers this on its own), so a caller
    // driving one of these dialogs per photo can chain to the next, carrying
    // forward the person each face ended up assigned to.
    win.connect_close_request(move |_| {
        let final_selection: Vec<Option<i64>> = resolved.iter().map(|r| *r.borrow()).collect();
        on_closed(final_selection);
        gtk4::glib::Propagation::Proceed
    });

    win.set_child(Some(&root));
    win.present();
}
