//! Cancellation token shared between the UI and background workers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A cancellable job handle. `begin` starts a new session (cancelling any prior
/// one); `stop` cancels the current session; `running` reports whether a session
/// is active. The returned `Arc<AtomicBool>` is the cancel flag passed to
/// workers: when it becomes true, workers must stop.
#[derive(Default)]
pub struct Controller {
    current: Mutex<Option<Arc<AtomicBool>>>,
}

impl Controller {
    /// Start a new session. Cancels any previous session and returns the cancel
    /// flag for the new one (false = keep going).
    pub fn begin(&self) -> Arc<AtomicBool> {
        let mut cur = self.current.lock().unwrap();
        if let Some(old) = cur.take() {
            old.store(true, Ordering::Relaxed);
        }
        let flag = Arc::new(AtomicBool::new(false));
        *cur = Some(flag.clone());
        flag
    }

    /// Mark the current session finished (without cancelling it).
    pub fn finish(&self) {
        let mut cur = self.current.lock().unwrap();
        *cur = None;
    }

    /// Whether a session is currently active.
    pub fn running(&self) -> bool {
        self.current.lock().unwrap().is_some()
    }

    /// Cancel the current session, if any.
    pub fn stop(&self) {
        let cur = self.current.lock().unwrap();
        if let Some(flag) = cur.as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}
