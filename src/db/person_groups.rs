//! Person-group tree CRUD and person <-> group membership.
//!
//! Mirrors `db/albums.rs`'s nesting shape, but membership is additive: a
//! person may belong to any number of groups at once, so `add_person_to_group`
//! never evicts other memberships the way `add_folder_to_album` does.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, OptionalExtension};

use crate::model::PersonGroup;

use super::{Library, Result};

impl Library {
    /// Insert a new person group. `parent_id` of 0 creates a top-level group.
    pub fn create_person_group(&self, name: &str, parent_id: i64) -> Result<i64> {
        let conn = self.lock();
        let pos: i64 = if parent_id == 0 {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM person_groups WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM person_groups WHERE parent_id = ?1",
                params![parent_id],
                |r| r.get(0),
            )?
        };
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "INSERT INTO person_groups(name, parent_id, position) VALUES(?1, ?2, ?3)",
            params![name, parent, pos + 1],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Change a group's display name.
    pub fn rename_person_group(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE person_groups SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    /// Remove a group. Sub-groups cascade; member persons are never deleted,
    /// only their membership rows in `person_group_members` cascade away.
    pub fn delete_person_group(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM person_groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Re-parent a group. `parent_id` of 0 makes it top-level. Refuses to
    /// create a cycle (making a group a descendant of itself).
    pub fn set_person_group_parent(&self, id: i64, parent_id: i64) -> Result<()> {
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
                    "SELECT parent_id FROM person_groups WHERE id = ?1",
                    params![cur],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            cur = next.unwrap_or(0);
        }
        let parent: Option<i64> = if parent_id == 0 { None } else { Some(parent_id) };
        conn.execute(
            "UPDATE person_groups SET parent_id = ?1 WHERE id = ?2",
            params![parent, id],
        )?;
        Ok(())
    }

    /// All person groups ordered by position then name.
    pub fn person_groups(&self) -> Result<Vec<PersonGroup>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, COALESCE(parent_id, 0), position, COALESCE(cover_face_id, 0) \
             FROM person_groups ORDER BY position ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PersonGroup {
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
    pub fn set_person_group_cover(&self, id: i64, face_id: i64) -> Result<()> {
        let conn = self.lock();
        let cover = if face_id == 0 { None } else { Some(face_id) };
        conn.execute(
            "UPDATE person_groups SET cover_face_id = ?2 WHERE id = ?1",
            params![id, cover],
        )?;
        Ok(())
    }

    /// Add a person to a group. Does NOT remove the person from any other
    /// group — true multi-membership, unlike `add_folder_to_album`. Idempotent.
    pub fn add_person_to_group(&self, person_id: i64, group_id: i64) -> Result<()> {
        let conn = self.lock();
        let pos: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM person_group_members WHERE group_id = ?1",
            params![group_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO person_group_members(group_id, person_id, position) \
             VALUES(?1, ?2, ?3)",
            params![group_id, person_id, pos + 1],
        )?;
        Ok(())
    }

    /// Remove a person from one specific group. Other memberships are untouched.
    pub fn remove_person_from_group(&self, person_id: i64, group_id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "DELETE FROM person_group_members WHERE group_id = ?1 AND person_id = ?2",
            params![group_id, person_id],
        )?;
        Ok(())
    }

    /// Every group id a person directly belongs to (not transitive).
    pub fn groups_of_person(&self, person_id: i64) -> Result<Vec<i64>> {
        let conn = self.read_lock();
        let mut stmt =
            conn.prepare("SELECT group_id FROM person_group_members WHERE person_id = ?1")?;
        let rows = stmt.query_map(params![person_id], |r| r.get::<_, i64>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A map of group id to its direct member person ids (not transitive).
    pub fn person_group_members(&self) -> Result<HashMap<i64, Vec<i64>>> {
        let conn = self.read_lock();
        let mut stmt = conn
            .prepare("SELECT group_id, person_id FROM person_group_members ORDER BY position ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            let (gid, pid) = row?;
            out.entry(gid).or_default().push(pid);
        }
        Ok(out)
    }

    /// Every person id under a group and its sub-groups (transitive union,
    /// de-duplicated).
    pub fn persons_under_group(&self, group_id: i64) -> Result<Vec<i64>> {
        let conn = self.lock();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT id, COALESCE(parent_id, 0) FROM person_groups")?;
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
        let mut persons = Vec::new();
        let mut stmt =
            conn.prepare("SELECT person_id FROM person_group_members WHERE group_id = ?1")?;
        for g in subtree {
            let rows = stmt.query_map(params![g], |r| r.get::<_, i64>(0))?;
            for row in rows {
                let pid = row?;
                if seen.insert(pid) {
                    persons.push(pid);
                }
            }
        }
        Ok(persons)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lib() -> (Library, std::path::PathBuf) {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-persongrouptest-{}-{}.db",
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
    fn create_rename_delete_person_group() {
        let (lib, path) = temp_lib();
        let gid = lib.create_person_group("Disney", 0).unwrap();
        assert_eq!(lib.person_groups().unwrap().len(), 1);
        lib.rename_person_group(gid, "Pixar").unwrap();
        assert_eq!(lib.person_groups().unwrap()[0].name, "Pixar");
        lib.delete_person_group(gid).unwrap();
        assert!(lib.person_groups().unwrap().is_empty());
        cleanup(&path);
    }

    #[test]
    fn set_person_group_parent_rejects_cycle() {
        let (lib, path) = temp_lib();
        let root = lib.create_person_group("root", 0).unwrap();
        let mid = lib.create_person_group("mid", root).unwrap();
        let leaf = lib.create_person_group("leaf", mid).unwrap();
        // Making root a child of its own descendant leaf must be refused.
        lib.set_person_group_parent(root, leaf).unwrap();
        let groups = lib.person_groups().unwrap();
        let root_parent = groups.iter().find(|g| g.id == root).unwrap().parent_id;
        assert_eq!(root_parent, 0);
        cleanup(&path);
    }

    #[test]
    fn person_can_belong_to_multiple_groups() {
        let (lib, path) = temp_lib();
        let pid = lib.create_person("Alice").unwrap();
        let g1 = lib.create_person_group("Disney", 0).unwrap();
        let g2 = lib.create_person_group("Furry", 0).unwrap();
        lib.add_person_to_group(pid, g1).unwrap();
        lib.add_person_to_group(pid, g2).unwrap();
        let mut groups = lib.groups_of_person(pid).unwrap();
        groups.sort();
        let mut want = vec![g1, g2];
        want.sort();
        assert_eq!(groups, want);
        lib.remove_person_from_group(pid, g1).unwrap();
        assert_eq!(lib.groups_of_person(pid).unwrap(), vec![g2]);
        cleanup(&path);
    }

    #[test]
    fn persons_under_group_covers_subtree_and_dedupes() {
        let (lib, path) = temp_lib();
        let root = lib.create_person_group("root", 0).unwrap();
        let sub = lib.create_person_group("sub", root).unwrap();
        let p1 = lib.create_person("Alice").unwrap();
        let p2 = lib.create_person("Bob").unwrap();
        lib.add_person_to_group(p1, root).unwrap();
        lib.add_person_to_group(p2, sub).unwrap();
        // Also add p1 directly to sub: should not duplicate in the union.
        lib.add_person_to_group(p1, sub).unwrap();
        let mut got = lib.persons_under_group(root).unwrap();
        got.sort();
        let mut want = vec![p1, p2];
        want.sort();
        assert_eq!(got, want);
        cleanup(&path);
    }

    #[test]
    fn set_person_group_cover_roundtrip() {
        let (lib, path) = temp_lib();
        let gid = lib.create_person_group("Disney", 0).unwrap();
        assert_eq!(lib.person_groups().unwrap()[0].cover_face_id, 0);
        lib.set_person_group_cover(gid, 42).unwrap();
        assert_eq!(lib.person_groups().unwrap()[0].cover_face_id, 42);
        lib.set_person_group_cover(gid, 0).unwrap();
        assert_eq!(lib.person_groups().unwrap()[0].cover_face_id, 0);
        cleanup(&path);
    }

    #[test]
    fn delete_group_does_not_delete_persons() {
        let (lib, path) = temp_lib();
        let pid = lib.create_person("Alice").unwrap();
        let gid = lib.create_person_group("Disney", 0).unwrap();
        lib.add_person_to_group(pid, gid).unwrap();
        lib.delete_person_group(gid).unwrap();
        let persons = lib.persons().unwrap();
        assert!(persons.iter().any(|(p, _)| p.id == pid));
        cleanup(&path);
    }
}
