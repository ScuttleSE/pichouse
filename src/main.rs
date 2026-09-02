//! pichouse — a Picasa-like photo library GUI application for Linux.

mod ai;
mod db;
mod edit;
mod face;
mod immich;
mod dedup;
mod model;
mod phash;
mod reconcile;
mod scan;
mod styleface;
mod thumb;
mod ui;
mod version;

use log::LevelFilter;

fn main() -> gtk4::glib::ExitCode {
    match parse_log_level() {
        Ok(level) => init_logging(level),
        Err(code) => return code,
    }
    ui::run()
}

/// Parse the console verbosity from the command line.
///
/// Flags (later ones win):
///   (none)        warn   — default: errors and warnings only
///   -v            info   — plus scan start/end summaries
///   -vv           debug  — plus per-directory DB and sidebar-reload timings
///   -vvv          trace  — plus per-photo detail
///   -q/--quiet    error  — errors only
///   -h/--help     print usage and exit
///
/// Returns the chosen level, or an `ExitCode` when the program should exit
/// immediately (help, or a bad argument).
fn parse_log_level() -> Result<LevelFilter, gtk4::glib::ExitCode> {
    let mut level = LevelFilter::Warn;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Err(gtk4::glib::ExitCode::SUCCESS);
            }
            "-q" | "--quiet" => level = LevelFilter::Error,
            "-v" => level = LevelFilter::Info,
            "-vv" => level = LevelFilter::Debug,
            "-vvv" => level = LevelFilter::Trace,
            other => {
                eprintln!("pichouse: unknown argument '{other}'");
                print_usage();
                return Err(gtk4::glib::ExitCode::FAILURE);
            }
        }
    }
    Ok(level)
}

/// Initialize console logging at the given level for the `pichouse` crate.
fn init_logging(level: LevelFilter) {
    use std::io::Write;
    env_logger::Builder::new()
        // Only our own crate's records; keep third-party noise out.
        .filter_module("pichouse", level)
        // Include an elapsed timestamp and the thread name on every line, so a
        // freeze is easy to attribute: the last line before a stall names the
        // thread and operation that hung.
        .format(|buf, record| {
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("main");
            writeln!(
                buf,
                "[{:>7}] [{}] {}: {}",
                record.level(),
                name,
                record.target(),
                record.args()
            )
        })
        .init();
}

fn print_usage() {
    eprintln!(
        "pichouse — Picasa-like photo library\n\
         \n\
         Usage: pichouse [OPTIONS]\n\
         \n\
         Options:\n\
         \x20 -v            verbose: scan start/end summaries (info)\n\
         \x20 -vv           more verbose: per-directory and sidebar-reload timings (debug)\n\
         \x20 -vvv          most verbose: per-photo detail (trace)\n\
         \x20 -q, --quiet   errors only\n\
         \x20 -h, --help    show this help\n\
         \n\
         With no option, only warnings and errors are printed. All log output\n\
         goes to the console (stderr)."
    );
}
