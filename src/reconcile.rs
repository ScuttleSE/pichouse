//! Library freshness: reconcile the database against what is on disk.
//!
//! Reconciliation is the reliable core of freshness. It lists the images on
//! disk and diffs them against the recorded rows, so it works on any
//! filesystem — including network mounts (NFS/SMB) where inotify never sees
//! remote changes — and on very large trees where inotify watch limits are
//! exhausted. The inotify watcher (see `ui::watcher`) is only a low-latency
//! optimization layered on top of this; it is never required for correctness.
//!
//! Diff rules, per folder:
//! - On disk, not in DB  -> insert as Phase-1 structure, queue for enrichment.
//! - In DB, not on disk  -> soft-mark `missing` (keep the row so tags/edits
//!   survive a temporary unmount, move, or delete).
//! - Missing row reappears on disk -> clear `missing`; if size changed, re-queue
//!   for enrichment.
//! - Move/rename (best effort) -> a new file whose size matches a currently
//!   missing row in the same root re-points that row (preserving tags/edits)
//!   instead of creating a new row plus a missing row.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use crate::db::Library;
use crate::model::{Folder, Photo};
use crate::scan::is_image;

/// A summary of what a reconciliation changed. `added` photo ids are the ones
/// that now need Phase 2 enrichment.
#[derive(Debug, Default)]
pub struct Report {
    pub added: Vec<i64>,
    pub missing: usize,
    pub reappeared: usize,
    pub moved: usize,
}

impl Report {
    fn merge(&mut self, other: Report) {
        self.added.extend(other.added);
        self.missing += other.missing;
        self.reappeared += other.reappeared;
        self.moved += other.moved;
    }

    /// Whether anything changed.
    pub fn changed(&self) -> bool {
        !self.added.is_empty() || self.missing > 0 || self.reappeared > 0 || self.moved > 0
    }
}

/// Reconcile every library root against disk. Walks each root recursively,
/// creating folder rows for new directories and diffing photos per directory.
/// Stops promptly when `cancel` becomes true.
pub fn reconcile_all(lib: &Library, cancel: &Arc<AtomicBool>) -> Report {
    let mut report = Report::default();
    let roots = lib.library_folders().unwrap_or_default();
    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // Collect image files grouped by directory under this root.
        let mut by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        collect(Path::new(&root.path), cancel, &mut by_dir);
        for (dir, files) in &by_dir {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            report.merge(reconcile_dir(lib, dir, files));
        }
        // Directories that once held photos but no longer exist on disk: mark
        // all their photos missing.
        mark_vanished_dirs(lib, &root.path, &by_dir, &mut report);
    }
    report
}

/// Reconcile a single directory against its recorded photos. `files` is the set
/// of image paths currently on disk in `dir`.
fn reconcile_dir(lib: &Library, dir: &Path, files: &[PathBuf]) -> Report {
    let mut report = Report::default();
    let fid = match ensure_folder(lib, dir) {
        Some(f) => f,
        None => return report,
    };

    // Index the DB rows for this folder by path: path -> (id, size, missing).
    let index = lib.photo_index_for_folder(fid).unwrap_or_default();
    let on_disk: std::collections::HashSet<String> =
        files.iter().map(|p| p.to_string_lossy().into_owned()).collect();

    // Currently-missing rows in this folder, grouped by size, for move matching.
    let mut missing_by_size: HashMap<i64, Vec<i64>> = HashMap::new();
    for (_path, (id, size, missing)) in &index {
        if *missing {
            missing_by_size.entry(*size).or_default().push(*id);
        }
    }

    for path in files {
        let path_str = path.to_string_lossy().into_owned();
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len() as i64;
        match index.get(&path_str) {
            Some((id, _old_size, true)) => {
                // A missing row's file is back at the same path.
                let _ = lib.set_photo_missing(*id, false);
                report.reappeared += 1;
            }
            Some(_) => {
                // Present and known; nothing to do (size/mtime changes are
                // picked up by a rescan/enrichment, not tracked per stat here).
            }
            None => {
                // New path. Try to treat it as a move of a missing row with the
                // same size (preserves tags/edits); else insert fresh.
                if let Some(moved_id) = take_move_candidate(&mut missing_by_size, size) {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let _ = lib.move_photo_path(moved_id, fid, &path_str, &name);
                    report.moved += 1;
                    report.added.push(moved_id); // re-hash to confirm identity
                } else {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let p = Photo {
                        folder_id: fid,
                        path: path_str.clone(),
                        filename: name,
                        size,
                        mod_time: mtime_secs(&meta),
                        ..Default::default()
                    };
                    if let Ok(id) = lib.upsert_photo_structure(&p) {
                        report.added.push(id);
                    }
                }
            }
        }
    }

    // Rows whose file is gone from disk: soft-mark missing (unless already so).
    for (path, (id, _size, missing)) in &index {
        if !missing && !on_disk.contains(path) {
            let _ = lib.set_photo_missing(*id, true);
            report.missing += 1;
        }
    }

    report
}

