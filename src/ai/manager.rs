//! Optional supervision of a local `ollama serve` subprocess.

use std::process::Child;
use std::time::{Duration, Instant};

use super::client::Client;
use super::config::Config;

/// Optionally launches and supervises a local `ollama serve` process. The
/// default value is usable; call `ensure_running` before inference.
#[derive(Default)]
pub struct Manager {
    child: Option<Child>,
}

impl Manager {
    /// Make a best effort to guarantee a reachable Ollama server. If the client
    /// already detects one, do nothing. Otherwise, when `cfg.manage` is set and
    /// an ollama binary is found, start `ollama serve` and wait for readiness.
    /// Returns an error only when a server could not be made available.
    pub fn ensure_running(&mut self, cfg: &Config, c: &Client) -> Result<(), String> {
        if let Ok((true, _)) = c.detect() {
            return Ok(());
        }
        if !cfg.manage {
            return Err(format!(
                "no local AI server detected at {} (start Ollama, or enable managed mode in settings)",
                c.base_url()
            ));
        }
        let bin = if cfg.binary_path.is_empty() {
            "ollama".to_string()
        } else {
            cfg.binary_path.clone()
        };
        let child = std::process::Command::new(&bin)
            .arg("serve")
            .spawn()
            .map_err(|e| {
                format!("could not launch {bin} serve: {e} (install Ollama or set its path in settings)")
            })?;
        self.child = Some(child);

        // Wait up to ~15s for readiness.
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok((true, _)) = c.detect() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err("started ollama serve but it did not become ready in time".to_string())
    }

    /// Terminate a managed server process, if one was started.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.stop();
    }
}
