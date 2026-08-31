//! Facial recognition storage: `persons`, `faces`, and `face_scan`.
//!
//! A face is one detected face box in one photo, with an embedding vector. A
//! person is a named group of faces. Clustering groups similar faces before the
//! user names them. See `src/db/schema.sql` for the coordinate convention.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension, Row};

use crate::model::{Face, Person, Photo};

use super::{library::map_photo, library::now, Library, Result};

/// A named person or an unnamed cluster, the two kinds of group the People
/// view shows. Used to key a group across a face-scan snapshot, since person
/// ids and cluster ids are separate id spaces that can otherwise collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceGroup {
    Person(i64),
    Cluster(i64),
}

/// The `photos` columns in `map_photo` order, for person photo queries.
const PHOTO_COLS: &str = "id, folder_id, path, filename, size, mod_time, taken_at, \
     width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan";

/// The `faces` columns in a fixed order, shared by the reader below.
const FACE_COLS: &str = "id, photo_id, person_id, cluster_id, \
     bbox_x, bbox_y, bbox_w, bbox_h, landmarks, embedding, det_score, \
     confirmed, source";

/// Pack an f32 slice into a little-endian byte blob.
fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for f in v {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// Unpack a little-endian byte blob into an f32 vector.
fn blob_to_floats(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn map_face(r: &Row) -> rusqlite::Result<Face> {
    let landmarks: Option<Vec<u8>> = r.get(8)?;
    let embedding: Option<Vec<u8>> = r.get(9)?;
    Ok(Face {
        id: r.get(0)?,
        photo_id: r.get(1)?,
        person_id: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        cluster_id: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
        bbox_x: r.get(4)?,
        bbox_y: r.get(5)?,
        bbox_w: r.get(6)?,
        bbox_h: r.get(7)?,
        landmarks: landmarks.map(|b| blob_to_floats(&b)).unwrap_or_default(),
        embedding: embedding.map(|b| blob_to_floats(&b)).unwrap_or_default(),
        // det_score is stored 0..1000; expose 0.0..1.0.
        det_score: r.get::<_, i64>(10)? as f32 / 1000.0,
        confirmed: r.get::<_, i64>(11)? != 0,
        source: r.get(12)?,
    })
}

fn map_person(r: &Row) -> rusqlite::Result<Person> {
    Ok(Person {
        id: r.get(0)?,
        name: r.get(1)?,
        cover_face_id: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
    })
}

impl Library {
    // --- Faces ---

    /// Insert one detected face. Returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_face(&self, face: &Face) -> Result<i64> {
        let conn = self.lock();
        let person = if face.person_id == 0 {
            None
        } else {
            Some(face.person_id)
        };
        let cluster = if face.cluster_id == 0 {
            None
        } else {
            Some(face.cluster_id)
        };
        conn.execute(
            "INSERT INTO faces(\
                photo_id, person_id, cluster_id, bbox_x, bbox_y, bbox_w, bbox_h, \
                landmarks, embedding, embedding_dim, det_score, confirmed, source, created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                face.photo_id,
                person,
                cluster,
                face.bbox_x,
                face.bbox_y,
                face.bbox_w,
                face.bbox_h,
                floats_to_blob(&face.landmarks),
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

    /// All faces detected in one photo.
    pub fn faces_for_photo(&self, photo_id: i64) -> Result<Vec<Face>> {        let conn = self.lock();
        let sql = format!("SELECT {FACE_COLS} FROM faces WHERE photo_id = ?1 ORDER BY id");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![photo_id], map_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// All faces detected in any of the given photos (bulk form of
    /// `faces_for_photo`, for populating a grid's face-box overlay in one
    /// query instead of one per photo).
    pub fn faces_for_photos(&self, photo_ids: &[i64]) -> Result<Vec<Face>> {
        if photo_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock();
        let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT {FACE_COLS} FROM faces WHERE photo_id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let ps: Vec<&dyn rusqlite::ToSql> = photo_ids.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(ps.as_slice(), map_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// One face by id, or `None`.
    pub fn face_by_id(&self, face_id: i64) -> Result<Option<Face>> {
        let conn = self.lock();
        let sql = format!("SELECT {FACE_COLS} FROM faces WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query_map(params![face_id], map_face)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// All faces that carry an embedding, for clustering. Returns (id, cluster,
    /// person, embedding) tuples to keep the payload small.
    pub fn faces_for_clustering(&self) -> Result<Vec<(i64, i64, i64, Vec<f32>)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, cluster_id, person_id, embedding FROM faces \
             WHERE embedding IS NOT NULL AND embedding_dim > 0",
        )?;
        let rows = stmt.query_map([], |r| {
            let cluster = r.get::<_, Option<i64>>(1)?.unwrap_or(0);
            let person = r.get::<_, Option<i64>>(2)?.unwrap_or(0);
            let blob: Vec<u8> = r.get(3)?;
            Ok((r.get::<_, i64>(0)?, cluster, person, blob_to_floats(&blob)))
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// Assign a face to a person (or clear it with `person_id` 0). Setting a
    /// person also marks the face confirmed.
    pub fn set_face_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        let conn = self.lock();
        if person_id == 0 {
            conn.execute(
                "UPDATE faces SET person_id = NULL, confirmed = 0 WHERE id = ?1",
                params![face_id],
            )?;
        } else {
            conn.execute(
                "UPDATE faces SET person_id = ?2, confirmed = 1 WHERE id = ?1",
                params![face_id, person_id],
            )?;
        }
        Ok(())
    }

    /// A map of face id -> the person ids it was rejected from. Used by
    /// clustering so a rejected face never rejoins that person.
    pub fn face_rejection_map(&self) -> Result<std::collections::HashMap<i64, Vec<i64>>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT face_id, person_id FROM face_rejections")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut map: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
        for row in rows {
            let (fid, pid) = row?;
            map.entry(fid).or_default().push(pid);
        }
        Ok(map)
    }

    /// Remove a face from a person and record the rejection, so a later re-scan
    /// never re-attaches this face to that person. The face becomes available
    /// for another group. Its cluster is cleared so the next clustering pass
    /// re-places it.
    pub fn reject_face_from_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT OR IGNORE INTO face_rejections(face_id, person_id) VALUES(?1, ?2)",
            params![face_id, person_id],
        )?;
        conn.execute(
            "UPDATE faces SET person_id = NULL, confirmed = 0, cluster_id = NULL WHERE id = ?1",
            params![face_id],
        )?;
        Ok(())
    }

    /// Set the cluster id of a face (0 clears it).
    /// Set the cluster id of many faces in one transaction. This holds the DB
    /// lock once, not once per face. A per-face write blocks the UI thread for a
    /// long time during a large scan.
    pub fn set_face_clusters(&self, pairs: &[(i64, i64)]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE faces SET cluster_id = ?2 WHERE id = ?1")?;
            for &(face_id, cluster_id) in pairs {
                let cluster = if cluster_id == 0 { None } else { Some(cluster_id) };
                stmt.execute(params![face_id, cluster])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Faces in one cluster that have no assigned person, for the naming UI.
    pub fn unassigned_faces_in_cluster(&self, cluster_id: i64) -> Result<Vec<Face>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {FACE_COLS} FROM faces \
             WHERE cluster_id = ?1 AND person_id IS NULL ORDER BY det_score DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![cluster_id], map_face)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// The next largest unnamed cluster ids with their face counts, for the
    /// "review unnamed people" flow. Ordered by count, largest first.
    pub fn unnamed_clusters(&self) -> Result<Vec<(i64, i64)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT cluster_id, COUNT(*) AS n FROM faces \
             WHERE person_id IS NULL AND cluster_id IS NOT NULL \
             GROUP BY cluster_id ORDER BY n DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// The photo ids currently in each named or unnamed face group, for
    /// diffing a before/after face-scan snapshot to find how many new photos
    /// a scan added to an existing group.
    pub fn group_photo_ids(&self) -> Result<HashMap<FaceGroup, HashSet<i64>>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT person_id, cluster_id, photo_id FROM faces \
             WHERE person_id IS NOT NULL OR cluster_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut map: HashMap<FaceGroup, HashSet<i64>> = HashMap::new();
        for row in rows {
            let (person_id, cluster_id, photo_id) = row?;
            let key = match (person_id, cluster_id) {
                (Some(pid), _) => FaceGroup::Person(pid),
                (None, Some(cid)) => FaceGroup::Cluster(cid),
                (None, None) => continue,
            };
            map.entry(key).or_default().insert(photo_id);
        }
        Ok(map)
    }

    // --- Persons ---

    /// Create a person. Returns its id.
    pub fn create_person(&self, name: &str) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO persons(name, created_at) VALUES(?1, ?2)",
            params![name, now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Rename a person.
    pub fn rename_person(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE persons SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Set the cover face for a person (0 clears it).
    pub fn set_person_cover(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        let cover = if face_id == 0 { None } else { Some(face_id) };
        conn.execute(
            "UPDATE persons SET cover_face_id = ?2 WHERE id = ?1",
            params![id, cover],
        )?;
        Ok(())
    }

    /// Give a person a default cover face, but only if they don't already
    /// have one. Used when a cluster of faces is folded into a person (a
    /// brand-new person still needs an initial cover; an existing one must
    /// keep whatever the user already chose).
    pub fn set_person_cover_if_unset(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE persons SET cover_face_id = ?2 WHERE id = ?1 AND cover_face_id IS NULL",
            params![id, face_id],
        )?;
        Ok(())
    }

    /// Delete a person. Their faces keep their rows but lose the person link
    /// (the schema sets `person_id` to NULL on delete).
    pub fn delete_person(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM persons WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete a person and ban every one of its faces. Each face records a
    /// rejection against this person, so a later re-scan and re-cluster never
    /// re-groups these faces under a person again. Photos on disk are not
    /// affected.
    pub fn delete_person_and_ban(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        // Collect the person's face ids first.
        let face_ids: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT id FROM faces WHERE person_id = ?1")?;
            let rows = stmt.query_map(params![id], |r| r.get::<_, i64>(0))?;
            let mut v = Vec::new();
            for row in rows {
                v.push(row?);
            }
            v
        };
        for fid in &face_ids {
            conn.execute(
                "INSERT OR IGNORE INTO face_rejections(face_id, person_id) VALUES(?1, ?2)",
                params![fid, id],
            )?;
            conn.execute(
                "UPDATE faces SET person_id = NULL, confirmed = 0, cluster_id = NULL WHERE id = ?1",
                params![fid],
            )?;
        }
        conn.execute("DELETE FROM persons WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Merge `from` person into `into`. All faces move to `into`, then `from`
    /// is deleted.
    pub fn merge_persons(&self, from: i64, into: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE faces SET person_id = ?2 WHERE person_id = ?1",
            params![from, into],
        )?;
        conn.execute("DELETE FROM persons WHERE id = ?1", params![from])?;
        Ok(())
    }

    /// All persons with their face counts, ordered by name.
    pub fn persons(&self) -> Result<Vec<(Person, i64)>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.cover_face_id, \
                (SELECT COUNT(*) FROM faces f WHERE f.person_id = p.id) AS n \
             FROM persons p ORDER BY p.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok((map_person(r)?, r.get::<_, i64>(3)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// The number of faces assigned to a person.
    pub fn person_face_count(&self, id: i64) -> Result<i64> {
        let conn = self.lock();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM faces WHERE person_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// The total number of detected faces in the library.
    pub fn total_face_count(&self) -> Result<i64> {
        let conn = self.read_lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM faces", [], |r| r.get(0))?;
        Ok(n)
    }

    /// A representative face id for a person: the cover face if set, else the
    /// highest-scoring assigned face. Returns 0 when the person has no face.
    pub fn person_representative_face(&self, id: i64) -> Result<i64> {
        let conn = self.lock();
        // Prefer the stored cover face.
        let cover: Option<i64> = conn
            .query_row(
                "SELECT cover_face_id FROM persons WHERE id = ?1",
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
                "SELECT id FROM faces WHERE person_id = ?1 ORDER BY det_score DESC LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(fid.unwrap_or(0))
    }

    /// All photos that contain a face of the given person, newest first.
    pub fn photos_of_person(&self, person_id: i64) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos WHERE id IN \
                (SELECT DISTINCT photo_id FROM faces WHERE person_id = ?1) \
             ORDER BY taken_at DESC, filename"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![person_id], map_photo)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// All photos that contain an unassigned face in the given cluster, newest
    /// first.
    pub fn photos_in_cluster(&self, cluster_id: i64) -> Result<Vec<Photo>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos WHERE id IN \
                (SELECT DISTINCT photo_id FROM faces WHERE cluster_id = ?1 AND person_id IS NULL) \
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

    /// Remove one photo from a person. Every face of that photo loses the
    /// person link and keeps its cluster id. A later re-cluster may group it
    /// again.
    pub fn remove_photo_from_person(&self, photo_id: i64, person_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE faces SET person_id = NULL WHERE photo_id = ?1 AND person_id = ?2",
            params![photo_id, person_id],
        )?;
        Ok(())
    }

    /// Remove one photo from an unnamed cluster. Every face of that photo in
    /// the cluster loses its cluster id. A later re-cluster may group it again.
    pub fn remove_photo_from_cluster(&self, photo_id: i64, cluster_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE faces SET cluster_id = NULL WHERE photo_id = ?1 AND cluster_id = ?2",
            params![photo_id, cluster_id],
        )?;
        Ok(())
    }

    // --- Face scan state ---

    /// Photo ids that still need a face-detection pass, capped by `limit`.
    /// A photo needs a pass when it has no `face_scan` row, or its row is not
    /// done. Only enriched, present photos are eligible.
    pub fn photos_needing_face_scan(&self, limit: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT p.id FROM photos p \
             LEFT JOIN face_scan fs ON fs.photo_id = p.id \
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

    /// Photo ids in the given folders that still need a face-detection pass.
    /// The same rule as `photos_needing_face_scan`, scoped to a folder set. An
    /// empty folder set returns an empty list.
    pub fn photos_needing_face_scan_in(&self, folder_ids: &[i64], limit: i64) -> Result<Vec<i64>> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock();
        let placeholders = folder_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT p.id FROM photos p \
             LEFT JOIN face_scan fs ON fs.photo_id = p.id \
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

    /// Clear the face-scan state and detected faces for photos in the given
    /// folders, so a rescan re-processes them. An empty folder set does nothing.
    pub fn clear_face_scan_in(&self, folder_ids: &[i64]) -> Result<()> {
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
                "DELETE FROM faces WHERE photo_id IN \
                 (SELECT id FROM photos WHERE folder_id IN ({placeholders}))"
            ),
            ps.as_slice(),
        )?;
        conn.execute(
            &format!(
                "DELETE FROM face_scan WHERE photo_id IN \
                 (SELECT id FROM photos WHERE folder_id IN ({placeholders}))"
            ),
            ps.as_slice(),
        )?;
        conn.execute(
            &format!("UPDATE photos SET face_status = 0 WHERE folder_id IN ({placeholders})"),
            ps.as_slice(),
        )?;
        Ok(())
    }

    /// Set the face-scan state of a photo (0 pending, 1 scanning, 2 done,
    /// 3 error). Also mirrors the value into `photos.face_status`.
    pub fn set_face_scan_state(&self, photo_id: i64, state: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO face_scan(photo_id, state, scanned_at) VALUES(?1, ?2, ?3) \
             ON CONFLICT(photo_id) DO UPDATE SET state = ?2, scanned_at = ?3",
            params![photo_id, state, now()],
        )?;
        conn.execute(
            "UPDATE photos SET face_status = ?2 WHERE id = ?1",
            params![photo_id, state],
        )?;
        Ok(())
    }

    /// Delete all detected faces for a photo before a re-scan.
    pub fn clear_faces_for_photo(&self, photo_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM faces WHERE photo_id = ?1", params![photo_id])?;
        Ok(())
    }

    /// Delete every face, person, and scan record. The privacy reset.
    pub fn delete_all_face_data(&self) -> Result<()> {
        let conn = self.lock();
        conn.execute_batch(
            "DELETE FROM faces; DELETE FROM persons; DELETE FROM face_scan; \
             UPDATE photos SET face_status = 0;",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Folder, Photo};

    fn temp_lib() -> Library {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-faces-{}-{:?}.db",
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
                path: format!("/tmp/pichouse-faces/{name}dir"),
                name: "d".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        lib.upsert_photo_structure(&Photo {
            folder_id: fid,
            path: format!("/tmp/pichouse-faces/{name}.jpg"),
            filename: format!("{name}.jpg"),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn face_roundtrip_preserves_embedding() {
        let lib = temp_lib();
        let pid = add_photo(&lib, "a");
        let face = Face {
            photo_id: pid,
            bbox_x: 100,
            bbox_y: 200,
            bbox_w: 300,
            bbox_h: 300,
            landmarks: vec![1.0, 2.0, 3.0, 4.0],
            embedding: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            det_score: 0.9,
            ..Default::default()
        };
        let fid = lib.insert_face(&face).unwrap();
        assert!(fid > 0);
        let got = lib.faces_for_photo(pid).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].embedding, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(got[0].landmarks, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(got[0].bbox_w, 300);
        // det_score survives the 0..1000 scale within one milli-unit.
        assert!((got[0].det_score - 0.9).abs() < 0.002);
    }

    #[test]
    fn person_assignment_and_photos() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "b");
        let p2 = add_photo(&lib, "c");
        let f1 = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0, 0.0],
                ..Default::default()
            })
            .unwrap();
        lib.insert_face(&Face {
            photo_id: p2,
            embedding: vec![0.0, 1.0],
            ..Default::default()
        })
        .unwrap();

        let alice = lib.create_person("Alice").unwrap();
        lib.set_face_person(f1, alice).unwrap();
        assert_eq!(lib.person_face_count(alice).unwrap(), 1);
        let photos = lib.photos_of_person(alice).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].id, p1);
    }

    #[test]
    fn photos_in_cluster_excludes_already_named_faces() {
        // Two faces share one cluster id: one gets named, one stays pending.
        // Opening the cluster (e.g. from the "Unnamed" tile) must show only
        // the still-unassigned face's photo, not the named one's.
        let lib = temp_lib();
        let p_named = add_photo(&lib, "g");
        let p_pending = add_photo(&lib, "h");
        let f_named = lib
            .insert_face(&Face {
                photo_id: p_named,
                cluster_id: 42,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        lib.insert_face(&Face {
            photo_id: p_pending,
            cluster_id: 42,
            embedding: vec![0.99],
            ..Default::default()
        })
        .unwrap();
        let alice = lib.create_person("Alice").unwrap();
        lib.set_face_person(f_named, alice).unwrap();

        let photos = lib.photos_in_cluster(42).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].id, p_pending);

        let unassigned = lib.unassigned_faces_in_cluster(42).unwrap();
        assert_eq!(unassigned.len(), 1);
        assert_eq!(unassigned[0].photo_id, p_pending);
    }

    #[test]
    fn merge_moves_faces_and_deletes_source() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "d");
        let f1 = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let a = lib.create_person("A").unwrap();
        let b = lib.create_person("B").unwrap();
        lib.set_face_person(f1, a).unwrap();
        lib.merge_persons(a, b).unwrap();
        assert_eq!(lib.person_face_count(b).unwrap(), 1);
        // A is gone.
        let names: Vec<String> = lib.persons().unwrap().into_iter().map(|(p, _)| p.name).collect();
        assert_eq!(names, vec!["B".to_string()]);
    }

    #[test]
    fn face_scan_state_gates_needing_list() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "e");
        // Not enriched yet (scan_state 0), so not eligible.
        assert!(lib.photos_needing_face_scan(10).unwrap().is_empty());
        // Mark enriched.
        {
            let conn = lib.lock();
            conn.execute("UPDATE photos SET scan_state = 2 WHERE id = ?1", params![p1])
                .unwrap();
        }
        assert_eq!(lib.photos_needing_face_scan(10).unwrap(), vec![p1]);
        // Mark done: no longer needed.
        lib.set_face_scan_state(p1, 2).unwrap();
        assert!(lib.photos_needing_face_scan(10).unwrap().is_empty());
    }

    #[test]
    fn delete_all_clears_everything() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "f");
        let f1 = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let a = lib.create_person("A").unwrap();
        lib.set_face_person(f1, a).unwrap();
        lib.set_face_scan_state(p1, 2).unwrap();
        lib.delete_all_face_data().unwrap();
        assert!(lib.faces_for_photo(p1).unwrap().is_empty());
        assert!(lib.persons().unwrap().is_empty());
    }

    #[test]
    fn cover_if_unset_fills_gap_but_not_an_existing_choice() {
        let lib = temp_lib();
        let p1 = add_photo(&lib, "g");
        let f1 = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let f2 = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0],
                ..Default::default()
            })
            .unwrap();
        let a = lib.create_person("A").unwrap();

        // No cover yet: the fallback fills it in.
        lib.set_person_cover_if_unset(a, f1).unwrap();
        assert_eq!(lib.person_representative_face(a).unwrap(), f1);

        // Already covered: a later merge's fallback must not replace it.
        lib.set_person_cover_if_unset(a, f2).unwrap();
        assert_eq!(lib.person_representative_face(a).unwrap(), f1);
    }
}

