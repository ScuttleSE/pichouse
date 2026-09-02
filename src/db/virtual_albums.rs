//! Virtual album CRUD, manual membership, rules, and live membership queries.
//!
//! A virtual album groups individual *photos* drawn from anywhere in the
//! library. Membership is computed at view time: rule-matched photos (built
//! from the album's structured rules) unioned with manually pinned photos, less
//! any manual exclusions.

use rusqlite::{params, OptionalExtension};

use crate::model::{Photo, RuleField, RuleGroup, RuleMatch, RuleOp, VirtualAlbum, VirtualRule};

use super::library::map_photo;
use super::{Library, Result};

/// The 16-column photo projection, in schema order, matching `map_photo`.
const PHOTO_COLS: &str = "id, folder_id, path, filename, size, mod_time, taken_at, \
     width, height, hash, thumb_ready, orientation, ai_status, scan_state, missing, added_at, phash, skip_face_scan";

impl Library {
    /// Insert a new virtual album. `parent_id` of 0 creates a top-level album.
    pub fn create_virtual_album(&self, name: &str, parent_id: i64) -> Result<i64> {
        let conn = self.lock();
        let pos: i64 = if parent_id == 0 {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM virtual_albums WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM virtual_albums WHERE parent_id = ?1",
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
            "INSERT INTO virtual_albums(name, parent_id, position) VALUES(?1, ?2, ?3)",
            params![name, parent, pos + 1],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Change a virtual album's display name.
    pub fn rename_virtual_album(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE virtual_albums SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    /// Remove a virtual album. Sub-albums, membership, and rules cascade.
    pub fn delete_virtual_album(&self, id: i64) -> Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM virtual_albums WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Re-parent a virtual album. `parent_id` of 0 makes it top-level. Refuses
    /// to create a cycle (making an album a descendant of itself).
    pub fn set_virtual_album_parent(&self, id: i64, parent_id: i64) -> Result<()> {
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
                    "SELECT parent_id FROM virtual_albums WHERE id = ?1",
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
            "UPDATE virtual_albums SET parent_id = ?1 WHERE id = ?2",
            params![parent, id],
        )?;
        Ok(())
    }

    /// All virtual albums ordered by position then name.
    pub fn virtual_albums(&self) -> Result<Vec<VirtualAlbum>> {
        let conn = self.read_lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, COALESCE(parent_id, 0), position, rule_match
             FROM virtual_albums ORDER BY position ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(VirtualAlbum {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                position: r.get(3)?,
                rule_match: RuleMatch::from_i64(r.get::<_, i64>(4)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Pin photos into a virtual album (manual membership). Any existing
    /// exclusion for the same photo is replaced by a pin.
    pub fn add_photos_to_virtual_album(&self, album_id: i64, photo_ids: &[i64]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for &pid in photo_ids {
            let pos: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position), -1) FROM virtual_album_photos WHERE album_id = ?1",
                params![album_id],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO virtual_album_photos(album_id, photo_id, position, kind)
                 VALUES(?1, ?2, ?3, 0)
                 ON CONFLICT(album_id, photo_id) DO UPDATE SET kind = 0",
                params![album_id, pid, pos + 1],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove photos from a virtual album. A pinned photo is unpinned. A
    /// rule-matched photo is excluded (kind = 1) so it stays hidden.
    pub fn remove_photos_from_virtual_album(&self, album_id: i64, photo_ids: &[i64]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let has_rules: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM virtual_album_rules WHERE album_id = ?1)",
            params![album_id],
            |r| r.get::<_, i64>(0),
        )? != 0;
        for &pid in photo_ids {
            // Drop any existing pin first.
            tx.execute(
                "DELETE FROM virtual_album_photos WHERE album_id = ?1 AND photo_id = ?2",
                params![album_id, pid],
            )?;
            // If the album has rules, record an exclusion so a rule match stays
            // hidden. With no rules there is nothing to exclude.
            if has_rules {
                tx.execute(
                    "INSERT INTO virtual_album_photos(album_id, photo_id, position, kind)
                     VALUES(?1, ?2, 0, 1)",
                    params![album_id, pid],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The top-level (ungrouped) rules of a virtual album, in id order.
    pub fn virtual_album_rules(&self, album_id: i64) -> Result<Vec<VirtualRule>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, album_id, field, op, value
             FROM virtual_album_rules WHERE album_id = ?1 AND group_id IS NULL ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![album_id], map_rule)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The rule groups of a virtual album (each with its own member rules and
    /// AND/OR mode), in id order.
    pub fn virtual_album_rule_groups(&self, album_id: i64) -> Result<Vec<RuleGroup>> {
        let conn = self.lock();
        let mut gstmt = conn.prepare(
            "SELECT id, rule_match FROM virtual_album_rule_groups
             WHERE album_id = ?1 ORDER BY id ASC",
        )?;
        let group_rows: Vec<(i64, i64)> = gstmt
            .query_map(params![album_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut rstmt = conn.prepare(
            "SELECT id, album_id, field, op, value
             FROM virtual_album_rules WHERE group_id = ?1 ORDER BY id ASC",
        )?;
        let mut groups = Vec::with_capacity(group_rows.len());
        for (gid, rule_match) in group_rows {
            let rules = rstmt
                .query_map(params![gid], map_rule)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            groups.push(RuleGroup {
                id: gid,
                rule_match: RuleMatch::from_i64(rule_match),
                rules,
            });
        }
        Ok(groups)
    }

    /// Replace all rules of a virtual album and set its match mode. Passing an
    /// empty slice makes the album purely manual. The album's rule groups (if
    /// any) are left untouched — use [`Library::set_virtual_album_rules_grouped`]
    /// to replace both together.
    pub fn set_virtual_album_rules(
        &self,
        album_id: i64,
        rule_match: RuleMatch,
        rules: &[VirtualRule],
    ) -> Result<()> {
        self.set_virtual_album_rules_grouped(album_id, rule_match, rules, &[])
    }

    /// Replace all top-level rules AND rule groups of a virtual album, and set
    /// its top-level match mode, in one transaction. Each group combines its
    /// own rules by its own AND/OR mode and counts as a single term in the
    /// album's top-level match; groups do not nest.
    pub fn set_virtual_album_rules_grouped(
        &self,
        album_id: i64,
        rule_match: RuleMatch,
        rules: &[VirtualRule],
        groups: &[RuleGroup],
    ) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE virtual_albums SET rule_match = ?1 WHERE id = ?2",
            params![rule_match.as_i64(), album_id],
        )?;
        // Deleting groups first cascades away their member rules; the second
        // delete then clears whatever top-level rules remain.
        tx.execute(
            "DELETE FROM virtual_album_rule_groups WHERE album_id = ?1",
            params![album_id],
        )?;
        tx.execute(
            "DELETE FROM virtual_album_rules WHERE album_id = ?1",
            params![album_id],
        )?;
        for group in groups {
            tx.execute(
                "INSERT INTO virtual_album_rule_groups(album_id, rule_match) VALUES(?1, ?2)",
                params![album_id, group.rule_match.as_i64()],
            )?;
            let gid = tx.last_insert_rowid();
            for rule in &group.rules {
                tx.execute(
                    "INSERT INTO virtual_album_rules(album_id, group_id, field, op, value)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![
                        album_id,
                        gid,
                        rule.field.as_str(),
                        rule.op.as_str(),
                        rule.value
                    ],
                )?;
            }
        }
        for rule in rules {
            tx.execute(
                "INSERT INTO virtual_album_rules(album_id, field, op, value)
                 VALUES(?1, ?2, ?3, ?4)",
                params![album_id, rule.field.as_str(), rule.op.as_str(), rule.value],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// All photos in a virtual album: rule-matched photos combined per the
    /// album's match mode, unioned with pins, minus exclusions. Ordered by taken
    /// date then filename.
    pub fn photos_in_virtual_album(&self, album_id: i64) -> Result<Vec<Photo>> {
        let rule_match = {
            let conn = self.lock();
            let m: Option<i64> = conn
                .query_row(
                    "SELECT rule_match FROM virtual_albums WHERE id = ?1",
                    params![album_id],
                    |r| r.get(0),
                )
                .optional()?;
            RuleMatch::from_i64(m.unwrap_or(RuleMatch::Or.as_i64()))
        };
        let rules = self.virtual_album_rules(album_id)?;
        let groups = self.virtual_album_rule_groups(album_id)?;

        // Build the id-selecting query. `params` collects bound values in order.
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // Album id is used by the pins/exclusions subqueries.
        let member_sql = build_membership_sql(album_id, rule_match, &rules, &groups, &mut params);

        let sql = format!(
            "SELECT {PHOTO_COLS} FROM photos WHERE id IN ({member_sql})
             ORDER BY taken_at ASC, filename ASC"
        );
        let conn = self.lock();
        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), map_photo)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The number of photos in a virtual album (for sidebar counts).
    pub fn virtual_album_photo_count(&self, album_id: i64) -> Result<i64> {
        Ok(self.photos_in_virtual_album(album_id)?.len() as i64)
    }
}

/// Build the inner SQL that yields the set of member photo ids for an album,
/// pushing bound values onto `params`. The result is a comma-free `SELECT`
/// suitable for `id IN ( ... )`.
fn build_membership_sql(
    album_id: i64,
    rule_match: RuleMatch,
    rules: &[VirtualRule],
    groups: &[RuleGroup],
    params: &mut Vec<Box<dyn rusqlite::ToSql>>,
) -> String {
    // Rule matches: each top-level rule is one term; each non-empty group is
    // one parenthesized term combining its own rules by its own match mode.
    let mut clauses: Vec<String> = Vec::new();
    for rule in rules {
        clauses.push(rule_clause(rule, params));
    }
    for group in groups {
        if group.rules.is_empty() {
            continue;
        }
        let sub: Vec<String> = group.rules.iter().map(|r| rule_clause(r, params)).collect();
        let sub_joiner = match group.rule_match {
            RuleMatch::And => " AND ",
            RuleMatch::Or => " OR ",
        };
        clauses.push(format!("({})", sub.join(sub_joiner)));
    }
    let rule_select = if clauses.is_empty() {
        None
    } else {
        let joiner = match rule_match {
            RuleMatch::And => " AND ",
            RuleMatch::Or => " OR ",
        };
        Some(format!(
            "SELECT id FROM photos WHERE ({})",
            clauses.join(joiner)
        ))
    };

    // Pins.
    params.push(Box::new(album_id));
    let pins = "SELECT photo_id AS id FROM virtual_album_photos WHERE album_id = ? AND kind = 0";

    // Combine rule matches with pins.
    let included = match rule_select {
        Some(rs) => format!("{rs} UNION {pins}"),
        None => pins.to_string(),
    };

    // Exclusions.
    params.push(Box::new(album_id));
    let exclusions = "SELECT photo_id FROM virtual_album_photos WHERE album_id = ? AND kind = 1";

    format!("SELECT id FROM ({included}) EXCEPT {exclusions}")
}

/// Map one row of `virtual_album_rules` (`id, album_id, field, op, value`) to
/// a `VirtualRule`.
fn map_rule(r: &rusqlite::Row) -> rusqlite::Result<VirtualRule> {
    Ok(VirtualRule {
        id: r.get(0)?,
        album_id: r.get(1)?,
        field: RuleField::from_str(&r.get::<_, String>(2)?),
        op: RuleOp::from_str(&r.get::<_, String>(3)?),
        value: r.get(4)?,
    })
}

/// Build one rule's SQL predicate over the `photos` table, pushing bound values.
fn rule_clause(rule: &VirtualRule, params: &mut Vec<Box<dyn rusqlite::ToSql>>) -> String {
    match rule.field {
        RuleField::Tag => {
            params.push(Box::new(rule.value.clone()));
            "id IN (SELECT pt.photo_id FROM photo_tags pt JOIN tags t ON t.id = pt.tag_id \
             WHERE t.name = ? COLLATE NOCASE)"
                .to_string()
        }
        RuleField::DateFrom => {
            let ts: i64 = rule.value.parse().unwrap_or(0);
            params.push(Box::new(ts));
            "taken_at >= ?".to_string()
        }
        RuleField::DateTo => {
            let ts: i64 = rule.value.parse().unwrap_or(0);
            params.push(Box::new(ts));
            "taken_at <= ?".to_string()
        }
        RuleField::Filename => {
            params.push(Box::new(format!("%{}%", rule.value)));
            "filename LIKE ? COLLATE NOCASE".to_string()
        }
        RuleField::Path => {
            params.push(Box::new(format!("%{}%", rule.value)));
            "path LIKE ? COLLATE NOCASE".to_string()
        }
        RuleField::Folder => {
            let fid: i64 = rule.value.parse().unwrap_or(0);
            params.push(Box::new(fid));
            "folder_id = ?".to_string()
        }
        RuleField::Person => {
            params.push(Box::new(rule.value.clone()));
            "id IN (SELECT f.photo_id FROM faces f JOIN persons p ON p.id = f.person_id \
             WHERE p.name = ? COLLATE NOCASE)"
                .to_string()
        }
        RuleField::Character => {
            params.push(Box::new(rule.value.clone()));
            "id IN (SELECT sf.photo_id FROM style_faces sf JOIN characters c ON c.id = sf.character_id \
             WHERE c.name = ? COLLATE NOCASE)"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Folder, Photo, RuleField, RuleGroup, RuleMatch, RuleOp, TagSource, VirtualRule,
    };

    fn temp_lib() -> (Library, std::path::PathBuf) {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-valbumtest-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        (Library::open_at(&p).unwrap(), p)
    }

    fn cleanup(p: std::path::PathBuf) {
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(p.with_extension("db-wal"));
        let _ = std::fs::remove_file(p.with_extension("db-shm"));
    }

    #[test]
    fn migration_adds_group_id_to_pre_existing_db() {
        // Simulate a database created before rule groups existed: a
        // virtual_album_rules table with no group_id column and no
        // virtual_album_rule_groups table at all.
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pichouse-migtest-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&p);
        {
            let conn = rusqlite::Connection::open(&p).unwrap();
            conn.execute_batch(
                "CREATE TABLE virtual_albums (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    name       TEXT NOT NULL,
                    parent_id  INTEGER,
                    position   INTEGER NOT NULL DEFAULT 0,
                    rule_match INTEGER NOT NULL DEFAULT 1
                 );
                 CREATE TABLE virtual_album_rules (
                    id       INTEGER PRIMARY KEY AUTOINCREMENT,
                    album_id INTEGER NOT NULL,
                    field    TEXT NOT NULL,
                    op       TEXT NOT NULL,
                    value    TEXT NOT NULL
                 );
                 INSERT INTO virtual_albums(name) VALUES('Old Album');
                 INSERT INTO virtual_album_rules(album_id, field, op, value)
                    VALUES(1, 'tag', 'has', 'vacation');",
            )
            .unwrap();
        }

        // Opening through the normal path runs the full schema (creating
        // virtual_album_rule_groups, since it's new) then migrate() (adding
        // group_id to the pre-existing virtual_album_rules table).
        let lib = Library::open_at(&p).unwrap();
        let rules = lib.virtual_album_rules(1).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].value, "vacation");
        assert!(lib.virtual_album_rule_groups(1).unwrap().is_empty());
        drop(lib);
        cleanup(p);
    }

    fn mk_photo(lib: &Library, fid: i64, name: &str, taken_at: i64) -> i64 {
        lib.upsert_photo(&Photo {
            folder_id: fid,
            path: format!("/tmp/vroot/{name}"),
            filename: name.into(),
            taken_at,
            ..Default::default()
        })
        .unwrap()
    }

    fn rule(field: RuleField, op: RuleOp, value: &str) -> VirtualRule {
        VirtualRule {
            id: 0,
            album_id: 0,
            field,
            op,
            value: value.into(),
        }
    }

    #[test]
    fn crud_nesting_and_cycle() {
        let (lib, p) = temp_lib();
        let a = lib.create_virtual_album("Trips", 0).unwrap();
        let sub = lib.create_virtual_album("2024", a).unwrap();
        let albums = lib.virtual_albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums.iter().find(|x| x.id == sub).unwrap().parent_id, a);

        // Cycle prevention: making the parent a child of its descendant is ignored.
        lib.set_virtual_album_parent(a, sub).unwrap();
        let parent_of_a = lib
            .virtual_albums()
            .unwrap()
            .iter()
            .find(|x| x.id == a)
            .unwrap()
            .parent_id;
        assert_eq!(parent_of_a, 0);

        lib.rename_virtual_album(a, "Journeys").unwrap();
        assert_eq!(
            lib.virtual_albums()
                .unwrap()
                .iter()
                .find(|x| x.id == a)
                .unwrap()
                .name,
            "Journeys"
        );

        // Deleting the parent cascades the sub-album.
        lib.delete_virtual_album(a).unwrap();
        assert!(lib.virtual_albums().unwrap().is_empty());
        cleanup(p);
    }

    #[test]
    fn manual_membership_roundtrip() {
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 1,
                year: 2024,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100);
        let p2 = mk_photo(&lib, fid, "b.jpg", 200);
        let al = lib.create_virtual_album("Manual", 0).unwrap();

        lib.add_photos_to_virtual_album(al, &[p1, p2]).unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1, p2]); // ordered by taken_at

        // Removing a pin (no rules) drops it outright.
        lib.remove_photos_from_virtual_album(al, &[p1]).unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p2]);
        cleanup(p);
    }

