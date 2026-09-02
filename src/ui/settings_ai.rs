//! AI Tagging settings pane.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, Entry, Label, Orientation, Separator, SpinButton,
};

use super::prefs;
use super::state::AppState;

/// Build the AI Tagging settings pane. Each control writes both the in-memory
/// config and the corresponding db setting on change.
pub fn ai_pane(state: &Rc<AppState>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 8);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let intro = Label::new(Some(
        "Tag photos with a local Ollama vision model. Nothing leaves your machine.",
    ));
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    root.append(&intro);

    let cfg = state.ai_config.borrow().clone();

    let enabled = CheckButton::with_label("Enable AI tagging");
    enabled.set_active(cfg.enabled);
    {
        let state = state.clone();
        enabled.connect_toggled(move |b| {
            state.ai_config.borrow_mut().enabled = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_AI_ENABLED, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&enabled);

    // Host + port.
    let host_row = GtkBox::new(Orientation::Horizontal, 6);
    host_row.append(&fixed_label("Host", 90));
    let host_entry = Entry::new();
    host_entry.set_text(&cfg.host);
    host_entry.set_hexpand(true);
    {
        let state = state.clone();
        host_entry.connect_changed(move |e| {
            state.ai_config.borrow_mut().host = e.text().to_string();
            let _ = state.lib.set_setting(prefs::KEY_AI_HOST, &e.text());
        });
    }
    let port_entry = Entry::new();
    port_entry.set_max_length(6);
    port_entry.set_text(&cfg.port.to_string());
    {
        let state = state.clone();
        port_entry.connect_changed(move |e| {
            if let Ok(n) = e.text().parse::<u16>() {
                if n > 0 {
                    state.ai_config.borrow_mut().port = n;
                    let _ = state.lib.set_setting(prefs::KEY_AI_PORT, &n.to_string());
                }
            }
        });
    }
    host_row.append(&host_entry);
    host_row.append(&Label::new(Some("Port")));
    host_row.append(&port_entry);
    root.append(&host_row);

    // Model.
    let model_row = GtkBox::new(Orientation::Horizontal, 6);
    model_row.append(&fixed_label("Model", 90));
    let model_entry = Entry::new();
    model_entry.set_text(&cfg.model);
    model_entry.set_hexpand(true);
    {
        let state = state.clone();
        model_entry.connect_changed(move |e| {
            state.ai_config.borrow_mut().model = e.text().to_string();
            let _ = state.lib.set_setting(prefs::KEY_AI_MODEL, &e.text());
        });
    }
    model_row.append(&model_entry);
    root.append(&model_row);

    root.append(&spin_row(
        state,
        "Concurrency",
        1.0,
        16.0,
        1.0,
        cfg.concurrency as f64,
        prefs::KEY_AI_CONCURRENCY,
        |c, v| c.concurrency = v,
    ));
    root.append(&spin_row(
        state,
        "CPU threads",
        0.0,
        128.0,
        1.0,
        cfg.num_thread as f64,
        prefs::KEY_AI_NUM_THREAD,
        |c, v| c.num_thread = v,
    ));
    root.append(&spin_row(
        state,
        "Context size",
        0.0,
        32768.0,
        256.0,
        cfg.num_ctx as f64,
        prefs::KEY_AI_NUM_CTX,
        |c, v| c.num_ctx = v,
    ));
    root.append(&spin_row(
        state,
        "Max tokens",
        16.0,
        4096.0,
        16.0,
        cfg.num_predict as f64,
        prefs::KEY_AI_NUM_PREDICT,
        |c, v| c.num_predict = v,
    ));

    root.append(&Separator::new(Orientation::Horizontal));

    let manage = CheckButton::with_label("Let pichouse start Ollama automatically when needed");
    manage.set_active(cfg.manage);
    {
        let state = state.clone();
        manage.connect_toggled(move |b| {
            state.ai_config.borrow_mut().manage = b.is_active();
            let _ = state
                .lib
                .set_setting(prefs::KEY_AI_MANAGE, prefs::bool_to_str(b.is_active()));
        });
    }
    root.append(&manage);

    let bin_row = GtkBox::new(Orientation::Horizontal, 6);
    bin_row.append(&fixed_label("ollama path", 90));
    let bin_entry = Entry::new();
    bin_entry.set_placeholder_text(Some("(search PATH)"));
    bin_entry.set_text(&cfg.binary_path);
    bin_entry.set_hexpand(true);
    {
        let state = state.clone();
        bin_entry.connect_changed(move |e| {
            state.ai_config.borrow_mut().binary_path = e.text().to_string();
            let _ = state.lib.set_setting(prefs::KEY_AI_BINARY, &e.text());
        });
    }
    bin_row.append(&bin_entry);
    root.append(&bin_row);

    root.append(&Separator::new(Orientation::Horizontal));

    let test = Button::with_label("Test Connection");
    let status = Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);
    {
        let state = state.clone();
        let status = status.clone();
        test.connect_clicked(move |_| {
            let cfg = state.ai_config.borrow().clone();
            status.set_text("Checking…");
            let (tx, rx) =
                glib::MainContext::channel::<String>(glib::Priority::DEFAULT);
            std::thread::spawn(move || {
                let client = crate::ai::Client::new(&cfg.host, cfg.port);
                let msg = match client.detect() {
                    Ok((true, models)) => {
                        let present = models.iter().any(|m| {
                            m == &cfg.model || m.starts_with(&format!("{}:", cfg.model))
                        });
                        if present {
                            format!("Server OK. Model \"{}\" is installed.", cfg.model)
                        } else {
                            format!(
                                "Server OK, but model \"{}\" is not installed. Run: ollama pull {}",
                                cfg.model, cfg.model
                            )
                        }
                    }
                    _ => format!(
                        "No Ollama server found at {}:{}. Start Ollama or enable managed mode.",
                        cfg.host, cfg.port
                    ),
                };
                let _ = tx.send(msg);
            });
            let status = status.clone();
            rx.attach(None, move |msg| {
                status.set_text(&msg);
                glib::ControlFlow::Break
            });
        });
    }
    let test_row = GtkBox::new(Orientation::Horizontal, 6);
    test_row.append(&test);
    test_row.append(&status);
    root.append(&test_row);

    root
}

fn spin_row(
    state: &Rc<AppState>,
    caption: &str,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
    key: &'static str,
    apply: fn(&mut crate::ai::Config, i32),
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.append(&fixed_label(caption, 90));
    let spin = SpinButton::with_range(min, max, step);
    spin.set_value(initial);
    {
        let state = state.clone();
        spin.connect_value_changed(move |s| {
            let v = s.value() as i32;
            apply(&mut state.ai_config.borrow_mut(), v);
            let _ = state.lib.set_setting(key, &v.to_string());
        });
    }
    row.append(&spin);
    row
}

fn fixed_label(text: &str, width: i32) -> Label {
    let l = Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_size_request(width, -1);
    l
}
