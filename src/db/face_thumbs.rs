//! Face-crop thumbnail database (`face-thumbs.db`).
//!
//! One row per face, keyed by the face id. The blob is a small square JPEG of
//! the face cropped from the source photo. The People UI reads it.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::library::now;
use super::Result;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS face_thumbs (
    face_id    INTEGER PRIMARY KEY,
    jpeg       BLOB NOT NULL,
    created_at INTEGER NOT NULL
);";

/// A handle to the `face-thumbs.db` database.
pub struct FaceThumbs {
    conn: Mutex<Connection>,
}

/// The face-thumbnail database file path.
pub fn face_thumbs_path() -> std::io::Result<std::path::PathBuf> {
    Ok(super::config::data_dir()?.join("face-thumbs.db"))
}

impl FaceThumbs {
    /// Open (and initialize) the face-thumbnail database.
    pub fn open() -> Result<FaceThumbs> {
        FaceThumbs::open_at(face_thumbs_path()?)
    }

    /// Open (and initialize) a face-thumbnail database at the given path.
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<FaceThumbs> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(FaceThumbs {
            conn: Mutex::new(conn),
        })
    }

    /// The cached JPEG for a face, or `None`.
    pub fn get(&self, face_id: i64) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let blob: Option<Vec<u8>> = conn
            .query_row(
                "SELECT jpeg FROM face_thumbs WHERE face_id = ?1",
                params![face_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(blob)
    }

    /// Store (or replace) the JPEG for a face.
    pub fn put(&self, face_id: i64, jpeg: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO face_thumbs(face_id, jpeg, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(face_id) DO UPDATE SET jpeg=excluded.jpeg, created_at=excluded.created_at",
            params![face_id, jpeg, now()],
        )?;
        Ok(())
    }

    /// Remove the cached thumbnail for a face.
    pub fn delete(&self, face_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM face_thumbs WHERE face_id = ?1", params![face_id])?;
        Ok(())
    }

    /// Remove all cached face thumbnails.
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM face_thumbs", [])?;
        Ok(())
    }
}

/// Delete the face-thumbnail database file. Callers drop open handles first.
pub fn remove_face_thumbs_database() -> Result<()> {
    let dir = super::config::data_dir()?;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("face-thumbs") && name.contains(".db") {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
