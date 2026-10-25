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

/// The periodic reconciliation interval. The time counts from the end of the
/// last reconcile, so two reconciles never run back to back.
const PERIODIC: Duration = Duration::from_secs(15 * 60);

/// How often the timer checks whether the interval has passed.
const TICK: Duration = Duration::from_secs(60);

thread_local! {
    /// When the last reconcile ended (on the UI thread).
    static LAST_END: std::cell::Cell<Option<std::time::Instant>> =
        const { std::cell::Cell::new(None) };
}

/// A message posted from the reconcile worker to the UI thread.
enum Msg {
    Done(Report),
}

/// Run a full reconciliation in the background, then enqueue anything new for
/// enrichment and refresh the view. No-op if a reconciliation is already
/// running (avoids overlapping walks; the next timer tick will catch up).
/// `trigger` names the caller in the log.
pub fn reconcile_now(state: &Rc<AppState>, trigger: &str) {
    run_reconcile(state, trigger, false, false, false);
}

/// Refresh Library: a user request. It cancels a running reconciliation (for
/// example the startup run) and starts a new one.
pub fn refresh_library(state: &Rc<AppState>) {
    run_reconcile(state, "refresh", false, true, false);
}

/// Tools > Full Library Check: read every folder and ignore the stored
/// folder mtimes. Slow, but it finds every change.
pub fn full_check(state: &Rc<AppState>) {
    run_reconcile(state, "full", true, true, true);
}

/// Tools > Scan for New Folders: run the same reconciliation on demand. It
/// shows a status message at the start and reports an empty result. It cancels
/// a running reconciliation (for example the startup run) and starts a new one.
pub fn scan_new_folders(state: &Rc<AppState>) {
    run_reconcile(state, "manual", true, true, false);
}

/// Shared reconcile run. `announce` adds start and empty-result messages. The
/// periodic timer and "Refresh Library" stay silent when nothing changed.
/// `preempt` cancels a running reconciliation. Without it, the call is skipped.
/// `full` reads every folder (see `reconcile::reconcile_all_with`).
fn run_reconcile(state: &Rc<AppState>, trigger: &str, announce: bool, preempt: bool, full: bool) {
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
            LAST_END.with(|c| c.set(Some(std::time::Instant::now())));
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
    let spawned = std::thread::Builder::new().name("reconcile".into()).spawn(move || {
        lower_priority();
        // Always send a result. If the walk panics, the UI must still clear
        // the job state. Otherwise every later scan reports "already running".
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reconcile::reconcile_all_with(&lib, &cancel, full)
        }));
        let report = result.unwrap_or_else(|_| {
            log::error!("reconcile: worker thread panicked. The job state is cleared.");
            Report::default()
        });
        let _ = tx.send(Msg::Done(report));
    });
    if let Err(e) = spawned {
        log::error!("reconcile: cannot start the worker thread: {e}");
        state.reconcile_job.finish();
    }
}

/// Give the calling thread a low CPU priority (nice 10) and the idle I/O
/// class. Then the walk does not slow down the UI.
fn lower_priority() {
    // SAFETY: both calls change only the calling thread. They take no pointers.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        if libc::setpriority(libc::PRIO_PROCESS, tid, 10) != 0 {
            log::debug!("reconcile: setpriority failed");
        }
        // IOPRIO_WHO_PROCESS = 1, IOPRIO_CLASS_IDLE = 3, class shift = 13.
        if libc::syscall(libc::SYS_ioprio_set, 1, tid, 3 << 13) != 0 {
            log::debug!("reconcile: ioprio_set failed");
        }
    }
}

/// Start the periodic reconciliation timer. Runs on the GLib main loop, so it
/// simply kicks `reconcile_now` on each tick.
pub fn start_periodic(state: &Rc<AppState>) {
    let state = state.clone();
    glib::timeout_add_local(TICK, move || {
        let due = LAST_END.with(|c| c.get().map_or(true, |t| t.elapsed() >= PERIODIC));
        if due && !state.reconcile_job.running() {
            reconcile_now(&state, "timer");
        }
        glib::ControlFlow::Continue
    });
}
