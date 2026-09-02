//! Auto-organize scanned folders into the Library album tree, mirroring the
//! on-disk directory hierarchy.
//!
//! During a root's FIRST scan, each scanned folder beneath the root is placed
//! into an album chain matching its directory path relative to the root. The
//! root's basename becomes a top-level album; intermediate directories become
//! nested sub-albums. Folders already filed in an album are never moved, so
//! user edits persist. After the first scan, newly discovered folders are NOT
//! auto-filed: they stay unassigned and surface under the sidebar's
//! "New folders" section for manual filing (the scan worker gates this — see
//! `ui::actions`).

use std::collections::HashMap;
use std::path::Path;

use crate::db::Library;
use crate::model::Folder;

/// Mirrors on-disk directory structure into the album tree, reusing a cache of
/// existing albums so repeated calls during a scan stay cheap. Build one per
/// root (or rebuild when albums may have changed) and file folders into it.
pub struct DiskAlbumMapper {
    /// (parent_album_id, album_name) -> album_id.
    album_by_key: HashMap<(i64, String), i64>,
    /// folder_id -> album_id, for folders already assigned to some album.
    folder_album: HashMap<i64, i64>,
}

impl DiskAlbumMapper {
    /// Build a mapper from the library's current albums and folder membership.
    pub fn new(lib: &Library) -> DiskAlbumMapper {
        let albums = lib.albums().unwrap_or_default();
        let folder_album = lib.folder_albums().unwrap_or_default();
        let mut album_by_key = HashMap::new();
        for a in &albums {
            album_by_key.insert((a.parent_id, a.name.clone()), a.id);
        }
        DiskAlbumMapper {
            album_by_key,
            folder_album,
        }
    }

    /// File a single scanned `folder` into its disk-mirrored album under `root`.
    /// Creates any missing album levels. No-op if the folder is already in an
    /// album (preserving user placements) or is not under `root`.
    pub fn file(&mut self, lib: &Library, root: &str, folder: &Folder) {
        if self.folder_album.contains_key(&folder.id) {
            return;
        }
        let root_path = Path::new(root);
        let fpath = Path::new(&folder.path);
        let Ok(rel) = fpath.strip_prefix(root_path) else {
            return;
        };
        let root_name = root_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string());

        // Album chain: root name, then each intermediate directory component of
        // the relative path (excluding the folder's own leaf — the folder itself
        // is the content, not an album).
        let mut chain: Vec<String> = vec![root_name];
        let comps: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        if comps.len() > 1 {
            chain.extend_from_slice(&comps[..comps.len() - 1]);
        }

        // Resolve/create the album chain.
        let mut parent_id = 0i64;
        for name in &chain {
            let key = (parent_id, name.clone());
            let aid = match self.album_by_key.get(&key) {
                Some(&id) => id,
                None => match lib.create_album(name, parent_id) {
                    Ok(id) => {
                        self.album_by_key.insert(key, id);
                        id
                    }
                    Err(_) => return,
                },
            };
            parent_id = aid;
        }

        if parent_id != 0 && lib.add_folder_to_album(folder.id, parent_id).is_ok() {
            self.folder_album.insert(folder.id, parent_id);
        }
    }

    /// Resolve an album by `(parent_id, name)`, creating it when missing, and
    /// cache the result. `None` on a DB error.
    pub fn ensure_album(&mut self, lib: &Library, parent_id: i64, name: &str) -> Option<i64> {
        let key = (parent_id, name.to_string());
        match self.album_by_key.get(&key) {
            Some(&id) => Some(id),
            None => match lib.create_album(name, parent_id) {
                Ok(id) => {
                    self.album_by_key.insert(key, id);
                    Some(id)
                }
                Err(_) => None,
            },
        }
    }

    /// Record that `folder_id` is now filed under `album_id`, so later calls in
    /// this mapper skip it.
    pub fn note_filed(&mut self, folder_id: i64, album_id: i64) {
        self.folder_album.insert(folder_id, album_id);
    }
}

/// Mirror the on-disk directory tree under `root` into the album tree in one
/// pass. A convenience wrapper over `DiskAlbumMapper` for the completion sweep
/// and reconcile paths. Only folders not already filed are placed.
pub fn sync_disk_tree(lib: &Library, root: &str) {
    let folders = match lib.folders() {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut mapper = DiskAlbumMapper::new(lib);
    for folder in &folders {
        mapper.file(lib, root, folder);
    }
}

/// File a dropped "New folders" subtree into `target_album`, preserving the
/// on-disk nesting relative to `base`.
///
/// First an album named after `base`'s basename is resolved or created under
/// `target_album` (so dropping a group `vacation` creates a sub-album
/// `vacation`). Then each folder in `folders` — `(id, path)` pairs, all at or
/// beneath `base` — is filed into the album chain that mirrors its path
/// relative to `base`. A folder directly at `base` files into the base album.
/// Already-filed folders are skipped, and user placements are never moved.
pub fn file_subtree_under_album(
    lib: &Library,
    base: &str,
    folders: &[(i64, String)],
    target_album: i64,
) {
    let base = base.trim_end_matches(std::path::MAIN_SEPARATOR);
    let base_name = std::path::Path::new(base)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| base.to_string());
    let mut mapper = DiskAlbumMapper::new(lib);
    // Resolve or create the base album under the drop target.
    let Some(base_album) = mapper.ensure_album(lib, target_album, &base_name) else {
        return;
    };
    for (fid, path) in folders {
        let p = path.trim_end_matches(std::path::MAIN_SEPARATOR);
        let rel = match std::path::Path::new(p).strip_prefix(base) {
            Ok(r) => r,
            Err(_) => continue, // not under base; skip
        };
        // Album chain under the base album: the intermediate directories of
        // the relative path (excluding the folder's own leaf).
        let comps: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let mut parent = base_album;
        let mut ok = true;
        for name in comps.iter().take(comps.len().saturating_sub(1)) {
            match mapper.ensure_album(lib, parent, name) {
                Some(id) => parent = id,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        if lib.add_folder_to_album(*fid, parent).is_ok() {
            mapper.note_filed(*fid, parent);
        }
    }
}
