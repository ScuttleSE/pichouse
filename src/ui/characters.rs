//! Character dialogs: name a stylised face cluster or merge it into an existing
//! character. The Characters view (`charactersview.rs`) drives these.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, DropDown, Label, Orientation, Separator, StringList, Window};

use super::dialogs::prompt_text;
use super::state::{show_error, AppState};

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
    for f in &faces {
        state
            .lib
            .set_style_face_character(f.id, character_id)
            .map_err(|e| e.to_string())?;
    }
    if let Some(first) = faces.first() {
        let _ = state.lib.set_character_cover(character_id, first.id);
    }
    Ok(())
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
