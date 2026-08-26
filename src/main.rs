//! pichouse — a Picasa-like photo library GUI application for Linux.

mod ai;
mod db;
mod model;
mod reconcile;
mod scan;
mod thumb;
mod ui;
mod version;

fn main() -> gtk4::glib::ExitCode {
    ui::run()
}
