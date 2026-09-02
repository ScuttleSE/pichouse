//! Tag vocabulary, photo-tag links, and FTS5 maintenance.

use rusqlite::{params, OptionalExtension, Transaction};

use crate::model::{AiStatus, Tag, TagCount, TagSource};

use super::library::now;
use super::{Library, Result};

impl Library {
    /// Upsert the given tag names into the global vocabulary and link them to
    /// the photo with the given source. Existing links are preserved; a user
    /// tag never downgrades an existing link's source. The photo's FTS row is
    /// rebuilt.
    pub fn add_photo_tags(&self, photo_id: i64, tags: &[String], source: TagSource) -> Result<()> {
        if tags.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let created = now();
        for raw in tags {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT INTO tags(name) VALUES(?1) ON CONFLICT(name) DO NOTHING",
                params![name],
            )?;
            let tag_id: i64 =
                tx.query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
                    r.get(0)
                })?;
            // Insert link if absent. If a user tag arrives, upgrade source to user.
            tx.execute(
                "INSERT INTO photo_tags(photo_id, tag_id, source, confirmed, created_at)
                 VALUES(?1, ?2, ?3, 0, ?4)
                 ON CONFLICT(photo_id, tag_id) DO UPDATE SET
                   source = CASE WHEN ?3 = ?5 THEN ?5 ELSE photo_tags.source END",
                params![
                    photo_id,
                    tag_id,
                    source.as_i64(),
                    created,
                    TagSource::User.as_i64()
                ],
            )?;
        }
        rebuild_photo_fts(&tx, photo_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Unlink a tag (by name) from a photo and rebuild the FTS row.
    pub fn remove_photo_tag(&self, photo_id: i64, name: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM photo_tags WHERE photo_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![photo_id, name],
        )?;
        rebuild_photo_fts(&tx, photo_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Mark an AI tag on a photo as confirmed by the user.
    pub fn confirm_photo_tag(&self, photo_id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photo_tags SET confirmed = 1
             WHERE photo_id = ?1 AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![photo_id, name],
        )?;
        Ok(())
    }

    /// The tags on a photo ordered by source then name.
    pub fn photo_tags(&self, photo_id: i64) -> Result<Vec<Tag>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT t.name, pt.source, pt.confirmed
             FROM photo_tags pt JOIN tags t ON t.id = pt.tag_id
             WHERE pt.photo_id = ?1 ORDER BY pt.source ASC, t.name ASC",
        )?;
        let rows = stmt.query_map(params![photo_id], |r| {
            Ok(Tag {
                name: r.get(0)?,
                source: TagSource::from_i64(r.get::<_, i64>(1)?),
                confirmed: r.get::<_, i64>(2)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Update a photo's AI tagging status.
    pub fn set_ai_status(&self, photo_id: i64, status: AiStatus) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE photos SET ai_status = ?1 WHERE id = ?2",
            params![status.as_i64(), photo_id],
        )?;
        Ok(())
    }

    /// Photo ids that have not yet been AI-tagged. If `folder_id > 0` the search
    /// is limited to that folder. Photos marked done or skipped are excluded
    /// unless `include_done` is true.
    pub fn photos_needing_tags(&self, folder_id: i64, include_done: bool) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut sql = String::from("SELECT id FROM photos WHERE 1=1");
        if folder_id > 0 {
            sql.push_str(" AND folder_id = ?1");
        }
        if !include_done {
            sql.push_str(&format!(
                " AND ai_status NOT IN ({}, {})",
                AiStatus::Done.as_i64(),
                AiStatus::Skipped.as_i64()
            ));
        }
        sql.push_str(" ORDER BY id ASC");
        let mut stmt = conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row| r.get::<_, i64>(0);
        let rows = if folder_id > 0 {
            stmt.query_map(params![folder_id], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map([], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    /// The set of photo ids whose tags match the query using FTS5. A trailing
    /// `*` is appended to the last token for prefix matching.
    pub fn search_photo_ids_by_tag(&self, query: &str) -> Result<std::collections::HashSet<i64>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let match_expr = fts_query(query);
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT rowid FROM photo_tags_fts WHERE photo_tags_fts MATCH ?1")?;
        let rows = stmt.query_map(params![match_expr], |r| r.get::<_, i64>(0))?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            out.insert(row?);
        }
        Ok(out)
    }

    /// Every tag with the number of photos carrying it.
    pub fn all_tags(&self) -> Result<Vec<TagCount>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT t.name, COUNT(pt.photo_id)
             FROM tags t LEFT JOIN photo_tags pt ON pt.tag_id = t.id
             GROUP BY t.id ORDER BY t.name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TagCount {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Rename a tag globally. If the new name already exists the two are merged.
    /// FTS rows for all affected photos are rebuilt.
    pub fn rename_tag(&self, old_name: &str, new_name: &str) -> Result<()> {
        let new_name = new_name.trim();
        if new_name.is_empty() || old_name.eq_ignore_ascii_case(new_name) {
            return Ok(());
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let old_id: Option<i64> = tx
            .query_row("SELECT id FROM tags WHERE name = ?1", params![old_name], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(old_id) = old_id else {
            return Ok(());
        };
        let new_id: Option<i64> = tx
            .query_row("SELECT id FROM tags WHERE name = ?1", params![new_name], |r| {
                r.get(0)
            })
            .optional()?;
        let mut affected = affected_photo_ids(&tx, old_id)?;
        match new_id {
            None => {
                tx.execute(
                    "UPDATE tags SET name = ?1 WHERE id = ?2",
                    params![new_name, old_id],
                )?;
            }
            Some(new_id) => {
                merge_tag_into(&tx, old_id, new_id)?;
                affected.extend(affected_photo_ids(&tx, new_id)?);
            }
        }
        for pid in affected {
            rebuild_photo_fts(&tx, pid)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Merge `src` into `dst` (both by name) and rebuild affected FTS rows.
    pub fn merge_tags(&self, src_name: &str, dst_name: &str) -> Result<()> {
        if src_name.eq_ignore_ascii_case(dst_name) {
            return Ok(());
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let src_id: i64 =
            tx.query_row("SELECT id FROM tags WHERE name = ?1", params![src_name], |r| {
                r.get(0)
            })?;
        let dst_id: i64 =
            tx.query_row("SELECT id FROM tags WHERE name = ?1", params![dst_name], |r| {
                r.get(0)
            })?;
        let affected = affected_photo_ids(&tx, src_id)?;
        merge_tag_into(&tx, src_id, dst_id)?;
        for pid in affected {
            rebuild_photo_fts(&tx, pid)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove a tag globally and rebuild affected FTS rows.
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let id: Option<i64> = tx
            .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(id) = id else {
            return Ok(());
        };
        let affected = affected_photo_ids(&tx, id)?;
        tx.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        for pid in affected {
            rebuild_photo_fts(&tx, pid)?;
        }
        tx.commit()?;
        Ok(())
    }
}

// --- helpers (operate within a transaction) ---

/// The photo ids linked to a tag id.
fn affected_photo_ids(tx: &Transaction, tag_id: i64) -> Result<Vec<i64>> {
    let mut stmt = tx.prepare("SELECT photo_id FROM photo_tags WHERE tag_id = ?1")?;
    let rows = stmt.query_map(params![tag_id], |r| r.get::<_, i64>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Repoint all links from `src_id` to `dst_id` (avoiding duplicates) and delete
/// `src_id`.
fn merge_tag_into(tx: &Transaction, src_id: i64, dst_id: i64) -> Result<()> {
    tx.execute(
        "UPDATE OR IGNORE photo_tags SET tag_id = ?1 WHERE tag_id = ?2",
        params![dst_id, src_id],
    )?;
    tx.execute("DELETE FROM photo_tags WHERE tag_id = ?1", params![src_id])?;
    tx.execute("DELETE FROM tags WHERE id = ?1", params![src_id])?;
    Ok(())
}

/// Replace the FTS row for a photo with its current tag text.
fn rebuild_photo_fts(tx: &Transaction, photo_id: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM photo_tags_fts WHERE rowid = ?1",
        params![photo_id],
    )?;
    let text: Option<String> = tx.query_row(
        "SELECT group_concat(t.name, ' ')
         FROM photo_tags pt JOIN tags t ON t.id = pt.tag_id
         WHERE pt.photo_id = ?1",
        params![photo_id],
        |r| r.get(0),
    )?;
    if let Some(text) = text {
        if !text.is_empty() {
            tx.execute(
                "INSERT INTO photo_tags_fts(rowid, tags) VALUES(?1, ?2)",
                params![photo_id, text],
            )?;
        }
    }
    Ok(())
}

/// Turn a free-text query into an FTS5 MATCH expression: each token is quoted;
/// the final token gets a prefix wildcard so typing is incremental.
fn fts_query(q: &str) -> String {
    let fields: Vec<&str> = q.split_whitespace().collect();
    let mut parts: Vec<String> = Vec::with_capacity(fields.len());
    let last = fields.len().saturating_sub(1);
    for (i, f) in fields.iter().enumerate() {
        let f = f.replace('"', "");
        if f.is_empty() {
            continue;
        }
        if i == last {
            parts.push(format!("\"{}\"*", f));
        } else {
            parts.push(format!("\"{}\"", f));
        }
    }
    parts.join(" ")
}
