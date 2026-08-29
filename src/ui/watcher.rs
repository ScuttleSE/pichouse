//! inotify fast-path for library freshness (local folders only).
//!
//! This watches library roots with the OS filesystem-notification API (inotify
//! on Linux) so local changes are reflected quickly, without waiting for the
//! periodic reconcile. It is purely a latency optimization:
//!
//! - Correctness never depends on it. The periodic and startup reconciles (see
//!   `super::freshness`) are the reliable path and catch everything.
//! - inotify does NOT see changes made by other machines on a network mount
//!   (NFS/SMB). Those are caught only by the periodic reconcile. So on network
//!   drives this watcher may be silent, which is expected.
//! - If a watch cannot be added (e.g. the inotify watch limit is reached on a
//!   very large tree), we log and rely on the periodic reconcile for that tree
//!   rather than failing.
//!
//! Events are debounced (a burst such as a bulk copy is coalesced) and then the
//! affected directories are reconciled individually, which is cheap.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk4::glib;
use notify::{RecursiveMode, Watcher};

use super::state::AppState;

/// How long to collect events before reconciling (coalesces bursts).
const DEBOUNCE: Duration = Duration::from_millis(1500);

/// Start watching every library root. Spawns:
///   1. the notify watcher (its own thread), feeding raw event paths to
///   2. a debounce thread, which batches affected directories and forwards them to
///   3. a GLib channel handler on the UI thread that reconciles each directory.
///
/// The watcher is leaked (kept alive for the process lifetime) intentionally:
/// freshness watching lasts as long as the window is open.
pub fn start(state: &Rc<AppState>) {
    let roots = match state.lib.library_folders() {
        Ok(r) => r,
        Err(_) => return,
    };
    if roots.is_empty() {
        return;
    }

    // Raw events: notify thread -> debounce thread.
    let (raw_tx, raw_rx) = mpsc::channel::<PathBuf>();
    // Batched directories: debounce thread -> UI thread.
    let (ui_tx, ui_rx) = glib::MainContext::channel::<Vec<PathBuf>>(glib::Priority::DEFAULT);

    // Attach the UI-thread handler first, before the background setup. The
    // handler reconciles each affected directory and enriches new files.
    {
        let state = state.clone();
        ui_rx.attach(None, move |dirs: Vec<PathBuf>| {
            let mut added = Vec::new();
            let mut changed = false;
            for dir in dirs {
                let report = crate::reconcile::reconcile_one_dir(&state.lib, &dir);
                if report.changed() {
                    changed = true;
                    added.extend(report.added);
                }
            }
            if changed {
                super::app::reload_folders(&state);
                state.grid().reload_from_source();
                state.refresh_new_files_if_active();
                if !added.is_empty() {
                    super::enrich::enqueue(&state, added.clone());
                    super::immich::autoupload_added(&state, &added);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // Build the watcher and add the recursive watches on a BACKGROUND thread.
    //
    // The inotify backend of `notify` walks the whole tree to add one watch per
    // subdirectory. On a large library that walk takes many seconds. It MUST
    // NOT run on the main thread, or it freezes the GLib main loop and the
    // window does not appear (see the strace: a multi-second main-thread futex
    // wait). The periodic reconcile covers the window before the watcher is
    // ready, so a delayed watcher is safe.
    let root_paths: Vec<String> = roots.into_iter().map(|r| r.path).collect();
    std::thread::spawn(move || {
        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    for path in event.paths {
                        // Reconcile the containing directory of any changed path.
                        let dir = if path.is_dir() {
                            path
                        } else {
                            path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
                        };
                        let _ = raw_tx.send(dir);
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    log::warn!("file watcher unavailable ({e}); relying on periodic reconcile");
                    return;
                }
            };

        let mut any = false;
        for path in &root_paths {
            match watcher.watch(std::path::Path::new(path), RecursiveMode::Recursive) {
                Ok(()) => any = true,
                Err(e) => {
                    // e.g. watch-limit reached on a huge tree; periodic reconcile
                    // still covers this root.
                    log::warn!("cannot watch {path} ({e}); relying on periodic reconcile");
                }
            }
        }
        if !any {
            return;
        }

        // Debounce loop: collect affected dirs for DEBOUNCE, then forward a
        // batch. This thread also OWNS the watcher, so the watcher lives for the
        // process lifetime (it stops watching when dropped).
        loop {
            // Block for the first event of a batch.
            let first = match raw_rx.recv() {
                Ok(p) => p,
                Err(_) => break, // watcher dropped
            };
            let mut batch: HashSet<PathBuf> = HashSet::new();
            batch.insert(first);
            // Drain everything that arrives during the debounce window.
            let deadline = std::time::Instant::now() + DEBOUNCE;
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match raw_rx.recv_timeout(remaining) {
                    Ok(p) => {
                        batch.insert(p);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            if ui_tx.send(batch.into_iter().collect()).is_err() {
                break;
            }
        }
        // Keep the watcher alive until the thread ends.
        drop(watcher);
    });
}

