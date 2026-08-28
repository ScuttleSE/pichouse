//! Perceptual hashing for the duplicate image finder.
//!
//! We use a 64-bit dHash (difference hash). The image is reduced to a small
//! grayscale grid. Each output bit records whether one pixel is brighter than
//! its right neighbour. dHash is robust to resize, re-compression, and minor
//! edits. Two near-duplicate images give hashes with a small Hamming distance.

use std::path::Path;

use crate::thumb;

/// Width of the reduced grid. We compare adjacent columns, so the hash has
/// `DHASH_H * DHASH_W_CMP` bits, where `DHASH_W_CMP = DHASH_W - 1`.
const DHASH_W: u32 = 9;
const DHASH_H: u32 = 8;

/// Compute the 64-bit dHash of the image at `path`, honouring `orientation`
/// (rotation in degrees clockwise). Returns `0` on any decode error, which the
/// caller treats as "not hashed".
pub fn dhash_file(path: &Path, orientation: i32) -> u64 {
    // Decode oriented RGB at a small size. We pass a generous max side and then
    // downscale to the exact grid below, so aspect changes do not skew the hash.
    match thumb::decode_oriented_rgb(path, orientation, 64) {
        Ok((rgb, w, h)) => dhash_rgb(&rgb, w, h),
        Err(_) => 0,
    }
}

/// Compute the dHash from a tight RGB8 buffer of size `w * h * 3`.
pub fn dhash_rgb(rgb: &[u8], w: u32, h: u32) -> u64 {
    if w == 0 || h == 0 || rgb.len() < (w as usize) * (h as usize) * 3 {
        return 0;
    }
    // Nearest-neighbour sample the source into a DHASH_W x DHASH_H grayscale
    // grid. This avoids pulling in a resize dependency for such a small target.
    let mut grid = [[0u16; DHASH_W as usize]; DHASH_H as usize];
    for gy in 0..DHASH_H {
        let sy = (gy * h) / DHASH_H;
        for gx in 0..DHASH_W {
            let sx = (gx * w) / DHASH_W;
            let idx = ((sy * w + sx) as usize) * 3;
            let r = rgb[idx] as u16;
            let g = rgb[idx + 1] as u16;
            let b = rgb[idx + 2] as u16;
            // Fast integer luma approximation.
            grid[gy as usize][gx as usize] = (r * 3 + g * 6 + b) / 10;
        }
    }
    let mut hash: u64 = 0;
    let mut bit = 0;
    for row in grid.iter() {
        for x in 0..(DHASH_W as usize - 1) {
            if row[x] > row[x + 1] {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// The number of differing bits between two hashes. `0` means identical hashes.
#[inline]
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_match() {
        let rgb = vec![128u8; 64 * 64 * 3];
        let a = dhash_rgb(&rgb, 64, 64);
        let b = dhash_rgb(&rgb, 64, 64);
        assert_eq!(a, b);
        assert_eq!(hamming(a, b), 0);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(dhash_rgb(&[], 0, 0), 0);
    }

    #[test]
    fn hamming_counts_bits() {
        assert_eq!(hamming(0b1011, 0b0010), 2);
    }
}
