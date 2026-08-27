//! GTK4 user interface for pichouse.

mod actions;
mod aitag;
mod albumtree;
mod app;
mod controller;
mod dialogs;
mod editor;
mod enrich;
mod foldertree;
mod freshness;
mod grid;
mod immich;
mod newfiles;
mod photo_object;
mod prefs;
mod properties;
mod settings;
mod settings_ai;
mod settings_immich;
mod shortcuts;
mod sidebar;
mod state;
mod status;
mod tagmanager;
mod thumbcache;
mod toolbar;
mod util;
mod viewer;
mod vmenu;
mod vrules;
mod watcher;

pub use app::run;
