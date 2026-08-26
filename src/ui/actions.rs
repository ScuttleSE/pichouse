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

        // Cumulative progress across every folder drained in this session, so
        // the counter and bar reflect the whole job, not just the current
        // folder. `base_done` is the number of photos finished in prior
        // folders; `prior_total` is the sum of prior folders' image counts.
        let mut base_done: usize = 0;
        let mut prior_total: usize = 0;

        // Drain the queue, including paths appended while scanning.
        loop {
            let path = {
                let mut q = queue.lock().unwrap();
                q.pop_front()
            };
            let Some(path) = path else { break };

            let _ = tx.send(Msg::Message(format!("Scanning {path}")));
            let tx_progress = tx.clone();
            // Per-folder running counts, read back after the folder completes.
            let this_done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let this_total = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let this_done_cb = this_done.clone();
            let this_total_cb = this_total.clone();
            let result = scanner.scan_folder(
                std::path::Path::new(&path),
                &cancel,
                move |p| {
                    use std::sync::atomic::Ordering;
                    this_done_cb.store(p.done, Ordering::Relaxed);
                    this_total_cb.store(p.total, Ordering::Relaxed);
                    // Cumulative across the whole session.
                    let done = base_done + p.done;
                    let total = prior_total + p.total;
                    let frac = if total > 0 {
                        done as f64 / total as f64
                    } else {
                        0.0
                    };
                    let _ = tx_progress.send(Msg::Progress(frac));
                    let _ = tx_progress.send(Msg::Message(format!(
                        "Scanning {} ({}/{})",
                        p.folder, done, total
                    )));
                    // Periodically refresh the sidebars so newly discovered
                    // folders appear during a long scan, not only at the end.
                    if p.done % 200 == 0 {
                        let _ = tx_progress.send(Msg::Reload);
                    }
                },
            );
            // Fold this folder's counts into the cumulative totals.
            {
                use std::sync::atomic::Ordering;
                base_done += this_done.load(Ordering::Relaxed);
                prior_total += this_total.load(Ordering::Relaxed);
            }
            match result {
                Ok(_) => {
                    // Record that this root's first scan is complete, so files
                    // added later count as "new".
                    let _ = lib.mark_first_scan_done(&path);
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
