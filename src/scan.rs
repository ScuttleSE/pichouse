//! Filesystem scanner: walk library folders and record photos into the library
//! database.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

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

/// Wall-clock milliseconds since the Unix epoch.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Sleep while `pause_until` is in the future, in short slices so cancellation
/// stays responsive. Used to yield the disk to the UI while the user browses.
fn wait_while_paused(
    pause_until: &Arc<std::sync::atomic::AtomicU64>,
    cancel: &Arc<AtomicBool>,
) {
    let mut logged = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let until = pause_until.load(Ordering::Relaxed);
        let now = now_millis();
        if now >= until {
            if logged {
                log::debug!("scan: resuming after browse pause");
            }
            return;
        }
        if !logged {
            log::debug!("scan: paused for browsing ({}ms left)", until - now);
            logged = true;
        }
        let wait = (until - now).min(200);
        std::thread::sleep(std::time::Duration::from_millis(wait));
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

    /// Walk `root` recursively, recording folders and photo *structure* only
    /// (Phase 1 of the two-phase import). Per photo this records just cheap
    /// `fs::metadata` (path, filename, folder_id, size, mod_time); EXIF,
    /// dimensions, and hash are left for the Phase 2 enrichment worker. The
    /// folder tree and grid can therefore populate almost immediately, even for
    /// tens of thousands of files.
    ///
    /// `progress` is called after each photo. `on_folder` is called once per
    /// directory, right after its photo rows are recorded, with the folder id
    /// and its path — the caller uses this to file the folder into the Library
    /// album tree immediately, so it never lingers under "New folders". When
    /// `cancel` becomes true the walk stops promptly. Returns the number of
    /// photos recorded.
    pub fn scan_folder<F, G>(
        &self,
        root: &Path,
        cancel: &Arc<AtomicBool>,
        pause_until: &Arc<std::sync::atomic::AtomicU64>,
        mut progress: F,
        mut on_folder: G,
    ) -> Result<usize, ScanError>
    where
        F: FnMut(Progress),
        G: FnMut(i64, &Path),
    {
        // First pass: collect image files grouped by directory.
        let mut by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut total = 0usize;
        log::info!("collect_images: walking {} …", root.display());
        let t_collect = std::time::Instant::now();
        collect_images(root, cancel, &mut by_dir, &mut total)?;
        log::info!(
            "collect_images {}: {} images in {} dirs, took {:.2?}",
            root.display(),
            total,
            by_dir.len(),
            t_collect.elapsed()
        );

        let t_scan = std::time::Instant::now();
        let mut done = 0usize;
        let ndirs = by_dir.len();
        for (i, (dir, files)) in by_dir.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled(done));
            }
            // Yield to the UI while the user is browsing: opening a folder sets a
            // short pause deadline so on-demand thumbnail work gets the disk and
            // the grid is not rebuilt from under the user by scan reloads.
            wait_while_paused(pause_until, cancel);
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled(done));
            }
            log::debug!(
                "scan dir [{}/{}] {} ({} files): start",
                i + 1,
                ndirs,
                dir.display(),
                files.len()
            );
            let t_dir = std::time::Instant::now();
            let t_upsert = std::time::Instant::now();
            let fid = self.upsert_folder_for(dir)?;
            let upsert_ms = t_upsert.elapsed();
            self.lib.set_scan_state(fid, ScanStatus::Running)?;

            // Record the whole directory's photos in one transaction. This holds
            // the DB lock once per directory instead of twice per photo, keeping
            // the scan fast and leaving the lock free between directories so the
            // Phase 2 enrichment/thumbnail workers and the UI are not starved.
            let batch: Vec<Photo> = files
                .iter()
                .filter_map(|path| structure_photo(fid, path))
                .collect();
            log::debug!(
                "scan dir {}: inserting {} photos …",
                dir.display(),
                batch.len()
            );
            let t_batch = std::time::Instant::now();
            self.lib.insert_structure_batch(&batch)?;
            let batch_ms = t_batch.elapsed();

            // File this folder into the Library album tree right away.
            log::trace!("scan dir {}: on_folder …", dir.display());
            on_folder(fid, dir);

            for path in files {
                done += 1;
                progress(Progress {
                    folder: dir.to_string_lossy().into_owned(),
                    done,
                    total,
                });
                log::trace!("recorded {}", path.display());
                if cancel.load(Ordering::Relaxed) {
                    return Err(ScanError::Cancelled(done));
                }
            }
            self.lib.set_scan_state(fid, ScanStatus::Done)?;
            log::debug!(
                "scan dir {}: done ({} photos, upsert_folder {:.2?}, insert_batch {:.2?}, dir total {:.2?})",
                dir.display(),
                batch.len(),
                upsert_ms,
                batch_ms,
                t_dir.elapsed()
            );
            // Yield so the enrichment workers get a turn on the DB lock between
            // directories rather than the scan monopolizing it.
            std::thread::yield_now();
        }
        log::info!(
            "scan {}: recorded {} photos in {:.2?}",
            root.display(),
            done,
            t_scan.elapsed()
        );
        Ok(done)
    }

    /// Record the folder for a directory. The `year` is derived from the folder
    /// mtime only; it is refined from the earliest EXIF `taken_at` later, once
    /// Phase 2 enrichment has run (see `Library::set_folder_year`). This keeps
    /// Phase 1 free of any per-file EXIF decode.
    fn upsert_folder_for(&self, dir: &Path) -> Result<i64, ScanError> {
        let meta = std::fs::metadata(dir)?;
        let mtime = mtime_secs(&meta);
        let year = year_of(mtime);
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
}

