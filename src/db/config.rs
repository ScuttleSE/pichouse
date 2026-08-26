//! Data-directory and config-file resolution.
//!
//! The config file's sole purpose is to record where the database files live so
//! the user can relocate the data directory. All other settings live in
//! `library.db`.

use std::io;
use std::path::PathBuf;

/// The config file path (`~/.config/pichouse/config`), honoring
/// `XDG_CONFIG_HOME`.
fn config_path() -> io::Result<PathBuf> {
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => home_dir()?.join(".config"),
    };
    Ok(base.join("pichouse").join("config"))
}

/// The built-in default data directory (`~/.local/share/pichouse`), honoring
/// `XDG_DATA_HOME`.
fn default_data_dir() -> io::Result<PathBuf> {
    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => home_dir()?.join(".local").join("share"),
    };
    Ok(base.join("pichouse"))
}

fn home_dir() -> io::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))
}

/// Read the data directory path from the config file. Returns `None` when the
/// config file does not exist.
fn read_configured_data_dir() -> io::Result<Option<PathBuf>> {
    let cp = config_path()?;
    match std::fs::read_to_string(&cp) {
        Ok(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(PathBuf::from(trimmed)))
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Persist the data directory path to the config file, creating the config
/// directory if necessary.
pub fn write_configured_data_dir(dir: &str) -> io::Result<()> {
    let cp = config_path()?;
    if let Some(parent) = cp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cp, format!("{}\n", dir.trim()))
}

/// The pichouse data directory, creating it if necessary. A path configured in
/// the config file wins; otherwise the default is used.
pub fn data_dir() -> io::Result<PathBuf> {
    let dir = match read_configured_data_dir()? {
        Some(d) => d,
        None => default_data_dir()?,
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
