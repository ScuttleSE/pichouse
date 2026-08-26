//! Persisted thumbnail/UI preferences, stored in `library.db` settings.

use crate::db::Library;

/// Setting keys stored in `library.db`.
pub const KEY_THUMB_SIZES: &str = "thumb.sizes";
pub const KEY_THUMB_ACTIVE: &str = "thumb.active";
pub const KEY_REGEN_ON_MOVE: &str = "thumb.regen";
pub const KEY_SAVE_ALL_SIZES: &str = "thumb.save_all";
pub const KEY_PROPS_VISIBLE: &str = "ui.props_visible";

/// The four slider preset sizes in pixels.
pub const DEFAULT_THUMB_SIZES: [i32; 4] = [96, 160, 240, 320];

/// User's persisted thumbnail/UI preferences.
#[derive(Debug, Clone)]
pub struct Prefs {
    /// Always length 4, ascending.
    pub sizes: Vec<i32>,
    /// 0..3.
    pub active: usize,
    pub regen_on_move: bool,
    pub save_all_sizes: bool,
    pub props_visible: bool,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            sizes: DEFAULT_THUMB_SIZES.to_vec(),
            active: 1,
            regen_on_move: false,
            save_all_sizes: false,
            props_visible: true,
        }
    }
}

impl Prefs {
    /// Read preferences from the library database, filling defaults.
    pub fn load(lib: &Library) -> Prefs {
        let mut p = Prefs::default();
        if let Ok(v) = lib.get_setting(KEY_THUMB_SIZES, "") {
            if let Some(s) = parse_sizes(&v) {
                p.sizes = s;
            }
        }
        if let Ok(v) = lib.get_setting(KEY_THUMB_ACTIVE, "") {
            if let Ok(i) = v.parse::<usize>() {
                if i < 4 {
                    p.active = i;
                }
            }
        }
        p.regen_on_move = bool_setting(lib, KEY_REGEN_ON_MOVE, false);
        p.save_all_sizes = bool_setting(lib, KEY_SAVE_ALL_SIZES, false);
        p.props_visible = bool_setting(lib, KEY_PROPS_VISIBLE, true);
        p
    }

    /// The active thumbnail size in pixels.
    pub fn active_size(&self) -> i32 {
        self.sizes.get(self.active).copied().unwrap_or(160)
    }
}

fn bool_setting(lib: &Library, key: &str, def: bool) -> bool {
    let d = if def { "1" } else { "0" };
    lib.get_setting(key, d).map(|v| v == "1").unwrap_or(def)
}

fn parse_sizes(s: &str) -> Option<Vec<i32>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let n: i32 = part.trim().parse().ok()?;
        if !(16..=4096).contains(&n) {
            return None;
        }
        out.push(n);
    }
    if out.len() == 4 {
        Some(out)
    } else {
        None
    }
}

/// Format sizes as a comma-separated string for storage.
pub fn format_sizes(sizes: &[i32]) -> String {
    sizes
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
