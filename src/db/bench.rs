//! Query benchmarks against a real library copy. These tests are ignored by
//! default. Run them with:
//!
//!     PICHOUSE_BENCH_DB=library-copy.db cargo test --release bench_ -- --ignored --nocapture --test-threads=1
//!
//! The benchmarks change the database file. Use a copy only.

use std::time::{Duration, Instant};

use rusqlite::params;

use crate::model::{Face, RuleField, RuleMatch, RuleOp, VirtualRule};

use super::Library;

fn open() -> Option<Library> {
    let path = std::env::var("PICHOUSE_BENCH_DB").ok()?;
    let t = Instant::now();
    let lib = Library::open_at(&path).expect("open bench db");
    println!("open_at (schema + migrate): {:.2?}", t.elapsed());
    Some(lib)
}

/// Run `f` three times. Print the median time.
fn time<T>(name: &str, mut f: impl FnMut() -> T) -> T {
    let mut times: Vec<Duration> = Vec::new();
    let mut out = None;
    for _ in 0..3 {
        let t = Instant::now();
        out = Some(f());
        times.push(t.elapsed());
    }
    times.sort();
    println!("{name:<40} {:>10.2?}", times[1]);
    out.unwrap()
}

/// Add fake persons, faces, and virtual albums when the tables are empty.
fn seed(lib: &Library) {
    let mut conn = lib.lock();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM faces", [], |r| r.get(0))
        .unwrap();
    if n == 0 {
        let t = Instant::now();
        let tx = conn.transaction().unwrap();
        for i in 0..200 {
            tx.execute("INSERT INTO persons(name) VALUES(?1)", params![format!("Person {i}")])
                .unwrap();
        }
        let first_person: i64 = tx
            .query_row("SELECT MIN(id) FROM persons", [], |r| r.get(0))
            .unwrap();
        let ids: Vec<i64> = {
            let mut s = tx
                .prepare("SELECT id FROM photos WHERE missing = 0 ORDER BY id LIMIT 150000")
                .unwrap();
            let v = s.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
            v
        };
        let emb = vec![0u8; 512];
        {
            let mut ins = tx
                .prepare(
                    "INSERT INTO faces(photo_id, person_id, cluster_id, bbox_x, bbox_y, bbox_w, \
                     bbox_h, embedding, embedding_dim, det_score, created_at, ignored) \
                     VALUES(?1,?2,?3,10,10,100,100,?4,128,?5,?6,?7)",
                )
                .unwrap();
            let mut scan = tx
                .prepare("INSERT OR REPLACE INTO face_scan(photo_id, state, scanned_at) VALUES(?1, 2, 0)")
                .unwrap();
            for (k, &pid) in ids.iter().enumerate() {
                scan.execute(params![pid]).unwrap();
                for j in 0..2 {
                    let f = (k * 2 + j) as i64;
                    let cluster = f % 2000 + 1;
                    // About half the clusters have a person.
                    let person: Option<i64> =
                        if cluster % 2 == 0 { Some(first_person + cluster % 200) } else { None };
                    let ignored = if f % 500 == 0 { 1 } else { 0 };
                    let (person, cluster) = if ignored == 1 { (None, None) } else { (person, Some(cluster)) };
                    ins.execute(params![pid, person, cluster, emb, (f * 7919) % 1000, f, ignored])
                        .unwrap();
                }
            }
        }
        tx.commit().unwrap();
        println!("seed faces: {:.2?}", t.elapsed());
    }
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM virtual_albums", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    if n == 0 {
        let t = Instant::now();
        for i in 0..50 {
            let id = lib.create_virtual_album(&format!("VA {i}"), 0).unwrap();
            let rules = vec![
                VirtualRule {
                    id: 0,
                    album_id: id,
                    field: RuleField::Filename,
                    op: RuleOp::Contains,
                    value: format!("{}", i % 10),
                },
                VirtualRule {
                    id: 0,
                    album_id: id,
                    field: RuleField::DateFrom,
                    op: RuleOp::Gte,
                    value: format!("{}", 1_400_000_000 + i * 10_000_000),
                },
            ];
            lib.set_virtual_album_rules(id, RuleMatch::And, &rules).unwrap();
            let pins: Vec<i64> = (0..200).map(|k| (i * 1000 + k) as i64 + 1).collect();
            let _ = lib.add_photos_to_virtual_album(id, &pins);
        }
        println!("seed virtual albums: {:.2?}", t.elapsed());
    }
}

