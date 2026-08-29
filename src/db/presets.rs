//! Saved, reusable color-levels presets (`level_presets`).
//!
//! A preset stores only per-channel black/white/gamma levels. Presets are used
//! for negative scans with known color casts and can be applied to a single
//! photo or a whole folder (see [`Library::apply_levels_to_folder`]).

use rusqlite::{params, Row};

use crate::model::{LevelPreset, Levels};

use super::{library::now, Library, Result};

/// The `level_presets` columns in a fixed order, shared by the reader below.
const PRESET_COLS: &str = "id, name, \
    lv_r_black, lv_r_white, lv_r_gamma_mille, \
    lv_g_black, lv_g_white, lv_g_gamma_mille, \
    lv_b_black, lv_b_white, lv_b_gamma_mille";

fn map_preset(r: &Row) -> rusqlite::Result<LevelPreset> {
    Ok(LevelPreset {
        id: r.get(0)?,
        name: r.get(1)?,
        levels: Levels {
            r_black: r.get(2)?,
            r_white: r.get(3)?,
            r_gamma_mille: r.get(4)?,
            g_black: r.get(5)?,
            g_white: r.get(6)?,
            g_gamma_mille: r.get(7)?,
            b_black: r.get(8)?,
            b_white: r.get(9)?,
            b_gamma_mille: r.get(10)?,
        },
    })
}

impl Library {
    /// All saved levels presets, ordered by name.
    pub fn level_presets(&self) -> Result<Vec<LevelPreset>> {
        let conn = self.lock();
        let sql = format!("SELECT {PRESET_COLS} FROM level_presets ORDER BY name COLLATE NOCASE");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], map_preset)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Save (create or overwrite by name) a levels preset. Returns its id.
    pub fn save_level_preset(&self, name: &str, levels: &Levels) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO level_presets(\
                name, lv_r_black, lv_r_white, lv_r_gamma_mille, \
                lv_g_black, lv_g_white, lv_g_gamma_mille, \
                lv_b_black, lv_b_white, lv_b_gamma_mille, created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) \
             ON CONFLICT(name) DO UPDATE SET \
                lv_r_black=?2, lv_r_white=?3, lv_r_gamma_mille=?4, \
                lv_g_black=?5, lv_g_white=?6, lv_g_gamma_mille=?7, \
                lv_b_black=?8, lv_b_white=?9, lv_b_gamma_mille=?10",
            params![
                name,
                levels.r_black,
                levels.r_white,
                levels.r_gamma_mille,
                levels.g_black,
                levels.g_white,
                levels.g_gamma_mille,
                levels.b_black,
                levels.b_white,
                levels.b_gamma_mille,
                now(),
            ],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM level_presets WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Delete a levels preset by id.
    pub fn delete_level_preset(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM level_presets WHERE id = ?1", params![id])?;
        Ok(())
    }
}
