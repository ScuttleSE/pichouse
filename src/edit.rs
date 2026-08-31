//! Shared non-destructive edit pipeline.
//!
//! One function, [`apply_edits`], turns a decoded RGBA image plus a
//! [`PhotoEdit`] into the edited RGBA image. The viewer and the thumbnail
//! generator both call it, so the full-size view and the thumbnail always
//! agree. Originals on disk are never changed.
//!
//! Order of operations (the stored `photos.orientation` 90-degree rotation is
//! already applied by the caller before this runs):
//!   1. flip horizontal / vertical
//!   2. straighten (arbitrary small angle) + auto-crop of the empty corners
//!   3. crop (per-mille rectangle)
//!   4. color levels (per-channel black/white/gamma)
//!   5. brightness / contrast

use image::{Rgba, RgbaImage};

use crate::model::{Levels, PhotoEdit};

/// Apply every edit in `edit` to `img` and return the result. When the edit is
/// the identity, `img` is returned unchanged.
pub fn apply_edits(img: RgbaImage, edit: &PhotoEdit) -> RgbaImage {
    if edit.is_identity() {
        return img;
    }
    let mut img = img;
    if edit.flip_h {
        image::imageops::flip_horizontal_in_place(&mut img);
    }
    if edit.flip_v {
        image::imageops::flip_vertical_in_place(&mut img);
    }
    if edit.straighten_mdeg != 0 {
        img = straighten(&img, edit.straighten_mdeg as f32 / 1000.0);
    }
    if edit.crop_w > 0 && edit.crop_h > 0 {
        img = crop_permille(&img, edit.crop_x, edit.crop_y, edit.crop_w, edit.crop_h);
    }
    if !edit.levels.is_identity() {
        apply_levels(&mut img, &edit.levels);
    }
    if edit.brightness != 0 || edit.contrast != 0 {
        apply_brightness_contrast(&mut img, edit.brightness, edit.contrast);
    }
    img
}

/// Crop `img` to a rectangle given in per-mille (0..1000) of its dimensions.
fn crop_permille(img: &RgbaImage, x: i32, y: i32, w: i32, h: i32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let px = ((x.clamp(0, 1000) as u32) * iw) / 1000;
    let py = ((y.clamp(0, 1000) as u32) * ih) / 1000;
    let pw = (((w.clamp(0, 1000) as u32) * iw) / 1000).max(1);
    let ph = (((h.clamp(0, 1000) as u32) * ih) / 1000).max(1);
    let pw = pw.min(iw.saturating_sub(px));
    let ph = ph.min(ih.saturating_sub(py));
    if pw == 0 || ph == 0 {
        return img.clone();
    }
    image::imageops::crop_imm(img, px, py, pw, ph).to_image()
}

/// Rotate `img` by `degrees` clockwise about its center with bilinear sampling,
/// then crop to the largest axis-aligned rectangle (same aspect ratio as the
/// source) that contains only real pixels. This removes the empty corners a
/// straighten introduces.
fn straighten(img: &RgbaImage, degrees: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;

    // Rotate into a same-size buffer; sample the source with the inverse map.
    let mut out = RgbaImage::new(w, h);
    for oy in 0..h {
        for ox in 0..w {
            let dx = ox as f32 + 0.5 - cx;
            let dy = oy as f32 + 0.5 - cy;
            // Inverse rotation (source = R(-theta) * dest).
            let sx = cos * dx + sin * dy + cx - 0.5;
            let sy = -sin * dx + cos * dy + cy - 0.5;
            out.put_pixel(ox, oy, sample_bilinear(img, sx, sy));
        }
    }

    // Largest inscribed rectangle with the source aspect ratio.
    let (cw, ch) = largest_inner_rect(w as f32, h as f32, rad.abs());
    let cw = cw.floor().max(1.0) as u32;
    let ch = ch.floor().max(1.0) as u32;
    let x0 = (w.saturating_sub(cw)) / 2;
    let y0 = (h.saturating_sub(ch)) / 2;
    image::imageops::crop_imm(&out, x0, y0, cw.min(w), ch.min(h)).to_image()
}

