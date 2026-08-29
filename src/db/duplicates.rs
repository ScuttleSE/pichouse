//! Database access for the duplicate image finder.
//!
//! The finder works over a scope of folders (derived from albums by the UI). It
//! reads the in-scope photos, backfills any missing perceptual hash, and hard
//! deletes a chosen "worse" copy. Deletion removes the file from disk and the
//! row from `photos`. The schema's `ON DELETE CASCADE` foreign keys clean up
//! tags, edits, faces, and album membership.

use rusqlite::params;

use super::library::map_photo;
use super::Result;
use crate::db::Library;
use crate::model::Photo;

const PHOTO_COLS: &str = "id, folder_id, path, filename, size, mod_time, taken_at, \
     width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan";

impl Library {
    /// Load the non-missing photos in a set of folders, ready for the duplicate
    /// scan. An empty `folder_ids` returns an empty list.
    pub fn photos_in_folders(&self, folder_ids: &[i64]) -> Result<Vec<Photo>> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = folder_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos \
             WHERE missing = 0 AND folder_id IN ({placeholders}) \
             ORDER BY id ASC"
        );
        let conn = self.lock();
        let mut stmt = conn.prepare(&sql)?;
        let ids: Vec<&dyn rusqlite::ToSql> =
            folder_ids.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(ids.as_slice(), map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Ban a pair of photos from ever grouping as duplicates again. Stores the
    /// pair normalised (low id first) so the ban is order independent. Idempotent.
    pub fn ban_dup_pair(&self, a: i64, b: i64) -> Result<()> {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let conn = self.lock();
        conn.execute(
            "INSERT INTO dup_bans(photo_a, photo_b, banned_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(photo_a, photo_b) DO NOTHING",
            params![lo, hi, super::library::now()],
        )?;
        Ok(())
    }

    /// Remove a duplicate-pair ban, so the pair can match again.
    pub fn unban_dup_pair(&self, a: i64, b: i64) -> Result<()> {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let conn = self.lock();
        conn.execute(
            "DELETE FROM dup_bans WHERE photo_a = ?1 AND photo_b = ?2",
            params![lo, hi],
        )?;
        Ok(())
    }

    /// All banned duplicate pairs as a set of normalised `(low, high)` ids, for
    /// the duplicate engine to skip.
    pub fn banned_dup_pairs(&self) -> Result<std::collections::HashSet<(i64, i64)>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare("SELECT photo_a, photo_b FROM dup_bans")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            out.insert(row?);
        }
        Ok(out)
    }

    /// Every banned pair with both photos, newest ban first, for the "Banned
    /// Matches" review view. Skips a ban whose photos are gone.
    pub fn banned_dup_photo_pairs(&self) -> Result<Vec<(Photo, Photo)>> {
        let conn = self.read_lock();
        let sql = format!(
            "SELECT {} FROM photos WHERE id IN (SELECT photo_a FROM dup_bans UNION SELECT photo_b FROM dup_bans)",
            PHOTO_COLS
        );
        let mut by_id: std::collections::HashMap<i64, Photo> = std::collections::HashMap::new();
        {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], map_photo)?;
            for p in rows {
                let p = p?;
                by_id.insert(p.id, p);
            }
        }
        let mut pairs = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT photo_a, photo_b FROM dup_bans ORDER BY banned_at DESC, photo_a ASC",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (a, b) = row?;
                if let (Some(pa), Some(pb)) = (by_id.get(&a), by_id.get(&b)) {
                    pairs.push((pa.clone(), pb.clone()));
                }
            }
        }
        Ok(pairs)
    }

    /// The number of banned duplicate pairs (for the sidebar count).
    pub fn banned_dup_count(&self) -> Result<i64> {
        let conn = self.read_lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM dup_bans", [], |r| r.get(0))?;
        Ok(n)
    }

    /// Remove every duplicate-pair ban.
    pub fn clear_all_dup_bans(&self) -> Result<usize> {
        let conn = self.lock();
        Ok(conn.execute("DELETE FROM dup_bans", [])?)
    }

    /// Store a computed perceptual hash for a photo (backfill path).
    pub fn set_photo_phash(&self, id: i64, phash: u64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET phash = ?1 WHERE id = ?2",
            params![phash as i64, id],
        )?;
        Ok(())
    }

    /// Hard delete a photo: remove its row (cascading to tags/edits/faces/album
    /// membership) and then remove the file from disk. The row is removed first
    /// so a failed file delete still leaves the library consistent. A
    /// `NotFound` file error is ignored, because a gone file is the goal.
    pub fn delete_photo_hard(&self, id: i64, path: &str) -> Result<()> {
        {
            let conn = self.lock();
            conn.execute("DELETE FROM photos WHERE id = ?1", params![id])?;
        }
        self.invalidate_count_cache();
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
