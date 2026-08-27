//! Persisted thumbnail/UI preferences, stored in `library.db` settings.

use crate::ai;
use crate::db::Library;

/// Setting keys stored in `library.db`.
pub const KEY_THUMB_SIZES: &str = "thumb.sizes";
pub const KEY_THUMB_ACTIVE: &str = "thumb.active";
pub const KEY_REGEN_ON_MOVE: &str = "thumb.regen";
pub const KEY_SAVE_ALL_SIZES: &str = "thumb.save_all";
pub const KEY_PROPS_VISIBLE: &str = "ui.props_visible";
/// When set, force the built-in Adwaita theme instead of the system theme. On
/// by default because some environments (e.g. Kasm/remote desktops) ship a
/// broken GTK theme that hides the folder-tree expander.
pub const KEY_THEME_OVERRIDE: &str = "ui.theme_override";
/// How many days a photo stays in the New Files view after being added.
pub const KEY_NEW_MAX_AGE_DAYS: &str = "ui.new_max_age_days";

/// AI setting keys stored in `library.db`.
pub const KEY_AI_ENABLED: &str = "ai.enabled";
pub const KEY_AI_HOST: &str = "ai.host";
pub const KEY_AI_PORT: &str = "ai.port";
pub const KEY_AI_MODEL: &str = "ai.model";
pub const KEY_AI_CONCURRENCY: &str = "ai.concurrency";
pub const KEY_AI_MANAGE: &str = "ai.manage";
pub const KEY_AI_BINARY: &str = "ai.binary";
pub const KEY_AI_PROMPT: &str = "ai.prompt";
pub const KEY_AI_NUM_THREAD: &str = "ai.num_thread";
pub const KEY_AI_NUM_CTX: &str = "ai.num_ctx";
pub const KEY_AI_NUM_PREDICT: &str = "ai.num_predict";

/// Immich setting keys stored in `library.db`.
pub const KEY_IMMICH_PAGE_SIZE: &str = "immich.page_size";
/// Default number of assets fetched per page when listing an Immich album.
pub const DEFAULT_IMMICH_PAGE_SIZE: i32 = 100;

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
    /// Force the built-in Adwaita theme (default true).
    pub theme_override: bool,
    /// How many days a file stays in the New Files view (default 14).
    pub new_max_age_days: i64,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            sizes: DEFAULT_THUMB_SIZES.to_vec(),
            active: 1,
            regen_on_move: false,
            save_all_sizes: false,
            props_visible: true,
            theme_override: true,
            new_max_age_days: 14,
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
        p.theme_override = bool_setting(lib, KEY_THEME_OVERRIDE, true);
        if let Ok(v) = lib.get_setting(KEY_NEW_MAX_AGE_DAYS, "") {
            if let Ok(n) = v.parse::<i64>() {
                if (1..=365).contains(&n) {
                    p.new_max_age_days = n;
                }
            }
        }
        p
    }

    /// The active thumbnail size in pixels.
    pub fn active_size(&self) -> i32 {
        self.sizes.get(self.active).copied().unwrap_or(160)
    }

    /// The New Files max age in seconds, for `Library::new_photos_grouped`.
    pub fn new_max_age_secs(&self) -> i64 {
        self.new_max_age_days * 24 * 60 * 60
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

/// Read the AI tagging configuration from the library database.
pub fn load_ai_config(lib: &Library) -> ai::Config {
    let mut c = ai::Config {
        enabled: bool_setting(lib, KEY_AI_ENABLED, false),
        manage: bool_setting(lib, KEY_AI_MANAGE, false),
        ..ai::Config::default()
    };
    if let Ok(v) = lib.get_setting(KEY_AI_HOST, "") {
        if !v.is_empty() {
            c.host = v;
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_PORT, "") {
        if let Ok(n) = v.parse::<u16>() {
            if n > 0 {
                c.port = n;
            }
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_MODEL, "") {
        if !v.is_empty() {
            c.model = v;
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_CONCURRENCY, "") {
        if let Ok(n) = v.parse::<i32>() {
            if n > 0 {
                c.concurrency = n;
            }
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_BINARY, "") {
        if !v.is_empty() {
            c.binary_path = v;
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_PROMPT, "") {
        if !v.is_empty() {
            c.prompt = v;
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_NUM_THREAD, "") {
        if let Ok(n) = v.parse::<i32>() {
            if n >= 0 {
                c.num_thread = n;
            }
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_NUM_CTX, "") {
        if let Ok(n) = v.parse::<i32>() {
            if n >= 0 {
                c.num_ctx = n;
            }
        }
    }
    if let Ok(v) = lib.get_setting(KEY_AI_NUM_PREDICT, "") {
        if let Ok(n) = v.parse::<i32>() {
            if n > 0 {
                c.num_predict = n;
            }
        }
    }
    c.normalize();
    c
}

/// Store a boolean setting as "1"/"0".
pub fn bool_to_str(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}
