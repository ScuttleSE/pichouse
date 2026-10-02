//! Library freshness: run reconciliation and drive the enrichment of anything
//! it finds.
//!
//! Reconciliation is the reliable path that works on every filesystem,
//! including network mounts where inotify never sees remote changes. It runs:
//!   - once on startup (catches everything that changed while closed),
//!   - on demand (a "Refresh library" action),
//!   - on a periodic timer (the only mechanism that catches remote NFS/SMB
//!     changes, and the fallback when inotify is unavailable).

use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;

use crate::reconcile::{self, Report};

use super::state::AppState;

/// The periodic reconciliation interval. A background timer reconciles the whole
/// library this often so remote changes on network drives are picked up.
const PERIODIC: Duration = Duration::from_secs(180);

/// A message posted from the reconcile worker to the UI thread.
enum Msg {
    Done(Report),
}

/// Run a full reconciliation in the background, then enqueue anything new for
/// enrichment and refresh the view. No-op if a reconciliation is already
/// running (avoids overlapping walks; the next timer tick will catch up).
/// `trigger` names the caller in the log.
pub fn reconcile_now(state: &Rc<AppState>, trigger: &str) {
    run_reconcile(state, trigger, false, false);
}

/// Refresh Library: a user request. It cancels a running reconciliation (for
/// example the startup run) and starts a new one.
pub fn refresh_library(state: &Rc<AppState>) {
    run_reconcile(state, "refresh", false, true);
}

/// Tools > Scan for New Folders: run the same reconciliation on demand. It
/// shows a status message at the start and reports an empty result. It cancels
/// a running reconciliation (for example the startup run) and starts a new one.
pub fn scan_new_folders(state: &Rc<AppState>) {
    run_reconcile(state, "manual", true, true);
}

/// Shared reconcile run. `announce` adds start and empty-result messages. The
/// periodic timer and "Refresh Library" stay silent when nothing changed.
/// `preempt` cancels a running reconciliation. Without it, the call is skipped.
fn run_reconcile(state: &Rc<AppState>, trigger: &str, announce: bool, preempt: bool) {
    if state.reconcile_job.running() {
        if !preempt {
            log::debug!("reconcile ({trigger}): skipped, a reconcile is already running");
            return;
        }
        log::info!("reconcile ({trigger}): cancels the running reconcile");
        // A joined manual scan must still report its result.
        let announce = announce || state.reconcile_announce.get();
        state.reconcile_announce.set(announce);
    } else {
        state.reconcile_announce.set(announce);
    }
    log::info!("reconcile ({trigger}): start");
    if announce {
        state.status().set_message("Scanning for new folders");
    }
    // `begin` cancels the old session. Its worker stops at the next check.
    let cancel = state.reconcile_job.begin();
    let (tx, rx) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT);

    {
        let state = state.clone();
        let session = cancel.clone();
        rx.attach(None, move |Msg::Done(report)| {
            // A newer reconcile replaced this one. Do not touch the job
            // state or the UI. The newer run owns both.
            if !state.reconcile_job.is_current(&session) {
                log::debug!("reconcile: old cancelled run ended");
                return glib::ControlFlow::Break;
            }
            state.reconcile_job.finish();
            let announce = state.reconcile_announce.replace(false);
            if session.load(std::sync::atomic::Ordering::Relaxed) {
                state.status().set_message_transient("Library scan cancelled.", 5);
                return glib::ControlFlow::Break;
            }
            if report.changed() {
                super::app::reload_folders(&state);
                state.grid().reload_from_source();
                state.refresh_new_files_if_active();
                if !report.added.is_empty() {
                    // Enrichment never starts on its own; browsing or Tools >
                    // Generate Thumbnails produces thumbnails for added files.
                    super::immich::autoupload_added(&state, &report.added);
                }
                // Auto-scan faces when the user opted in, routed by album kind.
                // A photo goes to the human or the stylised method based on its
                // album's Face type, so nothing is scanned twice. The scan reads
                // photos_needing_*_face_scan, which only list enriched photos, so
                // this picks up newly added photos once enrichment finishes.
                {
                    let want_face = {
                        let fc = state.face_config.borrow();
                        fc.enabled && fc.autoscan
                    };
                    let want_style = {
                        let sc = state.style_face_config.borrow();
                        sc.enabled && sc.autoscan
                    };
                    if want_face || want_style {
                        super::albumscan::autoscan_routed(&state, want_face, want_style);
                    }
                }
                let mut parts = Vec::new();
                if !report.added.is_empty() {
                    parts.push(format!("{} added", report.added.len()));
                }
                if report.missing > 0 {
                    parts.push(format!("{} missing", report.missing));
                }
                if report.reappeared > 0 {
                    parts.push(format!("{} back", report.reappeared));
                }
                if report.moved > 0 {
                    parts.push(format!("{} moved", report.moved));
                }
                if report.removed > 0 {
                    parts.push(format!("{} removed", report.removed));
                }
                state
                    .status()
                    .set_message(&format!("Library updated: {}", parts.join(", ")));
            } else if announce {
                state.status().set_message("No new folders or files found.");
            }
            glib::ControlFlow::Break
        });
    }

    let lib = state.lib.clone();
    std::thread::spawn(move || {
        // Always send a result. If the walk panics, the UI must still clear
        // the job state. Otherwise every later scan reports "already running".
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reconcile::reconcile_all(&lib, &cancel)
        }));
        let report = result.unwrap_or_else(|_| {
            log::error!("reconcile: worker thread panicked. The job state is cleared.");
            Report::default()
        });
        let _ = tx.send(Msg::Done(report));
    });
}

/// Start the periodic reconciliation timer. Runs on the GLib main loop, so it
/// simply kicks `reconcile_now` on each tick.
pub fn start_periodic(state: &Rc<AppState>) {
    let state = state.clone();
    glib::timeout_add_local(PERIODIC, move || {
        reconcile_now(&state, "timer");
        glib::ControlFlow::Continue
    });
}
