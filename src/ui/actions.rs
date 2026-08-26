//! Library scanning actions with cancellation, a shared queue, and progress.

use std::rc::Rc;

use gtk4::glib;

use crate::scan::{ScanError, Scanner};

use super::state::{show_error, show_message, AppState};

/// A scan status update posted from the worker to the UI thread.
enum Msg {
    Message(String),
    Progress(f64),
    Scanning(bool),
    Reload,
    Error(String),
    Finished,
}

/// Add a library folder, then scan it. If a scan is already running, the new
/// folder is appended to the scan queue instead of cancelling the running scan.
pub fn add_library_folder(state: &Rc<AppState>, path: &str) {
    if let Err(e) = state.lib.add_library_folder(path) {
        show_error(state, &e.to_string());
        return;
    }
    super::app::reload_folders(state);
    enqueue_scan(state, vec![path.to_string()]);
}

/// Rescan all library folders.
pub fn rescan_all(state: &Rc<AppState>) {
    let folders = match state.lib.library_folders() {
        Ok(f) => f,
        Err(e) => {
            show_error(state, &e.to_string());
            return;
        }
    };
    if folders.is_empty() {
        show_message(state, "Rescan", "No library folders to rescan.");
        return;
    }
    let paths = folders.into_iter().map(|f| f.path).collect();
    enqueue_scan(state, paths);
}

/// Append paths to the scan queue and start a scan worker if none is running.
fn enqueue_scan(state: &Rc<AppState>, paths: Vec<String>) {
    {
        let mut q = state.scan_queue.lock().unwrap();
        for p in paths {
            if !q.contains(&p) {
                q.push_back(p);
            }
        }
    }
    if !state.scan.running() {
        start_scan_worker(state);
    }
}

/// Start the background scan worker. It drains the shared queue, so folders
/// added mid-scan are picked up without cancelling the running scan.
fn start_scan_worker(state: &Rc<AppState>) {
    let cancel = state.scan.begin();
    let status = state.status();
    status.set_scanning(true);

    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);
    {
        let state = state.clone();
        rx.attach(None, move |msg| {
            let status = state.status();
            match msg {
                Msg::Message(m) => status.set_message(&m),
                Msg::Progress(p) => status.set_progress(p),
                Msg::Scanning(s) => status.set_scanning(s),
                Msg::Reload => {
                    super::app::reload_folders(&state);
                    // Re-query the visible folder so newly scanned photos appear.
                    state.grid().reload_from_source();
                    // Kick off Phase 2 enrichment for any structure-only photos.
                    super::enrich::ensure_running(&state);
                }
                Msg::Error(e) => show_error(&state, &e),
                Msg::Finished => state.scan.finish(),
            }
            glib::ControlFlow::Continue
        });
    }

    let lib = state.lib.clone();
    let queue = state.scan_queue_arc();
    std::thread::spawn(move || {
        let scanner = Scanner::new(&lib);
        let mut scan_err: Option<String> = None;
        let mut cancelled = false;

        // Drain the queue, including paths appended while scanning.
        loop {
            let path = {
                let mut q = queue.lock().unwrap();
                q.pop_front()
            };
            let Some(path) = path else { break };

            let _ = tx.send(Msg::Message(format!("Scanning {path}")));
            let _ = tx.send(Msg::Progress(0.0));
            let tx_progress = tx.clone();
            let result = scanner.scan_folder(
                std::path::Path::new(&path),
                &cancel,
                move |p| {
                    let frac = if p.total > 0 {
                        p.done as f64 / p.total as f64
                    } else {
                        0.0
                    };
                    let _ = tx_progress.send(Msg::Progress(frac));
                    let _ = tx_progress.send(Msg::Message(format!(
                        "Scanning {} ({}/{})",
                        p.folder, p.done, p.total
                    )));
                    // Periodically refresh the sidebars so newly discovered
                    // folders appear during a long scan, not only at the end.
                    if p.done % 200 == 0 {
                        let _ = tx_progress.send(Msg::Reload);
                    }
                },
            );
            match result {
                Ok(_) => {
                    // Auto-organize the just-scanned tree into the Library view,
                    // then refresh the sidebars incrementally.
                    super::albumtree::sync_disk_tree(&lib, &path);
                    let _ = tx.send(Msg::Reload);
                }
                Err(ScanError::Cancelled(_)) => {
                    cancelled = true;
                    break;
                }
                Err(e) => {
                    scan_err = Some(e.to_string());
                    break;
                }
            }
        }

        let _ = tx.send(Msg::Scanning(false));
        let _ = tx.send(Msg::Progress(-1.0));
        let _ = tx.send(Msg::Reload);
        if cancelled {
            let _ = tx.send(Msg::Message("Scan stopped".into()));
        } else if let Some(e) = scan_err {
            let _ = tx.send(Msg::Message("Scan failed".into()));
            let _ = tx.send(Msg::Error(e));
        } else {
            let _ = tx.send(Msg::Message("Scan complete".into()));
        }
        let _ = tx.send(Msg::Finished);
    });
}
