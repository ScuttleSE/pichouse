//! Filesystem scanner: walk library folders and record photos into the library
//! database.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::db::Library;
use crate::model::{Folder, Photo, ScanStatus};

/// Supported image extensions (lowercase, without the dot).
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];

/// Report whether a filename has a supported image extension.
pub fn is_image(name: &str) -> bool {
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) => IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// Scan progress. `done` is the number of photos processed so far; `total` is
/// the number discovered.
#[derive(Debug, Clone)]
pub struct Progress {
    pub folder: String,
    pub done: usize,
    pub total: usize,
}

/// Records photos from library folders into the library database.
pub struct Scanner<'a> {
    lib: &'a Library,
}

impl<'a> Scanner<'a> {
    /// Create a scanner backed by the given library.
    pub fn new(lib: &'a Library) -> Scanner<'a> {
        Scanner { lib }
    }

    /// Walk `root` recursively, recording folders and photos. `progress` is
    /// called after each photo. When `cancel` becomes true the walk stops
    /// promptly. Returns the number of photos recorded; `Err(Cancelled)`
    /// carries the count recorded so far via the caller's own bookkeeping.
    pub fn scan_folder<F>(
        &self,
        root: &Path,
        cancel: &Arc<AtomicBool>,
        mut progress: F,
    ) -> Result<usize, ScanError>
    where
        F: FnMut(Progress),
    {
        // First pass: collect image files grouped by directory.
        let mut by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut total = 0usize;
        collect_images(root, cancel, &mut by_dir, &mut total)?;

        let mut done = 0usize;
        for (dir, files) in &by_dir {
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled(done));
            }
            let fid = self.upsert_folder_for(dir, files)?;
            self.lib.set_scan_state(fid, ScanStatus::Running)?;
            for path in files {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ScanError::Cancelled(done));
                }
                self.record_photo(fid, path)?;
                done += 1;
                progress(Progress {
                    folder: dir.to_string_lossy().into_owned(),
                    done,
                    total,
                });
            }
            self.lib.set_scan_state(fid, ScanStatus::Done)?;
        }
        Ok(done)
    }

    /// Record the folder for a directory, deriving its year from the earliest
    /// photo taken date, falling back to the folder mtime year.
    fn upsert_folder_for(&self, dir: &Path, files: &[PathBuf]) -> Result<i64, ScanError> {
        let meta = std::fs::metadata(dir)?;
        let mtime = mtime_secs(&meta);
        let mut year = year_of(mtime);
        // Derive year from the earliest EXIF taken date, if any.
        let mut earliest: Option<i64> = None;
        for f in files {
            if let Some(t) = taken_at(f) {
                earliest = Some(match earliest {
                    Some(e) if e <= t => e,
                    _ => t,
                });
            }
        }
        if let Some(e) = earliest {
            year = year_of(e);
        }
        let id = self.lib.upsert_folder(&Folder {
            path: dir.to_string_lossy().into_owned(),
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            mtime,
            year,
            ..Default::default()
        })?;
        Ok(id)
    }

    fn record_photo(&self, folder_id: i64, path: &Path) -> Result<(), ScanError> {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return Ok(()), // file vanished; skip
        };
        let mut p = Photo {
            folder_id,
            path: path.to_string_lossy().into_owned(),
            filename: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: meta.len() as i64,
            mod_time: mtime_secs(&meta),
            ..Default::default()
        };
        if let Some(t) = taken_at(path) {
            p.taken_at = t;
        }
        if let Some((w, h)) = dimensions(path) {
            p.width = w;
            p.height = h;
        }
        if let Ok(hash) = hash_file(path) {
            p.hash = hash;
        }
        self.lib.upsert_photo(&p)?;
        Ok(())
    }
}

/// Recursively collect image files under `root`, grouped by parent directory.
fn collect_images(
    dir: &Path,
    cancel: &Arc<AtomicBool>,
    by_dir: &mut HashMap<PathBuf, Vec<PathBuf>>,
    total: &mut usize,
) -> Result<(), ScanError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled(0));
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // skip unreadable directories
    };
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled(0));
        }
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            collect_images(&path, cancel, by_dir, total)?;
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if is_image(name) {
                let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                by_dir.entry(parent).or_default().push(path.clone());
                *total += 1;
            }
        }
    }
    Ok(())
}

