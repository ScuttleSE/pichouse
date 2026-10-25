//! Data-directory and config-file resolution.
//!
//! The config file records where the database files live. The user can move
//! the data directory with it. All other settings live in `library.db`.
//!
//! A profile (`--profile NAME`) gets its own config file and its own data
//! directory. So each profile has separate settings and databases. Large
//! downloads (ONNX runtime, models, `ptr.db`) stay in the shared directory.

use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The active profile name. `None` is the default profile.
static PROFILE: OnceLock<Option<String>> = OnceLock::new();

/// Set the active profile. Call this one time, before any data access.
pub fn set_profile(name: Option<String>) {
    let _ = PROFILE.set(name);
}

/// The active profile name, or `None` for the default profile.
pub fn profile() -> Option<&'static str> {
    PROFILE.get().and_then(|p| p.as_deref())
}

/// Return true when the name is a valid profile name.
pub fn valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn config_base() -> io::Result<PathBuf> {
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => home_dir()?.join(".config"),
    };
    Ok(base.join("pichouse"))
}

/// The base config file path (`~/.config/pichouse/config`).
fn base_config_path() -> io::Result<PathBuf> {
    Ok(config_base()?.join("config"))
}

/// The config file path of the active profile.
/// The default profile uses `~/.config/pichouse/config`.
/// A named profile uses `~/.config/pichouse/profiles/<NAME>/config`.
fn config_path() -> io::Result<PathBuf> {
    match profile() {
        None => base_config_path(),
        Some(p) => Ok(config_base()?.join("profiles").join(p).join("config")),
    }
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

/// Read a data directory path from a config file. Returns `None` when the
/// file does not exist or is empty.
fn read_dir_from(cp: &PathBuf) -> io::Result<Option<PathBuf>> {
    match std::fs::read_to_string(cp) {
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

/// Write the data directory path to the config file of the active profile.
pub fn write_configured_data_dir(dir: &str) -> io::Result<()> {
    let cp = config_path()?;
    if let Some(parent) = cp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cp, format!("{}\n", dir.trim()))
}

/// The shared data directory. All profiles use it for large downloads
/// (ONNX runtime, models, `ptr.db`). The default profile also keeps its
/// databases here.
pub fn shared_dir() -> io::Result<PathBuf> {
    let dir = match read_dir_from(&base_config_path()?)? {
        Some(d) => d,
        None => default_data_dir()?,
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The data directory of the active profile, creating it if necessary.
/// A path in the profile config file wins. Otherwise a named profile uses
/// `<shared>/profiles/<NAME>`.
pub fn data_dir() -> io::Result<PathBuf> {
    let dir = match profile() {
        None => return shared_dir(),
        Some(p) => match read_dir_from(&config_path()?)? {
            Some(d) => d,
            None => shared_dir()?.join("profiles").join(p),
        },
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
