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
use crate::model::Folder;
use crate::scan::is_image;

/// A summary of what a reconciliation changed. `added` photo ids are the ones
/// that now need Phase 2 enrichment.
#[derive(Debug, Default)]
pub struct Report {
    pub added: Vec<i64>,
    pub missing: usize,
    pub reappeared: usize,
    pub moved: usize,
    pub removed: usize,
}

impl Report {
    /// Whether anything changed.
    pub fn changed(&self) -> bool {
        !self.added.is_empty()
            || self.missing > 0
            || self.reappeared > 0
            || self.moved > 0
            || self.removed > 0
    }
}


/// A batch of database changes computed by walking the disk with no DB lock
/// held. `Library::apply_reconcile_plan` writes the whole batch in one short
/// transaction, so a long library walk never starves the UI on the single
/// connection mutex.
#[derive(Default)]
pub struct ReconcilePlan {
    /// Directories that hold images: upsert a folder row for each.
    pub folder_upserts: Vec<Folder>,
    /// New image files to insert as Phase-1 structure rows.
    pub photo_inserts: Vec<PhotoInsert>,
    /// Ids of missing rows whose file is back at the same path.
    pub reappeared: Vec<i64>,
    /// Missing rows re-pointed at a new path (a move/rename).
    pub moves: Vec<PhotoMove>,
    /// Ids of rows whose file is gone from disk: soft-mark missing.
    pub mark_missing: Vec<i64>,
    /// Folder row ids to delete (empty on disk, no image subfolders).
    pub folder_deletes: Vec<i64>,
}

/// A new image file to insert. `dir` is its parent directory path, used to
/// resolve the folder id inside the apply transaction.
pub struct PhotoInsert {
    pub dir: String,
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub mod_time: i64,
}

/// A move/rename: re-point a missing row `id` at a new path under `new_dir`.
pub struct PhotoMove {
    pub id: i64,
    pub new_dir: String,
    pub new_path: String,
    pub new_name: String,
}

/// A read-only snapshot of the folder rows and their photo indexes, taken once
/// under a brief lock before the walk. The walk diffs against this in memory.
struct DbSnapshot {
    /// folder path -> folder id.
    folder_id_by_path: HashMap<String, i64>,
    /// folder id -> (photo path -> (id, size, missing)).
    index_by_folder: HashMap<i64, HashMap<String, (i64, i64, bool)>>,
}

impl DbSnapshot {
    /// Read the whole snapshot under one brief lock.
    fn read(lib: &Library) -> DbSnapshot {
        let mut folder_id_by_path = HashMap::new();
        let mut index_by_folder = HashMap::new();
        for f in lib.folders().unwrap_or_default() {
            folder_id_by_path.insert(f.path.clone(), f.id);
            index_by_folder
                .insert(f.id, lib.photo_index_for_folder(f.id).unwrap_or_default());
        }
        DbSnapshot {
            folder_id_by_path,
            index_by_folder,
        }
    }

    /// The photo index for a directory path, if the directory has a folder row.
    fn index_for(&self, dir: &str) -> Option<&HashMap<String, (i64, i64, bool)>> {
        let id = self.folder_id_by_path.get(dir)?;
        self.index_by_folder.get(id)
    }
}

/// Reconcile every library root against disk. Walks each root recursively with
/// no DB lock held, builds a `ReconcilePlan` in memory, then applies the plan
/// in one short transaction. Stops promptly when `cancel` becomes true.
pub fn reconcile_all(lib: &Library, cancel: &Arc<AtomicBool>) -> Report {
    let roots = lib.library_folders().unwrap_or_default();
    let snapshot = DbSnapshot::read(lib);
    let mut plan = ReconcilePlan::default();
    let mut report = Report::default();

    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // Collect image files grouped by directory under this root (no lock).
        let mut by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        collect(Path::new(&root.path), cancel, &mut by_dir);
        for (dir, files) in &by_dir {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            plan_dir(&snapshot, dir, files, &mut plan, &mut report);
        }
        // Directories that once held photos but no longer exist on disk: mark
        // all their photos missing.
        plan_vanished_dirs(&snapshot, &root.path, &by_dir, &mut plan, &mut report);
    }

    // Apply the whole batch in one transaction. This is the only DB write.
    if let Ok(added) = lib.apply_reconcile_plan(&plan) {
        report.added = added;
    }
    report
}

