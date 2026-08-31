//! Blocking HTTP client for a local Ollama server.

use std::time::Duration;

use serde::Deserialize;

use super::config::{DEFAULT_HOST, DEFAULT_PORT, HTTP_TIMEOUT};

/// An AI client error.
#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    Status(u16),
    Server(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "http: {e}"),
            Error::Status(s) => write!(f, "ollama http status {s}"),
            Error::Server(m) => write!(f, "ollama: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

/// Talks to a local Ollama HTTP server.
pub struct Client {
    base_url: String,
    http: reqwest::blocking::Client,
    detect_http: reqwest::blocking::Client,
}

impl Client {
    /// Build a client for the given host and port.
    pub fn new(host: &str, port: u16) -> Client {
        let host = if host.is_empty() { DEFAULT_HOST } else { host };
        let port = if port == 0 { DEFAULT_PORT } else { port };
        Client {
            base_url: format!("http://{host}:{port}"),
            http: reqwest::blocking::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .expect("build http client"),
            detect_http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("build detect http client"),
        }
    }

    /// The base URL this client targets.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Report whether the server is reachable and list installed models.
    pub fn detect(&self) -> Result<(bool, Vec<String>)> {
        #[derive(Deserialize)]
        struct Model {
            name: String,
        }
        #[derive(Deserialize)]
        struct Tags {
            #[serde(default)]
            models: Vec<Model>,
        }
        let resp = match self
            .detect_http
            .get(format!("{}/api/tags", self.base_url))
            .send()
        {
            Ok(r) => r,
            Err(_) => return Ok((false, Vec::new())),
        };
        if !resp.status().is_success() {
            return Ok((false, Vec::new()));
        }
        let body: Tags = resp.json()?;
        Ok((true, body.models.into_iter().map(|m| m.name).collect()))
    }

    /// Run a single vision inference: send the image bytes and prompt to the
    /// model and return the response text plus timing details (nanoseconds).
    pub fn generate(
        &self,
        model: &str,
        prompt: &str,
        image: &[u8],
        opt: &GenOptions,
    ) -> Result<GenResult> {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(image);

        let mut options = serde_json::Map::new();
        if opt.num_thread > 0 {
            options.insert("num_thread".into(), opt.num_thread.into());
        }
        if opt.num_ctx > 0 {
            options.insert("num_ctx".into(), opt.num_ctx.into());
        }
        if opt.num_predict > 0 {
            options.insert("num_predict".into(), opt.num_predict.into());
        }

        let mut body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "images": [b64],
            "stream": false,
        });
        if !options.is_empty() {
            body["options"] = serde_json::Value::Object(options);
        }
        if !opt.keep_alive.is_empty() {
            body["keep_alive"] = serde_json::Value::String(opt.keep_alive.clone());
        }

        let resp = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&body)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }

        #[derive(Deserialize, Default)]
        struct Out {
            #[serde(default)]
            response: String,
            #[serde(default)]
            error: String,
            #[serde(default)]
            total_duration: i64,
            #[serde(default)]
            load_duration: i64,
            #[serde(default)]
            prompt_eval_duration: i64,
            #[serde(default)]
            eval_duration: i64,
        }
        let out: Out = resp.json()?;
        if !out.error.is_empty() {
            return Err(Error::Server(out.error));
        }
        Ok(GenResult {
            response: out.response,
            total_duration: out.total_duration,
            load_duration: out.load_duration,
            prompt_eval_duration: out.prompt_eval_duration,
            eval_duration: out.eval_duration,
        })
    }
}

/// Per-request tuning passed to Ollama.
#[derive(Debug, Clone, Default)]
pub struct GenOptions {
    /// `options.num_thread`; 0 = auto.
    pub num_thread: i32,
    /// `options.num_ctx`; 0 = model default.
    pub num_ctx: i32,
    /// `options.num_predict`; 0 = unbounded (avoid).
    pub num_predict: i32,
    /// `keep_alive`, e.g. "10m"; empty = server default.
    pub keep_alive: String,
}

/// The model text plus Ollama's timing breakdown (nanoseconds).
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Duration fields mirror the Ollama response; kept for completeness.
pub struct GenResult {
    pub response: String,
    pub total_duration: i64,
    pub load_duration: i64,
    pub prompt_eval_duration: i64,
    pub eval_duration: i64,
}
