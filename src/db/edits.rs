//! Non-destructive per-photo edits (`photo_edits`).
//!
//! Edits are stored here and applied at view time and when generating
//! thumbnails; the original file on disk is never changed. A photo with no row
//! is unedited (identity). Every write bumps `edit_rev`, which forms part of the
//! thumbnail cache key so an edited thumbnail never collides with the original.

use rusqlite::{params, OptionalExtension, Row};

use crate::model::{Levels, PhotoEdit};

use super::{Library, Result};

/// The `photo_edits` columns in a fixed order, shared by the readers below.
const EDIT_COLS: &str = "photo_id, flip_h, flip_v, straighten_mdeg, \
    crop_x, crop_y, crop_w, crop_h, brightness, contrast, \
    lv_r_black, lv_r_white, lv_r_gamma_mille, \
    lv_g_black, lv_g_white, lv_g_gamma_mille, \
    lv_b_black, lv_b_white, lv_b_gamma_mille, edit_rev";

/// Map a `photo_edits` row (in `EDIT_COLS` order) to a [`PhotoEdit`].
fn map_edit(r: &Row) -> rusqlite::Result<PhotoEdit> {
    Ok(PhotoEdit {
        photo_id: r.get(0)?,
        flip_h: r.get::<_, i64>(1)? != 0,
        flip_v: r.get::<_, i64>(2)? != 0,
        straighten_mdeg: r.get(3)?,
        crop_x: r.get(4)?,
        crop_y: r.get(5)?,
        crop_w: r.get(6)?,
        crop_h: r.get(7)?,
        brightness: r.get(8)?,
        contrast: r.get(9)?,
        levels: Levels {
            r_black: r.get(10)?,
            r_white: r.get(11)?,
            r_gamma_mille: r.get(12)?,
            g_black: r.get(13)?,
            g_white: r.get(14)?,
            g_gamma_mille: r.get(15)?,
            b_black: r.get(16)?,
            b_white: r.get(17)?,
            b_gamma_mille: r.get(18)?,
        },
        edit_rev: r.get(19)?,
    })
}

impl Library {
    /// The edit record for a photo. Returns the identity edit (with the given
    /// `photo_id`) when the photo has no `photo_edits` row.
    pub fn photo_edit(&self, photo_id: i64) -> Result<PhotoEdit> {
        let conn = self.lock();
        let sql = format!("SELECT {EDIT_COLS} FROM photo_edits WHERE photo_id = ?1");
        let edit = conn
            .query_row(&sql, params![photo_id], map_edit)
            .optional()?;
        Ok(edit.unwrap_or(PhotoEdit {
            photo_id,
            ..Default::default()
        }))
    }

    /// Edit records for a set of photos, keyed by `photo_id` (bulk form of
    /// `photo_edit`, for warming a grid's texture cache in one query instead
    /// of one per photo). A photo absent from the map is unedited, same
    /// convention as `photo_edit`. Uses `read_lock()` so this never contends
    /// with a background scan/enrich holding the writer lock (see
    /// `Library::lock`'s doc comment on multi-second UI freezes).
    pub fn photo_edits_for_photos(
        &self,
        photo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, PhotoEdit>> {
        let mut out = std::collections::HashMap::new();
        if photo_ids.is_empty() {
            return Ok(out);
        }
        let conn = self.read_lock();
        let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT {EDIT_COLS} FROM photo_edits WHERE photo_id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let ps: Vec<&dyn rusqlite::ToSql> = photo_ids.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(ps.as_slice(), map_edit)?;
        for row in rows {
            let edit = row?;
            out.insert(edit.photo_id, edit);
        }
        Ok(out)
    }

    /// Insert or replace the whole edit record for a photo, bumping `edit_rev`.
    /// If the edit is the identity (no visible change), the row is removed
    /// instead so unedited photos keep no row.
    pub fn set_photo_edit(&self, edit: &PhotoEdit) -> Result<i64> {
        if edit.is_identity() {
            self.clear_photo_edit(edit.photo_id)?;
            return Ok(0);
        }
        let conn = self.lock();
        let next_rev: i64 = conn
            .query_row(
                "SELECT edit_rev FROM photo_edits WHERE photo_id = ?1",
                params![edit.photo_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            + 1;
        let l = &edit.levels;
        conn.execute(
            "INSERT INTO photo_edits(\
                photo_id, flip_h, flip_v, straighten_mdeg, \
                crop_x, crop_y, crop_w, crop_h, brightness, contrast, \
                lv_r_black, lv_r_white, lv_r_gamma_mille, \
                lv_g_black, lv_g_white, lv_g_gamma_mille, \
                lv_b_black, lv_b_white, lv_b_gamma_mille, edit_rev) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20) \
             ON CONFLICT(photo_id) DO UPDATE SET \
                flip_h=?2, flip_v=?3, straighten_mdeg=?4, \
                crop_x=?5, crop_y=?6, crop_w=?7, crop_h=?8, \
                brightness=?9, contrast=?10, \
                lv_r_black=?11, lv_r_white=?12, lv_r_gamma_mille=?13, \
                lv_g_black=?14, lv_g_white=?15, lv_g_gamma_mille=?16, \
                lv_b_black=?17, lv_b_white=?18, lv_b_gamma_mille=?19, edit_rev=?20",
            params![
                edit.photo_id,
                edit.flip_h as i64,
                edit.flip_v as i64,
                edit.straighten_mdeg,
                edit.crop_x,
                edit.crop_y,
                edit.crop_w,
                edit.crop_h,
                edit.brightness,
                edit.contrast,
                l.r_black,
                l.r_white,
                l.r_gamma_mille,
                l.g_black,
                l.g_white,
                l.g_gamma_mille,
                l.b_black,
                l.b_white,
                l.b_gamma_mille,
                next_rev,
            ],
        )?;
        Ok(next_rev)
    }

    /// Discard all edits for a photo (revert to the original view).
    pub fn clear_photo_edit(&self, photo_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM photo_edits WHERE photo_id = ?1",
            params![photo_id],
        )?;
        Ok(())
    }

    /// Merge a levels preset into every photo in a folder, changing only the
    /// levels part of each photo's edit and preserving crop/rotate/flip/etc.
    /// Bumps `edit_rev` for each touched photo. Returns the list of `(photo_id,
    /// hash)` pairs so the caller can invalidate the affected thumbnails.
    pub fn apply_levels_to_folder(
        &self,
        folder_id: i64,
        levels: &Levels,
    ) -> Result<Vec<(i64, String)>> {
        // Collect the photos (id + hash) up front to return them for thumb
        // invalidation.
        let photos: Vec<(i64, String)> = {
            let conn = self.lock();
            let mut stmt =
                conn.prepare("SELECT id, hash FROM photos WHERE folder_id = ?1")?;
            let rows = stmt.query_map(params![folder_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            let mut v = Vec::new();
            for row in rows {
                v.push(row?);
            }
            v
        };
        for (pid, _) in &photos {
            let mut edit = self.photo_edit(*pid)?;
            edit.levels = *levels;
            self.set_photo_edit(&edit)?;
        }
        Ok(photos)
    }
}
