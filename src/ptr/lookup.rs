//! Read-only tag lookup in `ptr.db` by the file SHA-256.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

/// The options of a lookup.
#[derive(Clone, Debug, Default)]
pub struct LookupOptions {
    /// Leave out tags in these namespaces (lower case, no colon).
    pub exclude_namespaces: Vec<String>,
    /// Add the parent tags of each tag.
    pub add_parents: bool,
}

pub struct PtrReader {
    conn: Connection,
}

/// The longest sibling chain or parent depth that the lookup follows. A
/// longer chain or a cycle stops here.
const MAX_DEPTH: usize = 16;

impl PtrReader {
    /// Open `ptr.db` read-only. Returns an error when the file is missing or
    /// has no lookup index (the initial sync did not finish).
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err(format!("{} does not exist", path.display()));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|e| e.to_string())?;
        let has_index: Option<i64> = conn
            .query_row("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'hashes_sha256'", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if has_index.is_none() {
            return Err("the database has no lookup index. Run ptr-sync to the end.".into());
        }
        Ok(Self { conn })
    }

    /// The tags of the file with the SHA-256 `hex`. Siblings are resolved.
    /// The result is sorted and has no duplicates. Unknown hashes give an
    /// empty list.
    pub fn tags_for_sha256(&self, hex: &str, opts: &LookupOptions) -> Result<Vec<String>, String> {
        let Some(bytes) = hex_to_bytes(hex) else { return Ok(Vec::new()) };
        let ids: Vec<i64> = {
            let mut st = self
                .conn
                .prepare_cached(
                    "SELECT m.tag_id FROM hashes h JOIN mappings m ON m.hash_id = h.id WHERE h.sha256 = ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = st.query_map(params![&bytes[..]], |r| r.get(0)).map_err(|e| e.to_string())?;
            rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
        };
        let mut tag_ids: BTreeSet<i64> = ids.into_iter().map(|t| self.resolve_sibling(t)).collect();
        if opts.add_parents {
            let mut todo: Vec<(i64, usize)> = tag_ids.iter().map(|t| (*t, 0)).collect();
            while let Some((t, depth)) = todo.pop() {
                if depth >= MAX_DEPTH {
                    continue;
                }
                for p in self.parents(t) {
                    let p = self.resolve_sibling(p);
                    if tag_ids.insert(p) {
                        todo.push((p, depth + 1));
                    }
                }
            }
        }
        let mut out = BTreeSet::new();
        for id in tag_ids {
            let Some(text) = self.tag_text(id) else { continue };
            if is_excluded(&text, &opts.exclude_namespaces) {
                continue;
            }
            out.insert(text);
        }
        Ok(out.into_iter().collect())
    }

    /// Follow the sibling chain to the ideal tag. Stops on a cycle.
    fn resolve_sibling(&self, tag_id: i64) -> i64 {
        let mut seen = HashSet::new();
        let mut cur = tag_id;
        while seen.insert(cur) && seen.len() <= MAX_DEPTH {
            let next: Option<i64> = self
                .conn
                .prepare_cached("SELECT good_tag_id FROM siblings WHERE bad_tag_id = ?1 LIMIT 1")
                .and_then(|mut s| s.query_row(params![cur], |r| r.get(0)).optional())
                .ok()
                .flatten();
            match next {
                Some(n) if n != cur => cur = n,
                _ => break,
            }
        }
        cur
    }

    fn parents(&self, tag_id: i64) -> Vec<i64> {
        self.conn
            .prepare_cached("SELECT parent_tag_id FROM parents WHERE child_tag_id = ?1")
            .and_then(|mut s| s.query_map(params![tag_id], |r| r.get(0))?.collect())
            .unwrap_or_default()
    }

    fn tag_text(&self, tag_id: i64) -> Option<String> {
        self.conn
            .prepare_cached("SELECT text FROM tags WHERE id = ?1")
            .and_then(|mut s| s.query_row(params![tag_id], |r| r.get(0)).optional())
            .ok()
            .flatten()
    }

    /// The last applied update index and the last sync time (unix seconds).
    pub fn sync_state(&self) -> Option<(i64, i64)> {
        self.conn
            .query_row("SELECT last_index, last_sync_at FROM sync_state WHERE id = 0", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok()
    }
}

/// True when the tag namespace is in the exclude list.
pub fn is_excluded(tag: &str, exclude: &[String]) -> bool {
    match tag.split_once(':') {
        Some((ns, _)) => exclude.iter().any(|e| e.eq_ignore_ascii_case(ns.trim())),
        None => false,
    }
}

/// Make a PTR tag fit the pichouse tag style: lower case, trimmed, with
/// single spaces.
pub fn normalize(tag: &str) -> String {
    tag.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

pub fn hex_to_bytes(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::db::PtrDb;

    fn make_db() -> (tempfile_path::TempPath, [u8; 32]) {
        let p = tempfile_path::TempPath::new();
        let db = PtrDb::open(&p.0).unwrap();
        let sha = [7u8; 32];
        db.conn
            .execute_batch(
                "INSERT INTO tags VALUES (1,'cat'),(2,'kitty'),(3,'animal'),(4,'filename:x.png'),(5,'a'),(6,'b');
                 INSERT INTO mappings VALUES (1,2),(1,4),(1,5);
                 INSERT INTO siblings VALUES (2,1),(5,6),(6,5);
                 INSERT INTO parents VALUES (1,3);",
            )
            .unwrap();
        db.conn.execute("INSERT INTO hashes VALUES (1, ?1)", params![&sha[..]]).unwrap();
        db.build_lookup_indexes().unwrap();
        (p, sha)
    }

    mod tempfile_path {
        pub struct TempPath(pub std::path::PathBuf);
        impl TempPath {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicUsize, Ordering};
                static N: AtomicUsize = AtomicUsize::new(0);
                let n = N.fetch_add(1, Ordering::Relaxed);
                Self(std::env::temp_dir().join(format!("pichouse-ptr-test-{}-{n}.db", std::process::id())))
            }
        }
        impl Drop for TempPath {
            fn drop(&mut self) {
                for s in ["", "-wal", "-shm"] {
                    let _ = std::fs::remove_file(format!("{}{s}", self.0.display()));
                }
            }
        }
    }

    #[test]
    fn lookup_resolves_siblings_parents_and_filters() {
        let (p, sha) = make_db();
        let r = PtrReader::open(&p.0).unwrap();
        let hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
        let opts = LookupOptions { exclude_namespaces: vec!["filename".into()], add_parents: false };
        // kitty -> cat. The a<->b cycle stops. filename: is excluded.
        let tags = r.tags_for_sha256(&hex, &opts).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"cat".to_string()));
        let opts = LookupOptions { add_parents: true, ..opts };
        assert!(r.tags_for_sha256(&hex, &opts).unwrap().contains(&"animal".to_string()));
        assert!(r.tags_for_sha256(&"00".repeat(32), &opts).unwrap().is_empty());
        assert!(r.tags_for_sha256("bad", &opts).unwrap().is_empty());
    }

    #[test]
    fn normalize_and_namespace() {
        assert_eq!(normalize("  Blue   Sky "), "blue sky");
        assert!(is_excluded("Filename:a", &["filename".into()]));
        assert!(!is_excluded("cat", &["filename".into()]));
    }
}
