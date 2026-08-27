//! Configurable viewer keybindings.

use std::collections::HashMap;

use gtk4::gdk;
use gtk4::glib::translate::{FromGlib, IntoGlib};

use crate::db::Library;

/// A viewer action bound to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Prev,
    Next,
    Rotate,
    Close,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Prev => "prev",
            Action::Next => "next",
            Action::Rotate => "rotate",
            Action::Close => "close",
        }
    }

    /// The db setting key, e.g. "keybind.prev".
    fn setting_key(self) -> String {
        format!("keybind.{}", self.as_str())
    }

    /// Label shown in settings.
    pub fn label(self) -> &'static str {
        match self {
            Action::Prev => "Previous image",
            Action::Next => "Next image",
            Action::Rotate => "Rotate 90°",
            Action::Close => "Close viewer",
        }
    }
}

/// The four actions with their default keyvals, in display order.
pub fn defaults() -> Vec<(Action, u32)> {
    vec![
        (Action::Prev, gdk::Key::Left.into_glib()),
        (Action::Next, gdk::Key::Right.into_glib()),
        (Action::Rotate, gdk::Key::r.into_glib()),
        (Action::Close, gdk::Key::Escape.into_glib()),
    ]
}

/// Keyval <-> action maps, one keyval per action.
pub struct Shortcuts {
    by_key: HashMap<u32, Action>,
    keys: HashMap<Action, u32>,
}

impl Shortcuts {
    /// Load bindings from the database, falling back to defaults.
    pub fn load(lib: &Library) -> Shortcuts {
        let mut s = Shortcuts {
            by_key: HashMap::new(),
            keys: HashMap::new(),
        };
        for (action, def_key) in defaults() {
            let mut key = def_key;
            if let Ok(name) = lib.get_setting(&action.setting_key(), "") {
                if !name.is_empty() {
                    if let Some(k) = gdk::Key::from_name(&name) {
                        let kv = k.into_glib();
                        if kv != 0 {
                            key = kv;
                        }
                    }
                }
            }
            s.set(action, key);
        }
        s
    }

    /// Bind a keyval to an action, removing the action's old keyval.
    pub fn set(&mut self, action: Action, keyval: u32) {
        if let Some(&old) = self.keys.get(&action) {
            self.by_key.remove(&old);
        }
        self.by_key.insert(keyval, action);
        self.keys.insert(action, keyval);
    }

    /// The action bound to a keyval, if any. Falls back to the lowercased key so
    /// 'R' matches 'r'.
    pub fn action(&self, keyval: u32) -> Option<Action> {
        if let Some(&a) = self.by_key.get(&keyval) {
            return Some(a);
        }
        let lower = unsafe { gdk::Key::from_glib(keyval) }.to_lower().into_glib();
        self.by_key.get(&lower).copied()
    }

    /// The keyval bound to an action.
    pub fn keyval(&self, action: Action) -> u32 {
        self.keys.get(&action).copied().unwrap_or(0)
    }
}

/// A human-readable label for a keyval.
pub fn keyval_label(keyval: u32) -> String {
    if keyval == 0 {
        return "(unset)".to_string();
    }
    match unsafe { gdk::Key::from_glib(keyval) }.name() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "?".to_string(),
    }
}
