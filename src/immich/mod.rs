//! Immich server integration for pichouse.
//!
//! This module connects to a remote Immich instance over HTTP. It reads
//! albums and assets and downloads thumbnails. The client is blocking. It runs
//! on a background thread, the same way the AI client runs. See
//! `src/ui/immich.rs` for the thread and channel wiring.

mod client;

pub use client::Client;
