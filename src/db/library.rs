//! `library.db` metadata database: folders, photos, settings, scan state.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{Folder, LibraryFolder, Photo, PhotoScanState, ScanStatus};

use super::Result;

const SCHEMA: &str = include_str!("schema.sql");

/// A handle to the `library.db` metadata database.
///
/// The connection is wrapped in a `Mutex`. This gives interior mutability and
/// serializes every access, which also serializes the multi-statement tag
/// writes and their FTS maintenance.
pub struct Library {
    pub(super) conn: Mutex<Connection>,
}

/// Current Unix time in seconds.
pub(super) fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Library {
    /// Acquire the shared connection lock, logging when the wait is long.
    ///
    /// Every DB method funnels through here, so a stall waiting on the single
    /// `Mutex<Connection>` (the main source of multi-second UI freezes during a
    /// scan) is visible in the log instead of being invisible. A `debug!` line
    /// is emitted before waiting and a `warn!` when the wait exceeds ~200 ms,
    /// tagged with the caller so a hang points at the exact operation.
    #[track_caller]
    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        let caller = std::panic::Location::caller();
        // Fast path: try to take the lock without blocking or logging.
        if let Ok(g) = self.conn.try_lock() {
            return g;
        }
        // The lock is held elsewhere; this call will block. Log before waiting
        // so a hang shows this "waiting" line with no matching "acquired".
        log::debug!("db lock: waiting for connection ({caller})");
        let t = std::time::Instant::now();
        let g = self.conn.lock().unwrap();
        let waited = t.elapsed();
        if waited.as_millis() >= 200 {
            log::warn!("db lock: waited {:.2?} ({caller})", waited);
        } else {
            log::debug!("db lock: acquired after {:.2?} ({caller})", waited);
        }
        g
    }
}

/// Add columns introduced after the first release to an existing `photos`
/// table, so a database created by an older build gains them without a rebuild.
/// Each `ALTER TABLE ... ADD COLUMN` is idempotent here because we first read
/// the existing column set.
fn migrate(conn: &Connection) -> Result<()> {
    let mut have: std::collections::HashSet<String> = std::collections::HashSet::new();
    {
        let mut stmt = conn.prepare("PRAGMA table_info(photos)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        for name in rows {
            have.insert(name?);
        }
    }
    if !have.contains("scan_state") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN scan_state INTEGER NOT NULL DEFAULT 0;",
        )?;
        // Existing rows already have their EXIF/dimensions/hash, so mark them
        // done rather than re-enriching the whole library.
        conn.execute_batch("UPDATE photos SET scan_state = 2 WHERE hash <> '';")?;
    }
    if !have.contains("missing") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN missing INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    if !have.contains("added_at") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN added_at INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // photos.face_status: per-photo face-scan state mirror. 0 = not scanned,
    // 1 = queued, 2 = done, 3 = error. The face_scan table holds the detail;
    // this column gives a cheap filter on the photos row.
    if !have.contains("face_status") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN face_status INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // photos.style_face_status: per-photo stylised-face-scan state mirror. Same
    // meaning as face_status but for the anime/cartoon/furry face system.
    if !have.contains("style_face_status") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN style_face_status INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // photos.skip_face_scan: 1 when the user marks the photo unimportant. A
    // skipped photo is excluded from every future face scan (human and
    // stylised).
    if !have.contains("skip_face_scan") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN skip_face_scan INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // photos.phash: 64-bit perceptual hash (dHash) for the duplicate finder,
    // stored as a signed INTEGER bit-cast from u64. 0 means not yet computed.
    // Existing rows are backfilled lazily on the first duplicate scan.
    if !have.contains("phash") {
        conn.execute_batch(
            "ALTER TABLE photos ADD COLUMN phash INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    // albums.kind: face-recognition kind (0 inherit, 1 Photo, 2 Art).
    {
        let mut ac: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut stmt = conn.prepare("PRAGMA table_info(albums)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            for name in rows {
                ac.insert(name?);
            }
        }
        if !ac.contains("kind") {
            conn.execute_batch("ALTER TABLE albums ADD COLUMN kind INTEGER NOT NULL DEFAULT 0;")?;
        }
    }
    // library_folders.first_scan_done_at (freshness "new files" boundary).
    {
        let mut lf: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut stmt = conn.prepare("PRAGMA table_info(library_folders)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            for name in rows {
                lf.insert(name?);
            }
        }
        if !lf.contains("first_scan_done_at") {
            conn.execute_batch(
                "ALTER TABLE library_folders ADD COLUMN first_scan_done_at INTEGER NOT NULL DEFAULT 0;",
            )?;
            // Existing libraries already finished their first scan; stamp now so
            // their photos are not treated as new (nothing recorded before this
            // moment counts as new).
            conn.execute_batch(
                "UPDATE library_folders SET first_scan_done_at = strftime('%s','now') WHERE first_scan_done_at = 0;",
            )?;
        }
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_photos_scan_state ON photos(scan_state);",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_photos_added_at ON photos(added_at);",
    )?;
    Ok(())
}