/// Plan the reconcile for a single directory against the snapshot. `files` is
/// the set of image paths currently on disk in `dir`. Pushes changes into
/// `plan` and counts them in `report`. Does no DB I/O.
fn plan_dir(
    snap: &DbSnapshot,
    dir: &Path,
    files: &[PathBuf],
    plan: &mut ReconcilePlan,
    report: &mut Report,
) {
    let dir_str = dir.to_string_lossy().into_owned();

    // A directory with no images does not get a folder row unless it already
    // has one. Pure-container directories (only subfolders) stay out of the
    // `folders` table — they exist only as albums in the Library tree.
    let has_folder_row = snap.folder_id_by_path.contains_key(&dir_str);
    if files.is_empty() && !has_folder_row {
        return; // never had photos; nothing to reconcile
    }

    // Upsert a folder row for a directory that holds images.
    if !files.is_empty() {
        if let Ok(meta) = std::fs::metadata(dir) {
            let mtime = mtime_secs(&meta);
            plan.folder_upserts.push(Folder {
                path: dir_str.clone(),
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                mtime,
                year: crate::scan::year_of(mtime),
                ..Default::default()
            });
        }
    }

    // The known rows for this directory, from the snapshot.
    let empty = HashMap::new();
    let index = snap.index_for(&dir_str).unwrap_or(&empty);
    let on_disk: std::collections::HashSet<String> =
        files.iter().map(|p| p.to_string_lossy().into_owned()).collect();

    // Currently-missing rows in this folder, grouped by size, for move matching.
    let mut missing_by_size: HashMap<i64, Vec<i64>> = HashMap::new();
    for (_path, (id, size, missing)) in index {
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
                plan.reappeared.push(*id);
                report.reappeared += 1;
            }
            Some(_) => {
                // Present and known; nothing to do.
            }
            None => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // New path. Try to treat it as a move of a missing row with the
                // same size (preserves tags/edits); else insert fresh.
                if let Some(moved_id) = take_move_candidate(&mut missing_by_size, size) {
                    plan.moves.push(PhotoMove {
                        id: moved_id,
                        new_dir: dir_str.clone(),
                        new_path: path_str.clone(),
                        new_name: name,
                    });
                    report.moved += 1;
                } else {
                    plan.photo_inserts.push(PhotoInsert {
                        dir: dir_str.clone(),
                        path: path_str.clone(),
                        filename: name,
                        size,
                        mod_time: mtime_secs(&meta),
                    });
                    // The id is assigned at apply time and added to the report
                    // there.
                }
            }
        }
    }

    // Rows whose file is gone from disk: soft-mark missing (unless already so).
    for (path, (id, _size, missing)) in index {
        if !missing && !on_disk.contains(path) {
            plan.mark_missing.push(*id);
            report.missing += 1;
        }
    }

    // If this directory holds no images on disk and has no image-containing
    // subfolders, remove the folder row outright (cascading its now-missing
    // photos). A pure container (subfolders that hold images) is NEVER removed —
    // it stays as an album in the Library tree.
    if files.is_empty() && !dir_has_images(dir) {
        if let Some(id) = snap.folder_id_by_path.get(&dir_str) {
            plan.folder_deletes.push(*id);
            report.removed += 1;
        }
    }
}

/// Whether `dir` contains any image anywhere beneath it (recursively). Used to
/// distinguish a truly empty folder (removable) from a container of subfolders
/// that hold images (kept as an album).
fn dir_has_images(dir: &Path) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {
                if dir_has_images(&path) {
                    return true;
                }
            }
            Ok(_) => {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(is_image)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            Err(_) => {}
        }
    }
    false
}

