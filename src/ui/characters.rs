//! Character dialogs: name a stylised face cluster or merge it into an existing
//! character. The Characters view (`charactersview.rs`) drives these.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Frame, Image, Label, Orientation, PolicyType,
    ScrolledWindow, Separator, StringList, Window,
};

use super::dialogs::prompt_text;
use super::state::{show_error, queue_crop_job, AppState, CropJob};
use super::util::texture_from_bytes;

/// Create a character and assign every face in every given cluster to it.
fn name_style_clusters(
    state: &Rc<AppState>,
    cluster_ids: &[i64],
    name: &str,
) -> Result<(), String> {
    let character_id = state
        .lib
        .create_character(name)
        .map_err(|e| e.to_string())?;
    for &cid in cluster_ids {
        assign_style_cluster_to_character(state, cid, character_id)?;
    }
    Ok(())
}

/// Assign every face in every given cluster to an existing character.
fn assign_style_clusters_to_character(
    state: &Rc<AppState>,
    cluster_ids: &[i64],
    character_id: i64,
) -> Result<(), String> {
    for &cid in cluster_ids {
        assign_style_cluster_to_character(state, cid, character_id)?;
    }
    Ok(())
}

/// Assign every unassigned face in a style cluster to a character.
fn assign_style_cluster_to_character(
    state: &Rc<AppState>,
    cluster_id: i64,
    character_id: i64,
) -> Result<(), String> {
    let faces = state
        .lib
        .unassigned_style_faces_in_cluster(cluster_id)
        .map_err(|e| e.to_string())?;
    let face_ids: Vec<i64> = faces.iter().map(|f| f.id).collect();
    assign_style_faces_to_character(state, &face_ids, character_id)
}

/// Assign the given stylised faces to a character, then give the character a
/// default cover from the first face (callers pass faces already ordered
/// best-first) — but only if they don't already have one, so assigning more
/// faces to an existing character never clobbers a cover the user chose.
fn assign_style_faces_to_character(
    state: &Rc<AppState>,
    face_ids: &[i64],
    character_id: i64,
) -> Result<(), String> {
    for &fid in face_ids {
        state
            .lib
            .set_style_face_character(fid, character_id)
            .map_err(|e| e.to_string())?;
    }
    if let Some(&first) = face_ids.first() {
        let _ = state.lib.set_character_cover_if_unset(character_id, first);
    }
    Ok(())
}

/// Create a character and assign the given stylised faces to it.
fn name_style_faces(state: &Rc<AppState>, face_ids: &[i64], name: &str) -> Result<(), String> {
    let character_id = state
        .lib
        .create_character(name)
        .map_err(|e| e.to_string())?;
    assign_style_faces_to_character(state, face_ids, character_id)
}