/// Bilinear sample of `img` at floating (`x`, `y`); out-of-range = transparent.
fn sample_bilinear(img: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return Rgba([0, 0, 0, 0]);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
        let bot = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
        out[c] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

/// Dimensions of the largest axis-aligned rectangle with the same aspect ratio
/// as `w`x`h` that fits inside `w`x`h` after a rotation of `angle` radians.
fn largest_inner_rect(w: f32, h: f32, angle: f32) -> (f32, f32) {
    let angle = angle.abs();
    if angle < 1e-4 {
        return (w, h);
    }
    let (sin, cos) = angle.sin_cos();
    // For an axis-aligned rect of the source aspect placed in the rotated frame,
    // scale it down uniformly until both constraints are met.
    let denom = w * cos + h * sin;
    let denom2 = w * sin + h * cos;
    let scale = (w / denom).min(h / denom2);
    (w * scale, h * scale)
}

/// Apply per-channel color levels in place using precomputed LUTs.
fn apply_levels(img: &mut RgbaImage, lv: &Levels) {
    let lut_r = channel_lut(lv.r_black, lv.r_white, lv.r_gamma_mille);
    let lut_g = channel_lut(lv.g_black, lv.g_white, lv.g_gamma_mille);
    let lut_b = channel_lut(lv.b_black, lv.b_white, lv.b_gamma_mille);
    for px in img.pixels_mut() {
        px.0[0] = lut_r[px.0[0] as usize];
        px.0[1] = lut_g[px.0[1] as usize];
        px.0[2] = lut_b[px.0[2] as usize];
    }
}

/// Build a 256-entry lookup table mapping an input value through the levels
/// transform: clamp to [black, white], normalize, apply gamma, scale to 0..255.
fn channel_lut(black: i32, white: i32, gamma_mille: i32) -> [u8; 256] {
    let black = black.clamp(0, 255) as f32;
    let white = white.clamp(0, 255) as f32;
    let span = (white - black).max(1.0);
    let gamma = (gamma_mille.max(1) as f32) / 1000.0;
    let inv_gamma = 1.0 / gamma;
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = ((i as f32 - black) / span).clamp(0.0, 1.0);
        let v = v.powf(inv_gamma);
        *slot = (v * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Apply brightness (-100..100 as a -128..128 offset) and contrast (-100..100)
/// in place, RGB only.
fn apply_brightness_contrast(img: &mut RgbaImage, brightness: i32, contrast: i32) {
    let b = (brightness.clamp(-100, 100) as f32) * 1.28;
    // Standard contrast factor.
    let c = contrast.clamp(-100, 100) as f32;
    let factor = (259.0 * (c + 255.0)) / (255.0 * (259.0 - c));
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = factor * (i as f32 - 128.0) + 128.0 + b;
        *slot = v.round().clamp(0.0, 255.0) as u8;
    }
    for px in img.pixels_mut() {
        px.0[0] = lut[px.0[0] as usize];
        px.0[1] = lut[px.0[1] as usize];
        px.0[2] = lut[px.0[2] as usize];
    }
}

/// Compute per-channel auto levels from `img`: pick each channel's black and
/// white input points so that `clip` fraction (e.g. 0.005 = 0.5%) of pixels are
/// clipped at each end. Gamma is left at 1.0. This removes the color cast common
/// in scanned negatives.
pub fn auto_levels(img: &RgbaImage, clip: f32) -> Levels {
    let mut hist = [[0u32; 256]; 3];
    for px in img.pixels() {
        hist[0][px.0[0] as usize] += 1;
        hist[1][px.0[1] as usize] += 1;
        hist[2][px.0[2] as usize] += 1;
    }
    let total = (img.width() as u64 * img.height() as u64).max(1);
    let cut = ((total as f32) * clip).round() as u64;
    let mut lv = Levels::default();
    let (rb, rw) = channel_bounds(&hist[0], cut);
    let (gb, gw) = channel_bounds(&hist[1], cut);
    let (bb, bw) = channel_bounds(&hist[2], cut);
    lv.r_black = rb;
    lv.r_white = rw;
    lv.g_black = gb;
    lv.g_white = gw;
    lv.b_black = bb;
    lv.b_white = bw;
    lv
}

/// Find the black and white input points for one channel histogram, clipping
/// `cut` pixels from each tail.
fn channel_bounds(hist: &[u32; 256], cut: u64) -> (i32, i32) {
    let mut acc = 0u64;
    let mut black = 0i32;
    for (i, &n) in hist.iter().enumerate() {
        acc += n as u64;
        if acc > cut {
            black = i as i32;
            break;
        }
    }
    acc = 0;
    let mut white = 255i32;
    for i in (0..256).rev() {
        acc += hist[i] as u64;
        if acc > cut {
            white = i as i32;
            break;
        }
    }
    if white <= black {
        (0, 255)
    } else {
        (black, white)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_noop() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([100, 120, 140, 255]));
        let edit = PhotoEdit::default();
        let out = apply_edits(img.clone(), &edit);
        assert_eq!(out, img);
    }

    #[test]
    fn levels_stretch_full_range() {
        // A flat mid-gray image with black=50 white=200 maps toward full range.
        let mut img = RgbaImage::from_pixel(2, 2, Rgba([200, 200, 200, 255]));
        let lv = Levels {
            r_white: 200,
            g_white: 200,
            b_white: 200,
            ..Default::default()
        };
        apply_levels(&mut img, &lv);
        // Input 200 at white point 200 -> full 255.
        assert_eq!(img.get_pixel(0, 0).0[0], 255);
    }

    #[test]
    fn auto_levels_finds_range() {
        // Half black, half white pixels -> bounds near 0 and 255.
        let mut img = RgbaImage::new(10, 10);
        for (i, px) in img.pixels_mut().enumerate() {
            let v = if i % 2 == 0 { 30 } else { 210 };
            *px = Rgba([v, v, v, 255]);
        }
        let lv = auto_levels(&img, 0.0);
        assert!(lv.r_black <= 30 && lv.r_white >= 210);
    }

    #[test]
    fn crop_reduces_size() {
        let img = RgbaImage::from_pixel(100, 100, Rgba([1, 2, 3, 255]));
        let out = crop_permille(&img, 250, 250, 500, 500);
        assert_eq!(out.dimensions(), (50, 50));
    }
}
