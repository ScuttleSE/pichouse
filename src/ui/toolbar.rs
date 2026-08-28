//! Top toolbar: settings, rescan, AI menu, search, zoom slider, props toggle.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Image, Orientation, PositionType, Scale, SearchEntry,
};

use super::state::AppState;

/// Build the top toolbar.
pub fn build_toolbar(state: &Rc<AppState>) -> GtkBox {
    let settings = Button::from_icon_name("emblem-system-symbolic");
    settings.set_tooltip_text(Some("Settings"));
    {
        let state = state.clone();
        settings.connect_clicked(move |_| super::settings::show_settings(&state));
    }

    let rescan = Button::from_icon_name("view-refresh-symbolic");
    rescan.set_tooltip_text(Some("Rescan all library folders"));
    {
        let state = state.clone();
        rescan.connect_clicked(move |_| super::actions::rescan_all(&state));
    }

    let refresh = Button::from_icon_name("emblem-synchronizing-symbolic");
    refresh.set_tooltip_text(Some(
        "Refresh library (detect files added or removed on disk)",
    ));
    {
        let state = state.clone();
        refresh.connect_clicked(move |_| super::freshness::reconcile_now(&state));
    }

    // AI menu button with a popover of actions.
    let ai_btn = Button::from_icon_name("insert-image-symbolic");
    ai_btn.set_tooltip_text(Some("AI tag photos"));
    let menu = gio::Menu::new();
    menu.append(Some("Tag Current Folder"), Some("ai.tagfolder"));
    menu.append(Some("Tag Entire Library"), Some("ai.taglibrary"));
    menu.append(Some("Tag Manager…"), Some("ai.manager"));
    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&ai_btn);
    {
        let popover = popover.clone();
        ai_btn.connect_clicked(move |_| popover.popup());
    }

    let group = gio::SimpleActionGroup::new();
    {
        let state = state.clone();
        let act = gio::SimpleAction::new("tagfolder", None);
        act.connect_activate(move |_, _| super::aitag::ai_tag_folder(&state));
        group.add_action(&act);
    }
    {
        let state = state.clone();
        let act = gio::SimpleAction::new("taglibrary", None);
        act.connect_activate(move |_, _| super::aitag::ai_tag_library(&state));
        group.add_action(&act);
    }
    {
        let state = state.clone();
        let act = gio::SimpleAction::new("manager", None);
        act.connect_activate(move |_, _| super::tagmanager::show_tag_manager(&state));
        group.add_action(&act);
    }
    ai_btn.insert_action_group("ai", Some(&group));

    // Tools menu button (duplicate finder, and future tools).
    let tools_btn = Button::from_icon_name("applications-utilities-symbolic");
    tools_btn.set_tooltip_text(Some("Tools"));
    let tools_menu = gio::Menu::new();
    tools_menu.append(Some("Find Duplicates…"), Some("tools.duplicates"));
    let tools_pop = gtk4::PopoverMenu::from_model(Some(&tools_menu));
    tools_pop.set_parent(&tools_btn);
    {
        let tools_pop = tools_pop.clone();
        tools_btn.connect_clicked(move |_| tools_pop.popup());
    }
    let tools_group = gio::SimpleActionGroup::new();
    {
        let state = state.clone();
        let act = gio::SimpleAction::new("duplicates", None);
        act.connect_activate(move |_, _| super::actions::find_duplicates(&state));
        tools_group.add_action(&act);
    }
    tools_btn.insert_action_group("tools", Some(&tools_group));

    let search = SearchEntry::new();
    search.set_hexpand(true);
    {
        let state = state.clone();
        search.connect_search_changed(move |e| {
            state.grid().set_filter(&e.text());
        });
    }

    let zoom = Image::from_icon_name("zoom-in-symbolic");

    let (presets, active) = {
        let p = state.prefs.borrow();
        (p.sizes.clone(), p.active)
    };
    let slider = Scale::with_range(
        Orientation::Horizontal,
        0.0,
        (presets.len().saturating_sub(1)) as f64,
        1.0,
    );
    slider.set_draw_value(false);
    slider.set_digits(0);
    slider.set_size_request(160, -1);
    slider.set_value(active as f64);
    for i in 0..presets.len() {
        slider.add_mark(i as f64, PositionType::Bottom, None);
    }
    {
        let state = state.clone();
        slider.connect_value_changed(move |s| {
            let mut i = (s.value() + 0.5) as usize;
            let sizes_len = state.prefs.borrow().sizes.len();
            if i >= sizes_len {
                i = sizes_len - 1;
            }
            let new_size = {
                let mut prefs = state.prefs.borrow_mut();
                prefs.active = i;
                prefs.sizes[i]
            };
            let _ = state
                .lib
                .set_setting(super::prefs::KEY_THUMB_ACTIVE, &i.to_string());
            state.apply_thumb_prefs();
            // When "regenerate on slider move" is on, drop the in-memory texture
            // cache so each cell re-renders at the new size instead of scaling a
            // cached texture.
            if state.prefs.borrow().regen_on_move {
                state.grid().clear_texture_cache();
            }
            state.grid().set_thumb_size(new_size);
            // The Faces view tiles track the slider too; rebuild if it is up.
            state.refresh_faces_if_active();
        });
    }

    let props_toggle = Button::from_icon_name("sidebar-show-right-symbolic");
    props_toggle.set_tooltip_text(Some("Toggle info panel"));
    {
        let state = state.clone();
        props_toggle.connect_clicked(move |_| toggle_properties(&state));
    }

    let slideshow = build_slideshow_button(state);

    let box_ = GtkBox::new(Orientation::Horizontal, 6);
    box_.set_margin_top(6);
    box_.set_margin_bottom(6);
    box_.set_margin_start(6);
    box_.set_margin_end(6);
    box_.append(&settings);
    box_.append(&rescan);
    box_.append(&refresh);
    box_.append(&ai_btn);
    box_.append(&tools_btn);
    box_.append(&search);
    box_.append(&zoom);
    box_.append(&slider);
    box_.append(&slideshow);
    box_.append(&props_toggle);
    box_
}

