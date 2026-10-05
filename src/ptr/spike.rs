//! Phase 0 spike. Measures the PTR with a small sample download.
//!
//! Run: cargo test --release spike_ptr -- --ignored --nocapture
//! Set PICHOUSE_PTR_KEY=<access key> (see ACCESS_KEY_HELP_URL).
//! Set PICHOUSE_PTR_SAMPLES=<n> to change the sample count (default 12).
//! The test keeps no files. It holds the samples in memory only.

use super::client::{decode, Client, ACCESS_KEY_HELP_URL, DEFAULT_BASE_URL};
use super::update::{parse, Update, ACTION_ADD, CONTENT_PARENTS, CONTENT_SIBLINGS};

#[test]
#[ignore]
fn spike_ptr() {
    let key = std::env::var("PICHOUSE_PTR_KEY")
        .unwrap_or_else(|_| panic!("set PICHOUSE_PTR_KEY; get the key at {ACCESS_KEY_HELP_URL}"));
    let client = Client::new(DEFAULT_BASE_URL, &key).unwrap();
    let t = std::time::Instant::now();
    let meta = client.metadata(0).unwrap();
    let all: Vec<(u64, &String)> = meta
        .entries
        .iter()
        .flat_map(|e| e.hashes.iter().map(move |h| (e.index, h)))
        .collect();
    println!(
        "metadata: {} indexes, {} update files, last index {}, next due {} ({:?})",
        meta.entries.len(),
        all.len(),
        meta.entries.last().map(|e| e.index).unwrap_or(0),
        meta.next_update_due,
        t.elapsed()
    );

    let n: usize = std::env::var("PICHOUSE_PTR_SAMPLES").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
    // Spread the samples evenly over the whole history.
    let picks: Vec<usize> = (0..n).map(|i| i * (all.len() - 1) / (n - 1).max(1)).collect();

    let (mut sum_raw, mut sum_json) = (0usize, 0usize);
    let (mut def_n, mut def_raw, mut con_n, mut con_raw) = (0usize, 0usize, 0usize, 0usize);
    let (mut sum_hashes, mut sum_tags, mut sum_map_add, mut sum_map_del) = (0usize, 0usize, 0usize, 0usize);
    for &i in &picks {
        let (index, hash) = all[i];
        let t = std::time::Instant::now();
        let raw = match client.update_raw(hash) {
            Ok(r) => r,
            Err(e) => {
                println!("#{index} {hash}: ERROR {e}");
                continue;
            }
        };
        let dl = t.elapsed();
        let v = decode(&raw).unwrap();
        let json_len = serde_json::to_string(&v).unwrap().len();
        sum_raw += raw.len();
        sum_json += json_len;
        match parse(&v).unwrap() {
            Update::Definitions { hashes, tags } => {
                def_n += 1;
                def_raw += raw.len();
                sum_hashes += hashes.len();
                sum_tags += tags.len();
                println!(
                    "#{index:>6} DEF  raw {:>9} json {:>10} hashes {:>8} tags {:>8} ({dl:?}) e.g. {:?}",
                    raw.len(), json_len, hashes.len(), tags.len(), tags.first().map(|t| &t.1)
                );
            }
            Update::Content { mappings, pairs } => {
                con_n += 1;
                con_raw += raw.len();
                let add: usize = mappings.iter().filter(|m| m.0 == ACTION_ADD).map(|m| m.2.len()).sum();
                let del: usize = mappings.iter().filter(|m| m.0 != ACTION_ADD).map(|m| m.2.len()).sum();
                sum_map_add += add;
                sum_map_del += del;
                let sib = pairs.iter().filter(|p| p.0 == CONTENT_SIBLINGS).count();
                let par = pairs.iter().filter(|p| p.0 == CONTENT_PARENTS).count();
                println!(
                    "#{index:>6} CONT raw {:>9} json {:>10} map+ {:>9} map- {:>8} sib {:>6} par {:>6} ({dl:?})",
                    raw.len(), json_len, add, del, sib, par
                );
            }
        }
    }

    let got = def_n + con_n;
    if got == 0 {
        return;
    }
    let scale = all.len() as f64 / got as f64;
    let gb = |b: f64| b / 1e9;
    println!("--- sample: {got} files, {def_n} definitions, {con_n} content");
    println!("sample raw {} B, json {} B", sum_raw, sum_json);
    println!(
        "def share {:.2}, avg def raw {} B, avg content raw {} B",
        def_n as f64 / got as f64,
        def_raw / def_n.max(1),
        con_raw / con_n.max(1)
    );
    println!("est. total download: {:.1} GB", gb(sum_raw as f64 * scale));
    let est_hashes = sum_hashes as f64 * scale;
    let est_tags = sum_tags as f64 * scale;
    let est_map = (sum_map_add as f64 - sum_map_del as f64) * scale;
    println!(
        "est. hashes {:.0} M, tags {:.0} M, net mappings {:.0} M",
        est_hashes / 1e6, est_tags / 1e6, est_map / 1e6
    );
    // Rough SQLite cost: hash row ~ 32 B blob + id + index copy ~ 90 B;
    // tag row ~ 40 B; mapping row in a WITHOUT ROWID (tag, hash) table plus
    // a (hash, tag) index ~ 2 x 12 B.
    let est_db = est_hashes * 90.0 + est_tags * 40.0 + est_map * 24.0;
    println!("est. ptr.db size: {:.1} GB (rough)", gb(est_db));
}