impl Library {
    /// Open (and initialize) `library.db` in the pichouse data directory.
    pub fn open() -> Result<Library> {
        let dir = super::config::data_dir()?;
        Library::open_at(dir.join("library.db"))
    }

    /// Open (and initialize) a library database at the given path.
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Library> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Library {
            conn: Mutex::new(conn),
        })
    }

    /// Record a user-added root folder. Idempotent.
    pub fn add_library_folder(&self, path: &str) -> Result<LibraryFolder> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO library_folders(path, added_at) VALUES(?1, ?2)
             ON CONFLICT(path) DO NOTHING",
            params![path, now()],
        )?;
        let lf = conn.query_row(
            "SELECT id, path, added_at, first_scan_done_at FROM library_folders WHERE path = ?1",
            params![path],
            |r| {
                Ok(LibraryFolder {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    added_at: r.get(2)?,
                    first_scan_done_at: r.get(3)?,
                })
            },
        )?;
        Ok(lf)
    }

    /// Delete a user-added root folder and all folders/photos scanned beneath it
    /// (matched by path prefix).
    pub fn remove_library_folder(&self, path: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let prefix = format!("{}{}%", path, std::path::MAIN_SEPARATOR);
        tx.execute(
            "DELETE FROM folders WHERE path = ?1 OR path LIKE ?2",
            params![path, prefix],
        )?;
        tx.execute("DELETE FROM library_folders WHERE path = ?1", params![path])?;
        tx.commit()?;
        Ok(())
    }

    /// All user-added root folders.
    pub fn library_folders(&self) -> Result<Vec<LibraryFolder>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, path, added_at, first_scan_done_at FROM library_folders ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(LibraryFolder {
                id: r.get(0)?,
                path: r.get(1)?,
                added_at: r.get(2)?,
                first_scan_done_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Mark a root's first full scan as complete (records the boundary after
    /// which added files count as "new"). No-op if already stamped, so a rescan
    /// does not move the boundary forward.
    pub fn mark_first_scan_done(&self, root_path: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE library_folders SET first_scan_done_at = ?1
             WHERE path = ?2 AND first_scan_done_at = 0",
            params![now(), root_path],
        )?;
        Ok(())
    }

    /// Insert or update a scanned folder and return its id.
    pub fn upsert_folder(&self, f: &Folder) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO folders(path, name, mtime, year) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET name=excluded.name, mtime=excluded.mtime, year=excluded.year",
            params![f.path, f.name, f.mtime, f.year],
        )?;
        let id =
            conn.query_row("SELECT id FROM folders WHERE path = ?1", params![f.path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// The id of a scanned folder by path, without creating it. `None` if no
    /// such folder row exists.
    pub fn folder_id_by_path(&self, path: &str) -> Result<Option<i64>> {
        let conn = self.lock();
        let id: Option<i64> = conn
            .query_row("SELECT id FROM folders WHERE path = ?1", params![path], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(id)
    }

    /// Delete a scanned folder row (and, by cascade, its photos and album
    /// membership). Used to drop a folder that no longer holds any images.
    pub fn delete_folder(&self, folder_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM folders WHERE id = ?1", params![folder_id])?;
        Ok(())
    }

    /// Count photos currently marked missing (soft-deleted from disk).
    pub fn missing_photo_count(&self) -> Result<i64> {
        let conn = self.lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM photos WHERE missing = 1", [], |r| {
            r.get(0)
        })?;
        Ok(n)
    }

    /// Hard-delete all photo rows marked missing. Their tags and virtual-album
    /// memberships are removed by ON DELETE CASCADE. Returns the number of rows
    /// deleted.
    pub fn delete_missing_photos(&self) -> Result<usize> {
        let conn = self.lock();
        let n = conn.execute("DELETE FROM photos WHERE missing = 1", [])?;
        Ok(n)
    }

    /// All photos currently marked missing (soft-deleted from disk), grouped by
    /// folder then filename.
    pub fn photos_missing(&self) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan
             FROM photos WHERE missing = 1 ORDER BY folder_id ASC, filename ASC",
        )?;
        let rows = stmt.query_map([], map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// All scanned folders ordered by year (desc) then name.
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, path, name, mtime, year FROM folders ORDER BY year DESC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                path: r.get(1)?,
                name: r.get(2)?,
                mtime: r.get(3)?,
                year: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Load a single folder by id.
    pub fn folder_by_id(&self, id: i64) -> Result<Option<Folder>> {
        let conn = self.lock();
        let f = conn
            .query_row(
                "SELECT id, path, name, mtime, year FROM folders WHERE id = ?1",
                params![id],
                |r| {
                    Ok(Folder {
                        id: r.get(0)?,
                        path: r.get(1)?,
                        name: r.get(2)?,
                        mtime: r.get(3)?,
                        year: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(f)
    }

    /// A map of folder id to its photo count.
    pub fn folder_photo_counts(&self) -> Result<std::collections::HashMap<i64, i64>> {        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT folder_id, COUNT(*) FROM photos GROUP BY folder_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (fid, n) = row?;
            out.insert(fid, n);
        }
        Ok(out)
    }

    /// Insert or update a photo by path and return its id. Does not modify an
    /// existing photo's `orientation` or `ai_status`. Sets `scan_state` from the
    /// photo (a fully populated photo should pass `PhotoScanState::Done`).
    #[allow(dead_code)] // Kept API for single-photo upsert.
    pub fn upsert_photo(&self, p: &Photo) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO photos(folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, scan_state, missing, added_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, 0, ?12)
             ON CONFLICT(path) DO UPDATE SET
               folder_id=excluded.folder_id, filename=excluded.filename, size=excluded.size,
               mod_time=excluded.mod_time, taken_at=excluded.taken_at, width=excluded.width,
               height=excluded.height, hash=excluded.hash, scan_state=excluded.scan_state,
               missing=0",
            params![
                p.folder_id, p.path, p.filename, p.size, p.mod_time, p.taken_at,
                p.width, p.height, p.hash, p.thumb_ready as i64, p.scan_state.as_i64(), now()
            ],
        )?;
        let id =
            conn.query_row("SELECT id FROM photos WHERE path = ?1", params![p.path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// Phase 1 insert: record only cheap stat data (path, filename, folder_id,
    /// size, mod_time) and return the photo id. EXIF, dimensions, and hash are
    /// left empty for the Phase 2 enrichment worker. Never clobbers an existing
    /// row's enriched fields (hash/dimensions/taken_at/scan_state), so a rescan
    /// preserves prior enrichment; it only clears the `missing` flag and updates
    /// size/mod_time.
    pub fn upsert_photo_structure(&self, p: &Photo) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO photos(folder_id, path, filename, size, mod_time, scan_state, missing, added_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 0, 0, ?6)
             ON CONFLICT(path) DO UPDATE SET
               folder_id=excluded.folder_id, filename=excluded.filename,
               size=excluded.size, mod_time=excluded.mod_time, missing=0",
            params![p.folder_id, p.path, p.filename, p.size, p.mod_time, now()],
        )?;
        let id =
            conn.query_row("SELECT id FROM photos WHERE path = ?1", params![p.path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// Apply a whole reconcile plan in one short transaction. The disk walk
    /// runs with no DB lock and builds the plan in memory. This method takes
    /// the lock once, so a long library walk never starves the UI on the
    /// single connection mutex. Returns the new photo ids that need Phase 2
    /// enrichment (fresh inserts plus moved rows).
    pub fn apply_reconcile_plan(
        &self,
        plan: &crate::reconcile::ReconcilePlan,
    ) -> Result<Vec<i64>> {
        let ts = now();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut added: Vec<i64> = Vec::new();
        {
            // 1. Upsert folders for directories that hold images. Build a
            //    path -> id map so photo inserts can resolve their folder id.
            let mut folder_ids: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            for f in &plan.folder_upserts {
                tx.execute(
                    "INSERT INTO folders(path, name, mtime, year) VALUES(?1, ?2, ?3, ?4)
                     ON CONFLICT(path) DO UPDATE SET name=excluded.name, mtime=excluded.mtime, year=excluded.year",
                    params![f.path, f.name, f.mtime, f.year],
                )?;
                let id: i64 = tx.query_row(
                    "SELECT id FROM folders WHERE path = ?1",
                    params![f.path],
                    |r| r.get(0),
                )?;
                folder_ids.insert(f.path.clone(), id);
            }

            // 2. Reappeared: a missing row's file is back at the same path.
            for id in &plan.reappeared {
                tx.execute(
                    "UPDATE photos SET missing = 0 WHERE id = ?1",
                    params![id],
                )?;
            }

            // 3. Moves: re-point a missing row at a new path (keeps tags/edits).
            for m in &plan.moves {
                let Some(fid) = folder_ids.get(&m.new_dir) else {
                    continue; // folder upsert failed; skip
                };
                tx.execute(
                    "UPDATE photos SET folder_id = ?1, path = ?2, filename = ?3, missing = 0 WHERE id = ?4",
                    params![fid, m.new_path, m.new_name, m.id],
                )?;
                added.push(m.id); // re-hash to confirm identity
            }

            // 4. New photos: insert a Phase-1 structure row, collect its id.
            {
                let mut ins = tx.prepare(
                    "INSERT INTO photos(folder_id, path, filename, size, mod_time, scan_state, missing, added_at)
                     VALUES(?1, ?2, ?3, ?4, ?5, 0, 0, ?6)
                     ON CONFLICT(path) DO UPDATE SET
                       folder_id=excluded.folder_id, filename=excluded.filename,
                       size=excluded.size, mod_time=excluded.mod_time, missing=0",
                )?;
                let mut sel = tx.prepare("SELECT id FROM photos WHERE path = ?1")?;
                for p in &plan.photo_inserts {
                    let Some(fid) = folder_ids.get(&p.dir) else {
                        continue; // folder upsert failed; skip
                    };
                    ins.execute(params![fid, p.path, p.filename, p.size, p.mod_time, ts])?;
                    let id: i64 = sel.query_row(params![p.path], |r| r.get(0))?;
                    added.push(id);
                }
            }

            // 5. Missing: soft-mark rows whose file is gone from disk.
            for id in &plan.mark_missing {
                tx.execute(
                    "UPDATE photos SET missing = 1 WHERE id = ?1",
                    params![id],
                )?;
            }

            // 6. Delete folder rows that hold no images and have no image
            //    subfolders (cascades their now-missing photos).
            for id in &plan.folder_deletes {
                tx.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
            }
        }
        tx.commit()?;
        Ok(added)
    }

    /// Record a whole directory's photos in one transaction (Phase 1). Far
    /// cheaper than `upsert_photo_structure` per file: it takes the DB lock once
    /// for the batch instead of twice per photo, which speeds up the scan and,
    /// crucially, leaves the lock free between batches so the Phase 2 enrichment
    /// workers (and the UI) are not starved during a large scan. Does not return
    /// ids.
    pub fn insert_structure_batch(&self, photos: &[Photo]) -> Result<()> {
        if photos.is_empty() {
            return Ok(());
        }
        let ts = now();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO photos(folder_id, path, filename, size, mod_time, scan_state, missing, added_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 0, 0, ?6)
                 ON CONFLICT(path) DO UPDATE SET
                   folder_id=excluded.folder_id, filename=excluded.filename,
                   size=excluded.size, mod_time=excluded.mod_time, missing=0",
            )?;
            for p in photos {
                stmt.execute(params![p.folder_id, p.path, p.filename, p.size, p.mod_time, ts])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Ids of photos still needing Phase 2 enrichment (structured, not missing).
    /// Pass `Some(folder_id)` to limit to one folder, `None` for the whole
    /// library. Ordered by folder then filename for a stable worklist.
    pub fn photos_needing_enrichment(&self, folder_id: Option<i64>) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut out = Vec::new();
        match folder_id {
            Some(fid) => {
                let mut stmt = conn.prepare(
                    "SELECT id FROM photos WHERE scan_state <> 2 AND missing = 0 AND folder_id = ?1
                     ORDER BY filename ASC",
                )?;
                let rows = stmt.query_map(params![fid], |r| r.get::<_, i64>(0))?;
                for row in rows {
                    out.push(row?);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id FROM photos WHERE scan_state <> 2 AND missing = 0
                     ORDER BY folder_id ASC, filename ASC",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
                for row in rows {
                    out.push(row?);
                }
            }
        }
        Ok(out)
    }

    /// Record the Phase 2 enrichment result for a photo: EXIF taken date,
    /// pixel dimensions, and content hash, and mark it done.
    pub fn enrich_photo(
        &self,
        id: i64,
        taken_at: i64,
        width: i32,
        height: i32,
        hash: &str,
        phash: u64,
    ) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET taken_at = ?1, width = ?2, height = ?3, hash = ?4, phash = ?5, scan_state = 2
             WHERE id = ?6",
            params![taken_at, width, height, hash, phash as i64, id],
        )?;
        Ok(())
    }

    /// Set a photo's two-phase import state.
    pub fn set_photo_scan_state(&self, id: i64, state: PhotoScanState) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET scan_state = ?1 WHERE id = ?2",
            params![state.as_i64(), id],
        )?;
        Ok(())
    }

    /// Mark a photo missing (file gone from disk) or present again. The row and
    /// its tags/edits are kept regardless.
    pub fn set_photo_missing(&self, id: i64, missing: bool) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET missing = ?1 WHERE id = ?2",
            params![missing as i64, id],
        )?;
        Ok(())
    }

    /// Set a folder's year (refined from the earliest enriched `taken_at`).
    pub fn set_folder_year(&self, folder_id: i64, year: i32) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE folders SET year = ?1 WHERE id = ?2",
            params![year, folder_id],
        )?;
        Ok(())
    }

    /// The earliest known EXIF taken date among a folder's enriched photos, if
    /// any (ignores the `0` "unknown" sentinel).
    pub fn earliest_taken_at(&self, folder_id: i64) -> Result<Option<i64>> {
        let conn = self.lock();
        let v: Option<i64> = conn.query_row(
            "SELECT MIN(taken_at) FROM photos WHERE folder_id = ?1 AND taken_at > 0",
            params![folder_id],
            |r| r.get(0),
        )?;
        Ok(v)
    }

    /// "New files" grouped by folder: photos added to the library after their
    /// owning root's first scan completed, within the last `max_age_secs`, and
    /// still present on disk. Returned as `(Folder, photos)` pairs, folders
    /// ordered by their most-recent addition first, photos newest first.
    pub fn new_photos_grouped(
        &self,
        max_age_secs: i64,
    ) -> Result<Vec<(Folder, Vec<Photo>)>> {
        // Per-root boundary: a photo is new only if added after the root that
        // owns it finished its first scan. Roots are matched by path prefix.
        let roots = self.library_folders()?;
        let now_ts = now();
        let age_threshold = now_ts - max_age_secs;

        let conn = self.lock();
        // Candidate photos: recent, not missing. Join the folder for its path.
        let mut stmt = conn.prepare(
            "SELECT p.id, p.folder_id, p.path, p.filename, p.size, p.mod_time, p.taken_at,
                    p.width, p.height, p.hash, p.thumb_ready, p.orientation, p.ai_status,
                    p.scan_state, p.missing, p.added_at, p.phash, p.skip_face_scan,
                    f.id, f.path, f.name, f.mtime, f.year
             FROM photos p JOIN folders f ON f.id = p.folder_id
             WHERE p.missing = 0 AND p.added_at >= ?1
             ORDER BY p.added_at DESC, p.filename ASC",
        )?;
        let rows = stmt.query_map(params![age_threshold], |r| {
            let photo = map_photo(r)?;
            let folder = Folder {
                id: r.get(18)?,
                path: r.get(19)?,
                name: r.get(20)?,
                mtime: r.get(21)?,
                year: r.get(22)?,
            };
            Ok((photo, folder))
        })?;

        // Group, applying the per-root first-scan boundary.
        let mut order: Vec<i64> = Vec::new();
        let mut folders: std::collections::HashMap<i64, Folder> =
            std::collections::HashMap::new();
        let mut grouped: std::collections::HashMap<i64, Vec<Photo>> =
            std::collections::HashMap::new();
        for row in rows {
            let (photo, folder) = row?;
            // Find the owning root by matching path prefix; use its boundary.
            let boundary = roots
                .iter()
                .filter(|root| {
                    folder.path == root.path
                        || folder.path.starts_with(&format!(
                            "{}{}",
                            root.path,
                            std::path::MAIN_SEPARATOR
                        ))
                })
                .map(|root| root.first_scan_done_at)
                .max()
                .unwrap_or(0);
            // Boundary 0 means the first scan has not completed yet: nothing is
            // "new" until the initial scan finishes. Strictly-after comparison
            // avoids counting photos inserted in the same second the scan
            // completed (which belong to the initial import).
            if boundary == 0 || photo.added_at <= boundary {
                continue;
            }
            if !grouped.contains_key(&folder.id) {
                order.push(folder.id);
                folders.insert(folder.id, folder);
            }
            grouped.entry(photo.folder_id).or_default().push(photo);
        }

        let mut out = Vec::new();
        for fid in order {
            if let (Some(folder), Some(photos)) =
                (folders.remove(&fid), grouped.remove(&fid))
            {
                out.push((folder, photos));
            }
        }
        Ok(out)
    }

    /// The number of "new files" across the whole library (see
    /// `new_photos_grouped`). Used for the sidebar count.
    pub fn new_photos_count(&self, max_age_secs: i64) -> Result<i64> {
        let grouped = self.new_photos_grouped(max_age_secs)?;
        Ok(grouped.iter().map(|(_, ps)| ps.len() as i64).sum())
    }


    /// All photos for a folder ordered by taken date then name.
    pub fn photos_in_folder(&self, folder_id: i64) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan
             FROM photos WHERE folder_id = ?1 ORDER BY taken_at ASC, filename ASC",
        )?;
        let rows = stmt.query_map(params![folder_id], map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Load a single photo by id.
    pub fn photo_by_id(&self, id: i64) -> Result<Option<Photo>> {
        let conn = self.lock();
        let p = conn
            .query_row(
                "SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan
                 FROM photos WHERE id = ?1",
                params![id],
                map_photo,
            )
            .optional()?;
        Ok(p)
    }

    /// A map of file path to (photo id, size, missing) for every photo in a
    /// folder. Used by reconciliation to diff disk against the database.
    pub fn photo_index_for_folder(
        &self,
        folder_id: i64,
    ) -> Result<std::collections::HashMap<String, (i64, i64, bool)>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT path, id, size, missing FROM photos WHERE folder_id = ?1")?;
        let rows = stmt.query_map(params![folder_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? != 0,
            ))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, id, size, missing) = row?;
            out.insert(path, (id, size, missing));
        }
        Ok(out)
    }

    /// Re-point a photo row at a new path (used when a missing file reappears
    /// under a new name/location — a move/rename — so tags and edits follow it).
    pub fn move_photo_path(&self, id: i64, new_folder_id: i64, new_path: &str, new_name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET folder_id = ?1, path = ?2, filename = ?3, missing = 0 WHERE id = ?4",
            params![new_folder_id, new_path, new_name, id],
        )?;
        Ok(())
    }

    /// A map of file path to content hash for all photos whose parent directory
    /// is `dir`. Used by the raw folder view to reuse scanned thumbnails.
    pub fn hashes_by_dir(&self, dir: &str) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT p.path, p.hash FROM photos p
             JOIN folders f ON f.id = p.folder_id
             WHERE f.path = ?1",
        )?;
        let rows = stmt.query_map(params![dir], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, hash) = row?;
            out.insert(path, hash);
        }
        Ok(out)
    }

    /// Mark whether a photo's thumbnail has been generated.
    #[allow(dead_code)] // Kept API; thumbnails are cached by hash, not by this flag.
    pub fn set_thumb_ready(&self, photo_id: i64, ready: bool) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET thumb_ready = ?1 WHERE id = ?2",
            params![ready as i64, photo_id],
        )?;
        Ok(())
    }

    /// Store the user-applied rotation (degrees clockwise, normalized to
    /// 0/90/180/270). Never written to disk; lives only in the database.
    pub fn set_orientation(&self, photo_id: i64, degrees: i32) -> Result<()> {
        let degrees = ((degrees % 360) + 360) % 360;
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET orientation = ?1 WHERE id = ?2",
            params![degrees, photo_id],
        )?;
        Ok(())
    }

    /// The stored value for `key`, or `def` if unset.
    pub fn get_setting(&self, key: &str, def: &str) -> Result<String> {
        let conn = self.lock();
        let v: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or_else(|| def.to_string()))
    }

    /// Store a value for `key`.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Record the scan status for a folder.
    pub fn set_scan_state(&self, folder_id: i64, status: ScanStatus) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO scan_state(folder_id, last_scanned, status) VALUES(?1, ?2, ?3)
             ON CONFLICT(folder_id) DO UPDATE SET last_scanned=excluded.last_scanned, status=excluded.status",
            params![folder_id, now(), status.as_str()],
        )?;
        Ok(())
    }
}