/// A dialog to assign one stylised face to a character, or to remove it from
/// its current character. Removing records a rejection so a re-scan never
/// re-attaches it there. Runs `on_done` on success.
pub fn assign_style_face_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    face_id: i64,
    on_done: F,
) {
    let win = Window::builder()
        .title("Assign Character")
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

    let characters: Vec<crate::model::Character> = state
        .lib
        .characters()
        .unwrap_or_default()
        .into_iter()
        .map(|(c, _)| c)
        .collect();

    let on_done = Rc::new(on_done);

    // If this face is currently assigned, offer to remove it from that
    // character. Removing records a rejection so a re-scan never re-attaches it.
    let current = state.lib.style_face_by_id(face_id).ok().flatten();
    if let Some(face) = &current {
        if face.character_id != 0 {
            let cname = characters
                .iter()
                .find(|c| c.id == face.character_id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let remove =
                Button::with_label(&format!("Not {cname} — remove from this character"));
            remove.add_css_class("destructive-action");
            root.append(&remove);
            root.append(&Separator::new(Orientation::Horizontal));
            let state2 = state.clone();
            let win2 = win.clone();
            let on_done2 = on_done.clone();
            let character_id = face.character_id;
            remove.connect_clicked(move |_| {
                if let Err(e) = state2
                    .lib
                    .reject_style_face_from_character(face_id, character_id)
                {
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

    if !characters.is_empty() {
        root.append(&Label::new(Some("Assign to an existing character:")));
        let labels: Vec<String> = characters.iter().map(|c| c.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        root.append(&drop);
        let assign = Button::with_label("Assign");
        assign.add_css_class("suggested-action");
        root.append(&assign);
        let state2 = state.clone();
        let chars2 = characters.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        assign.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(c) = chars2.get(idx) {
                if let Err(e) = state2.lib.set_style_face_character(face_id, c.id) {
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

    let new_btn = Button::with_label("New character…");
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
                "New Character",
                "Character name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    match state2.lib.create_character(&name) {
                        Ok(cid) => {
                            let _ = state2.lib.set_style_face_character(face_id, cid);
                            let _ = state2.lib.set_character_cover(cid, face_id);
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

/// A dialog to name a set of individually-selected stylised faces (e.g. from
/// photos picked out of a mixed "Unnamed character" group, or misplaced
/// photos picked out of a named character's own group) as one new character,
/// or assign them to an existing character. Runs `on_done` on success.
/// `photo_count` is the number of distinct photos the faces came from, used
/// only for the label text — a photo can contribute more than one face (e.g.
/// two instances of the same character in one image). `exclude_character_id`
/// hides one character from the "existing character" list — the character
/// the faces are currently assigned to, when reassigning away from it.
pub fn assign_photos_to_character_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    face_ids: Vec<i64>,
    photo_count: usize,
    exclude_character_id: Option<i64>,
    on_done: F,
) {
    if face_ids.is_empty() {
        return;
    }
    let win = Window::builder()
        .title("Assign to Character")
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

    let characters: Vec<crate::model::Character> = state
        .lib
        .characters()
        .unwrap_or_default()
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| Some(c.id) != exclude_character_id)
        .collect();

    let on_done = Rc::new(on_done);
    let face_ids = Rc::new(face_ids);

    let new_label = if photo_count > 1 {
        format!("Name these {photo_count} pictures as a new character:")
    } else {
        "Name this picture as a new character:".to_string()
    };
    root.append(&Label::new(Some(&new_label)));
    let name_btn = Button::with_label("New character…");
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
            prompt_text(
                &state,
                Some(&win),
                "New Character",
                "Character name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    if let Err(e) = name_style_faces(&state2, &face_ids2, &name) {
                        show_error(&state2, &e);
                        return;
                    }
                    on_done2();
                    win2.close();
                },
            );
        });
    }

    if !characters.is_empty() {
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&Label::new(Some("Or assign to an existing character:")));
        let labels: Vec<String> = characters.iter().map(|c| c.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        // Pre-select the character from the last assignment, if it still exists.
        if let Some(last) = *state.last_merged_character.borrow() {
            if let Some(pos) = characters.iter().position(|c| c.id == last) {
                drop.set_selected(pos as u32);
            }
        }
        let assign = Button::with_label("Assign");
        assign.add_css_class("suggested-action");
        root.append(&drop);
        root.append(&assign);
        let state = state.clone();
        let chars2 = characters.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        let face_ids = face_ids.clone();
        assign.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(c) = chars2.get(idx) {
                if let Err(e) = assign_style_faces_to_character(&state, &face_ids, c.id) {
                    show_error(&state, &e);
                    return;
                }
                *state.last_merged_character.borrow_mut() = Some(c.id);
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

/// A dialog to resolve every unidentified stylised face in one photo at once:
/// one card per face (its crop as the title image, its own
/// assign-to-existing/new-character controls below), laid out side by side.
/// Each card resolves independently — assigning one face does not close the
/// dialog or affect the others, so a photo with several different unnamed
/// faces can be fully resolved in one sitting. `on_assigned` runs after each
/// successful per-face assignment (not once for the whole dialog), so the
/// caller's view/sidebar stays live as faces are resolved one at a time.
/// `on_closed` runs exactly once, when the dialog window closes (via "Done"
/// or the window's own close control) regardless of how many faces were
/// actually resolved, so a caller showing one of these per photo can chain to
/// the next photo's dialog. It receives the character each face ended up
/// assigned to (`None` for a face left unresolved), in the same order as
/// `face_ids`, so the caller can carry it forward as the next call's
/// `preselect`.
///
/// `preselect` pre-selects each face's existing-character dropdown, by
/// position (same order as `face_ids`) — typically the characters the
/// previous photo in a batch was resolved to, so assigning the same people
/// across a run of photos needs no re-picking each time. A position past the
/// end of `face_ids`, or naming a character no longer in the list, is
/// ignored. Pass an empty `Vec` for none.
pub fn assign_style_faces_per_face_dialog<F: Fn() + 'static, G: Fn(Vec<Option<i64>>) + 'static>(
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
        .title("Assign to Character")
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

    let characters: Rc<Vec<crate::model::Character>> = Rc::new(
        state
            .lib
            .characters()
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
    // face's dropdown (`None` when there are no characters to pick from) and
    // its resolved character, so both can drive an assignment the same way
    // the per-card "Assign" button does.
    let mut drops: Vec<Option<DropDown>> = Vec::with_capacity(n);
    let mut resolved: Vec<Rc<RefCell<Option<i64>>>> = Vec::with_capacity(n);

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

        // The face crop, rendered the same way as a Characters-view tile.
        let image = Image::new();
        image.set_pixel_size(110);
        image.set_size_request(110, 110);
        image.set_icon_name(Some("avatar-default-symbolic"));
        card.append(&image);
        if let Some(jpeg) = state.style_face_crop_cached(face_id) {
            if let Some(tex) = texture_from_bytes(&jpeg) {
                image.set_paintable(Some(&tex));
            }
        } else if let Some((path, orientation, bbox)) = state.style_face_crop_inputs(face_id) {
            let thumbs = state.style_face_thumbs();
            let (tx, rx) =
                gtk4::glib::MainContext::channel::<Option<Vec<u8>>>(gtk4::glib::Priority::DEFAULT);
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
                gtk4::glib::ControlFlow::Break
            });
        }

        // The controls: existing-character picker + "New character…". Both
        // are replaced by the confirmation label below once this face is
        // resolved.
        let controls = GtkBox::new(Orientation::Vertical, 6);
        card.append(&controls);
        let confirm = Label::new(None);
        confirm.set_wrap(true);
        confirm.set_visible(false);
        card.append(&confirm);

        let card_resolved: Rc<RefCell<Option<i64>>> = Rc::new(RefCell::new(None));

        if !characters.is_empty() {
            let labels: Vec<String> = characters.iter().map(|c| c.name.clone()).collect();
            let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let sl = StringList::new(&label_refs);
            let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
            if let Some(Some(cid)) = preselect.get(i) {
                if let Some(pos) = characters.iter().position(|c| c.id == *cid) {
                    drop.set_selected(pos as u32);
                }
            }
            controls.append(&drop);
            let assign = Button::with_label("Assign");
            assign.add_css_class("suggested-action");
            controls.append(&assign);

            let state2 = state.clone();
            let characters2 = characters.clone();
            let controls2 = controls.clone();
            let confirm2 = confirm.clone();
            let on_assigned2 = on_assigned.clone();
            let drop2 = drop.clone();
            let resolved2 = card_resolved.clone();
            assign.connect_clicked(move |_| {
                let idx = drop2.selected() as usize;
                let Some(c) = characters2.get(idx) else {
                    return;
                };
                if let Err(e) = state2.lib.set_style_face_character(face_id, c.id) {
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

        let new_btn = Button::with_label("New character…");
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
                "New Character",
                "Character name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    let cid = match state3.lib.create_character(&name) {
                        Ok(cid) => cid,
                        Err(e) => {
                            show_error(&state3, &e.to_string());
                            return;
                        }
                    };
                    if let Err(e) = state3.lib.set_style_face_character(face_id, cid) {
                        show_error(&state3, &e.to_string());
                        return;
                    }
                    let _ = state3.lib.set_character_cover(cid, face_id);
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

        resolved.push(card_resolved);
    }

    // "Swap" trades the two faces' currently-selected characters — handy for
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
        let characters = characters.clone();
        let on_assigned = on_assigned.clone();
        let face_ids = face_ids.clone();
        let drops = drops.clone();
        let resolved = resolved.clone();
        done.connect_clicked(move |_| {
            // Clicking "Done" with a character still selected in a card's
            // dropdown assigns it first, so the user doesn't have to click
            // "Assign" then "Done" for every face.
            for i in 0..face_ids.len() {
                if resolved[i].borrow().is_some() {
                    continue;
                }
                let Some(drop) = &drops[i] else { continue };
                let idx = drop.selected() as usize;
                let Some(c) = characters.get(idx) else { continue };
                let face_id = face_ids[i];
                if let Err(e) = state.lib.set_style_face_character(face_id, c.id) {
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
    // forward the character each face ended up assigned to.
    win.connect_close_request(move |_| {
        let final_selection: Vec<Option<i64>> = resolved.iter().map(|r| *r.borrow()).collect();
        on_closed(final_selection);
        gtk4::glib::Propagation::Proceed
    });

    win.set_child(Some(&root));
    win.present();
}

/// A dialog to name one or more unnamed style clusters as one new character, or
/// to merge them all into an existing character. Runs `on_done` on success.
pub fn name_style_clusters_dialog<F: Fn() + 'static>(
    state: &Rc<AppState>,
    cluster_ids: Vec<i64>,
    on_done: F,
) {
    if cluster_ids.is_empty() {
        return;
    }
    let multi = cluster_ids.len() > 1;
    let win = Window::builder()
        .title("Name Character")
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

    let characters: Vec<crate::model::Character> = state
        .lib
        .characters()
        .unwrap_or_default()
        .into_iter()
        .map(|(c, _)| c)
        .collect();

    let on_done = Rc::new(on_done);
    let cluster_ids = Rc::new(cluster_ids);

    // New character.
    let new_label = if multi {
        format!("Name these {} groups as one new character:", cluster_ids.len())
    } else {
        "Name this group as a new character:".to_string()
    };
    root.append(&Label::new(Some(&new_label)));
    let name_btn = Button::with_label("New character…");
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
                "New Character",
                "Character name:",
                "",
                move |name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    if let Err(e) = name_style_clusters(&state2, &cluster_ids2, &name) {
                        show_error(&state2, &e);
                        return;
                    }
                    on_done2();
                    win2.close();
                },
            );
        });
    }

    // Merge into an existing character.
    if !characters.is_empty() {
        root.append(&Separator::new(Orientation::Horizontal));
        root.append(&Label::new(Some("Or merge into an existing character:")));
        let labels: Vec<String> = characters.iter().map(|c| c.name.clone()).collect();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let sl = StringList::new(&label_refs);
        let drop = DropDown::new(Some(sl), gtk4::Expression::NONE);
        // Pre-select the character from the last merge, if it still exists.
        if let Some(last) = *state.last_merged_character.borrow() {
            if let Some(pos) = characters.iter().position(|c| c.id == last) {
                drop.set_selected(pos as u32);
            }
        }
        let merge = Button::with_label("Merge");
        root.append(&drop);
        root.append(&merge);
        let state = state.clone();
        let chars2 = characters.clone();
        let win2 = win.clone();
        let on_done2 = on_done.clone();
        let cluster_ids = cluster_ids.clone();
        merge.connect_clicked(move |_| {
            let idx = drop.selected() as usize;
            if let Some(c) = chars2.get(idx) {
                if let Err(e) = assign_style_clusters_to_character(&state, &cluster_ids, c.id) {
                    show_error(&state, &e);
                    return;
                }
                *state.last_merged_character.borrow_mut() = Some(c.id);
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
