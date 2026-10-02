//! Cancellation token shared between the UI and background workers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A cancellable job handle. `begin` starts a new session (cancelling any prior
/// one); `stop` cancels the current session; `running` reports whether a session
/// is active. The returned `Arc<AtomicBool>` is the cancel flag passed to
/// workers: when it becomes true, workers must stop.
#[derive(Default)]
pub struct Controller {
    current: Mutex<Option<Session>>,
}

/// One active session: its cancel flag and its start time.
struct Session {
    flag: Arc<AtomicBool>,
    started: Instant,
}

impl Controller {
    /// Start a new session. Cancels any previous session and returns the cancel
    /// flag for the new one (false = keep going).
    pub fn begin(&self) -> Arc<AtomicBool> {
        let mut cur = self.current.lock().unwrap();
        if let Some(old) = cur.take() {
            old.flag.store(true, Ordering::Relaxed);
        }
        let flag = Arc::new(AtomicBool::new(false));
        *cur = Some(Session {
            flag: flag.clone(),
            started: Instant::now(),
        });
        flag
    }

    /// Mark the current session finished (without cancelling it).
    pub fn finish(&self) {
        let mut cur = self.current.lock().unwrap();
        *cur = None;
    }

    /// Whether `flag` belongs to the current session. A cancelled session
    /// that `begin` replaced is not current.
    pub fn is_current(&self, flag: &Arc<AtomicBool>) -> bool {
        matches!(self.current.lock().unwrap().as_ref(), Some(cur) if Arc::ptr_eq(&cur.flag, flag))
    }

    /// Whether a session is currently active.
    pub fn running(&self) -> bool {
        self.current.lock().unwrap().is_some()
    }

    /// The time since the current session started, if a session is active.
    pub fn elapsed(&self) -> Option<Duration> {
        self.current.lock().unwrap().as_ref().map(|s| s.started.elapsed())
    }

    /// Whether the current session is active and its cancel flag is set.
    pub fn stopping(&self) -> bool {
        matches!(self.current.lock().unwrap().as_ref(), Some(s) if s.flag.load(Ordering::Relaxed))
    }

    /// Cancel the current session, if any.
    pub fn stop(&self) {
        let cur = self.current.lock().unwrap();
        if let Some(s) = cur.as_ref() {
            s.flag.store(true, Ordering::Relaxed);
        }
    }
}
