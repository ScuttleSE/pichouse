//! The local PTR database (`ptr.db`): schema and update apply.

use rusqlite::{params, Connection};

use super::update::{Update, ACTION_ADD, CONTENT_PARENTS, CONTENT_SIBLINGS};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS hashes (id INTEGER PRIMARY KEY, sha256 BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS tags (id INTEGER PRIMARY KEY, text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS mappings (
    tag_id INTEGER NOT NULL, hash_id INTEGER NOT NULL,
    PRIMARY KEY (tag_id, hash_id)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS siblings (
    bad_tag_id INTEGER NOT NULL, good_tag_id INTEGER NOT NULL,
    PRIMARY KEY (bad_tag_id, good_tag_id)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS parents (
    child_tag_id INTEGER NOT NULL, parent_tag_id INTEGER NOT NULL,
    PRIMARY KEY (child_tag_id, parent_tag_id)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    last_index INTEGER NOT NULL DEFAULT -1,
    next_update_due INTEGER NOT NULL DEFAULT 0,
    last_sync_at INTEGER NOT NULL DEFAULT 0);
INSERT OR IGNORE INTO sync_state (id) VALUES (0);
";

/// The lookup indexes. The initial sync builds them at the end, because an
/// index slows the bulk insert down.
const LOOKUP_INDEXES: &str = "
CREATE UNIQUE INDEX IF NOT EXISTS hashes_sha256 ON hashes (sha256);
CREATE INDEX IF NOT EXISTS mappings_hash ON mappings (hash_id, tag_id);
";

pub struct PtrDb {
    pub conn: Connection,
}

impl PtrDb {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -262144;
             PRAGMA temp_store = MEMORY;",
        )?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// The last applied update index, or -1 when none.
    pub fn last_index(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT last_index FROM sync_state WHERE id = 0", [], |r| r.get(0))
    }

    pub fn build_lookup_indexes(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(LOOKUP_INDEXES)
    }

    /// Apply all updates of one index in one transaction. The caller gives
    /// the updates in any order. This function applies the definitions first.
    pub fn apply_index(&mut self, index: u64, next_due: i64, mut updates: Vec<Update>) -> rusqlite::Result<()> {
        updates.sort_by_key(|u| matches!(u, Update::Content { .. }));
        let tx = self.conn.transaction()?;
        {
            let mut ins_hash = tx.prepare_cached("INSERT OR REPLACE INTO hashes (id, sha256) VALUES (?1, ?2)")?;
            let mut ins_tag = tx.prepare_cached("INSERT OR REPLACE INTO tags (id, text) VALUES (?1, ?2)")?;
            let mut add_map = tx.prepare_cached("INSERT OR IGNORE INTO mappings (tag_id, hash_id) VALUES (?1, ?2)")?;
            let mut del_map = tx.prepare_cached("DELETE FROM mappings WHERE tag_id = ?1 AND hash_id = ?2")?;
            let mut add_sib = tx.prepare_cached("INSERT OR IGNORE INTO siblings VALUES (?1, ?2)")?;
            let mut del_sib = tx.prepare_cached("DELETE FROM siblings WHERE bad_tag_id = ?1 AND good_tag_id = ?2")?;
            let mut add_par = tx.prepare_cached("INSERT OR IGNORE INTO parents VALUES (?1, ?2)")?;
            let mut del_par = tx.prepare_cached("DELETE FROM parents WHERE child_tag_id = ?1 AND parent_tag_id = ?2")?;
            for u in &updates {
                match u {
                    Update::Definitions { hashes, tags } => {
                        for (id, h) in hashes {
                            ins_hash.execute(params![*id as i64, &h[..]])?;
                        }
                        for (id, t) in tags {
                            ins_tag.execute(params![*id as i64, t])?;
                        }
                    }
                    Update::Content { mappings, pairs } => {
                        for (action, tag_id, hash_ids) in mappings {
                            let st = if *action == ACTION_ADD { &mut add_map } else { &mut del_map };
                            for h in hash_ids {
                                st.execute(params![*tag_id as i64, *h as i64])?;
                            }
                        }
                        for (ct, action, l, r) in pairs {
                            let add = *action == ACTION_ADD;
                            let st = match (*ct, add) {
                                (CONTENT_SIBLINGS, true) => &mut add_sib,
                                (CONTENT_SIBLINGS, false) => &mut del_sib,
                                (CONTENT_PARENTS, true) => &mut add_par,
                                (CONTENT_PARENTS, false) => &mut del_par,
                                _ => continue,
                            };
                            st.execute(params![*l as i64, *r as i64])?;
                        }
                    }
                }
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        tx.execute(
            "UPDATE sync_state SET last_index = ?1, next_update_due = ?2, last_sync_at = ?3 WHERE id = 0",
            params![index as i64, next_due, now],
        )?;
        tx.commit()
    }
}
