//! Immich server records in `library.db`.
//!
//! Each row in `immich_servers` is a remote Immich instance. pichouse can
//! connect to more than one server. This module gives create, read, update,
//! and delete access to those rows.

use rusqlite::params;

use crate::model::ImmichServer;

use super::library::{now, Library};
use super::Result;

impl Library {
    /// Add an Immich server. Returns the new server with its assigned id.
    pub fn add_immich_server(&self, name: &str, base_url: &str, api_key: &str) -> Result<ImmichServer> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE immich_servers SET name = ?1, base_url = ?2, api_key = ?3 WHERE id = ?4",
            params![name, base_url, api_key, id],
        )?;
        Ok(())
    }

    /// Delete an Immich server by id.
    pub fn delete_immich_server(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM immich_servers WHERE id = ?1", params![id])?;
        Ok(())
    }
}
