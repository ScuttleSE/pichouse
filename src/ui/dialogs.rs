//! Small modal dialogs: text prompt and yes/no confirmation.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, Label, Orientation, Window};

use super::state::AppState;

/// Prompt for a line of text. `on_ok` is called with the entered text if the
/// user confirms and the text is non-empty.
pub fn prompt_text<F: Fn(String) + 'static>(
    state: &Rc<AppState>,
    parent: Option<&Window>,
    title: &str,
    label: &str,
    initial: &str,
    on_ok: F,
) {
    let entry = Entry::new();
    entry.set_text(initial);
    entry.set_hexpand(true);

    let msg = Label::new(Some(label));
    msg.set_xalign(0.0);
    msg.set_wrap(true);

    let ok = Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = Button::with_label("Cancel");

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&cancel);
    buttons.append(&ok);

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&msg);
    root.append(&entry);
    root.append(&buttons);

    let window = Window::builder()
        .title(title)
        .modal(true)
        .default_width(360)
        .child(&root)
        .build();
    let parent_win = parent.cloned().or_else(|| state.window().map(|w| w.upcast::<Window>()));
    if let Some(p) = &parent_win {
        window.set_transient_for(Some(p));
    }

    let on_ok = Rc::new(on_ok);
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        let on_ok = on_ok.clone();
        let entry_for_ok = entry.clone();
        ok.connect_clicked(move |_| {
            let text = entry_for_ok.text().to_string();
            if !text.trim().is_empty() {
                on_ok(text);
            }
            window.close();
        });
    }
    {
        let window = window.clone();
        let on_ok = on_ok.clone();
        entry.connect_activate(move |e| {
            let text = e.text().to_string();
            if !text.trim().is_empty() {
                on_ok(text);
            }
            window.close();
        });
    }

    window.set_visible(true);
}

/// Ask a yes/no question. `on_yes` is called if the user confirms.
pub fn confirm<F: Fn() + 'static>(
    state: &Rc<AppState>,
    parent: Option<&Window>,
    title: &str,
    detail: &str,
    on_yes: F,
) {
    let msg = Label::new(Some(detail));
    msg.set_xalign(0.0);
    msg.set_wrap(true);

    let yes = Button::with_label("Yes");
    yes.add_css_class("destructive-action");
    let no = Button::with_label("Cancel");

    let buttons = GtkBox::new(Orientation::Horizontal, 6);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&no);
    buttons.append(&yes);

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&msg);
    root.append(&buttons);

    let window = Window::builder()
        .title(title)
        .modal(true)
        .default_width(360)
        .child(&root)
        .build();
    let parent_win = parent.cloned().or_else(|| state.window().map(|w| w.upcast::<Window>()));
    if let Some(p) = &parent_win {
        window.set_transient_for(Some(p));
    }

    {
        let window = window.clone();
        no.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        let on_yes = Rc::new(on_yes);
        yes.connect_clicked(move |_| {
            on_yes();
            window.close();
        });
    }

    window.set_visible(true);
}
