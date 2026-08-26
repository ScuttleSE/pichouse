//! Album tree CRUD and folder↔album membership.

use rusqlite::{params, OptionalExtension};

use crate::model::Album;

use super::{Library, Result};

impl Library {
    /// Insert a new album. `parent_id` of 0 creates a top-level album.
    pub fn create_album(&self, name: &str, parent_id: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let pos: i64 = if parent_id == 0 {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM albums WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM albums WHERE parent_id = ?1",
                params![parent_id],
                |r| r.get(0),
            )?
        };
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "INSERT INTO albums(name, parent_id, position) VALUES(?1, ?2, ?3)",
            params![name, parent, pos + 1],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Change an album's display name.
    pub fn rename_album(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE albums SET name = ?1 WHERE id = ?2", params![name, id])?;
        Ok(())
    }

    /// Remove an album. Sub-albums cascade; member folders revert to the Library
    /// root ("New folders").
    pub fn delete_album(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM albums WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Re-parent an album. `parent_id` of 0 makes it top-level. Refuses to
    /// create a cycle (making an album a descendant of itself).
    pub fn set_album_parent(&self, id: i64, parent_id: i64) -> Result<()> {
        if id == parent_id {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        // Walk up from the proposed parent; if we reach id, this would create a
        // cycle, so refuse.
        let mut cur = parent_id;
        while cur != 0 {
            if cur == id {
                return Ok(()); // would create a cycle; ignore
            }
            let next: Option<i64> = conn
                .query_row("SELECT parent_id FROM albums WHERE id = ?1", params![cur], |r| {
                    r.get(0)
                })
                .optional()?
                .flatten();
            cur = next.unwrap_or(0);
        }
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "UPDATE albums SET parent_id = ?1 WHERE id = ?2",
            params![parent, id],
        )?;
        Ok(())
    }

    /// All albums ordered by position then name.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, COALESCE(parent_id, 0), position
             FROM albums ORDER BY position ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Album {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                position: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Place a folder into an album, removing it from any other album first (a
    /// folder belongs to at most one album in the tree).
    pub fn add_folder_to_album(&self, folder_id: i64, album_id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM album_folders WHERE folder_id = ?1",
            params![folder_id],
        )?;
        let pos: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM album_folders WHERE album_id = ?1",
            params![album_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO album_folders(album_id, folder_id, position) VALUES(?1, ?2, ?3)",
            params![album_id, folder_id, pos + 1],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Detach a folder from any album, returning it to the Library root.
    pub fn remove_folder_from_album(&self, folder_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM album_folders WHERE folder_id = ?1",
            params![folder_id],
        )?;
        Ok(())
    }

    /// A map of folder id to the album id it belongs to. Folders not in any
    /// album are absent from the map.
    pub fn folder_albums(&self) -> Result<std::collections::HashMap<i64, i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT folder_id, album_id FROM album_folders")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (fid, aid) = row?;
            out.insert(fid, aid);
        }
        Ok(out)
    }
}
