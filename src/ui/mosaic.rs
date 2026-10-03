//! The 3x3 face mosaic on a folder tile in the People and Characters views.
//!
//! The mosaic shows one representative face for each member, up to 9. When a
//! folder has fewer than 9 members, random faces of the members fill the empty
//! cells. The random pick uses a seed from the folder id and the face count.
//! So the mosaic changes only when the number of faces in the folder changes.

use std::collections::HashMap;

use gtk4::prelude::*;
use gtk4::{Grid, Image};

/// The number of cells in the mosaic.
pub const CELLS: usize = 9;

/// A small deterministic random generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Pick up to 9 face ids for a folder mosaic.
///
/// `owners` is the member list in display order. `reps` maps a member to its
/// representative face. `pool` holds every `(member, face)` pair. The result
/// starts with one representative face for each member. Random faces of random
/// members fill the remaining cells. A face never shows twice.
pub fn pick_faces(
    folder_id: i64,
    owners: &[i64],
    reps: &HashMap<i64, i64>,
    pool: &[(i64, i64)],
) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for o in owners {
        if out.len() >= CELLS {
            break;
        }
        let rep = reps
            .get(o)
            .copied()
            .filter(|f| *f != 0)
            .or_else(|| pool.iter().find(|(p, _)| p == o).map(|(_, f)| *f));
        if let Some(f) = rep {
            if !out.contains(&f) {
                out.push(f);
            }
        }
    }

    // The remaining faces of each member, in a stable order.
    let mut left: Vec<(i64, Vec<i64>)> = Vec::new();
    for o in owners {
        let faces: Vec<i64> = pool
            .iter()
            .filter(|(p, f)| p == o && !out.contains(f))
            .map(|(_, f)| *f)
            .collect();
        if !faces.is_empty() {
            left.push((*o, faces));
        }
    }

    let seed = (folder_id as u64).wrapping_mul(0x1000_0000_01B3) ^ (pool.len() as u64);
    let mut rng = Rng(seed);
    while out.len() < CELLS && !left.is_empty() {
        let mi = rng.below(left.len());
        let faces = &mut left[mi].1;
        let fi = rng.below(faces.len());
        out.push(faces.swap_remove(fi));
        if faces.is_empty() {
            left.swap_remove(mi);
        }
    }
    out
}

/// Build the 3x3 mosaic widget. `fill` loads one face crop into one cell.
/// Cells with no face stay empty.
pub fn build(tile_px: i32, faces: &[i64], fill: impl Fn(&Image, i64)) -> Grid {
    let grid = Grid::new();
    grid.set_size_request(tile_px, tile_px);
    grid.set_row_homogeneous(true);
    grid.set_column_homogeneous(true);
    grid.set_halign(gtk4::Align::Center);
    let cell = (tile_px / 3).max(1);
    for i in 0..CELLS {
        let image = Image::new();
        image.set_pixel_size(cell);
        image.set_size_request(cell, cell);
        if let Some(f) = faces.get(i) {
            fill(&image, *f);
        }
        grid.attach(&image, (i % 3) as i32, (i / 3) as i32, 1, 1);
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reps_first_then_fill_without_repeats() {
        let owners = vec![1, 2];
        let reps: HashMap<i64, i64> = [(1, 10), (2, 20)].into_iter().collect();
        let pool: Vec<(i64, i64)> = (10..16)
            .map(|f| (1, f))
            .chain((20..26).map(|f| (2, f)))
            .collect();
        let got = pick_faces(7, &owners, &reps, &pool);
        assert_eq!(got.len(), 9);
        assert_eq!(&got[..2], &[10, 20]);
        let mut d = got.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), 9);
    }

    #[test]
    fn stable_for_same_count_and_short_pool() {
        let owners = vec![1];
        let reps: HashMap<i64, i64> = [(1, 1)].into_iter().collect();
        let pool: Vec<(i64, i64)> = (1..=20).map(|f| (1, f)).collect();
        assert_eq!(
            pick_faces(3, &owners, &reps, &pool),
            pick_faces(3, &owners, &reps, &pool)
        );
        let small: Vec<(i64, i64)> = (1..=4).map(|f| (1, f)).collect();
        assert_eq!(pick_faces(3, &owners, &reps, &small).len(), 4);
    }
}
