//! Initial sync of the Hydrus PTR into a local `ptr.db`.
//!
//! Usage: ptr-sync <path/to/ptr.db> [--key <access key>] [--stop-at <index>]
//!        [--max-gb-per-day <GB>] [--url <url>]
//! The key can also come from PICHOUSE_PTR_KEY. pichouse does not ship the
//! key. Get it at https://hydrusnetwork.github.io/hydrus/access_keys.html.
//! The tool can stop at any time (Ctrl+C). The next run continues.

#[path = "../ptr/mod.rs"]
#[allow(dead_code)]
mod ptr;
#[path = "../version.rs"]
#[allow(dead_code)]
mod version;

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use ptr::client::{Client, ACCESS_KEY_HELP_URL, DEFAULT_BASE_URL};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut key = std::env::var("PICHOUSE_PTR_KEY").ok();
    let mut stop_at = None;
    let mut max_per_day: Option<u64> = None;
    let mut url = DEFAULT_BASE_URL.to_string();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--key" => key = args.next(),
            "--stop-at" => stop_at = args.next().and_then(|s| s.parse().ok()),
            "--max-gb-per-day" => {
                max_per_day = args.next().and_then(|s| s.parse::<f64>().ok()).map(|g| (g * 1e9) as u64)
            }
            "--url" => url = args.next().unwrap_or(url),
            "-h" | "--help" => return usage(),
            _ => path = Some(a),
        }
    }
    let (Some(path), Some(key)) = (path, key) else { return usage() };

    let mut db = ptr::db::PtrDb::open(std::path::Path::new(&path)).unwrap_or_else(|e| die(&e.to_string()));
    let client = Client::new(&url, &key).unwrap_or_else(|e| die(&e));
    println!("ptr-sync: {path}, from index {}", db.last_index().unwrap_or(-1) + 1);

    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let (mut files, mut bytes) = (0usize, 0usize);
    let res = ptr::sync::sync(client, &mut db, &cancel, stop_at, max_per_day, |p| {
        files += p.files;
        bytes += p.bytes;
        let secs = start.elapsed().as_secs_f64().max(0.001);
        println!(
            "index {}/{}  files {}  {:.1} MB  {:.1} MB/s  {:.0} s",
            p.index, p.last_index, files, bytes as f64 / 1e6, bytes as f64 / 1e6 / secs, secs
        );
    });
    match res {
        Ok(n) => println!("applied {n} indexes"),
        Err(e) => die(&e),
    }
    if stop_at.is_none() {
        println!("building lookup indexes (this can take a long time) ...");
        db.build_lookup_indexes().unwrap_or_else(|e| die(&e.to_string()));
    }
    println!("done in {:.0} s", start.elapsed().as_secs_f64());
}

fn usage() {
    eprintln!("usage: ptr-sync <path/to/ptr.db> [--key <access key>] [--stop-at <index>] [--max-gb-per-day <GB>] [--url <url>]");
    eprintln!("The key can also come from PICHOUSE_PTR_KEY. Get the key at {ACCESS_KEY_HELP_URL}");
    std::process::exit(2);
}

fn die(msg: &str) -> ! {
    eprintln!("ptr-sync: {msg}");
    std::process::exit(1);
}