/// Map a photo row (16 columns, in schema order) to a `Photo`.
pub(super) fn map_photo(r: &rusqlite::Row) -> rusqlite::Result<Photo> {
    Ok(Photo {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        path: r.get(2)?,
        filename: r.get(3)?,
        size: r.get(4)?,
        mod_time: r.get(5)?,
        taken_at: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        hash: r.get(9)?,
        thumb_ready: r.get::<_, i64>(10)? != 0,
        orientation: r.get(11)?,
        ai_status: crate::model::AiStatus::from_i64(r.get::<_, i64>(12)?),
        scan_state: crate::model::PhotoScanState::from_i64(r.get::<_, i64>(13)?),
        missing: r.get::<_, i64>(14)? != 0,
        added_at: r.get(15)?,
        phash: r.get::<_, i64>(16)? as u64,
        skip_face_scan: r.get::<_, i64>(17)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Folder, Photo};

    fn temp_lib() -> (Library, std::path::PathBuf) {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-lib-{}-{:?}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        (Library::open_at(&p).unwrap(), p)
    }

    #[test]
    fn new_files_respects_first_scan_boundary() {
        let (lib, path) = temp_lib();
        let root = "/tmp/pichouse-newfiles-root";
        lib.add_library_folder(root).unwrap();
        let fid = lib
            .upsert_folder(&Folder {
                path: format!("{root}/sub"),
                name: "sub".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();

        // Photo added during the initial scan (before boundary).
        let before = lib
            .upsert_photo_structure(&Photo {
                folder_id: fid,
                path: format!("{root}/sub/old.jpg"),
                filename: "old.jpg".into(),
                ..Default::default()
            })
            .unwrap();

        // Complete the first scan: sets the boundary to "now".
        lib.mark_first_scan_done(root).unwrap();

        // Before the boundary there are no new files.
        assert_eq!(lib.new_photos_count(3600).unwrap(), 0);

        // Ensure the next insert lands in a later second than the boundary
        // (added_at is second-granularity; "new" is strictly after the scan).
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // A photo added after the boundary is "new".
        let after = lib
            .upsert_photo_structure(&Photo {
                folder_id: fid,
                path: format!("{root}/sub/new.jpg"),
                filename: "new.jpg".into(),
                ..Default::default()
            })
            .unwrap();
        assert_ne!(before, after);

        let groups = lib.new_photos_grouped(3600).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0.id, fid);
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[0].1[0].filename, "new.jpg");
        assert_eq!(lib.new_photos_count(3600).unwrap(), 1);

        // A very small age window drops it (age-based expiry).
        assert_eq!(lib.new_photos_count(-1).unwrap(), 0);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
