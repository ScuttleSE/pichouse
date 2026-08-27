//! SQLite-backed storage for pichouse.
//!
//! Two database files: `library.db` (metadata) and per-size `thumbs-<N>.db`
//! (thumbnail blobs).

mod albums;
mod config;
mod library;
mod tags;
mod thumbs;
mod virtual_albums;

pub use config::{data_dir, write_configured_data_dir};
pub use library::Library;
pub use thumbs::{remove_all_thumb_databases, Thumbs};

/// A database error: either a SQLite error or an I/O error resolving paths.
#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "sqlite: {e}"),
            Error::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// A database result.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AiStatus, Folder, Photo, TagSource};

    fn temp_library() -> (tempdir::TempPath, Library) {
        // Use a unique temp file path without an external crate.
        let mut p = std::env::temp_dir();
        let unique = format!(
            "pichouse-test-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        p.push(unique);
        let lib = Library::open_at(&p).unwrap();
        (tempdir::TempPath(p), lib)
    }

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    // Minimal RAII temp-file cleanup, avoiding an external tempfile dependency.
    mod tempdir {
        use std::path::PathBuf;
        pub struct TempPath(pub PathBuf);
        impl Drop for TempPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
                // WAL/SHM sidecars.
                let _ = std::fs::remove_file(self.0.with_extension("db-wal"));
                let _ = std::fs::remove_file(self.0.with_extension("db-shm"));
            }
        }
    }

    fn new_test_photo(l: &Library) -> i64 {
        let fid = l
            .upsert_folder(&Folder {
                path: "/tmp/f".into(),
                name: "f".into(),
                mtime: 1,
                year: 2024,
                ..Default::default()
            })
            .unwrap();
        l.upsert_photo(&Photo {
            folder_id: fid,
            path: "/tmp/f/a.jpg".into(),
            filename: "a.jpg".into(),
            mod_time: 1,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn library_folder_roundtrip() {
        let (_g, l) = temp_library();
        let lf = l.add_library_folder("/photos").unwrap();
        assert_eq!(lf.path, "/photos");
        // Idempotent.
        let lf2 = l.add_library_folder("/photos").unwrap();
        assert_eq!(lf.id, lf2.id);
        assert_eq!(l.library_folders().unwrap().len(), 1);
    }

    #[test]
    fn tags_add_search() {
        let (_g, l) = temp_library();
        let pid = new_test_photo(&l);
        l.add_photo_tags(
            pid,
            &["Beach".into(), "sunset".into(), "dog".into()],
            TagSource::Ai,
        )
        .unwrap();
        l.add_photo_tags(pid, &["vacation".into()], TagSource::User)
            .unwrap();
        assert_eq!(l.photo_tags(pid).unwrap().len(), 4);
        assert!(l.search_photo_ids_by_tag("dog").unwrap().contains(&pid));
        // Prefix.
        assert!(l.search_photo_ids_by_tag("sun").unwrap().contains(&pid));
    }

    #[test]
    fn tags_rename_merge_delete() {
        let (_g, l) = temp_library();
        let pid = new_test_photo(&l);
        l.add_photo_tags(pid, &["beach".into(), "dog".into()], TagSource::Ai)
            .unwrap();

        l.rename_tag("dog", "cat").unwrap();
        assert!(l.search_photo_ids_by_tag("cat").unwrap().contains(&pid));
        assert!(!l.search_photo_ids_by_tag("dog").unwrap().contains(&pid));

        l.merge_tags("cat", "beach").unwrap();
        assert_eq!(l.photo_tags(pid).unwrap().len(), 1);

        l.remove_photo_tag(pid, "beach").unwrap();
        assert!(!l.search_photo_ids_by_tag("beach").unwrap().contains(&pid));
    }

    #[test]
    fn ai_status_and_needing() {
        let (_g, l) = temp_library();
        let pid = new_test_photo(&l);
        assert_eq!(l.photos_needing_tags(0, false).unwrap().len(), 1);
        l.set_ai_status(pid, AiStatus::Done).unwrap();
        assert_eq!(l.photos_needing_tags(0, false).unwrap().len(), 0);
    }

    #[test]
    fn albums_membership() {
        let (_g, l) = temp_library();
        let fid = l
            .upsert_folder(&Folder {
                path: "/tmp/g".into(),
                name: "g".into(),
                mtime: 1,
                year: 2024,
                ..Default::default()
            })
            .unwrap();
        let a = l.create_album("Trips", 0).unwrap();
        let sub = l.create_album("2024", a).unwrap();
        l.add_folder_to_album(fid, sub).unwrap();
        let fa = l.folder_albums().unwrap();
        assert_eq!(fa.get(&fid), Some(&sub));
        // Cycle prevention: making the parent a child of its descendant is ignored.
        l.set_album_parent(a, sub).unwrap();
        let albums = l.albums().unwrap();
        let parent_of_a = albums.iter().find(|x| x.id == a).unwrap().parent_id;
        assert_eq!(parent_of_a, 0);

        l.remove_folder_from_album(fid).unwrap();
        assert!(l.folder_albums().unwrap().is_empty());
    }

    #[test]
    fn thumbs_roundtrip() {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-thumbs-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _g = tempdir::TempPath(p.clone());
        let t = Thumbs::open_at(&p).unwrap();
        assert!(t.get("abc").unwrap().is_none());
        t.put("abc", 320, &[1, 2, 3]).unwrap();
        assert_eq!(t.get("abc").unwrap().unwrap(), vec![1, 2, 3]);
        t.delete("abc").unwrap();
        assert!(t.get("abc").unwrap().is_none());
    }
}
