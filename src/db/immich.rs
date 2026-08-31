//! Immich server records in `library.db`.
//!
//! Each row in `immich_servers` is a remote Immich instance. pichouse can
//! connect to more than one server. This module gives create, read, update,
//! and delete access to those rows.

use rusqlite::params;

use crate::model::{ImmichFolderLink, ImmichServer};

use super::library::{now, Library};
use super::Result;
impl Library {
    /// Add an Immich server. Returns the new server with its assigned id.
    pub fn add_immich_server(&self, name: &str, base_url: &str, api_key: &str) -> Result<ImmichServer> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO immich_servers(name, base_url, api_key, added_at) VALUES(?1, ?2, ?3, ?4)",
            params![name, base_url, api_key, now()],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ImmichServer {
            id,
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            added_at: now(),
        })
    }

    /// Every Immich server, ordered by id.
    pub fn immich_servers(&self) -> Result<Vec<ImmichServer>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, api_key, added_at FROM immich_servers ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ImmichServer {
                id: r.get(0)?,
                name: r.get(1)?,
                base_url: r.get(2)?,
                api_key: r.get(3)?,
                added_at: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// One Immich server by id, or `None` if it does not exist.
    pub fn immich_server(&self, id: i64) -> Result<Option<ImmichServer>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                "SELECT id, name, base_url, api_key, added_at FROM immich_servers WHERE id = ?1",
                params![id],
                |r| {
                    Ok(ImmichServer {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        base_url: r.get(2)?,
                        api_key: r.get(3)?,
                        added_at: r.get(4)?,
                    })
                },
            )
            .ok();
        Ok(row)
    }

    /// Change the name, URL, and API key of an Immich server.
    pub fn update_immich_server(&self, id: i64, name: &str, base_url: &str, api_key: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE immich_servers SET name = ?1, base_url = ?2, api_key = ?3 WHERE id = ?4",
            params![name, base_url, api_key, id],
        )?;
        Ok(())
    }

    /// Delete an Immich server by id.
    pub fn delete_immich_server(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM immich_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Link a folder to an Immich album for auto-upload. Replaces any existing
    /// link for that folder.
    pub fn set_immich_folder_link(
        &self,
        folder_id: i64,
        server_id: i64,
        immich_album_id: &str,
    ) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO immich_folder_links(folder_id, server_id, immich_album_id, created_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(folder_id) DO UPDATE SET
                server_id=excluded.server_id,
                immich_album_id=excluded.immich_album_id,
                created_at=excluded.created_at",
            params![folder_id, server_id, immich_album_id, now()],
        )?;
        Ok(())
    }

    /// The Immich album link for a folder, or `None` if the folder is not
    /// linked.
    pub fn immich_folder_link(&self, folder_id: i64) -> Result<Option<ImmichFolderLink>> {
        let conn = self.lock();
        let row = conn
            .query_row(
                "SELECT folder_id, server_id, immich_album_id, created_at
                 FROM immich_folder_links WHERE folder_id = ?1",
                params![folder_id],
                |r| {
                    Ok(ImmichFolderLink {
                        folder_id: r.get(0)?,
                        server_id: r.get(1)?,
                        immich_album_id: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                },
            )
            .ok();
        Ok(row)
    }

    /// Remove a folder's Immich link.
    pub fn delete_immich_folder_link(&self, folder_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM immich_folder_links WHERE folder_id = ?1",
            params![folder_id],
        )?;
        Ok(())
    }

    /// Every folder id that is currently linked to an Immich album.
    pub fn linked_immich_folders(&self) -> Result<std::collections::HashSet<i64>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare("SELECT folder_id FROM immich_folder_links")?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            out.insert(row?);
        }
        Ok(out)
    }
}
