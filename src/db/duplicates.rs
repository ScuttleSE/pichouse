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
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
