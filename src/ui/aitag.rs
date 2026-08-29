//! AI tagging: background worker pool and single-photo tagging.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gtk4::glib;

use crate::ai::{self, Client, GenOptions};
use crate::db::Library;
use crate::model::AiStatus;
use crate::thumb;

use super::state::{show_error, show_message, AppState};

/// A status update posted from a worker to the UI thread.
enum Msg {
    Message(String),
    Progress(f64),
    Scanning(bool),
    Error(String),
    Done,
}

/// Tag the whole library.
pub fn ai_tag_library(state: &Rc<AppState>) {
    start_ai_tagging(state, 0);
}

/// Tag the folder currently shown in the grid.
pub fn ai_tag_folder(state: &Rc<AppState>) {
    let folder = *state.current_folder.borrow();
    if folder == 0 {
        show_message(
            state,
            "AI Tagging",
            "Open a library folder first, or use \"Tag Library\".",
        );
        return;
    }
    start_ai_tagging(state, folder);
}

/// Start a background AI tagging session over the photos needing tags.
fn start_ai_tagging(state: &Rc<AppState>, folder_id: i64) {
    if !state.ai_config.borrow().enabled {
        show_message(
            state,
            "AI Tagging",
            "AI tagging is disabled. Enable it in Settings → AI Tagging.",
        );
        return;
    }
    if state.ai_job.running() {
        show_message(state, "AI Tagging", "AI tagging is already running.");
        return;
    }
    let ids = match state.lib.photos_needing_tags(folder_id, false) {
        Ok(v) => v,
        Err(e) => {
            show_error(state, &e.to_string());
            return;
        }
    };
    if ids.is_empty() {
        show_message(state, "AI Tagging", "No untagged photos found.");
        return;
    }

    let cfg = state.ai_config.borrow().clone();
    let cancel = state.ai_job.begin();

    let status = state.status();
    status.set_scanning(true);
    status.set_message("Contacting AI server…");
    status.set_progress(0.0);

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);

    // Attach the receiver to update the UI.
    {
        let state = state.clone();
        rx.attach(None, move |msg| {
            let status = state.status();
            match msg {
                Msg::Message(m) => status.set_message(&m),
                Msg::Progress(p) => status.set_progress(p),
                Msg::Scanning(s) => status.set_scanning(s),
                Msg::Error(e) => show_error(&state, &e),
                Msg::Done => {
                    // The coordinator finished (completed or cancelled). Clear
                    // the controller so a new session can start, and refresh the
                    // visible tags.
                    state.ai_job.finish();
                    state.properties().reload_tags();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // Spawn a coordinator thread that ensures the server, then fans out workers.
    let lib = state.lib.clone();
    // The AI manager may need to launch a subprocess; do it under its lock.
    let manager = state.ai_manager_arc();
    std::thread::spawn(move || {
        let client = Client::new(&cfg.host, cfg.port);

        // Ensure the server is running.
        {
            let mut mgr = manager.lock().unwrap();
            if let Err(e) = mgr.ensure_running(&cfg, &client) {
                let _ = tx.send(Msg::Scanning(false));
                let _ = tx.send(Msg::Progress(-1.0));
                let _ = tx.send(Msg::Message("AI tagging unavailable".into()));
                let _ = tx.send(Msg::Error(e));
                let _ = tx.send(Msg::Done);
                return;
            }
        }

        let total = ids.len();
        let _ = tx.send(Msg::Message(format!(
            "Loading model \"{}\"… (0/{})",
            cfg.model, total
        )));

        let client = Arc::new(client);
        let cfg = Arc::new(cfg);
        let done = Arc::new(Mutex::new((0usize, 0usize))); // (done, errors)
        let started = Arc::new(Mutex::new(0usize));
        let jobs: Arc<Mutex<std::collections::VecDeque<i64>>> =
            Arc::new(Mutex::new(ids.into_iter().collect()));

        let workers = cfg.concurrency.max(1) as usize;
        let mut handles = Vec::new();
        for _ in 0..workers {
            let jobs = jobs.clone();
            let client = client.clone();
            let cfg = cfg.clone();
            let lib = lib.clone();
            let cancel = cancel.clone();
            let done = done.clone();
            let started = started.clone();
            let tx = tx.clone();
            handles.push(std::thread::spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let id = {
                    let mut q = jobs.lock().unwrap();
                    q.pop_front()
                };
                let Some(id) = id else { return };

                {
                    let mut s = started.lock().unwrap();
                    *s += 1;
                    let _ = tx.send(Msg::Message(format!("AI tagging {}/{}…", *s, total)));
                }

                let ok = tag_one_photo(&lib, &client, &cfg, &cancel, id);

                let (d, e) = {
                    let mut g = done.lock().unwrap();
                    g.0 += 1;
                    if !ok {
                        g.1 += 1;
                    }
                    (g.0, g.1)
                };
                let frac = d as f64 / total as f64;
                let _ = tx.send(Msg::Progress(frac));
                if e > 0 {
                    let _ = tx.send(Msg::Message(format!("AI tagging {}/{} ({} failed)", d, total, e)));
                } else {
                    let _ = tx.send(Msg::Message(format!("AI tagging {}/{}", d, total)));
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }

        let (d, e) = *done.lock().unwrap();
        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Progress(-1.0));
        let final_msg = if cancel.load(Ordering::Relaxed) {
            format!("AI tagging stopped ({}/{} done)", d, total)
        } else if e > 0 {
            format!("AI tagging complete: {} tagged, {} failed", d - e, e)
        } else {
            format!("AI tagging complete: {} tagged", d)
        };
        let _ = tx.send(Msg::Message(final_msg));
        let _ = tx.send(Msg::Done);
    });
}

/// Tag a single photo. Returns whether it succeeded.
fn tag_one_photo(
    lib: &Library,
    client: &Client,
    cfg: &ai::Config,
    cancel: &Arc<AtomicBool>,
    id: i64,
) -> bool {
    let p = match lib.photo_by_id(id) {
        Ok(Some(p)) => p,
        _ => return false,
    };
    let _ = lib.set_ai_status(id, AiStatus::Queued);
    let img = match thumb::encode_for_ai(std::path::Path::new(&p.path), p.orientation, cfg.max_side)
    {
        Ok(b) => b,
        Err(_) => {
            let _ = lib.set_ai_status(id, AiStatus::Error);
            return false;
        }
    };
    let opts = GenOptions {
        num_thread: cfg.num_thread,
        num_ctx: cfg.num_ctx,
        num_predict: cfg.num_predict,
        keep_alive: cfg.keep_alive.clone(),
    };
    let res = match client.generate(&cfg.model, &cfg.prompt, &img, &opts) {
        Ok(r) => r,
        Err(_) => {
            if !cancel.load(Ordering::Relaxed) {
                let _ = lib.set_ai_status(id, AiStatus::Error);
            }
            return false;
        }
    };
    let tags = ai::parse_tags(&res.response, cfg.max_tags);
    if tags.is_empty() {
        let _ = lib.set_ai_status(id, AiStatus::Done);
        return true;
    }
    if lib
        .add_photo_tags(id, &tags, crate::model::TagSource::Ai)
        .is_err()
    {
        let _ = lib.set_ai_status(id, AiStatus::Error);
        return false;
    }
    let _ = lib.set_ai_status(id, AiStatus::Done);
    true
}

/// Tag a single photo synchronously in the background (the per-photo "Tag now").
pub fn tag_one_photo_now(state: &Rc<AppState>, p: crate::model::Photo) {
    if !state.ai_config.borrow().enabled {
        show_message(
            state,
            "AI Tagging",
            "AI tagging is disabled. Enable it in Settings → AI Tagging.",
        );
        return;
    }
    let cfg = state.ai_config.borrow().clone();
    state.status().set_message(&format!("Tagging {}…", p.filename));

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |msg| {
            match msg {
                Msg::Message(m) => state.status().set_message(&m),
                Msg::Error(e) => show_error(&state, &e),
                Msg::Done => state.properties().reload_tags(),
                _ => {}
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    let manager = state.ai_manager_arc();
    let never_cancel = Arc::new(AtomicBool::new(false));
    std::thread::spawn(move || {
        let client = Client::new(&cfg.host, cfg.port);
        {
            let mut mgr = manager.lock().unwrap();
            if let Err(e) = mgr.ensure_running(&cfg, &client) {
                let _ = tx.send(Msg::Message("AI tagging unavailable".into()));
                let _ = tx.send(Msg::Error(e));
                return;
            }
        }
        tag_one_photo(&lib, &client, &cfg, &never_cancel, p.id);
        let _ = tx.send(Msg::Message(format!("Tagged {}", p.filename)));
        let _ = tx.send(Msg::Done);
    });
}
