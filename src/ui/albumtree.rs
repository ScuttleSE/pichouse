//! Auto-organize scanned folders into the Library album tree, mirroring the
//! on-disk directory hierarchy.
//!
//! After a library root is scanned, each scanned folder beneath it is placed
//! into an album chain that matches its directory path relative to the root.
//! The root's basename becomes a top-level album. Intermediate directories
//! become nested sub-albums. This runs once per scan; albums remain fully
//! user-editable afterward, and existing album membership is never overwritten.

use std::collections::HashMap;
use std::path::Path;

use crate::db::Library;

/// Mirror the on-disk directory tree under `root` into the album tree. Only
/// folders not already assigned to an album are placed, so user edits persist.
pub fn sync_disk_tree(lib: &Library, root: &str) {
    let folders = match lib.folders() {
        Ok(f) => f,
        Err(_) => return,
    };
    let albums = lib.albums().unwrap_or_default();
    let folder_album = lib.folder_albums().unwrap_or_default();

    // Index existing albums by (parent_id, name) so we reuse rather than
    // duplicate on rescans.
    let mut album_by_key: HashMap<(i64, String), i64> = HashMap::new();
    for a in &albums {
        album_by_key.insert((a.parent_id, a.name.clone()), a.id);
    }

    let root_path = Path::new(root);
    let root_name = root_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string());

    for folder in &folders {
        // Only handle folders under this root.
        let fpath = Path::new(&folder.path);
        let Ok(rel) = fpath.strip_prefix(root_path) else {
            continue;
        };
        // Skip folders the user already filed somewhere.
        if folder_album.contains_key(&folder.id) {
            continue;
        }

        // Build the album chain: root name, then each intermediate directory
        // component of the relative path (excluding the folder's own leaf, since
        // the folder itself is the content, not an album).
        let mut chain: Vec<String> = vec![root_name.clone()];
        let comps: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        // All but the last component are intermediate album levels. If the
        // folder IS the root (rel is empty), it just goes under the root album.
        if comps.len() > 1 {
            chain.extend_from_slice(&comps[..comps.len() - 1]);
        }

        // Resolve/create the album chain.
        let mut parent_id = 0i64;
        for name in &chain {
            let key = (parent_id, name.clone());
            let aid = match album_by_key.get(&key) {
                Some(&id) => id,
                None => match lib.create_album(name, parent_id) {
                    Ok(id) => {
                        album_by_key.insert(key, id);
                        id
                    }
                    Err(_) => {
                        parent_id = 0;
                        break;
                    }
                },
            };
            parent_id = aid;
        }

        if parent_id != 0 {
            let _ = lib.add_folder_to_album(folder.id, parent_id);
        }
    }
}
