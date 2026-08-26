//! Local, HTTP-based image-to-text tagging via an Ollama server on the user's
//! machine. Never downloads models; never sends data off the machine.

mod client;
mod config;
mod manager;
mod tagger;

pub use client::{Client, GenOptions};
pub use config::Config;
pub use manager::Manager;
pub use tagger::parse_tags;