#[test]
#[ignore]
fn bench_all() {
    let Some(lib) = open() else {
        println!("PICHOUSE_BENCH_DB is not set");
        return;
    };
    seed(&lib);

    // H1: autoscan routing.
    time("H1 autoscan route (face)", || {
        let mut n = 0;
        for id in lib.photos_needing_face_scan(100_000).unwrap() {
            if lib.photo_effective_face_kind(id).unwrap() != 2 {
                n += 1;
            }
        }
        n
    });
    time("H1 autoscan route (art)", || {
        let mut n = 0;
        for id in lib.photos_needing_style_face_scan(100_000).unwrap() {
            if lib.photo_effective_face_kind(id).unwrap() == 2 {
                n += 1;
            }
        }
        n
    });
    let folders: Vec<i64> = lib.folders().unwrap().iter().map(|f| f.id).collect();
    time("H1 folder kinds (all folders)", || {
        folders
            .iter()
            .filter(|&&f| lib.folder_effective_face_kind(f).unwrap() == 2)
            .count()
    });

    // H2: virtual album counts.
    let vas = lib.virtual_albums().unwrap();
    time("H2 virtual album counts", || {
        vas.iter()
            .map(|v| lib.virtual_album_photo_count(v.id).unwrap())
            .sum::<i64>()
    });

    // H3: unnamed person cluster tiles.
    time("H3 unnamed cluster tiles", || {
        let mut n = 0;
        for (cid, _) in lib.unnamed_clusters().unwrap() {
            if let Some(f) = lib.unassigned_faces_in_cluster(cid).unwrap().first() {
                n += f.id;
            }
        }
        n
    });

    // H4: representative faces.
    time("H4 person rep faces", || {
        lib.persons()
            .unwrap()
            .iter()
            .map(|(p, _)| lib.person_representative_face(p.id).unwrap())
            .sum::<i64>()
    });
    time("H4 character rep faces", || {
        lib.characters()
            .unwrap()
            .iter()
            .map(|(c, _)| lib.character_representative_face(c.id).unwrap())
            .sum::<i64>()
    });
    time("H4 style cluster rep faces", || {
        lib.unnamed_style_clusters()
            .unwrap()
            .iter()
            .map(|(c, _)| lib.cluster_representative_face(*c).unwrap())
            .sum::<i64>()
    });

    // M1: reconcile snapshot.
    time("M1 reconcile snapshot", || {
        let mut n = 0;
        for f in lib.folders().unwrap() {
            n += lib.photo_index_for_folder(f.id).unwrap().len();
        }
        n
    });

    // Sidebar counts.
    time("sidebar missing_photo_count", || lib.missing_photo_count().unwrap());
    time("sidebar ignored face count", || lib.ignored_face_photo_count().unwrap());
    time("sidebar ignored style count", || {
        lib.ignored_style_face_photo_count().unwrap()
    });
    time("M7 persons()", || lib.persons().unwrap().len());
    time("M7 characters()", || lib.characters().unwrap().len());

    // H5: face scan writes for 1,000 photos with 3 faces each.
    let ids: Vec<i64> = {
        let conn = lib.lock();
        let mut s = conn
            .prepare("SELECT id FROM photos ORDER BY id DESC LIMIT 1000")
            .unwrap();
        let v = s.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        v
    };
    let emb = vec![0.5f32; 128];
    let t = Instant::now();
    for &id in &ids {
        let _ = lib.photo_by_id(id);
        lib.set_face_scan_state(id, 1).unwrap();
        let ignored = lib.ignored_face_boxes(id).unwrap();
        lib.clear_faces_for_photo(id).unwrap();
        for k in 0..3 {
            let row = Face {
                photo_id: id,
                bbox_x: k * 100,
                bbox_w: 50,
                bbox_h: 50,
                embedding: emb.clone(),
                det_score: 0.9,
                ..Default::default()
            };
            let fid = lib.insert_face(&row).unwrap();
            let b = (row.bbox_x, row.bbox_y, row.bbox_w, row.bbox_h);
            if ignored.iter().any(|&ib| super::faces::box_matches(b, ib)) {
                lib.set_face_ignored(fid).unwrap();
            }
        }
        lib.set_face_scan_state(id, 2).unwrap();
    }
    println!("{:<40} {:>10.2?}", "H5 scan writes, 1000 photos", t.elapsed());
}