    #[test]
    fn rule_tag_and_date_with_and_or() {
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 1,
                year: 2024,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 1_000);
        let p2 = mk_photo(&lib, fid, "b.jpg", 5_000);
        let p3 = mk_photo(&lib, fid, "c.jpg", 9_000);
        lib.add_photo_tags(p1, &["vacation".into()], TagSource::User)
            .unwrap();
        lib.add_photo_tags(p2, &["vacation".into()], TagSource::User)
            .unwrap();

        let al = lib.create_virtual_album("Smart", 0).unwrap();

        // OR: tag=vacation OR date in [8000, ..] => p1, p2 (tag) + p3 (date).
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[
                rule(RuleField::Tag, RuleOp::Has, "vacation"),
                rule(RuleField::DateFrom, RuleOp::Gte, "8000"),
            ],
        )
        .unwrap();
        let mut ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec![p1, p2, p3]);

        // AND: tag=vacation AND date >= 3000 => only p2.
        lib.set_virtual_album_rules(
            al,
            RuleMatch::And,
            &[
                rule(RuleField::Tag, RuleOp::Has, "vacation"),
                rule(RuleField::DateFrom, RuleOp::Gte, "3000"),
            ],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p2]);
        cleanup(p);
    }

    #[test]
    fn pin_and_exclusion_over_rules() {
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 1,
                year: 2024,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 1_000);
        let p2 = mk_photo(&lib, fid, "b.jpg", 2_000);
        let p3 = mk_photo(&lib, fid, "c.jpg", 3_000);
        lib.add_photo_tags(p1, &["dog".into()], TagSource::User)
            .unwrap();
        lib.add_photo_tags(p2, &["dog".into()], TagSource::User)
            .unwrap();

        let al = lib.create_virtual_album("Mixed", 0).unwrap();
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[rule(RuleField::Tag, RuleOp::Has, "dog")],
        )
        .unwrap();

        // Rules match p1, p2. Pin p3 in, exclude p1.
        lib.add_photos_to_virtual_album(al, &[p3]).unwrap();
        lib.remove_photos_from_virtual_album(al, &[p1]).unwrap();

        let mut ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec![p2, p3]);
        assert_eq!(lib.virtual_album_photo_count(al).unwrap(), 2);
        cleanup(p);
    }

    #[test]
    fn person_rule_matches_photos_of_that_person() {
        use crate::model::Face;
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100);
        let p2 = mk_photo(&lib, fid, "b.jpg", 200);
        // p1 has a face of Alice, p2 does not.
        let f = lib
            .insert_face(&Face {
                photo_id: p1,
                embedding: vec![1.0, 0.0],
                ..Default::default()
            })
            .unwrap();
        let alice = lib.create_person("Alice").unwrap();
        lib.set_face_person(f, alice).unwrap();

        let al = lib.create_virtual_album("Alice photos", 0).unwrap();
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[rule(RuleField::Person, RuleOp::Has, "Alice")],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1]);
        assert!(!ids.contains(&p2));
        cleanup(p);
    }

    #[test]
    fn path_rule_matches_full_path_not_just_filename() {
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        // p1 lives under a "Xennos" subdirectory that isn't part of its filename.
        let p1 = lib
            .upsert_photo(&Photo {
                folder_id: fid,
                path: "/tmp/vroot/Xennos/1001530 Extra Funding 3/test.png".into(),
                filename: "test.png".into(),
                taken_at: 100,
                ..Default::default()
            })
            .unwrap();
        let p2 = mk_photo(&lib, fid, "other.png", 200);

        let al = lib.create_virtual_album("Xennos", 0).unwrap();

        // "Filename contains" cannot see the directory component.
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[rule(RuleField::Filename, RuleOp::Contains, "Xennos")],
        )
        .unwrap();
        assert!(lib.photos_in_virtual_album(al).unwrap().is_empty());

        // "Path contains" matches on the full path.
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[rule(RuleField::Path, RuleOp::Contains, "Xennos")],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1]);
        assert!(!ids.contains(&p2));
        cleanup(p);
    }

    #[test]
    fn character_rule_matches_photos_of_that_character() {
        use crate::model::StyleFace;
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100);
        let p2 = mk_photo(&lib, fid, "b.jpg", 200);
        // p1 has a style face of Kyubey, p2 does not.
        let sf = lib
            .insert_style_face(&StyleFace {
                photo_id: p1,
                embedding: vec![1.0, 0.0],
                ..Default::default()
            })
            .unwrap();
        let kyubey = lib.create_character("Kyubey").unwrap();
        lib.set_style_face_character(sf, kyubey).unwrap();

        let al = lib.create_virtual_album("Kyubey photos", 0).unwrap();
        lib.set_virtual_album_rules(
            al,
            RuleMatch::Or,
            &[rule(RuleField::Character, RuleOp::Has, "Kyubey")],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1]);
        assert!(!ids.contains(&p2));
        cleanup(p);
    }

    #[test]
    fn rule_group_ors_within_a_top_level_and() {
        // Tag = vacation AND (Person = Alice OR Person = Bob).
        use crate::model::Face;
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100); // vacation + Alice: matches
        let p2 = mk_photo(&lib, fid, "b.jpg", 200); // vacation + Bob: matches
        let p3 = mk_photo(&lib, fid, "c.jpg", 300); // vacation only, no person: excluded
        let p4 = mk_photo(&lib, fid, "d.jpg", 400); // Alice only, no tag: excluded
        for pid in [p1, p2, p3] {
            lib.add_photo_tags(pid, &["vacation".into()], TagSource::User)
                .unwrap();
        }
        let alice = lib.create_person("Alice").unwrap();
        let bob = lib.create_person("Bob").unwrap();
        for (pid, person) in [(p1, alice), (p4, alice), (p2, bob)] {
            let f = lib
                .insert_face(&Face {
                    photo_id: pid,
                    embedding: vec![1.0, 0.0],
                    ..Default::default()
                })
                .unwrap();
            lib.set_face_person(f, person).unwrap();
        }

        let al = lib.create_virtual_album("Vacation people", 0).unwrap();
        lib.set_virtual_album_rules_grouped(
            al,
            RuleMatch::And,
            &[rule(RuleField::Tag, RuleOp::Has, "vacation")],
            &[RuleGroup {
                id: 0,
                rule_match: RuleMatch::Or,
                rules: vec![
                    rule(RuleField::Person, RuleOp::Has, "Alice"),
                    rule(RuleField::Person, RuleOp::Has, "Bob"),
                ],
            }],
        )
        .unwrap();
        let mut ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec![p1, p2]);
        cleanup(p);
    }

    #[test]
    fn empty_rule_group_contributes_no_clause() {
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100);
        lib.add_photo_tags(p1, &["vacation".into()], TagSource::User)
            .unwrap();

        let al = lib.create_virtual_album("Solo", 0).unwrap();
        lib.set_virtual_album_rules_grouped(
            al,
            RuleMatch::And,
            &[rule(RuleField::Tag, RuleOp::Has, "vacation")],
            &[RuleGroup {
                id: 0,
                rule_match: RuleMatch::Or,
                rules: vec![],
            }],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1]);
        cleanup(p);
    }

    #[test]
    fn rules_entirely_in_groups_still_match() {
        // No top-level rules at all: two OR-groups combined by the album's AND.
        let (lib, p) = temp_lib();
        let fid = lib
            .upsert_folder(&Folder {
                path: "/tmp/vroot".into(),
                name: "vroot".into(),
                mtime: 0,
                year: 2020,
                ..Default::default()
            })
            .unwrap();
        let p1 = mk_photo(&lib, fid, "a.jpg", 100);
        lib.add_photo_tags(p1, &["vacation".into()], TagSource::User)
            .unwrap();
        lib.add_photo_tags(p1, &["sunny".into()], TagSource::User)
            .unwrap();
        let p2 = mk_photo(&lib, fid, "b.jpg", 200);
        lib.add_photo_tags(p2, &["vacation".into()], TagSource::User)
            .unwrap();

        let al = lib.create_virtual_album("Grouped only", 0).unwrap();
        lib.set_virtual_album_rules_grouped(
            al,
            RuleMatch::And,
            &[],
            &[
                RuleGroup {
                    id: 0,
                    rule_match: RuleMatch::Or,
                    rules: vec![rule(RuleField::Tag, RuleOp::Has, "vacation")],
                },
                RuleGroup {
                    id: 0,
                    rule_match: RuleMatch::Or,
                    rules: vec![rule(RuleField::Tag, RuleOp::Has, "sunny")],
                },
            ],
        )
        .unwrap();
        let ids: Vec<i64> = lib
            .photos_in_virtual_album(al)
            .unwrap()
            .iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(ids, vec![p1]);
        cleanup(p);
    }
}
