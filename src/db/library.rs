//! `library.db` metadata database: folders, photos, settings, scan state.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{Folder, LibraryFolder, Photo, ScanStatus};

use super::Result;

const SCHEMA: &str = include_str!("schema.sql");

/// A handle to the `library.db` metadata database.
///
/// The connection is wrapped in a `Mutex`. This gives interior mutability and
/// serializes every access, which also serializes the multi-statement tag
/// writes and their FTS maintenance.
pub struct Library {
    pub(super) conn: Mutex<Connection>,
}

/// Current Unix time in seconds.
pub(super) fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Library {
    /// Open (and initialize) `library.db` in the pichouse data directory.
    pub fn open() -> Result<Library> {
        let dir = super::config::data_dir()?;
        Library::open_at(dir.join("library.db"))
    }

    /// Open (and initialize) a library database at the given path.
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Library> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Library {
            conn: Mutex::new(conn),
        })
    }

    /// Record a user-added root folder. Idempotent.
    pub fn add_library_folder(&self, path: &str) -> Result<LibraryFolder> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO library_folders(path, added_at) VALUES(?1, ?2)
             ON CONFLICT(path) DO NOTHING",
            params![path, now()],
        )?;
        let lf = conn.query_row(
            "SELECT id, path, added_at FROM library_folders WHERE path = ?1",
            params![path],
            |r| {
                Ok(LibraryFolder {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    added_at: r.get(2)?,
                })
            },
        )?;
        Ok(lf)
    }

    /// Delete a user-added root folder and all folders/photos scanned beneath it
    /// (matched by path prefix).
    pub fn remove_library_folder(&self, path: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let prefix = format!("{}{}%", path, std::path::MAIN_SEPARATOR);
        tx.execute(
            "DELETE FROM folders WHERE path = ?1 OR path LIKE ?2",
            params![path, prefix],
        )?;
        tx.execute("DELETE FROM library_folders WHERE path = ?1", params![path])?;
        tx.commit()?;
        Ok(())
    }

    /// All user-added root folders.
    pub fn library_folders(&self) -> Result<Vec<LibraryFolder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, path, added_at FROM library_folders ORDER BY path")?;
        let rows = stmt.query_map([], |r| {
            Ok(LibraryFolder {
                id: r.get(0)?,
                path: r.get(1)?,
                added_at: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Insert or update a scanned folder and return its id.
    pub fn upsert_folder(&self, f: &Folder) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO folders(path, name, mtime, year) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET name=excluded.name, mtime=excluded.mtime, year=excluded.year",
            params![f.path, f.name, f.mtime, f.year],
        )?;
        let id =
            conn.query_row("SELECT id FROM folders WHERE path = ?1", params![f.path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// All scanned folders ordered by year (desc) then name.
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, name, mtime, year FROM folders ORDER BY year DESC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                path: r.get(1)?,
                name: r.get(2)?,
                mtime: r.get(3)?,
                year: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A map of folder id to its photo count.
    pub fn folder_photo_counts(&self) -> Result<std::collections::HashMap<i64, i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT folder_id, COUNT(*) FROM photos GROUP BY folder_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (fid, n) = row?;
            out.insert(fid, n);
        }
        Ok(out)
    }

    /// Insert or update a photo by path and return its id. Does not modify an
    /// existing photo's `orientation` or `ai_status`.
    pub fn upsert_photo(&self, p: &Photo) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO photos(folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
             ON CONFLICT(path) DO UPDATE SET
               folder_id=excluded.folder_id, filename=excluded.filename, size=excluded.size,
               mod_time=excluded.mod_time, taken_at=excluded.taken_at, width=excluded.width,
               height=excluded.height, hash=excluded.hash",
            params![
                p.folder_id, p.path, p.filename, p.size, p.mod_time, p.taken_at,
                p.width, p.height, p.hash, p.thumb_ready as i64
            ],
        )?;
        let id =
            conn.query_row("SELECT id FROM photos WHERE path = ?1", params![p.path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    /// All photos for a folder ordered by taken date then name.
    pub fn photos_in_folder(&self, folder_id: i64) -> Result<Vec<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status
             FROM photos WHERE folder_id = ?1 ORDER BY taken_at ASC, filename ASC",
        )?;
        let rows = stmt.query_map(params![folder_id], map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Load a single photo by id.
    pub fn photo_by_id(&self, id: i64) -> Result<Option<Photo>> {
        let conn = self.conn.lock().unwrap();
        let p = conn
            .query_row(
                "SELECT id, folder_id, path, filename, size, mod_time, taken_at, width, height, hash, thumb_ready, orientation, ai_status
                 FROM photos WHERE id = ?1",
                params![id],
                map_photo,
            )
            .optional()?;
        Ok(p)
    }

    /// A map of file path to content hash for all photos whose parent directory
    /// is `dir`. Used by the raw folder view to reuse scanned thumbnails.
    pub fn hashes_by_dir(&self, dir: &str) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.path, p.hash FROM photos p
             JOIN folders f ON f.id = p.folder_id
             WHERE f.path = ?1",
        )?;
        let rows = stmt.query_map(params![dir], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, hash) = row?;
            out.insert(path, hash);
        }
        Ok(out)
    }

    /// Mark whether a photo's thumbnail has been generated.
    pub fn set_thumb_ready(&self, photo_id: i64, ready: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE photos SET thumb_ready = ?1 WHERE id = ?2",
            params![ready as i64, photo_id],
        )?;
        Ok(())
    }

    /// Store the user-applied rotation (degrees clockwise, normalized to
    /// 0/90/180/270). Never written to disk; lives only in the database.
    pub fn set_orientation(&self, photo_id: i64, degrees: i32) -> Result<()> {
        let degrees = ((degrees % 360) + 360) % 360;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE photos SET orientation = ?1 WHERE id = ?2",
            params![degrees, photo_id],
        )?;
        Ok(())
    }

    /// The stored value for `key`, or `def` if unset.
    pub fn get_setting(&self, key: &str, def: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let v: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or_else(|| def.to_string()))
    }

    /// Store a value for `key`.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Record the scan status for a folder.
    pub fn set_scan_state(&self, folder_id: i64, status: ScanStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO scan_state(folder_id, last_scanned, status) VALUES(?1, ?2, ?3)
             ON CONFLICT(folder_id) DO UPDATE SET last_scanned=excluded.last_scanned, status=excluded.status",
            params![folder_id, now(), status.as_str()],
        )?;
        Ok(())
    }
}

/// Map a photo row (13 columns, in schema order) to a `Photo`.
pub(super) fn map_photo(r: &rusqlite::Row) -> rusqlite::Result<Photo> {
    Ok(Photo {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        path: r.get(2)?,
        filename: r.get(3)?,
        size: r.get(4)?,
        mod_time: r.get(5)?,
        taken_at: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        hash: r.get(9)?,
        thumb_ready: r.get::<_, i64>(10)? != 0,
        orientation: r.get(11)?,
        ai_status: crate::model::AiStatus::from_i64(r.get::<_, i64>(12)?),
    })
}