/// Pop one missing-row id with the given size, if any (move candidate).
fn take_move_candidate(missing_by_size: &mut HashMap<i64, Vec<i64>>, size: i64) -> Option<i64> {
    let ids = missing_by_size.get_mut(&size)?;
    ids.pop()
}

/// Plan missing-marks for recorded folders under `root_path` whose directory no
/// longer appears on disk. Does no DB I/O; reads only the snapshot.
fn plan_vanished_dirs(
    snap: &DbSnapshot,
    root_path: &str,
    by_dir: &HashMap<PathBuf, Vec<PathBuf>>,
    plan: &mut ReconcilePlan,
    report: &mut Report,
) {
    let live: std::collections::HashSet<String> = by_dir
        .keys()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let prefix = format!("{}{}", root_path, std::path::MAIN_SEPARATOR);
    for (path, fid) in &snap.folder_id_by_path {
        if path != root_path && !path.starts_with(&prefix) {
            continue; // not under this root
        }
        if live.contains(path) {
            continue; // still has photos on disk
        }
        // Directory gone (or now empty). Mark its non-missing photos missing.
        if let Some(index) = snap.index_by_folder.get(fid) {
            for (p, (id, _size, missing)) in index {
                if !*missing && !Path::new(p).exists() {
                    plan.mark_missing.push(*id);
                    report.missing += 1;
                }
            }
        }
    }
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
    // Build a one-directory plan against a fresh snapshot, then apply it.
    let snapshot = DbSnapshot::read(lib);
    let mut plan = ReconcilePlan::default();
    let mut report = Report::default();
    plan_dir(&snapshot, dir, &files, &mut plan, &mut report);
    if let Ok(added) = lib.apply_reconcile_plan(&plan) {
        report.added = added;
    }
    report
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
        // Image lives in a subfolder so the root acts as a container and the
        // subfolder as the image-holding leaf.
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.png"), tiny_png()).unwrap();

        let db_path = std::env::temp_dir().join(format!("pichouse-recon-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let lib = Library::open_at(&db_path).unwrap();
        lib.add_library_folder(&base.to_string_lossy()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        // First reconcile: a.png is added. Only the leaf subfolder gets a folder
        // row; the container root does not (no phantom 0-image folder).
        let r = reconcile_all(&lib, &cancel);
        assert_eq!(r.added.len(), 1);
        assert_eq!(r.missing, 0);
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 1, "only the image-holding subfolder is a folder row");
        assert!(folders[0].path.ends_with("sub"));

        // Second reconcile: nothing changed.
        let r = reconcile_all(&lib, &cancel);
        assert!(!r.changed());

        // Remove the only image: the leaf folder is now empty on disk and has no
        // image subfolders, so its folder row (and photo) is removed outright.
        std::fs::remove_file(sub.join("a.png")).unwrap();
        let r = reconcile_all(&lib, &cancel);
        assert_eq!(r.removed, 1);
        assert!(lib.folders().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }

    #[test]
    fn reconcile_keeps_container_but_not_phantom_folder() {
        // A directory that holds only a subfolder-of-images must not become a
        // folder row; only the image-holding subfolder does.
        let base = std::env::temp_dir()
            .join(format!("pichouse-recon2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let container = base.join("Trips");
        let leaf = container.join("2020");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(leaf.join("p.png"), tiny_png()).unwrap();

        let db_path = std::env::temp_dir()
            .join(format!("pichouse-recon2-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let lib = Library::open_at(&db_path).unwrap();
        lib.add_library_folder(&base.to_string_lossy()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        reconcile_all(&lib, &cancel);
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 1, "only the leaf holding images is a folder");
        assert!(folders[0].path.ends_with("2020"));
        // Neither the container nor the root has a folder row.
        assert!(lib.folder_id_by_path(&container.to_string_lossy()).unwrap().is_none());
        assert!(lib.folder_id_by_path(&base.to_string_lossy()).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