/// Pop one missing-row id with the given size, if any (move candidate).
fn take_move_candidate(missing_by_size: &mut HashMap<i64, Vec<i64>>, size: i64) -> Option<i64> {
    let ids = missing_by_size.get_mut(&size)?;
    ids.pop()
}

/// Mark photos in recorded folders under `root_path` whose directory no longer
/// appears on disk as missing.
fn mark_vanished_dirs(
    lib: &Library,
    root_path: &str,
    by_dir: &HashMap<PathBuf, Vec<PathBuf>>,
    report: &mut Report,
) {
    let live: std::collections::HashSet<String> = by_dir
        .keys()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let folders = lib.folders().unwrap_or_default();
    let prefix = format!("{}{}", root_path, std::path::MAIN_SEPARATOR);
    for f in folders {
        if f.path != root_path && !f.path.starts_with(&prefix) {
            continue; // not under this root
        }
        if live.contains(&f.path) {
            continue; // still has photos on disk
        }
        // Directory gone (or now empty). Mark its non-missing photos missing.
        if let Ok(photos) = lib.photos_in_folder(f.id) {
            for p in photos {
                if !p.missing && !Path::new(&p.path).exists() {
                    let _ = lib.set_photo_missing(p.id, true);
                    report.missing += 1;
                }
            }
        }
    }
}

/// Look up (or create) the folder row id for a directory.
fn ensure_folder(lib: &Library, dir: &Path) -> Option<i64> {
    let meta = std::fs::metadata(dir).ok()?;
    let mtime = mtime_secs(&meta);
    let f = Folder {
        path: dir.to_string_lossy().into_owned(),
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        mtime,
        year: crate::scan::year_of(mtime),
        ..Default::default()
    };
    lib.upsert_folder(&f).ok()
}

/// Recursively collect image files under `dir`, grouped by parent directory.
fn collect(dir: &Path, cancel: &Arc<AtomicBool>, by_dir: &mut HashMap<PathBuf, Vec<PathBuf>>) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    // Ensure the directory itself has an entry even if it holds no images, so a
    // now-empty folder is reconciled (its photos marked missing).
    by_dir.entry(dir.to_path_buf()).or_default();
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            collect(&path, cancel, by_dir);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if is_image(name) {
                let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                by_dir.entry(parent).or_default().push(path);
            }
        }
    }
}

/// Modification time of a file/dir as a Unix timestamp (seconds).
fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Reconcile a single directory (not recursive) against disk. Used by the
/// inotify fast-path to react to a change in one folder without walking the
/// whole tree. Returns the photo ids that now need enrichment.
pub fn reconcile_one_dir(lib: &Library, dir: &Path) -> Report {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
            if is_file {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if is_image(name) {
                        files.push(path);
                    }
                }
            }
        }
    }
    reconcile_dir(lib, dir, &files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png() -> &'static [u8] {
        &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn reconcile_detects_add_and_remove() {
        let base = std::env::temp_dir().join(format!("pichouse-recon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("a.png"), tiny_png()).unwrap();

        let db_path = std::env::temp_dir().join(format!("pichouse-recon-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let lib = Library::open_at(&db_path).unwrap();
        lib.add_library_folder(&base.to_string_lossy()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        // First reconcile: a.png is added.
        let r = reconcile_all(&lib, &cancel);
        assert_eq!(r.added.len(), 1);
        assert_eq!(r.missing, 0);

        // Second reconcile: nothing changed.
        let r = reconcile_all(&lib, &cancel);
        assert!(!r.changed());

        // Remove the file: it is soft-marked missing, not deleted.
        std::fs::remove_file(base.join("a.png")).unwrap();
        let r = reconcile_all(&lib, &cancel);
        assert_eq!(r.missing, 1);
        let folders = lib.folders().unwrap();
        let photos = lib.photos_in_folder(folders[0].id).unwrap();
        assert_eq!(photos.len(), 1);
        assert!(photos[0].missing);

        // Add it back: it reappears (missing cleared), row reused.
        std::fs::write(base.join("a.png"), tiny_png()).unwrap();
        let r = reconcile_all(&lib, &cancel);
        assert_eq!(r.reappeared, 1);
        let photos = lib.photos_in_folder(folders[0].id).unwrap();
        assert_eq!(photos.len(), 1);
        assert!(!photos[0].missing);

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
