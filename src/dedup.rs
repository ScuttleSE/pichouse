//! Duplicate image finder engine.
//!
//! The engine groups photos that are the same picture. It uses two signals:
//!
//! * the SHA-256 content hash for byte-identical files (exact duplicates), and
//! * a 64-bit dHash for visually similar files (near-duplicates), matched by a
//!   Hamming-distance threshold.
//!
//! Each group names a "keep" photo. The keep photo is the best copy by a
//! quality ranking. Every other photo in the group is a delete candidate.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::Photo;

/// A group of photos that are the same picture. `keep_id` is the id of the best
/// copy to keep. All other photos are delete candidates.
#[derive(Debug, Clone)]
pub struct DupGroup {
    pub photos: Vec<Photo>,
    pub keep_id: i64,
}

/// Format-quality rank. A higher number is a more preferred (more lossless)
/// format. Unknown extensions get the lowest rank.
fn format_rank(filename: &str) -> u8 {
    let ext = filename
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "tif" | "tiff" | "bmp" => 3,
        "webp" => 2,
        "jpg" | "jpeg" => 1,
        _ => 0,
    }
}

/// Choose the "keep" photo of a group. Better means, in order: larger pixel
/// area, then more lossless format, then larger file size, then older
/// `added_at`, then lower id. The comparison is total, so ties break the same
/// way every run.
fn choose_keep(photos: &[Photo]) -> i64 {
    photos
        .iter()
        .max_by(|a, b| {
            let area_a = (a.width as i64) * (a.height as i64);
            let area_b = (b.width as i64) * (b.height as i64);
            area_a
                .cmp(&area_b)
                .then(format_rank(&a.filename).cmp(&format_rank(&b.filename)))
                .then(a.size.cmp(&b.size))
                .then(b.added_at.cmp(&a.added_at)) // older added_at wins
                .then(b.id.cmp(&a.id)) // lower id wins
        })
        .map(|p| p.id)
        .unwrap_or(0)
}

/// A simple union-find over vector indices, used to merge near-duplicate pairs
/// into connected groups.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Find duplicate groups among `photos`.
///
/// `threshold` is the maximum Hamming distance (0..=64) between two dHashes for
/// a near-duplicate match. A threshold of `0` matches only identical dHashes
/// (in practice, exact or visually identical). Photos with a `0` perceptual
/// hash (not computable) match only by exact SHA-256.
///
/// The scan stops early and returns what it has if `cancel` is set.
pub fn find_duplicates(photos: &[Photo], threshold: u32, cancel: &AtomicBool) -> Vec<DupGroup> {
    let n = photos.len();
    let mut uf = UnionFind::new(n);

    // Exact pass: union photos that share a non-empty SHA-256 hash.
    {
        use std::collections::HashMap;
        let mut by_hash: HashMap<&str, usize> = HashMap::new();
        for (i, p) in photos.iter().enumerate() {
            if p.hash.is_empty() {
                continue;
            }
            match by_hash.get(p.hash.as_str()) {
                Some(&j) => uf.union(i, j),
                None => {
                    by_hash.insert(p.hash.as_str(), i);
                }
            }
        }
    }

    // Near pass: pairwise Hamming compare on the perceptual hash. O(n^2) in the
    // worst case, but the photo set is one scope (album), not the whole library.
    if threshold > 0 {
        for i in 0..n {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let hi = photos[i].phash;
            if hi == 0 {
                continue;
            }
            for j in (i + 1)..n {
                let hj = photos[j].phash;
                if hj == 0 {
                    continue;
                }
                if crate::phash::hamming(hi, hj) <= threshold {
                    uf.union(i, j);
                }
            }
        }
    }

    // Collect connected components with more than one member.
    use std::collections::HashMap;
    let mut groups: HashMap<usize, Vec<Photo>> = HashMap::new();
    for i in 0..n {
        let root = uf.find(i);
        groups.entry(root).or_default().push(photos[i].clone());
    }

    let mut out: Vec<DupGroup> = groups
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|g| {
            let keep_id = choose_keep(&g);
            DupGroup {
                photos: g,
                keep_id,
            }
        })
        .collect();

    // Stable output: order groups by their keep photo id, and photos within a
    // group with the keep first, then by id.
    for g in out.iter_mut() {
        let keep = g.keep_id;
        g.photos.sort_by(|a, b| {
            (b.id == keep)
                .cmp(&(a.id == keep))
                .then(a.id.cmp(&b.id))
        });
    }
    out.sort_by_key(|g| g.keep_id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(id: i64, hash: &str, phash: u64, w: i32, h: i32, name: &str) -> Photo {
        Photo {
            id,
            hash: hash.to_string(),
            phash,
            width: w,
            height: h,
            filename: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn exact_hash_groups() {
        let ps = vec![
            photo(1, "aa", 0, 100, 100, "a.jpg"),
            photo(2, "aa", 0, 100, 100, "b.jpg"),
            photo(3, "bb", 0, 100, 100, "c.jpg"),
        ];
        let g = find_duplicates(&ps, 0, &AtomicBool::new(false));
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].photos.len(), 2);
    }

    #[test]
    fn near_phash_groups_within_threshold() {
        let ps = vec![
            photo(1, "", 0b1111, 100, 100, "a.jpg"),
            photo(2, "", 0b1110, 100, 100, "b.jpg"),
        ];
        assert_eq!(find_duplicates(&ps, 1, &AtomicBool::new(false)).len(), 1);
        assert_eq!(find_duplicates(&ps, 0, &AtomicBool::new(false)).len(), 0);
    }

    #[test]
    fn keep_prefers_larger_then_lossless() {
        let ps = vec![
            photo(1, "aa", 0, 100, 100, "small.jpg"),
            photo(2, "aa", 0, 200, 200, "big.jpg"),
        ];
        let g = find_duplicates(&ps, 0, &AtomicBool::new(false));
        assert_eq!(g[0].keep_id, 2);

        let ps = vec![
            photo(1, "aa", 0, 100, 100, "x.jpg"),
            photo(2, "aa", 0, 100, 100, "x.png"),
        ];
        let g = find_duplicates(&ps, 0, &AtomicBool::new(false));
        assert_eq!(g[0].keep_id, 2);
    }
}
