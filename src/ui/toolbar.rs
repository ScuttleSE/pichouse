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
            state.grid().set_thumb_size(new_size);
        });
    }

    let props_toggle = Button::from_icon_name("sidebar-show-right-symbolic");
    props_toggle.set_tooltip_text(Some("Toggle info panel"));
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
    box_.append(&rescan);
    box_.append(&ai_btn);
    box_.append(&search);
    box_.append(&zoom);
    box_.append(&slider);
    box_.append(&props_toggle);
    box_
}

/// Toggle the properties panel and persist the choice.
fn toggle_properties(state: &Rc<AppState>) {
    let visible = {
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
