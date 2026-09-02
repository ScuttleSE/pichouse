//! Top toolbar: settings, rescan, AI menu, search, zoom slider, props toggle.

use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Image, Orientation, PositionType, Scale, SearchEntry,
};

use super::state::AppState;

/// Build the top toolbar.
pub fn build_toolbar(state: &Rc<AppState>) -> GtkBox {
    let settings = compact_button("emblem-system-symbolic", "Settings");
    {
        let state = state.clone();
        settings.connect_clicked(move |_| super::settings::show_settings(&state));
    }

    // Tools menu button. Holds rescan, the new-folder scan, refresh, slideshow,
    // AI tagging, and the duplicate finder.
    let tools_btn = compact_button("applications-utilities-symbolic", "Tools");
    let tools_menu = gio::Menu::new();

    let library_section = gio::Menu::new();
    library_section.append(Some("Rescan All Folders"), Some("tools.rescan"));
    library_section.append(Some("Scan for New Folders"), Some("tools.scan_new_folders"));
    library_section.append(Some("Refresh Library"), Some("tools.refresh"));
    library_section.append(Some("Generate Thumbnails"), Some("tools.gen_thumbs"));
    tools_menu.append_section(None, &library_section);

    let view_section = gio::Menu::new();
    view_section.append(Some("Play Slideshow"), Some("tools.slideshow"));
    view_section.append(Some("Show Filenames"), Some("tools.show_filenames"));
    // Sort submenu. Each item targets the sort action with a string value; the
    // action state carries the active order so GTK draws the check mark.
    let sort_menu = gio::Menu::new();
    for (label, value) in [
        ("Date (newest first)", "date"),
        ("Date (oldest first)", "date_asc"),
        ("Name (A–Z)", "name_asc"),
        ("Name (Z–A)", "name_desc"),
        ("Size (largest first)", "size_desc"),
        ("Size (smallest first)", "size_asc"),
    ] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(
            Some("tools.sort"),
            Some(&value.to_variant()),
        );
        sort_menu.append_item(&item);
    }
    view_section.append_submenu(Some("Sort By"), &sort_menu);
    tools_menu.append_section(None, &view_section);

    let ai_menu = gio::Menu::new();
    ai_menu.append(Some("Tag Current Folder"), Some("tools.ai_tagfolder"));
    ai_menu.append(Some("Tag Entire Library"), Some("tools.ai_taglibrary"));
    ai_menu.append(Some("Tag Manager…"), Some("tools.ai_manager"));
    tools_menu.append_submenu(Some("AI Tag"), &ai_menu);

    let tools_section = gio::Menu::new();
    tools_section.append(Some("Find Duplicates…"), Some("tools.duplicates"));
    tools_menu.append_section(None, &tools_section);

    let tools_pop = gtk4::PopoverMenu::from_model(Some(&tools_menu));
    tools_pop.set_parent(&tools_btn);
    {
        let tools_pop = tools_pop.clone();
        tools_btn.connect_clicked(move |_| tools_pop.popup());
    }
    let tools_group = gio::SimpleActionGroup::new();
    let add_action = |name: &str, f: Box<dyn Fn()>| {
        let act = gio::SimpleAction::new(name, None);
        act.connect_activate(move |_, _| f());
        tools_group.add_action(&act);
    };
    add_action("rescan", {
        let state = state.clone();
        Box::new(move || super::actions::rescan_all(&state))
    });
    add_action("scan_new_folders", {
        let state = state.clone();
        Box::new(move || super::freshness::scan_new_folders(&state))
    });
    add_action("refresh", {
        let state = state.clone();
        Box::new(move || super::freshness::reconcile_now(&state))
    });
    add_action("slideshow", {
        let state = state.clone();
        Box::new(move || start_slideshow_from_prefs(&state))
    });
    add_action("ai_tagfolder", {
        let state = state.clone();
        Box::new(move || super::aitag::ai_tag_folder(&state))
    });
    add_action("ai_taglibrary", {
        let state = state.clone();
        Box::new(move || super::aitag::ai_tag_library(&state))
    });
    add_action("ai_manager", {
        let state = state.clone();
        Box::new(move || super::tagmanager::show_tag_manager(&state))
    });
    add_action("duplicates", {
        let state = state.clone();
        Box::new(move || super::actions::find_duplicates(&state))
    });

    // Stateful "Show Filenames" toggle. Initial state mirrors the grid setting.
    {
        let show_now = state.grid().show_filenames();
        let act = gio::SimpleAction::new_stateful(
            "show_filenames",
            None,
            &show_now.to_variant(),
        );
        let state2 = state.clone();
        act.connect_activate(move |act, _| {
            let on = act.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            let new = !on;
            act.set_state(&new.to_variant());
            state2.grid().set_show_filenames(new);
        });
        tools_group.add_action(&act);
    }

    // Stateful "Sort By" radio action. The state string is the active order.
    {
        let order_now = state.grid().sort_order_setting().to_string();
        let act = gio::SimpleAction::new_stateful(
            "sort",
            Some(glib::VariantTy::STRING),
            &order_now.to_variant(),
        );
        let state2 = state.clone();
        act.connect_activate(move |act, param| {
            let Some(value) = param.and_then(|p| p.get::<String>()) else {
                return;
            };
            act.set_state(&value.to_variant());
            let order = super::grid::SortOrder::from_setting(&value);
            state2.grid().set_sort_order(order);
        });
        tools_group.add_action(&act);
    }
    // Stateful "Generate Thumbnails" toggle. Off starts a nice (throttled) full
    // enrichment pass; on stops it. The pass is in-memory only: stopping the app
    // discards it, and the user re-runs this to continue. The state mirrors
    // whether an enrichment pass is currently running.
    {
        let running_now = super::enrich::running(state);
        let act = gio::SimpleAction::new_stateful(
            "gen_thumbs",
            None,
            &running_now.to_variant(),
        );
        let state2 = state.clone();
        act.connect_activate(move |act, _| {
            let on = act.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            if on {
                super::enrich::stop(&state2);
                act.set_state(&false.to_variant());
            } else {
                super::enrich::generate_all(&state2);
                act.set_state(&true.to_variant());
            }
        });
        tools_group.add_action(&act);
        *state.gen_thumbs_action.borrow_mut() = Some(act);
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

    let props_toggle = compact_button("sidebar-show-right-symbolic", "Toggle info panel");
    {
        let state = state.clone();
        props_toggle.connect_clicked(move |_| toggle_properties(&state));
    }

    let box_ = GtkBox::new(Orientation::Horizontal, 6);
    box_.set_margin_top(6);
    box_.set_margin_bottom(6);
    box_.set_margin_start(6);
    box_.set_margin_end(6);
    box_.append(&settings);
    box_.append(&tools_btn);
    box_.append(&search);
    box_.append(&zoom);
    box_.append(&slider);
    box_.append(&props_toggle);
    box_
}

/// Build a small toolbar button from an icon. The button has no frame, so it
/// takes only as much space as the icon needs.
fn compact_button(icon: &str, tooltip: &str) -> Button {
    let btn = Button::from_icon_name(icon);
    btn.set_tooltip_text(Some(tooltip));
    btn.set_has_frame(false);
    btn
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

/// Start a slideshow with the options saved in `library.db`. The Slideshow
/// settings pane sets these options.
fn start_slideshow_from_prefs(state: &Rc<AppState>) {
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
    start_slideshow(state, secs as u32, shuffle_on, loop_on);
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