/// A scan error. `Cancelled` carries the number of photos recorded before the
/// cancel was observed.
#[derive(Debug)]
pub enum ScanError {
    Db(crate::db::Error),
    Io(std::io::Error),
    Cancelled(usize),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Db(e) => write!(f, "db: {e}"),
            ScanError::Io(e) => write!(f, "io: {e}"),
            ScanError::Cancelled(n) => write!(f, "cancelled after {n} photos"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<crate::db::Error> for ScanError {
    fn from(e: crate::db::Error) -> Self {
        ScanError::Db(e)
    }
}

impl From<std::io::Error> for ScanError {
    fn from(e: std::io::Error) -> Self {
        ScanError::Io(e)
    }
}

/// The EXIF DateTimeOriginal for a file as a Unix timestamp, if present.
fn taken_at(path: &Path) -> Option<i64> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .ok()?;
    // Prefer DateTimeOriginal; fall back to DateTime.
    let field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))?;
    let text = field.display_value().to_string();
    parse_exif_datetime(&text)
}

/// Parse an EXIF datetime string ("YYYY:MM:DD HH:MM:SS") into a Unix timestamp,
/// interpreting it as local time is unnecessary — EXIF has no zone, so treat it
/// as UTC for a stable, comparable value.
fn parse_exif_datetime(s: &str) -> Option<i64> {
    let s = s.trim();
    // Expect "YYYY:MM:DD HH:MM:SS" (or with '-' separators).
    let (date, time) = s.split_once(' ')?;
    let date: Vec<&str> = date.split([':', '-']).collect();
    let time: Vec<&str> = time.split(':').collect();
    if date.len() != 3 || time.len() < 3 {
        return None;
    }
    let year: i64 = date[0].parse().ok()?;
    let month: i64 = date[1].parse().ok()?;
    let day: i64 = date[2].parse().ok()?;
    let hour: i64 = time[0].parse().ok()?;
    let min: i64 = time[1].parse().ok()?;
    let sec: i64 = time[2].parse().ok()?;
    Some(civil_to_unix(year, month, day, hour, min, sec))
}

/// Convert a UTC civil date-time to a Unix timestamp (seconds). Uses Howard
/// Hinnant's days-from-civil algorithm; valid for the Gregorian calendar.
fn civil_to_unix(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + hh * 3600 + mm * 60 + ss
}

/// The year for a Unix timestamp (UTC).
fn year_of(unix: i64) -> i32 {
    // Inverse of civil_to_unix for the year component only.
    let days = unix.div_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }) as i32
}

/// The pixel width and height of an image file.
fn dimensions(path: &Path) -> Option<(i32, i32)> {
    let reader = image::ImageReader::open(path).ok()?;
    let reader = reader.with_guessed_format().ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    Some((w as i32, h as i32))
}

/// A sha256 hex digest of a file's contents.
fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Modification time of a file/dir as a Unix timestamp (seconds).
fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_image_matches_extensions() {
        assert!(is_image("a.JPG"));
        assert!(is_image("b.tiff"));
        assert!(!is_image("c.txt"));
        assert!(!is_image("noext"));
    }

    #[test]
    fn exif_datetime_parses() {
        // 2020-06-15 12:30:00 UTC.
        let unix = parse_exif_datetime("2020:06:15 12:30:00").unwrap();
        assert_eq!(year_of(unix), 2020);
    }

    #[test]
    fn civil_unix_year_roundtrip() {
        for &y in &[1970, 1999, 2000, 2024, 2038] {
            let u = civil_to_unix(y, 1, 1, 0, 0, 0);
            assert_eq!(year_of(u), y as i32);
        }
    }

    #[test]
    fn scan_records_images() {
        // Build a temp tree with one image-like file (a tiny valid PNG).
        let mut dir = std::env::temp_dir();
        dir.push(format!("pichouse-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        // 1x1 PNG.
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("sub/one.png"), png).unwrap();
        std::fs::write(dir.join("sub/notes.txt"), b"hi").unwrap();

        let mut db_path = std::env::temp_dir();
        db_path.push(format!("pichouse-scan-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let lib = Library::open_at(&db_path).unwrap();
        let scanner = Scanner::new(&lib);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut seen = 0;
        let n = scanner
            .scan_folder(&dir, &cancel, |_p| seen += 1)
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(seen, 1);
        // The photo has real dimensions decoded from the PNG.
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 1);
        let photos = lib.photos_in_folder(folders[0].id).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!((photos[0].width, photos[0].height), (1, 1));
        assert!(!photos[0].hash.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
