//! Per-server Immich thumbnail-blob database (`immich-thumbs-<server_id>.db`).
//!
//! Immich assets have no local file, so their thumbnails cannot use the
//! hash-keyed `thumbs-<N>.db` cache. Each Immich server gets its own database
//! keyed by asset id. This lets a thumbnail survive between sessions and avoids
//! a repeat HTTP download.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::library::now;
use super::Result;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS thumbnails (
    asset_id   TEXT PRIMARY KEY,
    jpeg       BLOB NOT NULL,
    created_at INTEGER NOT NULL
);";

/// A handle to an `immich-thumbs-<server_id>.db` database.
///
/// The connection is wrapped in a `Mutex`, which serializes writers. The grid
/// downloads thumbnails from several workers at the same time; one serialized
/// connection avoids "database is locked" errors.
pub struct ImmichThumbs {
    conn: Mutex<Connection>,
}

/// The thumbnail database file path for one Immich server.
pub fn immich_thumbs_path_for_server(server_id: i64) -> std::io::Result<std::path::PathBuf> {
    let dir = super::config::data_dir()?;
    Ok(dir.join(format!("immich-thumbs-{}.db", server_id)))
}

impl ImmichThumbs {
    /// Open (and initialize) the thumbnail database for one server.
    pub fn open_for_server(server_id: i64) -> Result<ImmichThumbs> {
        let path = immich_thumbs_path_for_server(server_id)?;
        ImmichThumbs::open_at(path)
    }

    /// Open (and initialize) a thumbnail database at the given path.
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<ImmichThumbs> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(ImmichThumbs {
            conn: Mutex::new(conn),
        })
    }

    /// The cached JPEG thumbnail for an asset id, or `None` if none is cached.
    pub fn get(&self, asset_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let blob: Option<Vec<u8>> = conn
            .query_row(
                "SELECT jpeg FROM thumbnails WHERE asset_id = ?1",
                params![asset_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(blob)
    }

    /// Store (or replace) a JPEG thumbnail for an asset id.
    pub fn put(&self, asset_id: &str, jpeg: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO thumbnails(asset_id, jpeg, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(asset_id) DO UPDATE SET jpeg=excluded.jpeg, created_at=excluded.created_at",
            params![asset_id, jpeg, now()],
        )?;
        Ok(())
    }
}

/// Delete the thumbnail database file for one Immich server. Callers must drop
/// any open handle first.
pub fn remove_immich_thumbs_for_server(server_id: i64) -> Result<()> {
    let path = immich_thumbs_path_for_server(server_id)?;
    remove_db_files(&path)
}

/// Delete every Immich thumbnail database file (`immich-thumbs-*.db*`) in the
/// data directory. Callers must drop any open handles first.
pub fn remove_all_immich_thumb_databases() -> Result<()> {
    let dir = super::config::data_dir()?;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("immich-thumbs") && name.contains(".db") {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}

/// Delete a SQLite database file and its WAL/SHM sidecars.
fn remove_db_files(path: &Path) -> Result<()> {
    for p in [
        path.to_path_buf(),
        path.with_extension("db-wal"),
        path.with_extension("db-shm"),
    ] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
