//! Character-group tree CRUD and character <-> group membership. Mirrors
//! `db/person_groups.rs` exactly, kept as a separate pool of groups since
//! People and Characters are already fully parallel, independent pipelines.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};

use crate::model::CharacterGroup;

use super::{Library, Result};

impl Library {
    /// Insert a new character group. `parent_id` of 0 creates a top-level group.
    pub fn create_character_group(&self, name: &str, parent_id: i64) -> Result<i64> {
        let conn = self.lock();
        let pos: i64 = if parent_id == 0 {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM character_groups WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM character_groups WHERE parent_id = ?1",
                params![parent_id],
                |r| r.get(0),
            )?
        };
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "INSERT INTO character_groups(name, parent_id, position) VALUES(?1, ?2, ?3)",
            params![name, parent, pos + 1],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Change a group's display name.
    pub fn rename_character_group(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE character_groups SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    /// Remove a group. Sub-groups cascade; member characters are never
    /// deleted, only their membership rows in `character_group_members`
    /// cascade away.
    pub fn delete_character_group(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM character_groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Re-parent a group. `parent_id` of 0 makes it top-level. Refuses to
    /// create a cycle (making a group a descendant of itself).
    pub fn set_character_group_parent(&self, id: i64, parent_id: i64) -> Result<()> {
        if id == parent_id {
            return Ok(());
        }
        let conn = self.lock();
        let mut cur = parent_id;
        while cur != 0 {
            if cur == id {
                return Ok(()); // would create a cycle; ignore
            }
            let next: Option<i64> = conn
                .query_row(
                    "SELECT parent_id FROM character_groups WHERE id = ?1",
                    params![cur],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            cur = next.unwrap_or(0);
        }
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "UPDATE character_groups SET parent_id = ?1 WHERE id = ?2",
            params![parent, id],
        )?;
        Ok(())
    }

    /// All character groups ordered by position then name.
    pub fn character_groups(&self) -> Result<Vec<CharacterGroup>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, COALESCE(parent_id, 0), position, COALESCE(cover_face_id, 0) \
             FROM character_groups ORDER BY position ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CharacterGroup {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                position: r.get(3)?,
                cover_face_id: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Set the representative face shown as a group's tile icon (0 clears it).
    pub fn set_character_group_cover(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        let cover = if face_id == 0 { None } else { Some(face_id) };
        conn.execute(
            "UPDATE character_groups SET cover_face_id = ?2 WHERE id = ?1",
            params![id, cover],
        )?;
        Ok(())
    }

    /// Add a character to a group. Does NOT remove the character from any
    /// other group — true multi-membership. Idempotent.
    pub fn add_character_to_group(&self, character_id: i64, group_id: i64) -> Result<()> {
        let conn = self.lock();
        let pos: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM character_group_members WHERE group_id = ?1",
            params![group_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO character_group_members(group_id, character_id, position) \
             VALUES(?1, ?2, ?3)",
            params![group_id, character_id, pos + 1],
        )?;
        Ok(())
    }

    /// Remove a character from one specific group. Other memberships are
    /// untouched.
    pub fn remove_character_from_group(&self, character_id: i64, group_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM character_group_members WHERE group_id = ?1 AND character_id = ?2",
            params![group_id, character_id],
        )?;
        Ok(())
    }

    /// Every group id a character directly belongs to (not transitive).
    pub fn groups_of_character(&self, character_id: i64) -> Result<Vec<i64>> {
        let conn = self.read_lock();
        let mut stmt = conn
            .prepare("SELECT group_id FROM character_group_members WHERE character_id = ?1")?;
        let rows = stmt.query_map(params![character_id], |r| r.get::<_, i64>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A map of group id to its direct member character ids (not transitive).
    pub fn character_group_members(&self) -> Result<HashMap<i64, Vec<i64>>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT group_id, character_id FROM character_group_members ORDER BY position ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            let (gid, cid) = row?;
            out.entry(gid).or_default().push(cid);
        }
        Ok(out)
    }

    /// Every character id under a group and its sub-groups (transitive union,
    /// de-duplicated).
    pub fn characters_under_group(&self, group_id: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT id, COALESCE(parent_id, 0) FROM character_groups")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (id, parent) = row?;
                children.entry(parent).or_default().push(id);
            }
        }
        let mut subtree = Vec::new();
        let mut stack = vec![group_id];
        while let Some(g) = stack.pop() {
            subtree.push(g);
            if let Some(kids) = children.get(&g) {
                stack.extend(kids.iter().copied());
            }
        }
        let mut seen = HashSet::new();
        let mut characters = Vec::new();
        let mut stmt =
            conn.prepare("SELECT character_id FROM character_group_members WHERE group_id = ?1")?;
        for g in subtree {
            let rows = stmt.query_map(params![g], |r| r.get::<_, i64>(0))?;
            for row in rows {
                let cid = row?;
                if seen.insert(cid) {
                    characters.push(cid);
                }
            }
        }
        Ok(characters)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lib() -> (Library, std::path::PathBuf) {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-charactergrouptest-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        (Library::open_at(&p).unwrap(), p)
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn create_rename_delete_character_group() {
        let (lib, path) = temp_lib();
        let gid = lib.create_character_group("Disney", 0).unwrap();
        assert_eq!(lib.character_groups().unwrap().len(), 1);
        lib.rename_character_group(gid, "Pixar").unwrap();
        assert_eq!(lib.character_groups().unwrap()[0].name, "Pixar");
        lib.delete_character_group(gid).unwrap();
        assert!(lib.character_groups().unwrap().is_empty());
        cleanup(&path);
    }

    #[test]
    fn set_character_group_parent_rejects_cycle() {
        let (lib, path) = temp_lib();
        let root = lib.create_character_group("root", 0).unwrap();
        let mid = lib.create_character_group("mid", root).unwrap();
        let leaf = lib.create_character_group("leaf", mid).unwrap();
        lib.set_character_group_parent(root, leaf).unwrap();
        let groups = lib.character_groups().unwrap();
        let root_parent = groups.iter().find(|g| g.id == root).unwrap().parent_id;
        assert_eq!(root_parent, 0);
        cleanup(&path);
    }

    #[test]
    fn character_can_belong_to_multiple_groups() {
        let (lib, path) = temp_lib();
        let cid = lib.create_character("Mickey").unwrap();
        let g1 = lib.create_character_group("Disney", 0).unwrap();
        let g2 = lib.create_character_group("Furry", 0).unwrap();
        lib.add_character_to_group(cid, g1).unwrap();
        lib.add_character_to_group(cid, g2).unwrap();
        let mut groups = lib.groups_of_character(cid).unwrap();
        groups.sort();
        let mut want = vec![g1, g2];
        want.sort();
        assert_eq!(groups, want);
        lib.remove_character_from_group(cid, g1).unwrap();
        assert_eq!(lib.groups_of_character(cid).unwrap(), vec![g2]);
        cleanup(&path);
    }

    #[test]
    fn characters_under_group_covers_subtree_and_dedupes() {
        let (lib, path) = temp_lib();
        let root = lib.create_character_group("root", 0).unwrap();
        let sub = lib.create_character_group("sub", root).unwrap();
        let c1 = lib.create_character("Mickey").unwrap();
        let c2 = lib.create_character("Goofy").unwrap();
        lib.add_character_to_group(c1, root).unwrap();
        lib.add_character_to_group(c2, sub).unwrap();
        lib.add_character_to_group(c1, sub).unwrap();
        let mut got = lib.characters_under_group(root).unwrap();
        got.sort();
        let mut want = vec![c1, c2];
        want.sort();
        assert_eq!(got, want);
        cleanup(&path);
    }

    #[test]
    fn set_character_group_cover_roundtrip() {
        let (lib, path) = temp_lib();
        let gid = lib.create_character_group("Disney", 0).unwrap();
        assert_eq!(lib.character_groups().unwrap()[0].cover_face_id, 0);
        lib.set_character_group_cover(gid, 42).unwrap();
        assert_eq!(lib.character_groups().unwrap()[0].cover_face_id, 42);
        lib.set_character_group_cover(gid, 0).unwrap();
        assert_eq!(lib.character_groups().unwrap()[0].cover_face_id, 0);
        cleanup(&path);
    }

    #[test]
    fn delete_group_does_not_delete_characters() {
        let (lib, path) = temp_lib();
        let cid = lib.create_character("Mickey").unwrap();
        let gid = lib.create_character_group("Disney", 0).unwrap();
        lib.add_character_to_group(cid, gid).unwrap();
        lib.delete_character_group(gid).unwrap();
        let characters = lib.characters().unwrap();
        assert!(characters.iter().any(|(c, _)| c.id == cid));
        cleanup(&path);
    }
}