/// Toggle the properties panel and persist the choice.
fn toggle_properties(state: &Rc<AppState>) {    let visible = {
        let mut prefs = state.prefs.borrow_mut();
        prefs.props_visible = !prefs.props_visible;
        prefs.props_visible
    };
    state.properties().set_visible(visible);
    let _ = state.lib.set_setting(
        super::prefs::KEY_PROPS_VISIBLE,
        super::prefs::bool_to_str(visible),
    );
}

/// Build the "Play slideshow" button. A left-click starts a slideshow of the
/// photos currently shown in the grid (or the current selection). A small
/// popover under the button sets the per-image duration, shuffle, and loop; the
/// choices persist in `library.db`.
fn build_slideshow_button(state: &Rc<AppState>) -> Button {
    use gtk4::{CheckButton, Label, Popover, SpinButton};

    let btn = Button::from_icon_name("media-playback-start-symbolic");
    btn.set_tooltip_text(Some("Play slideshow (right-click for options)"));

    // Load persisted settings.
    let secs = state
        .lib
        .get_setting(
            super::prefs::KEY_SLIDESHOW_SECS,
            &super::prefs::DEFAULT_SLIDESHOW_SECS.to_string(),
        )
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(super::prefs::DEFAULT_SLIDESHOW_SECS)
        .clamp(1, 120);
    let shuffle_on = state
        .lib
        .get_setting(super::prefs::KEY_SLIDESHOW_SHUFFLE, "0")
        .map(|v| v == "1")
        .unwrap_or(false);
    let loop_on = state
        .lib
        .get_setting(super::prefs::KEY_SLIDESHOW_LOOP, "1")
        .map(|v| v == "1")
        .unwrap_or(true);

    // Options popover.
    let secs_spin = SpinButton::with_range(1.0, 120.0, 1.0);
    secs_spin.set_value(secs as f64);
    let secs_row = GtkBox::new(Orientation::Horizontal, 6);
    secs_row.append(&Label::new(Some("Seconds per image")));
    secs_row.append(&secs_spin);
    let shuffle_chk = CheckButton::with_label("Shuffle");
    shuffle_chk.set_active(shuffle_on);
    let loop_chk = CheckButton::with_label("Loop");
    loop_chk.set_active(loop_on);

    let pbox = GtkBox::new(Orientation::Vertical, 6);
    pbox.set_margin_top(8);
    pbox.set_margin_bottom(8);
    pbox.set_margin_start(8);
    pbox.set_margin_end(8);
    pbox.append(&secs_row);
    pbox.append(&shuffle_chk);
    pbox.append(&loop_chk);
    let start_btn = Button::with_label("Start");
    start_btn.add_css_class("suggested-action");
    pbox.append(&start_btn);

    let popover = Popover::new();
    popover.set_child(Some(&pbox));
    popover.set_parent(&btn);

    // Persist each option as it changes.
    {
        let state = state.clone();
        secs_spin.connect_value_changed(move |s| {
            let _ = state.lib.set_setting(
                super::prefs::KEY_SLIDESHOW_SECS,
                &(s.value().round() as i32).to_string(),
            );
        });
    }
    {
        let state = state.clone();
        shuffle_chk.connect_toggled(move |b| {
            let _ = state.lib.set_setting(
                super::prefs::KEY_SLIDESHOW_SHUFFLE,
                super::prefs::bool_to_str(b.is_active()),
            );
        });
    }
    {
        let state = state.clone();
        loop_chk.connect_toggled(move |b| {
            let _ = state.lib.set_setting(
                super::prefs::KEY_SLIDESHOW_LOOP,
                super::prefs::bool_to_str(b.is_active()),
            );
        });
    }

    // Right-click opens the options popover.
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    {
        let popover = popover.clone();
        gesture.connect_pressed(move |_, _, _, _| popover.popup());
    }
    btn.add_controller(gesture);

    // "Start" (from the popover) and a plain left-click both start the show.
    let start = {
        let state = state.clone();
        let secs_spin = secs_spin.clone();
        let shuffle_chk = shuffle_chk.clone();
        let loop_chk = loop_chk.clone();
        move || {
            start_slideshow(
                &state,
                secs_spin.value().round() as u32,
                shuffle_chk.is_active(),
                loop_chk.is_active(),
            );
        }
    };
    {
        let start = start.clone();
        let popover = popover.clone();
        start_btn.connect_clicked(move |_| {
            popover.popdown();
            start();
        });
    }
    {
        let start = start.clone();
        btn.connect_clicked(move |_| start());
    }

    btn
}

/// Start a slideshow of the photos currently in the grid (or the current
/// selection if more than one is selected). Opens the viewer, then plays.
fn start_slideshow(state: &Rc<AppState>, secs: u32, shuffle: bool, do_loop: bool) {
    let grid = state.grid();
    let selected = grid.selected_photos();
    let photos = if selected.len() > 1 {
        selected
    } else {
        grid.visible_photos()
    };
    if photos.is_empty() {
        state
            .status()
            .set_message("Nothing to play — open a folder or album first.");
        return;
    }
    state.open_viewer(photos, 0);
    state.viewer().start_slideshow(secs, shuffle, do_loop);
}
