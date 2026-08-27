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

    /// Every photo in an album, across all its member folders. Ordered by taken
    /// date then filename. Used by the Immich upload path.
    pub fn photos_in_album(&self, album_id: i64) -> Result<Vec<crate::model::Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.folder_id, p.path, p.filename, p.size, p.mod_time, p.taken_at, \
                    p.width, p.height, p.hash, p.thumb_ready, p.orientation, p.ai_status, \
                    p.scan_state, p.missing, p.added_at \
             FROM photos p \
             JOIN album_folders af ON af.folder_id = p.folder_id \
             WHERE af.album_id = ?1 AND p.missing = 0 \
             ORDER BY p.taken_at ASC, p.filename ASC",
        )?;
        let rows = stmt.query_map([album_id], super::library::map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Folder, Photo};

    fn temp_lib() -> (Library, std::path::PathBuf) {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-albtest-{}-{}.db",
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
    fn remove_library_folder_then_use_albums() {
        let (lib, path) = temp_lib();
        let root = "/tmp/pichouse-albtest-root";
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
        lib.upsert_photo(&Photo {
            folder_id: fid,
            path: format!("{root}/sub/a.jpg"),
            filename: "a.jpg".into(),
            ..Default::default()
        })
        .unwrap();
        let aid = lib.create_album("My Album", 0).unwrap();
        lib.add_folder_to_album(fid, aid).unwrap();

        // Remove the library folder: folders + photos + album_folders cascade.
        lib.remove_library_folder(root).unwrap();

        // The album survives but is empty. These calls must not panic/error.
        let albums = lib.albums().unwrap();
        assert_eq!(albums.len(), 1);
        let fa = lib.folder_albums().unwrap();
        assert!(fa.is_empty());
        lib.rename_album(aid, "Renamed").unwrap();
        lib.delete_album(aid).unwrap();
        assert!(lib.albums().unwrap().is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
