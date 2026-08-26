//! Local, HTTP-based image-to-text tagging via an Ollama server on the user's
//! machine. Never downloads models; never sends data off the machine.

mod client;
mod config;
mod manager;
mod tagger;

pub use client::{Client, GenOptions, GenResult};
pub use config::{
    Config, DEFAULT_CONCURRENCY, DEFAULT_HOST, DEFAULT_KEEP_ALIVE, DEFAULT_MAX_SIDE,
    DEFAULT_MAX_TAGS, DEFAULT_MODEL, DEFAULT_NUM_PREDICT, DEFAULT_PORT, DEFAULT_PROMPT,
};
pub use manager::Manager;
pub use tagger::parse_tags;
