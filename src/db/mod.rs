//! SQLite-backed storage for pichouse.
//!
//! Two database files: `library.db` (metadata) and per-size `thumbs-<N>.db`
//! (thumbnail blobs).

mod albums;
mod config;
mod duplicates;
mod edits;
mod face_thumbs;
mod faces;
mod immich;
mod immich_thumbs;
mod library;
mod presets;
mod style_face_thumbs;
mod style_faces;
mod tags;
mod thumbs;
mod virtual_albums;

pub use config::{data_dir, write_configured_data_dir};
pub use face_thumbs::{remove_face_thumbs_database, FaceThumbs};
pub use immich_thumbs::{
    remove_all_immich_thumb_databases, remove_immich_thumbs_for_server, ImmichThumbs,
};
pub use library::Library;
pub use style_face_thumbs::{open_style_face_thumbs, remove_style_face_thumbs_database};
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

    #[test]
    fn photo_edit_roundtrip_and_identity() {
        use crate::model::PhotoEdit;
        let (_g, l) = temp_library();
        let pid = new_test_photo(&l);
        // No row yet -> identity edit with the right id.
        let e = l.photo_edit(pid).unwrap();
        assert!(e.is_identity());
        assert_eq!(e.photo_id, pid);
        // Save a real edit; edit_rev increments from 1.
        let mut edit = PhotoEdit {
            photo_id: pid,
            brightness: 20,
            ..Default::default()
        };
        edit.levels.r_black = 15;
        let rev = l.set_photo_edit(&edit).unwrap();
        assert_eq!(rev, 1);
        let got = l.photo_edit(pid).unwrap();
        assert_eq!(got.brightness, 20);
        assert_eq!(got.levels.r_black, 15);
        assert_eq!(got.edit_rev, 1);
        // Saving again bumps the revision.
        let rev2 = l.set_photo_edit(&got).unwrap();
        assert_eq!(rev2, 2);
        // Saving the identity edit removes the row.
        let ident = PhotoEdit {
            photo_id: pid,
            ..Default::default()
        };
        l.set_photo_edit(&ident).unwrap();
        assert!(l.photo_edit(pid).unwrap().is_identity());
    }

    #[test]
    fn level_preset_roundtrip() {
        use crate::model::Levels;
        let (_g, l) = temp_library();
        let mut lv = Levels::default();
        lv.b_black = 40;
        let id = l.save_level_preset("Kodak Gold", &lv).unwrap();
        let presets = l.level_presets().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Kodak Gold");
        assert_eq!(presets[0].levels.b_black, 40);
        // Overwrite by name keeps a single row.
        lv.b_black = 55;
        l.save_level_preset("Kodak Gold", &lv).unwrap();
        assert_eq!(l.level_presets().unwrap().len(), 1);
        assert_eq!(l.level_presets().unwrap()[0].levels.b_black, 55);
        l.delete_level_preset(id).unwrap();
        assert!(l.level_presets().unwrap().is_empty());
    }
}
