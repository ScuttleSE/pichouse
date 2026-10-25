//! Per-size thumbnail-blob database (`thumbs-<N>.db`).

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::library::now;
use super::Result;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS thumbnails (
    photo_hash TEXT PRIMARY KEY,
    size       INTEGER NOT NULL,
    jpeg       BLOB NOT NULL,
    created_at INTEGER NOT NULL
);";

/// A handle to a `thumbs-<size>.db` thumbnail-blob database.
///
/// The connection is wrapped in a `Mutex`, which serializes writers. The UI
/// generates thumbnails from several workers concurrently; a single serialized
/// connection avoids "database is locked" errors.
pub struct Thumbs {
    conn: Mutex<Connection>,
    /// Read-only connections for `get`. WAL mode lets them read in parallel
    /// with each other and with the write connection.
    readers: Vec<Mutex<Connection>>,
}

/// The number of read connections for each thumbnail database.
const THUMB_READERS: usize = 4;

/// The connection settings for a thumbnail database.
const THUMB_PRAGMAS: &str = "PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL; \
     PRAGMA cache_size=-32768; PRAGMA mmap_size=268435456; PRAGMA temp_store=MEMORY;";

/// The thumbnail database file path for a given thumbnail size (longest side in
/// pixels). Each size gets its own file so switching thumbnail quality never
/// overwrites another size's cache.
pub fn thumbs_path_for_size(size: i32) -> std::io::Result<std::path::PathBuf> {
    let dir = super::config::data_dir()?;
    Ok(dir.join(format!("thumbs-{}.db", size)))
}

impl Thumbs {
    /// Open (and initialize) the per-size thumbnail database.
    pub fn open_for_size(size: i32) -> Result<Thumbs> {
        let path = thumbs_path_for_size(size)?;
        Thumbs::open_at(path)
    }

    /// Open (and initialize) a thumbnail database at the given path.
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Thumbs> {
        let path = path.as_ref();
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(THUMB_PRAGMAS)?;
        conn.execute_batch(SCHEMA)?;
        let mut readers = Vec::with_capacity(THUMB_READERS);
        for _ in 0..THUMB_READERS {
            let r = Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            r.execute_batch(THUMB_PRAGMAS)?;
            readers.push(Mutex::new(r));
        }
        Ok(Thumbs {
            conn: Mutex::new(conn),
            readers,
        })
    }

    /// The cached JPEG thumbnail for a photo hash, or `None` if none is cached.
    pub fn get(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        // Take the first free read connection. Wait on the first one when all
        // are busy.
        let conn = self
            .readers
            .iter()
            .find_map(|m| m.try_lock().ok())
            .unwrap_or_else(|| self.readers[0].lock().unwrap());
        let blob: Option<Vec<u8>> = conn
            .prepare_cached("SELECT jpeg FROM thumbnails WHERE photo_hash = ?1")?
            .query_row(
                params![hash],
                |r| r.get(0),
            )
            .optional()?;
        Ok(blob)
    }

    /// Store (or replace) a JPEG thumbnail for a photo hash.
    pub fn put(&self, hash: &str, size: i32, jpeg: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO thumbnails(photo_hash, size, jpeg, created_at) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(photo_hash) DO UPDATE SET size=excluded.size, jpeg=excluded.jpeg, created_at=excluded.created_at",
            params![hash, size, jpeg, now()],
        )?;
        Ok(())
    }

    /// Remove any cached thumbnail for a photo hash.
    #[allow(dead_code)] // Superseded by delete_hash_and_edits; kept as API.
    pub fn delete(&self, hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM thumbnails WHERE photo_hash = ?1", params![hash])?;
        Ok(())
    }

    /// Remove the plain-hash thumbnail and every edited variant, whose keys are
    /// `<hash>|<edit_rev>`. Used when a photo's rotation or edits change.
    pub fn delete_hash_and_edits(&self, hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM thumbnails WHERE photo_hash = ?1 OR photo_hash LIKE ?2",
            params![hash, format!("{hash}|%")],
        )?;
        Ok(())
    }

    /// Remove all cached thumbnails.
    #[allow(dead_code)] // Kept API; cache is cleared via file removal in Generator.
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM thumbnails", [])?;
        Ok(())
    }
}

/// Delete every thumbnail database file (`thumbs*.db*`) in the data directory.
/// Callers must drop any open handles first.
pub fn remove_all_thumb_databases() -> Result<()> {
    let dir = super::config::data_dir()?;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("thumbs") && name.contains(".db") {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
