//! Stylised face storage: `characters`, `style_faces`, and `style_face_scan`.
//!
//! This mirrors `faces.rs` for the anime/cartoon/furry face system. A style face
//! is one detected face box with a 768-value CCIP embedding. A character is a
//! named group. HDBSCAN groups similar faces before the user names them. Noise
//! faces have `cluster_id` -1. See `src/db/schema.sql` for the schema.

#![allow(dead_code)]

use rusqlite::{params, OptionalExtension, Row};

use crate::model::{Character, Photo, StyleFace};

use super::{library::map_photo, library::now, Library, Result};

/// The `photos` columns in `map_photo` order, for character photo queries.
const PHOTO_COLS: &str = "id, folder_id, path, filename, size, mod_time, taken_at, \
     width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan";

/// The `style_faces` columns in a fixed order.
const FACE_COLS: &str = "id, photo_id, character_id, cluster_id, \
     bbox_x, bbox_y, bbox_w, bbox_h, embedding, det_score, confirmed, source";

fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for f in v {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

fn blob_to_floats(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn map_style_face(r: &Row) -> rusqlite::Result<StyleFace> {
    let embedding: Option<Vec<u8>> = r.get(8)?;
    Ok(StyleFace {
        id: r.get(0)?,
        photo_id: r.get(1)?,
        character_id: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        cluster_id: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
        bbox_x: r.get(4)?,
        bbox_y: r.get(5)?,
        bbox_w: r.get(6)?,
        bbox_h: r.get(7)?,
        embedding: embedding.map(|b| blob_to_floats(&b)).unwrap_or_default(),
        det_score: r.get::<_, i64>(9)? as f32 / 1000.0,
        confirmed: r.get::<_, i64>(10)? != 0,
        source: r.get(11)?,
    })
}

fn map_character(r: &Row) -> rusqlite::Result<Character> {
    Ok(Character {
        id: r.get(0)?,
        name: r.get(1)?,
        cover_face_id: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
    })
}

impl Library {
    // --- Style faces ---

    /// Insert one detected stylised face. Returns its id.
    pub fn insert_style_face(&self, face: &StyleFace) -> Result<i64> {
        let conn = self.lock();
        let character = if face.character_id == 0 {
            None
        } else {
            Some(face.character_id)
        };
        let cluster = if face.cluster_id == 0 {
            None
        } else {
            Some(face.cluster_id)
        };
        conn.execute(
            "INSERT INTO style_faces(\
                photo_id, character_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, \
                embedding, embedding_dim, det_score, confirmed, source, created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                face.photo_id,
                character,
                cluster,
                face.bbox_x,
                face.bbox_y,
                face.bbox_w,
                face.bbox_h,
                floats_to_blob(&face.embedding),
                face.embedding.len() as i64,
                (face.det_score * 1000.0) as i64,
                face.confirmed as i64,
                face.source,
                now(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// All stylised faces detected in one photo.
    pub fn style_faces_for_photo(&self, photo_id: i64) -> Result<Vec<StyleFace>> {
        let conn = self.lock();
        let sql = format!("SELECT {FACE_COLS} FROM style_faces WHERE photo_id = ?1 ORDER BY id");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![photo_id], map_style_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// All stylised faces detected in any of the given photos (bulk form of
    /// `style_faces_for_photo`, for populating a grid's face-box overlay in
    /// one query instead of one per photo).
    pub fn style_faces_for_photos(&self, photo_ids: &[i64]) -> Result<Vec<StyleFace>> {
        if photo_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock();
        let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT {FACE_COLS} FROM style_faces WHERE photo_id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let ps: Vec<&dyn rusqlite::ToSql> = photo_ids.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(ps.as_slice(), map_style_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// One stylised face by id, or `None`.
    pub fn style_face_by_id(&self, face_id: i64) -> Result<Option<StyleFace>> {
        let conn = self.lock();
        let sql = format!("SELECT {FACE_COLS} FROM style_faces WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![face_id], map_style_face)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// All stylised faces that carry an embedding, for clustering. Returns
    /// (id, cluster, character, embedding) tuples.
    pub fn style_faces_for_clustering(&self) -> Result<Vec<(i64, i64, i64, Vec<f32>)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, cluster_id, character_id, embedding FROM style_faces \
             WHERE embedding IS NOT NULL AND embedding_dim > 0",
        )?;
        let rows = stmt.query_map([], |r| {
            let cluster = r.get::<_, Option<i64>>(1)?.unwrap_or(0);
            let character = r.get::<_, Option<i64>>(2)?.unwrap_or(0);
            let blob: Vec<u8> = r.get(3)?;
            Ok((r.get::<_, i64>(0)?, cluster, character, blob_to_floats(&blob)))
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Assign a stylised face to a character (0 clears it and unconfirms).
    pub fn set_style_face_character(&self, face_id: i64, character_id: i64) -> Result<()> {
        let conn = self.lock();
        if character_id == 0 {
            conn.execute(
                "UPDATE style_faces SET character_id = NULL, confirmed = 0 WHERE id = ?1",
                params![face_id],
            )?;
        } else {
            conn.execute(
                "UPDATE style_faces SET character_id = ?2, confirmed = 1 WHERE id = ?1",
                params![face_id, character_id],
            )?;
        }
        Ok(())
    }

    /// A map of style face id -> the character ids it was rejected from.
    pub fn style_face_rejection_map(&self) -> Result<std::collections::HashMap<i64, Vec<i64>>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT face_id, character_id FROM style_face_rejections")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut map: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for row in rows {
            let (fid, cid) = row?;
            map.entry(fid).or_default().push(cid);
        }
        Ok(map)
    }

    /// Remove a stylised face from a character and record the rejection.
    pub fn reject_style_face_from_character(&self, face_id: i64, character_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT OR IGNORE INTO style_face_rejections(face_id, character_id) VALUES(?1, ?2)",
            params![face_id, character_id],
        )?;
        conn.execute(
            "UPDATE style_faces SET character_id = NULL, confirmed = 0, cluster_id = NULL WHERE id = ?1",
            params![face_id],
        )?;
        Ok(())
    }

    /// Set the cluster id of many stylised faces in one transaction. This holds
    /// the DB lock once, not once per face. A per-face write blocks the UI
    /// thread for a long time during a large scan.
    pub fn set_style_face_clusters(&self, pairs: &[(i64, i64)]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE style_faces SET cluster_id = ?2 WHERE id = ?1")?;
            for &(face_id, cluster_id) in pairs {
                let cluster = if cluster_id == 0 { None } else { Some(cluster_id) };
                stmt.execute(params![face_id, cluster])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Stylised faces in one cluster that have no character, for the naming UI.
    pub fn unassigned_style_faces_in_cluster(&self, cluster_id: i64) -> Result<Vec<StyleFace>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {FACE_COLS} FROM style_faces \
             WHERE cluster_id = ?1 AND character_id IS NULL ORDER BY det_score DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![cluster_id], map_style_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// The representative face id for an unnamed cluster: the highest-scoring
    /// unassigned face, or 0 when the cluster has none. This is a cheap query.
    /// It reads no embedding blob, unlike `unassigned_style_faces_in_cluster`.
    pub fn cluster_representative_face(&self, cluster_id: i64) -> Result<i64> {
        let conn = self.lock();
        let fid: Option<i64> = conn
            .query_row(
                "SELECT id FROM style_faces \
                 WHERE cluster_id = ?1 AND character_id IS NULL \
                 ORDER BY det_score DESC LIMIT 1",
                params![cluster_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(fid.unwrap_or(0))
    }

    /// The unnamed style clusters with their face counts. The order is stable:
    /// by cluster id, with the noise cluster (-1) last. A stable order stops the
    /// Characters grid from re-ordering while a scan adds faces.
    pub fn unnamed_style_clusters(&self) -> Result<Vec<(i64, i64)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT cluster_id, COUNT(*) AS n FROM style_faces \
             WHERE character_id IS NULL AND cluster_id IS NOT NULL \
             GROUP BY cluster_id ORDER BY (cluster_id = -1), cluster_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    // --- Characters ---

    /// Create a character. Returns its id.
    pub fn create_character(&self, name: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO characters(name, created_at) VALUES(?1, ?2)",
            params![name, now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Rename a character.
    pub fn rename_character(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE characters SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Set the cover face for a character (0 clears it).
    pub fn set_character_cover(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        let cover = if face_id == 0 { None } else { Some(face_id) };
        conn.execute(
            "UPDATE characters SET cover_face_id = ?2 WHERE id = ?1",
            params![id, cover],
        )?;
        Ok(())
    }

    /// Give a character a default cover face, but only if they don't already
    /// have one. Used when a cluster of stylised faces is folded into a
    /// character (a brand-new character still needs an initial cover; an
    /// existing one must keep whatever the user already chose).
    pub fn set_character_cover_if_unset(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE characters SET cover_face_id = ?2 WHERE id = ?1 AND cover_face_id IS NULL",
            params![id, face_id],
        )?;
        Ok(())
    }

    /// Delete a character. Their faces keep their rows but lose the link.
    pub fn delete_character(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete a character and ban every one of its faces. Each face records a
    /// rejection against this character, so a later re-scan and re-cluster never
    /// re-groups these faces under a character again. Photos on disk are not
    /// affected.
    pub fn delete_character_and_ban(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        let face_ids: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT id FROM style_faces WHERE character_id = ?1")?;
            let rows = stmt.query_map(params![id], |r| r.get::<_, i64>(0))?;
            let mut v = Vec::new();
            for row in rows {
                v.push(row?);
            }
            v
        };
        for fid in &face_ids {
            conn.execute(
                "INSERT OR IGNORE INTO style_face_rejections(face_id, character_id) VALUES(?1, ?2)",
                params![fid, id],
            )?;
            conn.execute(
                "UPDATE style_faces SET character_id = NULL, confirmed = 0, cluster_id = NULL WHERE id = ?1",
                params![fid],
            )?;
        }
        conn.execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Merge `from` character into `into`. All faces move, then `from` is gone.
    pub fn merge_characters(&self, from: i64, into: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE style_faces SET character_id = ?2 WHERE character_id = ?1",
            params![from, into],
        )?;
        conn.execute("DELETE FROM characters WHERE id = ?1", params![from])?;
        Ok(())
    }

    /// All characters with their face counts, ordered by name.
    pub fn characters(&self) -> Result<Vec<(Character, i64)>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.name, c.cover_face_id, \
                (SELECT COUNT(*) FROM style_faces f WHERE f.character_id = c.id) AS n \
             FROM characters c ORDER BY c.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok((map_character(r)?, r.get::<_, i64>(3)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// The number of stylised faces assigned to a character.
    pub fn character_face_count(&self, id: i64) -> Result<i64> {
        let conn = self.lock();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM style_faces WHERE character_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// The total number of detected stylised faces in the library.
    pub fn total_style_face_count(&self) -> Result<i64> {
        let conn = self.read_lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM style_faces", [], |r| r.get(0))?;
        Ok(n)
    }

    /// A representative face id for a character: the cover face if set, else the
    /// highest-scoring assigned face. Returns 0 when the character has no face.
    pub fn character_representative_face(&self, id: i64) -> Result<i64> {
        let conn = self.lock();
        let cover: Option<i64> = conn
            .query_row(
                "SELECT cover_face_id FROM characters WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        if let Some(fid) = cover {
            return Ok(fid);
        }
        let fid: Option<i64> = conn
            .query_row(
                "SELECT id FROM style_faces WHERE character_id = ?1 ORDER BY det_score DESC LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(fid.unwrap_or(0))
    }

    /// All photos that contain a face of the given character, newest first.
    pub fn photos_of_character(&self, character_id: i64) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos WHERE id IN \
                (SELECT DISTINCT photo_id FROM style_faces WHERE character_id = ?1) \
             ORDER BY taken_at DESC, filename"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![character_id], map_photo)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// All photos that contain an unassigned face in the given style cluster,
    /// newest first.
    pub fn photos_in_style_cluster(&self, cluster_id: i64) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos WHERE id IN \
                (SELECT DISTINCT photo_id FROM style_faces WHERE cluster_id = ?1 AND character_id IS NULL) \
             ORDER BY taken_at DESC, filename"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![cluster_id], map_photo)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    // --- Style face scan state ---

    /// Photo ids that still need a stylised-face-detection pass, capped by
    /// `limit`. A photo needs a pass when it has no `style_face_scan` row, or its
    /// row is not done. Only enriched, present photos are eligible.
    pub fn photos_needing_style_face_scan(&self, limit: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT p.id FROM photos p \
             LEFT JOIN style_face_scan fs ON fs.photo_id = p.id \
             WHERE p.missing = 0 AND p.scan_state = 2 AND p.skip_face_scan = 0 \
               AND (fs.state IS NULL OR fs.state <> 2) \
             ORDER BY p.added_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| r.get::<_, i64>(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Photo ids in the given folders that still need a stylised-face pass.
    /// Scoped variant of `photos_needing_style_face_scan`. Empty set -> empty.
    pub fn photos_needing_style_face_scan_in(
        &self,
        folder_ids: &[i64],
        limit: i64,
    ) -> Result<Vec<i64>> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock();
        let placeholders = folder_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT p.id FROM photos p \
             LEFT JOIN style_face_scan fs ON fs.photo_id = p.id \
             WHERE p.missing = 0 AND p.scan_state = 2 AND p.skip_face_scan = 0 \
               AND (fs.state IS NULL OR fs.state <> 2) \
               AND p.folder_id IN ({placeholders}) \
             ORDER BY p.added_at DESC LIMIT ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut ps: Vec<&dyn rusqlite::ToSql> = folder_ids
            .iter()
            .map(|f| f as &dyn rusqlite::ToSql)
            .collect();
        ps.push(&limit);
        let rows = stmt.query_map(ps.as_slice(), |r| r.get::<_, i64>(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Clear stylised-face-scan state and faces for photos in the given folders,
    /// so a rescan re-processes them. Empty folder set does nothing.
    pub fn clear_style_face_scan_in(&self, folder_ids: &[i64]) -> Result<()> {
        if folder_ids.is_empty() {
            return Ok(());
        }
        let conn = self.lock();
        let placeholders = folder_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let ps: Vec<&dyn rusqlite::ToSql> = folder_ids
            .iter()
            .map(|f| f as &dyn rusqlite::ToSql)
            .collect();
        conn.execute(
            &format!(
                "DELETE FROM style_faces WHERE photo_id IN \
                 (SELECT id FROM photos WHERE folder_id IN ({placeholders}))"
            ),
            ps.as_slice(),
        )?;
        conn.execute(
            &format!(
                "DELETE FROM style_face_scan WHERE photo_id IN \
                 (SELECT id FROM photos WHERE folder_id IN ({placeholders}))"
            ),
            ps.as_slice(),
        )?;
        conn.execute(
            &format!("UPDATE photos SET style_face_status = 0 WHERE folder_id IN ({placeholders})"),
            ps.as_slice(),
        )?;
        Ok(())
    }

    /// Set the stylised-face-scan state of a photo. Mirrors into
    /// `photos.style_face_status`.
    pub fn set_style_face_scan_state(&self, photo_id: i64, state: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO style_face_scan(photo_id, state, scanned_at) VALUES(?1, ?2, ?3) \
             ON CONFLICT(photo_id) DO UPDATE SET state = ?2, scanned_at = ?3",
            params![photo_id, state, now()],
        )?;
        conn.execute(
            "UPDATE photos SET style_face_status = ?2 WHERE id = ?1",
            params![photo_id, state],
        )?;
        Ok(())
    }

    /// Delete all detected stylised faces for a photo before a re-scan.
    pub fn clear_style_faces_for_photo(&self, photo_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM style_faces WHERE photo_id = ?1",
            params![photo_id],
        )?;
        Ok(())
    }

    /// Delete every stylised face, character, and scan record. The reset.
    pub fn delete_all_style_face_data(&self) -> Result<()> {
        let conn = self.lock();
        conn.execute_batch(
            "DELETE FROM style_faces; DELETE FROM characters; DELETE FROM style_face_scan; \
             UPDATE photos SET style_face_status = 0;",
        )?;
        Ok(())
    }

    // --- Group and photo management ---

    /// Remove one photo from a character. Every face of that photo loses the
    /// character link and keeps its cluster id. A later re-cluster may group it
    /// again. Use `ban_photo_from_character` to make the removal permanent.
    pub fn remove_photo_from_character(&self, photo_id: i64, character_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE style_faces SET character_id = NULL, confirmed = 0 \
             WHERE photo_id = ?1 AND character_id = ?2",
            params![photo_id, character_id],
        )?;
        Ok(())
    }

    /// Ban one photo from a character. Every face of that photo loses the
    /// character link, loses its cluster id, and records a rejection. A
    /// re-cluster never groups these faces under this character again.
    pub fn ban_photo_from_character(&self, photo_id: i64, character_id: i64) -> Result<()> {
        let conn = self.lock();
        let face_ids: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM style_faces WHERE photo_id = ?1 AND character_id = ?2")?;
            let rows = stmt.query_map(params![photo_id, character_id], |r| r.get::<_, i64>(0))?;
            let mut v = Vec::new();
            for row in rows {
                v.push(row?);
            }
            v
        };
        for fid in &face_ids {
            conn.execute(
                "INSERT OR IGNORE INTO style_face_rejections(face_id, character_id) VALUES(?1, ?2)",
                params![fid, character_id],
            )?;
            conn.execute(
                "UPDATE style_faces SET character_id = NULL, confirmed = 0, cluster_id = NULL WHERE id = ?1",
                params![fid],
            )?;
        }
        Ok(())
    }

    /// Remove one photo from an unnamed style cluster. Every face of that photo
    /// in the cluster loses its cluster id, so the group no longer shows it.
    pub fn remove_photo_from_style_cluster(&self, photo_id: i64, cluster_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE style_faces SET cluster_id = NULL \
             WHERE photo_id = ?1 AND cluster_id = ?2",
            params![photo_id, cluster_id],
        )?;
        Ok(())
    }

    /// Clear the name from a character. Every face keeps its cluster id and
    /// loses the character link. The character row is deleted. The old cluster
    /// reappears as an unnamed group. No re-scan is needed.
    pub fn unname_character(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE style_faces SET character_id = NULL, confirmed = 0 WHERE character_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Mark photos as unimportant. A skipped photo is excluded from every future
    /// face scan (human and stylised). Setting skip on also deletes the photos'
    /// human and stylised face rows, so the photos leave every face group at
    /// once.
    pub fn set_photos_skip_face_scan(&self, photo_ids: &[i64], skip: bool) -> Result<()> {
        if photo_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut set = tx.prepare("UPDATE photos SET skip_face_scan = ?2 WHERE id = ?1")?;
            let mut del_style = tx.prepare("DELETE FROM style_faces WHERE photo_id = ?1")?;
            let mut del_face = tx.prepare("DELETE FROM faces WHERE photo_id = ?1")?;
            for &pid in photo_ids {
                set.execute(params![pid, skip as i64])?;
                if skip {
                    del_style.execute(params![pid])?;
                    del_face.execute(params![pid])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Photo ids that have a face of the given character.
    pub fn photo_ids_of_character(&self, character_id: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT DISTINCT photo_id FROM style_faces WHERE character_id = ?1")?;
        let rows = stmt.query_map(params![character_id], |r| r.get::<_, i64>(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Photo ids that have an unassigned face in the given style cluster.
    pub fn photo_ids_in_style_cluster(&self, cluster_id: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT photo_id FROM style_faces WHERE cluster_id = ?1 AND character_id IS NULL",
        )?;
        let rows = stmt.query_map(params![cluster_id], |r| r.get::<_, i64>(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Folder;

    fn temp_lib() -> Library {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-style-faces-{}-{:?}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        Library::open_at(&p).unwrap()
    }

    fn add_photo(lib: &Library, name: &str) -> i64 {
        let fid = lib
            .upsert_folder(&Folder {
                path: format!("/tmp/pichouse-style-faces/{name}dir"),
                name: "d".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        lib.upsert_photo_structure(&Photo {
            folder_id: fid,
            path: format!("/tmp/pichouse-style-faces/{name}.jpg"),
            filename: format!("{name}.jpg"),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn photos_in_style_cluster_excludes_already_named_faces() {
        // Two style faces share one cluster id: one gets named as a character,
        // one stays pending. Opening the cluster (e.g. from an "Unnamed" tile)
        // must show only the still-unassigned face's photo, not the named one's.
        let lib = temp_lib();
        let p_named = add_photo(&lib, "g");
        let p_pending = add_photo(&lib, "h");
        let f_named = lib
            .insert_style_face(&StyleFace {
                photo_id: p_named,
                cluster_id: 42,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        lib.insert_style_face(&StyleFace {
            photo_id: p_pending,
            cluster_id: 42,
            embedding: vec![0.99],
            ..Default::default()
        })
        .unwrap();
        let alice = lib.create_character("Alice").unwrap();
        lib.set_style_face_character(f_named, alice).unwrap();

        let photos = lib.photos_in_style_cluster(42).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].id, p_pending);

        let unassigned = lib.unassigned_style_faces_in_cluster(42).unwrap();
        assert_eq!(unassigned.len(), 1);
        assert_eq!(unassigned[0].photo_id, p_pending);

        let ids = lib.photo_ids_in_style_cluster(42).unwrap();
        assert_eq!(ids, vec![p_pending]);
    }

    #[test]
    fn cover_if_unset_fills_gap_but_not_an_existing_choice() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "i");
        let f1 = lib
            .insert_style_face(&StyleFace {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let f2 = lib
            .insert_style_face(&StyleFace {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let a = lib.create_character("A").unwrap();

        // No cover yet: the fallback fills it in.
        lib.set_character_cover_if_unset(a, f1).unwrap();
        assert_eq!(lib.character_representative_face(a).unwrap(), f1);

        // Already covered: a later merge's fallback must not replace it.
        lib.set_character_cover_if_unset(a, f2).unwrap();
        assert_eq!(lib.character_representative_face(a).unwrap(), f1);
    }
}
