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


/// The phase of the running reconciliation, for the job indicator.
pub mod progress {
    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering::Relaxed};

    pub const IDLE: u8 = 0;
    pub const SNAPSHOT: u8 = 1;
    pub const WALK: u8 = 2;
    pub const CHECK: u8 = 3;
    pub const VANISHED: u8 = 4;
    pub const APPLY: u8 = 5;

    pub static PHASE: AtomicU8 = AtomicU8::new(IDLE);
    /// Directories read in the walk.
    pub static DIRS: AtomicU64 = AtomicU64::new(0);
    /// Image files found in the walk.
    pub static FILES: AtomicU64 = AtomicU64::new(0);
    /// Directories checked against the DB snapshot.
    pub static CHECKED_DIRS: AtomicU64 = AtomicU64::new(0);
    /// Total directories to check.
    pub static TOTAL_DIRS: AtomicU64 = AtomicU64::new(0);
    /// `stat()` calls made by this run (metadata and exists).
    pub static STATS: AtomicU64 = AtomicU64::new(0);

    /// Reset all counters at the start of a run.
    pub fn reset() {
        for c in [&DIRS, &FILES, &CHECKED_DIRS, &TOTAL_DIRS, &STATS] {
            c.store(0, Relaxed);
        }
        PHASE.store(SNAPSHOT, Relaxed);
    }

    pub(super) fn stat() {
        STATS.fetch_add(1, Relaxed);
    }

    /// A one-line text of the current phase and counts.
    pub fn text() -> Option<String> {
        let stats = STATS.load(Relaxed);
        let t = match PHASE.load(Relaxed) {
            SNAPSHOT => "reading the database".to_string(),
            WALK => format!(
                "walking: {} folders, {} images",
                DIRS.load(Relaxed),
                FILES.load(Relaxed)
            ),
            CHECK => format!(
                "checking folders {} / {}, {} file stats",
                CHECKED_DIRS.load(Relaxed),
                TOTAL_DIRS.load(Relaxed),
                stats
            ),
            VANISHED => format!("checking removed folders, {stats} file stats"),
            APPLY => "saving changes".to_string(),
            _ => return None,
        };
        Some(t)
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
    /// The new content of the `scan_dirs` table: (path, mtime in ns). `None`
    /// keeps the table as it is (the one-directory reconcile).
    pub scan_dirs: Option<Vec<(String, i64)>>,
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

/// A read-only snapshot of the folder rows and the photo indexes of the
/// folders that the walk must check. The walk diffs against it in memory.
struct DbSnapshot {
    /// folder path -> folder id.
    folder_id_by_path: HashMap<String, i64>,
    /// folder id -> (photo path -> (id, size, missing)). Only the folders
    /// that the plan reads have an entry.
    index_by_folder: HashMap<i64, HashMap<String, (i64, i64, bool)>>,
}

impl DbSnapshot {
    /// Read the folder rows. Read the photo index only for the folder paths
    /// that `need` accepts. Each read takes the lock briefly.
    fn read(lib: &Library, need: impl Fn(&str) -> bool) -> DbSnapshot {
        let mut folder_id_by_path = HashMap::new();
        let mut ids = Vec::new();
        for f in lib.folders().unwrap_or_default() {
            if need(&f.path) {
                ids.push(f.id);
            }
            folder_id_by_path.insert(f.path, f.id);
        }
        let mut index_by_folder = lib.photo_index_for_folders(&ids).unwrap_or_default();
        for id in ids {
            index_by_folder.entry(id).or_default();
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

/// A directory with an mtime this close to the scan start can change again in
/// the same timestamp tick. The walk does not trust such an mtime.
const MTIME_SLACK_NS: i64 = 2_000_000_000;

/// The state of one recursive walk over all roots.
struct Walk<'a> {
    /// The `scan_dirs` rows from the last walk: path -> mtime in ns.
    known: &'a HashMap<String, i64>,
    /// Parent path -> child paths, built from `known`.
    children: HashMap<String, Vec<String>>,
    /// Read every directory. Ignore the stored mtimes.
    full: bool,
    /// An mtime at or after this value is too recent to trust.
    trust_before_ns: i64,
    cancel: &'a Arc<AtomicBool>,
    /// The directories that the walk read: dir -> image files in it.
    by_dir: HashMap<PathBuf, Vec<PathBuf>>,
    /// The directories that the walk did not read (mtime not changed).
    clean: std::collections::HashSet<String>,
    /// The new `scan_dirs` rows.
    seen: Vec<(String, i64)>,
}

impl<'a> Walk<'a> {
    fn new(known: &'a HashMap<String, i64>, full: bool, cancel: &'a Arc<AtomicBool>) -> Self {
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for path in known.keys() {
            if let Some(parent) = Path::new(path).parent() {
                children
                    .entry(parent.to_string_lossy().into_owned())
                    .or_default()
                    .push(path.clone());
            }
        }
        let now_ns = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        Walk {
            known,
            children,
            full,
            trust_before_ns: now_ns - MTIME_SLACK_NS,
            cancel,
            by_dir: HashMap::new(),
            clean: Default::default(),
            seen: Vec::new(),
        }
    }

    /// Walk `dir` and its subdirectories. Read a directory only when its mtime
    /// changed since the last walk. Get the subdirectories of an unchanged
    /// directory from `known`. An add or a remove of a subdirectory changes
    /// the parent mtime, so that list is correct.
    fn walk(&mut self, dir: &Path) {
        if self.cancel.load(Ordering::Relaxed) {
            return;
        }
        progress::stat();
        let meta = match std::fs::metadata(dir) {
            Ok(m) if m.is_dir() => m,
            _ => return,
        };
        let mtime = mtime_ns(&meta);
        let trusted = mtime > 0 && mtime < self.trust_before_ns;
        let dir_str = dir.to_string_lossy().into_owned();
        progress::DIRS.fetch_add(1, Ordering::Relaxed);

        let unchanged = !self.full && trusted && self.known.get(&dir_str) == Some(&mtime);
        if unchanged {
            let kids = self.children.get(&dir_str).cloned().unwrap_or_default();
            self.clean.insert(dir_str.clone());
            self.seen.push((dir_str, mtime));
            for kid in kids {
                self.walk(Path::new(&kid));
            }
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return, // unreadable: not live, not recorded
        };
        self.seen.push((dir_str, if trusted { mtime } else { -1 }));
        // Ensure the directory itself has an entry even if it holds no images,
        // so a now-empty folder is reconciled (its photos marked missing).
        self.by_dir.entry(dir.to_path_buf()).or_default();
        for entry in entries.flatten() {
            if self.cancel.load(Ordering::Relaxed) {
                return;
            }
            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                self.walk(&path);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_image(name) {
                    progress::FILES.fetch_add(1, Ordering::Relaxed);
                    self.by_dir.entry(dir.to_path_buf()).or_default().push(path);
                }
            }
        }
    }

    /// Whether the walk found `path` (read or unchanged).
    fn is_live(&self, path: &str) -> bool {
        self.clean.contains(path) || self.by_dir.contains_key(Path::new(path))
    }
}

/// Whether `path` is `root` or lies under it.
fn under_root(path: &str, root: &str) -> bool {
    path == root
        || (path.starts_with(root)
            && path[root.len()..].starts_with(std::path::MAIN_SEPARATOR))
}

/// Reconcile every library root against disk. Skips the directories whose
/// mtime did not change since the last walk.
pub fn reconcile_all(lib: &Library, cancel: &Arc<AtomicBool>) -> Report {
    reconcile_all_with(lib, cancel, false)
}

/// Reconcile every library root against disk. Walks each root recursively with
/// no DB lock held, builds a `ReconcilePlan` in memory, then applies the plan
/// in one short transaction. Stops promptly when `cancel` becomes true.
/// `full` reads every directory and ignores the stored mtimes.
pub fn reconcile_all_with(lib: &Library, cancel: &Arc<AtomicBool>, full: bool) -> Report {
    use std::time::Instant;
    let started = Instant::now();
    progress::reset();
    let roots = lib.library_folders().unwrap_or_default();
    let known = if full { HashMap::new() } else { lib.scan_dirs().unwrap_or_default() };
    log::info!(
        "reconcile: {} roots, {} known dirs, full = {full}",
        roots.len(),
        known.len()
    );

    // 1. Walk every root. No DB lock.
    progress::PHASE.store(progress::WALK, Ordering::Relaxed);
    let mut walk = Walk::new(&known, full, cancel);
    for root in &roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let walk_start = Instant::now();
        let (d0, r0) = (progress::DIRS.load(Ordering::Relaxed), walk.by_dir.len());
        walk.walk(Path::new(&root.path));
        log::info!(
            "reconcile: root {} walked in {:.2?} ({} dirs, {} read)",
            root.path,
            walk_start.elapsed(),
            progress::DIRS.load(Ordering::Relaxed) - d0,
            walk.by_dir.len() - r0
        );
    }
    // A cancelled walk saw only part of the disk. Its plan can mark present
    // photos as missing. Stop now. A newer reconcile does the work.
    if cancel.load(Ordering::Relaxed) {
        progress::PHASE.store(progress::IDLE, Ordering::Relaxed);
        log::info!("reconcile: cancelled after {:.2?}.", started.elapsed());
        return Report::default();
    }

    // 2. Read the snapshot for the read directories and the vanished ones.
    progress::PHASE.store(progress::SNAPSHOT, Ordering::Relaxed);
    let snap_start = Instant::now();
    let root_paths: Vec<&str> = roots.iter().map(|r| r.path.as_str()).collect();
    let snapshot = DbSnapshot::read(lib, |p| {
        walk.by_dir.contains_key(Path::new(p))
            || (!walk.is_live(p) && root_paths.iter().any(|r| under_root(p, r)))
    });
    log::info!(
        "reconcile: snapshot read in {:.2?} ({} folders, {} indexed)",
        snap_start.elapsed(),
        snapshot.folder_id_by_path.len(),
        snapshot.index_by_folder.len()
    );

    // 3. Diff the read directories against the snapshot.
    let mut plan = ReconcilePlan::default();
    let mut report = Report::default();
    progress::PHASE.store(progress::CHECK, Ordering::Relaxed);
    progress::TOTAL_DIRS.store(walk.by_dir.len() as u64, Ordering::Relaxed);
    let check_start = Instant::now();
    for (dir, files) in &walk.by_dir {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        plan_dir(&snapshot, dir, files, &mut plan, &mut report);
        progress::CHECKED_DIRS.fetch_add(1, Ordering::Relaxed);
    }
    log::info!(
        "reconcile: checked {} dirs in {:.2?}",
        walk.by_dir.len(),
        check_start.elapsed()
    );

    // 4. Directories that once held photos but are gone: mark photos missing.
    progress::PHASE.store(progress::VANISHED, Ordering::Relaxed);
    plan_vanished_dirs(&snapshot, &root_paths, &walk, cancel, &mut plan, &mut report);
    if cancel.load(Ordering::Relaxed) {
        progress::PHASE.store(progress::IDLE, Ordering::Relaxed);
        return Report::default();
    }

    // 5. Apply the whole batch in one transaction. This is the only DB write.
    plan.scan_dirs = Some(std::mem::take(&mut walk.seen));
    progress::PHASE.store(progress::APPLY, Ordering::Relaxed);
    let apply_start = Instant::now();
    match lib.apply_reconcile_plan(&plan) {
        Ok(added) => report.added = added,
        Err(e) => log::warn!("reconcile: apply plan failed: {e}"),
    }
    progress::PHASE.store(progress::IDLE, Ordering::Relaxed);
    log::info!(
        "reconcile: plan applied in {:.2?} ({} folder upserts, {} inserts)",
        apply_start.elapsed(),
        plan.folder_upserts.len(),
        plan.photo_inserts.len()
    );
    log::info!(
        "reconcile: done in {:.2?} ({} added, {} missing, {} back, {} moved, {} removed)",
        started.elapsed(),
        report.added.len(),
        report.missing,
        report.reappeared,
        report.moved,
        report.removed
    );
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

    // Count the changes in this directory. The folder row needs an upsert
    // only for a new folder or a folder with a change.
    let mut changes = 0usize;
    let mut new_files: Vec<(String, String, i64, i64)> = Vec::new();

    for path in files {
        let path_str = path.to_string_lossy().into_owned();
        match index.get(&path_str) {
            Some((id, _old_size, true)) => {
                // A missing row's file is back at the same path.
                plan.reappeared.push(*id);
                report.reappeared += 1;
                changes += 1;
            }
            Some(_) => {
                // Present and known. Do not stat it. The walk found it.
            }
            None => {
                // A new path. Only a new path needs the size and the mtime.
                progress::stat();
                let meta = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                new_files.push((path_str, name, meta.len() as i64, mtime_secs(&meta)));
            }
        }
    }

    for (path_str, name, size, mod_time) in new_files {
        changes += 1;
        // Try to treat it as a move of a missing row with the same size
        // (preserves tags/edits). Else insert fresh.
        if let Some(moved_id) = take_move_candidate(&mut missing_by_size, size) {
            plan.moves.push(PhotoMove {
                id: moved_id,
                new_dir: dir_str.clone(),
                new_path: path_str,
                new_name: name,
            });
            report.moved += 1;
        } else {
            plan.photo_inserts.push(PhotoInsert {
                dir: dir_str.clone(),
                path: path_str,
                filename: name,
                size,
                mod_time,
            });
            // The id is assigned at apply time and added to the report there.
        }
    }

    // Rows whose file is gone from disk: soft-mark missing (unless already so).
    for (path, (id, _size, missing)) in index {
        if !missing && !on_disk.contains(path) {
            plan.mark_missing.push(*id);
            report.missing += 1;
            changes += 1;
        }
    }

    // Upsert a folder row for a directory that holds images, when it is new
    // or changed. Moves and inserts need the row to resolve the folder id.
    if !files.is_empty() && (!has_folder_row || changes > 0) {
        progress::stat();
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

/// Plan missing-marks for recorded folders under a root whose directory the
/// walk did not find. Does no DB I/O. It reads only the snapshot.
fn plan_vanished_dirs(
    snap: &DbSnapshot,
    roots: &[&str],
    walk: &Walk,
    cancel: &Arc<AtomicBool>,
    plan: &mut ReconcilePlan,
    report: &mut Report,
) {
    for (path, fid) in &snap.folder_id_by_path {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if walk.is_live(path) || !roots.iter().any(|r| under_root(path, r)) {
            continue;
        }
        // Directory gone (or unreadable). Mark its non-missing photos missing.
        if let Some(index) = snap.index_by_folder.get(fid) {
            if !index.values().any(|(_, _, m)| !*m) {
                continue; // all rows already missing
            }
            // A directory that is gone needs no per-file stat. Stat each file
            // only when the directory still exists (for example, unreadable).
            progress::stat();
            let dir_exists = Path::new(path).exists();
            for (p, (id, _size, missing)) in index {
                if *missing {
                    continue;
                }
                let gone = if dir_exists {
                    progress::stat();
                    !Path::new(p).exists()
                } else {
                    true
                };
                if gone {
                    plan.mark_missing.push(*id);
                    report.missing += 1;
                }
            }
        }
    }
}

/// Modification time of a file/dir in nanoseconds since the Unix epoch.
fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
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
    let dir_key = dir.to_string_lossy().into_owned();
    let snapshot = DbSnapshot::read(lib, |p| p == dir_key);
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

    /// Set the mtime of `p` to one hour ago, so the walk trusts it.
    fn age(p: &Path) {
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::open(p).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn reconcile_skips_unchanged_dirs() {
        let base = std::env::temp_dir().join(format!("pichouse-recon3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.png"), tiny_png()).unwrap();
        std::fs::write(b.join("y.png"), tiny_png()).unwrap();
        for d in [&base, &a, &b] {
            age(d);
        }
        let db_path = std::env::temp_dir().join(format!("pichouse-recon3-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let lib = Library::open_at(&db_path).unwrap();
        lib.add_library_folder(&base.to_string_lossy()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        assert_eq!(reconcile_all(&lib, &cancel).added.len(), 2);
        assert_eq!(lib.scan_dirs().unwrap().len(), 3);

        // A file in `b` with the old mtime put back: the walk does not read `b`.
        std::fs::write(b.join("z.png"), tiny_png()).unwrap();
        let old = lib.scan_dirs().unwrap()[&b.to_string_lossy().into_owned()];
        let t = UNIX_EPOCH + std::time::Duration::from_nanos(old as u64);
        std::fs::File::open(&b).unwrap().set_modified(t).unwrap();
        assert!(!reconcile_all(&lib, &cancel).changed(), "unchanged dir is skipped");
        // A full check reads it.
        assert_eq!(reconcile_all_with(&lib, &cancel, true).added.len(), 1);

        // A new subfolder in `a` changes the mtime of `a`. The walk finds it.
        for d in [&base, &a, &b] {
            age(d);
        }
        reconcile_all(&lib, &cancel);
        let c = a.join("c");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(c.join("w.png"), tiny_png()).unwrap();
        assert_eq!(reconcile_all(&lib, &cancel).added.len(), 1, "new subfolder found");

        // A removed folder changes the mtime of `base`. Its photos go missing.
        std::fs::remove_dir_all(&b).unwrap();
        assert_eq!(reconcile_all(&lib, &cancel).missing, 2, "vanished folder found");

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }
}
