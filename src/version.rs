//! Application version, read and updated by CI.
//!
//! Versioning policy (see AGENTS.md, RULE THREE):
//!   - Format is major.minor.build.
//!   - The series starts at 0.0.0.
//!   - CI increments build by 1 on every push to main.
//!   - A release bumps major, minor, or build on request.
//!
//! The canonical version lives in `Cargo.toml`. This constant mirrors it so the
//! running app can show it without reading the manifest at runtime.

/// The current application version (major.minor.build).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