/// Build a Phase 1 `Photo` (cheap structure only) for a file, or `None` if the
/// file vanished before it could be stat'd.
fn structure_photo(folder_id: i64, path: &Path) -> Option<Photo> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Photo {
        folder_id,
        path: path.to_string_lossy().into_owned(),
        filename: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: meta.len() as i64,
        mod_time: mtime_secs(&meta),
        ..Default::default()
    })
}

/// The Phase 2 enrichment result for a single file: EXIF taken date, pixel
/// dimensions, and content hash.
pub struct Enrichment {
    pub taken_at: i64,
    pub width: i32,
    pub height: i32,
    pub hash: String,
    /// 64-bit perceptual hash (dHash) of the decoded image, `0` when the file
    /// could not be decoded. Used by the duplicate finder.
    pub phash: u64,
}

/// Compute the Phase 2 enrichment for a file: EXIF `taken_at`, dimensions, and
/// the SHA-256 content hash. Missing pieces default to `0`/empty. Returns
/// `None` only if the file cannot be hashed (e.g. it vanished).
///
/// The live enrichment path uses [`enrich_file_with_image`], which reads the
/// file once; this per-operation variant is kept for tests and as a fallback.
#[allow(dead_code)]
pub fn enrich_file(path: &Path) -> Option<Enrichment> {
    let hash = hash_file(path).ok()?;
    let taken_at = taken_at(path).unwrap_or(0);
    let (width, height) = dimensions(path).unwrap_or((0, 0));
    Some(Enrichment {
        taken_at,
        width,
        height,
        hash,
        phash: 0,
    })
}

/// Enrich a file reading it from disk exactly once, and return the decoded
/// pixels so the caller can build the thumbnail without a second read/decode.
///
/// This collapses what used to be three separate reads of a (potentially large,
/// slow-disk) image — hash, dimensions, thumbnail decode — into a single read:
/// the bytes are read once, hashed in memory, and decoded in memory. On any
/// failure the fields fall back to the per-operation path so behaviour matches
/// `enrich_file`. Returns the enrichment plus the decoded RGBA image (the image
/// is `None` when decoding fails, e.g. an unsupported format).
pub fn enrich_file_with_image(path: &Path) -> Option<(Enrichment, Option<image::RgbaImage>)> {
    // One read of the whole file.
    let bytes = std::fs::read(path).ok()?;

    // Hash the bytes we already have.
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hex_encode(&hasher.finalize());

    // EXIF taken date: parse from the same in-memory bytes.
    let taken_at = taken_at_from_bytes(&bytes).unwrap_or(0);

    // Decode once from memory; derive dimensions from the decoded image.
    let decoded = image::load_from_memory(&bytes).ok().map(|i| i.to_rgba8());
    let (width, height) = match &decoded {
        Some(img) => (img.width() as i32, img.height() as i32),
        None => (0, 0),
    };

    // Perceptual hash from the decoded pixels (dropping alpha). `0` when the
    // image did not decode.
    let phash = match &decoded {
        Some(img) => {
            let (w, h) = (img.width(), img.height());
            let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
            for px in img.pixels() {
                rgb.push(px[0]);
                rgb.push(px[1]);
                rgb.push(px[2]);
            }
            crate::phash::dhash_rgb(&rgb, w, h)
        }
        None => 0,
    };

    Some((
        Enrichment {
            taken_at,
            width,
            height,
            hash,
            phash,
        },
        decoded,
    ))
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
    log::trace!("read_dir {}", dir.display());
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("skip unreadable dir {}: {e}", dir.display());
            return Ok(()); // skip unreadable directories
        }
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
#[allow(dead_code)]
fn taken_at(path: &Path) -> Option<i64> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .ok()?;
    taken_at_from_exif(&exif)
}

/// The EXIF DateTimeOriginal from already-read image bytes, if present.
fn taken_at_from_bytes(bytes: &[u8]) -> Option<i64> {
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = exif::Reader::new()
        .read_from_container(&mut cursor)
        .ok()?;
    taken_at_from_exif(&exif)
}

/// Extract and parse the taken date from a decoded EXIF container.
fn taken_at_from_exif(exif: &exif::Exif) -> Option<i64> {
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
pub fn year_of(unix: i64) -> i32 {
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
#[allow(dead_code)]
fn dimensions(path: &Path) -> Option<(i32, i32)> {
    let reader = image::ImageReader::open(path).ok()?;
    let reader = reader.with_guessed_format().ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    Some((w as i32, h as i32))
}

/// A sha256 hex digest of a file's contents.
#[allow(dead_code)]
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
        let pause = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut seen = 0;
        let n = scanner
            .scan_folder(&dir, &cancel, &pause, |_p| seen += 1, |_fid, _dir| {})
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(seen, 1);
        // Phase 1 records structure only: no dimensions or hash yet.
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 1);
        let photos = lib.photos_in_folder(folders[0].id).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!((photos[0].width, photos[0].height), (0, 0));
        assert!(photos[0].hash.is_empty());
        assert_eq!(
            photos[0].scan_state,
            crate::model::PhotoScanState::Structured
        );

        // The photo is listed as needing enrichment; enrich it (Phase 2).
        let need = lib.photos_needing_enrichment(None).unwrap();
        assert_eq!(need, vec![photos[0].id]);
        let enr = enrich_file(std::path::Path::new(&photos[0].path)).unwrap();
        lib.enrich_photo(photos[0].id, enr.taken_at, enr.width, enr.height, &enr.hash, enr.phash)
            .unwrap();
        let photos = lib.photos_in_folder(folders[0].id).unwrap();
        assert_eq!((photos[0].width, photos[0].height), (1, 1));
        assert!(!photos[0].hash.is_empty());
        assert_eq!(photos[0].scan_state, crate::model::PhotoScanState::Done);
        // Now nothing needs enrichment.
        assert!(lib.photos_needing_enrichment(None).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
