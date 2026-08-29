//! AI tagging configuration and defaults.

use std::time::Duration;

/// Default Ollama host.
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// Default Ollama port.
pub const DEFAULT_PORT: u16 = 11434;
/// Default vision model.
pub const DEFAULT_MODEL: &str = "moondream";
/// Longest image side (px) sent to the model. Vision models downscale
/// internally; a compact image keeps requests fast.
pub const DEFAULT_MAX_SIDE: i32 = 768;
/// Parallel inference bound. For a single local GPU, serial requests are best.
pub const DEFAULT_CONCURRENCY: i32 = 1;
/// How many keywords are kept per image.
pub const DEFAULT_MAX_TAGS: i32 = 25;
/// Keep the model resident between requests to avoid reload CPU spikes.
pub const DEFAULT_KEEP_ALIVE: &str = "10m";
/// Cap on tokens the model may generate per image.
pub const DEFAULT_NUM_PREDICT: i32 = 128;

/// Asks the model for a plain comma-separated keyword list.
pub const DEFAULT_PROMPT: &str = "List the main visual keywords for this image as a \
comma-separated list of short lowercase tags (objects, people, scene, setting, colors, \
mood). Do not write sentences. Output only the tags.";

/// The per-request ceiling for a single inference.
pub(super) const HTTP_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// AI tagging settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub model: String,
    pub prompt: String,
    pub max_side: i32,
    pub max_tags: i32,
    pub concurrency: i32,
    /// CPU threads Ollama uses for CPU-side inference. 0 lets Ollama decide.
    pub num_thread: i32,
    /// Context window (`options.num_ctx`). 0 uses the model default.
    pub num_ctx: i32,
    /// Cap on generated tokens per image (`options.num_predict`). Always
    /// positive after `normalize`.
    pub num_predict: i32,
    /// Keeps the model resident between requests (e.g. "10m").
    pub keep_alive: String,
    /// Launch a local `ollama serve` subprocess when no server is running.
    pub manage: bool,
    /// Override the ollama binary location (empty = search PATH).
    pub binary_path: String,
}

impl Default for Config {
    /// A disabled configuration with sensible defaults.
    fn default() -> Config {
        Config {
            enabled: false,
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            model: DEFAULT_MODEL.to_string(),
            prompt: DEFAULT_PROMPT.to_string(),
            max_side: DEFAULT_MAX_SIDE,
            max_tags: DEFAULT_MAX_TAGS,
            concurrency: DEFAULT_CONCURRENCY,
            num_thread: 0,
            num_ctx: 0,
            num_predict: DEFAULT_NUM_PREDICT,
            keep_alive: DEFAULT_KEEP_ALIVE.to_string(),
            manage: false,
            binary_path: String::new(),
        }
    }
}

impl Config {
    /// Fill empty fields with defaults and clamp ranges.
    pub fn normalize(&mut self) {
        if self.host.is_empty() {
            self.host = DEFAULT_HOST.to_string();
        }
        if self.port == 0 {
            self.port = DEFAULT_PORT;
        }
        if self.model.is_empty() {
            self.model = DEFAULT_MODEL.to_string();
        }
        if self.prompt.is_empty() {
            self.prompt = DEFAULT_PROMPT.to_string();
        }
        if self.max_side <= 0 {
            self.max_side = DEFAULT_MAX_SIDE;
        }
        if self.max_tags <= 0 {
            self.max_tags = DEFAULT_MAX_TAGS;
        }
        if self.concurrency <= 0 {
            self.concurrency = DEFAULT_CONCURRENCY;
        }
        if self.keep_alive.is_empty() {
            self.keep_alive = DEFAULT_KEEP_ALIVE.to_string();
        }
        if self.num_thread < 0 {
            self.num_thread = 0;
        }
        if self.num_ctx < 0 {
            self.num_ctx = 0;
        }
        if self.num_predict <= 0 {
            self.num_predict = DEFAULT_NUM_PREDICT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fills_defaults_and_clamps() {
        let mut c = Config {
            host: String::new(),
            port: 0,
            model: String::new(),
            prompt: String::new(),
            max_side: 0,
            max_tags: 0,
            concurrency: 0,
            num_thread: -5,
            num_ctx: -1,
            num_predict: 0,
            keep_alive: String::new(),
            ..Config::default()
        };
        c.normalize();
        assert_eq!(c.host, DEFAULT_HOST);
        assert_eq!(c.port, DEFAULT_PORT);
        assert_eq!(c.model, DEFAULT_MODEL);
        assert!(c.concurrency >= 1 && c.max_tags >= 1 && c.max_side >= 1);
        assert_eq!(c.num_thread, 0);
        assert_eq!(c.num_ctx, 0);
        assert_eq!(c.num_predict, DEFAULT_NUM_PREDICT);
    }
}
