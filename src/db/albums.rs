//! Album tree CRUD and folder↔album membership.

use rusqlite::{params, OptionalExtension};

use crate::model::Album;

use super::{Library, Result};

/// A recursive CTE `album_kind(id, k)` with the effective face kind of every
/// album. An explicit Photo (1) or Art (2) kind wins. An Inherit (0) album
/// takes the kind of its parent. A top-level Inherit album is Photo (1). This
/// gives the same result as `album_effective_kind`.
const ALBUM_KIND_CTE: &str = "WITH RECURSIVE album_kind(id, k) AS ( \
     SELECT id, CASE WHEN kind IN (1, 2) THEN kind ELSE 1 END \
       FROM albums WHERE parent_id IS NULL \
     UNION ALL \
     SELECT a.id, CASE WHEN a.kind IN (1, 2) THEN a.kind ELSE ak.k END \
       FROM albums a JOIN album_kind ak ON a.parent_id = ak.id)";

/// The folder ids with the effective kind Art. Use it after `ALBUM_KIND_CTE`.
const ART_FOLDERS_SQL: &str = "SELECT af.folder_id FROM album_folders af \
     JOIN album_kind ak ON ak.id = af.album_id WHERE ak.k = 2";

impl Library {
    /// Insert a new album. `parent_id` of 0 creates a top-level album.
    pub fn create_album(&self, name: &str, parent_id: i64) -> Result<i64> {
        let conn = self.lock();
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
        let parent: Option<i64> = if parent_id == 0 {
            None
        } else {
            Some(parent_id)
        };
        conn.execute(
            "INSERT INTO albums(name, parent_id, position) VALUES(?1, ?2, ?3)",
            params![name, parent, pos + 1],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Change an album's display name.
    pub fn rename_album(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE albums SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    /// Remove an album. Sub-albums cascade; member folders revert to the Library
    /// root ("New folders").
    pub fn delete_album(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM albums WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Re-parent an album. `parent_id` of 0 makes it top-level. Refuses to
    /// create a cycle (making an album a descendant of itself).
    pub fn set_album_parent(&self, id: i64, parent_id: i64) -> Result<()> {
        if id == parent_id {
            return Ok(());
        }
        let conn = self.lock();
        // Walk up from the proposed parent; if we reach id, this would create a
        // cycle, so refuse.
        let mut cur = parent_id;
        while cur != 0 {
            if cur == id {
                return Ok(()); // would create a cycle; ignore
            }
            let next: Option<i64> = conn
                .query_row(
                    "SELECT parent_id FROM albums WHERE id = ?1",
                    params![cur],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            cur = next.unwrap_or(0);
        }
        let parent: Option<i64> = if parent_id == 0 {
            None
        } else {
            Some(parent_id)
        };
        conn.execute(
            "UPDATE albums SET parent_id = ?1 WHERE id = ?2",
            params![parent, id],
        )?;
        Ok(())
    }

    /// All albums ordered by position then name.
    pub fn albums(&self) -> Result<Vec<Album>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, COALESCE(parent_id, 0), position, kind, squashed
             FROM albums ORDER BY position ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Album {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                position: r.get(3)?,
                kind: crate::model::AlbumKind::from_i64(r.get::<_, i64>(4)?),
                squashed: r.get::<_, i64>(5)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Set or clear the squashed flag of an album.
    pub fn set_album_squashed(&self, id: i64, squashed: bool) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE albums SET squashed = ?1 WHERE id = ?2",
            params![squashed as i64, id],
        )?;
        Ok(())
    }

    /// Set an album's face-recognition kind (0 inherit, 1 Photo, 2 Art).
    pub fn set_album_kind(&self, id: i64, kind: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE albums SET kind = ?1 WHERE id = ?2",
            params![kind, id],
        )?;
        Ok(())
    }

    /// The effective face-recognition kind of an album: 1 = Photo, 2 = Art.
    /// Walks up the parent chain. An explicit Photo/Art wins. If every ancestor
    /// is Inherit (0), the default is Photo (1).
    pub fn album_effective_kind(&self, album_id: i64) -> Result<i64> {
        let conn = self.read_lock();
        let mut cur = album_id;
        while cur != 0 {
            let row: Option<(i64, Option<i64>)> = conn
                .query_row(
                    "SELECT kind, parent_id FROM albums WHERE id = ?1",
                    params![cur],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let Some((kind, parent)) = row else { break };
            if kind == 1 || kind == 2 {
                return Ok(kind);
            }
            cur = parent.unwrap_or(0);
        }
        Ok(1)
    }

    /// Used by the tests and the benchmarks only.
    #[cfg(test)]
    /// The album id a photo belongs to through its folder, or 0 when the photo's
    /// folder is in no album.
    pub fn album_of_photo(&self, photo_id: i64) -> Result<i64> {
        let conn = self.lock();
        let aid: Option<i64> = conn
            .query_row(
                "SELECT af.album_id FROM photos p \
                 JOIN album_folders af ON af.folder_id = p.folder_id \
                 WHERE p.id = ?1",
                params![photo_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(aid.unwrap_or(0))
    }

    /// Used by the tests and the benchmarks only.
    #[cfg(test)]
    /// The effective face kind for one photo: 1 = Photo, 2 = Art. A photo whose
    /// folder is in no album defaults to Photo.
    pub fn photo_effective_face_kind(&self, photo_id: i64) -> Result<i64> {
        let aid = self.album_of_photo(photo_id)?;
        if aid == 0 {
            return Ok(1);
        }
        self.album_effective_kind(aid)
    }

    /// Used by the tests and the benchmarks only.
    #[cfg(test)]
    /// The effective face kind for a folder: the effective kind of the album
    /// it belongs to (Inherit resolved up the chain), or Photo (1) if the
    /// folder is in no album. Routes a folder-scoped face scan to the right
    /// pipeline when the folder isn't reached through an album's own scan
    /// action.
    pub fn folder_effective_face_kind(&self, folder_id: i64) -> Result<i64> {
        let aid: Option<i64> = {
            let conn = self.lock();
            conn.query_row(
                "SELECT album_id FROM album_folders WHERE folder_id = ?1",
                params![folder_id],
                |r| r.get(0),
            )
            .optional()?
        };
        match aid {
            Some(aid) => self.album_effective_kind(aid),
            None => Ok(1),
        }
    }

    /// The ids of the folders whose effective face kind is Art (2). A folder
    /// that is not in the set is Photo (1). One query resolves the kind of
    /// every album.
    pub fn art_folder_ids(&self) -> Result<std::collections::HashSet<i64>> {
        let conn = self.read_lock();
        let sql = format!("{ALBUM_KIND_CTE} {ART_FOLDERS_SQL}");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Photo ids that still need a face pass, routed by the effective face
    /// kind of their album. `style` selects the stylised scan table. `art`
    /// selects the Art photos (kind 2). Otherwise it selects the Photo photos
    /// (kind 1). The newest photos come first.
    pub fn photos_needing_scan_of_kind(
        &self,
        style: bool,
        art: bool,
        limit: i64,
    ) -> Result<Vec<i64>> {
        let scan = if style {
            "style_face_scan"
        } else {
            "face_scan"
        };
        let op = if art { "IN" } else { "NOT IN" };
        let sql = format!(
            "{ALBUM_KIND_CTE} \
             SELECT p.id FROM photos p \
             LEFT JOIN {scan} fs ON fs.photo_id = p.id \
             WHERE p.scan_state = 2 AND p.missing = 0 \
               AND (fs.state IS NULL OR fs.state <> 2) \
               AND p.folder_id {op} ({ART_FOLDERS_SQL}) \
             ORDER BY p.added_at DESC LIMIT ?1"
        );
        let conn = self.read_lock();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit], |r| r.get::<_, i64>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// All folder ids under an album and its sub-albums (the album subtree).
    pub fn folders_under_album(&self, album_id: i64) -> Result<Vec<i64>> {
        let conn = self.read_lock();
        // Build parent -> children from the album list.
        let mut children: std::collections::HashMap<i64, Vec<i64>> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT id, COALESCE(parent_id, 0) FROM albums")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (id, parent) = row?;
                children.entry(parent).or_default().push(id);
            }
        }
        // Collect the subtree album ids (breadth-first from album_id).
        let mut subtree = Vec::new();
        let mut stack = vec![album_id];
        while let Some(a) = stack.pop() {
            subtree.push(a);
            if let Some(kids) = children.get(&a) {
                stack.extend(kids.iter().copied());
            }
        }
        // Gather folder ids for every album in the subtree.
        let mut folders = Vec::new();
        let mut stmt = conn.prepare("SELECT folder_id FROM album_folders WHERE album_id = ?1")?;
        for a in subtree {
            let rows = stmt.query_map(params![a], |r| r.get::<_, i64>(0))?;
            for row in rows {
                folders.push(row?);
            }
        }
        Ok(folders)
    }

    /// Place a folder into an album, removing it from any other album first (a
    /// folder belongs to at most one album in the tree).
    pub fn add_folder_to_album(&self, folder_id: i64, album_id: i64) -> Result<()> {
        let mut conn = self.lock();
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
        let conn = self.lock();
        conn.execute(
            "DELETE FROM album_folders WHERE folder_id = ?1",
            params![folder_id],
        )?;
        Ok(())
    }

    /// A map of folder id to the album id it belongs to. Folders not in any
    /// album are absent from the map.
    pub fn folder_albums(&self) -> Result<std::collections::HashMap<i64, i64>> {
        let conn = self.read_lock();
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
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.folder_id, p.path, p.filename, p.size, p.mod_time, p.taken_at, \
                    p.width, p.height, p.hash, p.thumb_ready, p.orientation, p.ai_status, \
                    p.scan_state, p.missing, p.added_at, p.phash, p.skip_face_scan \
             FROM photos p \
             JOIN album_folders af ON af.folder_id = p.folder_id \
             WHERE af.album_id = ?1 AND p.missing = 0 \
             ORDER BY p.taken_at ASC, p.filename ASC",
        )?;
        let rows = stmt.query_map([album_id], super::library::map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every photo in an album and in all its sub-albums, at all depths.
    /// Missing photos are not included. Ordered by taken date then filename.
    /// The grid uses this for a squashed album.
    pub fn photos_in_album_tree(&self, album_id: i64) -> Result<Vec<crate::model::Photo>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "WITH RECURSIVE tree(id) AS ( \
                 SELECT ?1 \
                 UNION SELECT a.id FROM albums a JOIN tree t ON a.parent_id = t.id \
             ) \
             SELECT DISTINCT p.id, p.folder_id, p.path, p.filename, p.size, p.mod_time, \
                    p.taken_at, p.width, p.height, p.hash, p.thumb_ready, p.orientation, \
                    p.ai_status, p.scan_state, p.missing, p.added_at, p.phash, p.skip_face_scan \
             FROM photos p \
             JOIN album_folders af ON af.folder_id = p.folder_id \
             WHERE af.album_id IN (SELECT id FROM tree) AND p.missing = 0 \
             ORDER BY p.taken_at ASC, p.filename ASC",
        )?;
        let rows = stmt.query_map([album_id], super::library::map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The albums that `delete_empty_albums` removes with the same flags.
    ///
    /// An album is "emptied" when its subtree has folders but no photos. An
    /// album is "never filled" when its subtree has no folders. An album
    /// with photos in its subtree is never selected. A parent is selected only
    /// when all of its sub-albums are selected too.
    pub fn empty_albums(&self, emptied: bool, never_filled: bool) -> Result<Vec<i64>> {
        use std::collections::HashMap;
        let (albums, mut own) = {
            let conn = self.read_lock();
            let mut stmt = conn.prepare("SELECT id, COALESCE(parent_id, 0) FROM albums")?;
            let albums: Vec<(i64, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            // Direct folder count and photo count per album.
            let mut stmt = conn.prepare(
                "SELECT af.album_id, COUNT(DISTINCT af.folder_id), \
                    COUNT(p.id) \
             FROM album_folders af LEFT JOIN photos p ON p.folder_id = af.folder_id \
             GROUP BY af.album_id",
            )?;
            let own: HashMap<i64, (i64, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
                .collect::<rusqlite::Result<_>>()?;
            (albums, own)
        };

        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(id, parent) in &albums {
            children.entry(parent).or_default().push(id);
        }
        // Post-order walk from the roots. Returns (folders, photos, all_selected).
        fn walk(
            id: i64,
            children: &HashMap<i64, Vec<i64>>,
            own: &mut HashMap<i64, (i64, i64)>,
            flags: (bool, bool),
            out: &mut Vec<i64>,
        ) -> (i64, i64, bool) {
            let (mut f, mut p) = own.remove(&id).unwrap_or((0, 0));
            let mut kids_ok = true;
            for &c in children.get(&id).map(|v| v.as_slice()).unwrap_or(&[]) {
                let (cf, cp, ok) = walk(c, children, own, flags, out);
                f += cf;
                p += cp;
                kids_ok &= ok;
            }
            let pick = kids_ok && p == 0 && ((f > 0 && flags.0) || (f == 0 && flags.1));
            if pick {
                out.push(id);
            }
            (f, p, pick)
        }
        let mut out = Vec::new();
        for &root in children.get(&0).cloned().unwrap_or_default().iter() {
            walk(root, &children, &mut own, (emptied, never_filled), &mut out);
        }
        Ok(out)
    }

    /// Delete the empty albums that `empty_albums` selects. Folder rows that
    /// belong to a deleted album and have no photos are deleted too, so they
    /// do not show under "New folders". Returns the number of albums deleted.
    pub fn delete_empty_albums(&self, emptied: bool, never_filled: bool) -> Result<usize> {
        let ids = self.empty_albums(emptied, never_filled)?;
        if ids.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for id in &ids {
            tx.execute(
                "DELETE FROM folders WHERE id IN \
                   (SELECT folder_id FROM album_folders WHERE album_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM photos p WHERE p.folder_id = folders.id)",
                params![id],
            )?;
        }
        for id in &ids {
            tx.execute("DELETE FROM albums WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len())
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

    #[test]
    fn squashed_album_shows_photos_of_all_sub_albums() {
        let (lib, path) = temp_lib();
        let root = "/tmp/pichouse-squashtest-root";
        lib.add_library_folder(root).unwrap();
        let top = lib.create_album("top", 0).unwrap();
        let mid = lib.create_album("mid", top).unwrap();
        let leaf = lib.create_album("leaf", mid).unwrap();
        let other = lib.create_album("other", 0).unwrap();
        for (i, aid) in [top, mid, leaf, other].into_iter().enumerate() {
            let fid = lib
                .upsert_folder(&Folder {
                    path: format!("{root}/f{i}"),
                    name: format!("f{i}"),
                    ..Default::default()
                })
                .unwrap();
            lib.upsert_photo(&Photo {
                folder_id: fid,
                path: format!("{root}/f{i}/p{i}.jpg"),
                filename: format!("p{i}.jpg"),
                ..Default::default()
            })
            .unwrap();
            lib.add_folder_to_album(fid, aid).unwrap();
        }
        assert_eq!(lib.photos_in_album_tree(top).unwrap().len(), 3);
        assert_eq!(lib.photos_in_album_tree(mid).unwrap().len(), 2);
        assert_eq!(lib.photos_in_album_tree(leaf).unwrap().len(), 1);

        lib.set_album_squashed(top, true).unwrap();
        let a = lib.albums().unwrap();
        assert!(a.iter().find(|x| x.id == top).unwrap().squashed);
        assert!(!a.iter().find(|x| x.id == mid).unwrap().squashed);
        lib.set_album_squashed(top, false).unwrap();
        assert!(!lib.albums().unwrap().iter().any(|x| x.squashed));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn album_kind_inherits_down_the_tree() {
        let (lib, path) = temp_lib();
        // root(Art) -> mid(Inherit) -> leaf(Inherit). Leaf resolves to Art.
        let root = lib.create_album("root", 0).unwrap();
        let mid = lib.create_album("mid", root).unwrap();
        let leaf = lib.create_album("leaf", mid).unwrap();
        // Default is Inherit -> root resolves to Photo (1).
        assert_eq!(lib.album_effective_kind(leaf).unwrap(), 1);
        lib.set_album_kind(root, 2).unwrap();
        assert_eq!(lib.album_effective_kind(root).unwrap(), 2);
        assert_eq!(lib.album_effective_kind(mid).unwrap(), 2);
        assert_eq!(lib.album_effective_kind(leaf).unwrap(), 2);
        // An explicit Photo on mid overrides the inherited Art for mid + leaf.
        lib.set_album_kind(mid, 1).unwrap();
        assert_eq!(lib.album_effective_kind(mid).unwrap(), 1);
        assert_eq!(lib.album_effective_kind(leaf).unwrap(), 1);
        // root is still Art.
        assert_eq!(lib.album_effective_kind(root).unwrap(), 2);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn folder_effective_face_kind_follows_its_album_or_defaults_to_photo() {
        let (lib, path) = temp_lib();
        let rootdir = "/tmp/pichouse-folder-kind-root";
        lib.add_library_folder(rootdir).unwrap();
        let mk = |name: &str| -> i64 {
            lib.upsert_folder(&Folder {
                path: format!("{rootdir}/{name}"),
                name: name.into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap()
        };
        let f_art = mk("art");
        let f_unassigned = mk("unassigned");

        // A folder in no album defaults to Photo (1).
        assert_eq!(lib.folder_effective_face_kind(f_unassigned).unwrap(), 1);

        let album = lib.create_album("characters", 0).unwrap();
        lib.set_album_kind(album, 2).unwrap();
        lib.add_folder_to_album(f_art, album).unwrap();
        assert_eq!(lib.folder_effective_face_kind(f_art).unwrap(), 2);
        // Unrelated folders are unaffected.
        assert_eq!(lib.folder_effective_face_kind(f_unassigned).unwrap(), 1);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn folders_under_album_covers_subtree() {
        let (lib, path) = temp_lib();
        let rootdir = "/tmp/pichouse-albkind-root";
        lib.add_library_folder(rootdir).unwrap();
        let mk = |name: &str| -> i64 {
            lib.upsert_folder(&Folder {
                path: format!("{rootdir}/{name}"),
                name: name.into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap()
        };
        let f_root = mk("a");
        let f_sub = mk("b");
        let root = lib.create_album("root", 0).unwrap();
        let sub = lib.create_album("sub", root).unwrap();
        lib.add_folder_to_album(f_root, root).unwrap();
        lib.add_folder_to_album(f_sub, sub).unwrap();
        let mut got = lib.folders_under_album(root).unwrap();
        got.sort();
        let mut want = vec![f_root, f_sub];
        want.sort();
        assert_eq!(got, want);
        // The sub album alone yields only its own folder.
        assert_eq!(lib.folders_under_album(sub).unwrap(), vec![f_sub]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn empty_albums_by_kind() {
        let (lib, path) = temp_lib();
        let root = "/tmp/pichouse-emptyalb-root";
        lib.add_library_folder(root).unwrap();
        let mk = |name: &str, photo: bool| {
            let fid = lib
                .upsert_folder(&Folder {
                    path: format!("{root}/{name}"),
                    name: name.into(),
                    ..Default::default()
                })
                .unwrap();
            if photo {
                lib.upsert_photo(&Photo {
                    folder_id: fid,
                    path: format!("{root}/{name}/a.jpg"),
                    filename: "a.jpg".into(),
                    ..Default::default()
                })
                .unwrap();
            }
            fid
        };
        // top (no folders) > [emptied (empty folder), never (nothing)]
        let top = lib.create_album("top", 0).unwrap();
        let emptied = lib.create_album("emptied", top).unwrap();
        let never = lib.create_album("never", top).unwrap();
        let ef = mk("e", false);
        lib.add_folder_to_album(ef, emptied).unwrap();
        // full > [blank]: full keeps photos, blank is never filled.
        let full = lib.create_album("full", 0).unwrap();
        let blank = lib.create_album("blank", full).unwrap();
        lib.add_folder_to_album(mk("f", true), full).unwrap();

        assert_eq!(lib.empty_albums(true, false).unwrap(), vec![emptied]);
        let mut n = lib.empty_albums(false, true).unwrap();
        n.sort();
        assert_eq!(n, {
            let mut v = vec![never, blank];
            v.sort();
            v
        });
        // Both: top cascades since all of its children go.
        let mut both = lib.empty_albums(true, true).unwrap();
        both.sort();
        let mut want = vec![top, emptied, never, blank];
        want.sort();
        assert_eq!(both, want);

        assert_eq!(lib.delete_empty_albums(true, true).unwrap(), 4);
        let left: Vec<i64> = lib.albums().unwrap().iter().map(|a| a.id).collect();
        assert_eq!(left, vec![full]);
        assert!(lib.folders().unwrap().iter().all(|f| f.id != ef));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
