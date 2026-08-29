//! Auto-organize scanned folders into the Library album tree, mirroring the
//! on-disk directory hierarchy.
//!
//! After (and during) a scan, each scanned folder beneath a library root is
//! placed into an album chain matching its directory path relative to the root.
//! The root's basename becomes a top-level album; intermediate directories
//! become nested sub-albums. Folders already filed in an album are never moved,
//! so user edits persist.

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
