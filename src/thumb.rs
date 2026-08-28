//! Thumbnail generation and per-size caching.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use fast_image_resize::images::{Image as FirImage, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{ImageEncoder, RgbaImage};

use crate::db::{self, Thumbs};

/// The default maximum thumbnail dimension in pixels.
pub const DEFAULT_SIZE: i32 = 320;

/// Produces thumbnails and caches them in per-size thumbnail databases. Each
/// size uses its own database file, so switching quality never overwrites
/// another size's cache. Set `all_sizes` to pre-generate every size on a miss.
pub struct Generator {
    inner: Mutex<Inner>,
}

struct Inner {
    size: i32,
    all_sizes: Vec<i32>,
    stores: HashMap<i32, Thumbs>,
}

/// A thumbnail error.
#[derive(Debug)]
pub enum Error {
    Db(db::Error),
    Image(image::ImageError),
    Resize(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Db(e) => write!(f, "db: {e}"),
            Error::Image(e) => write!(f, "image: {e}"),
            Error::Resize(e) => write!(f, "resize: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<db::Error> for Error {
    fn from(e: db::Error) -> Self {
        Error::Db(e)
    }
}

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Error::Image(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

impl Generator {
    /// Create a generator caching into per-size databases. `size <= 0` uses
    /// `DEFAULT_SIZE`.
    pub fn new(size: i32) -> Generator {
        let size = if size <= 0 { DEFAULT_SIZE } else { size };
        Generator {
            inner: Mutex::new(Inner {
                size,
                all_sizes: Vec::new(),
                stores: HashMap::new(),
            }),
        }
    }

    /// Select the active thumbnail size used by `get`.
    pub fn set_size(&self, size: i32) {
        let size = if size <= 0 { DEFAULT_SIZE } else { size };
        self.inner.lock().unwrap().size = size;
    }

    /// The active thumbnail size.
    #[allow(dead_code)] // Kept API accessor.
    pub fn size(&self) -> i32 {
        self.inner.lock().unwrap().size
    }

    /// Set the sizes to pre-generate on a cache miss. Pass an empty slice to
    /// disable pre-generation.
    pub fn set_all_sizes(&self, sizes: &[i32]) {
        self.inner.lock().unwrap().all_sizes = sizes.to_vec();
    }

    /// Close all open per-size stores.
    pub fn close(&self) {
        self.inner.lock().unwrap().stores.clear();
    }

    /// Close all stores and delete every thumbnail database file.
    pub fn clear_all(&self) -> Result<()> {
        self.close();
        db::remove_all_thumb_databases()?;
        Ok(())
    }

    /// A cached JPEG thumbnail for the photo identified by `hash` at the active
    /// size, applying `rotation` (degrees clockwise). On a cache miss it renders
    /// from `src_path` and caches the result. When `all_sizes` is set, every
    /// configured size is pre-generated.
    pub fn get(&self, hash: &str, src_path: &Path, rotation: i32) -> Result<Vec<u8>> {
        self.get_edited(hash, src_path, rotation, &crate::model::PhotoEdit::default())
    }

    /// Like [`Generator::get`], but also applies the non-destructive `edit`
    /// (flip, straighten, crop, levels, brightness/contrast). The cache key
    /// includes the edit revision so an edited thumbnail never collides with the
    /// original; an identity edit reuses the plain `hash` key.
    pub fn get_edited(
        &self,
        hash: &str,
        src_path: &Path,
        rotation: i32,
        edit: &crate::model::PhotoEdit,
    ) -> Result<Vec<u8>> {
        let (active, all) = {
            let inner = self.inner.lock().unwrap();
            (inner.size, inner.all_sizes.clone())
        };

        let key = cache_key(hash, edit);
        if !key.is_empty() {
            if let Some(blob) = self.with_store(active, |s| s.get(&key))? {
                return Ok(blob);
            }
        }

        // Decode once, then produce every requested size.
        let src = decode(src_path)?;
        let src = rotate(src, rotation);
        let src = crate::edit::apply_edits(src, edit);

        let sizes: Vec<i32> = if all.is_empty() { vec![active] } else { all };
        let mut active_blob: Option<Vec<u8>> = None;
        for sz in &sizes {
            let blob = encode(&src, *sz)?;
            if *sz == active {
                active_blob = Some(blob.clone());
            }
            if !key.is_empty() {
                self.with_store(*sz, |s| s.put(&key, *sz, &blob))?;
            }
        }
        let active_blob = match active_blob {
            Some(b) => b,
            None => {
                // Active size not among all-sizes: produce it directly.
                let b = encode(&src, active)?;
                if !key.is_empty() {
                    let _ = self.with_store(active, |s| s.put(&key, active, &b));
                }
                b
            }
        };
        Ok(active_blob)
    }

    /// Render and cache thumbnails from an already-decoded image, avoiding a
    /// re-read and re-decode of the source file. Used by Phase 2 enrichment,
    /// which decodes each file once and passes the pixels straight here. The
    /// caching behaviour (active size, `all_sizes`, cache key) matches
    /// [`Generator::get`]; `rotation` is applied to `src`. An identity edit is
    /// assumed (enrichment runs before any edit exists).
    pub fn cache_from_image(&self, hash: &str, src: RgbaImage, rotation: i32) -> Result<()> {
        if hash.is_empty() {
            return Ok(());
        }
        let all = { self.inner.lock().unwrap().all_sizes.clone() };
        let active = { self.inner.lock().unwrap().size };
        let key = hash.to_string();

        // If already cached at the active size, do nothing.
        if self.with_store(active, |s| s.get(&key))?.is_some() {
            return Ok(());
        }

        let src = rotate(src, rotation);
        let sizes: Vec<i32> = if all.is_empty() { vec![active] } else { all };
        for sz in &sizes {
            let blob = encode(&src, *sz)?;
            self.with_store(*sz, |s| s.put(&key, *sz, &blob))?;
        }
        Ok(())
    }

    /// Remove cached thumbnails for a hash across all open sizes. Used when a
    /// photo's rotation changes.
    pub fn invalidate(&self, hash: &str) -> Result<()> {        if hash.is_empty() {
            return Ok(());
        }
        let inner = self.inner.lock().unwrap();
        for store in inner.stores.values() {
            store.delete_hash_and_edits(hash)?;
        }
        Ok(())
    }

    /// Run `f` against the (lazily opened) per-size store.
    fn with_store<T>(
        &self,
        size: i32,
        f: impl FnOnce(&Thumbs) -> db::Result<T>,
    ) -> Result<T> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.stores.contains_key(&size) {
            let store = Thumbs::open_for_size(size)?;
            inner.stores.insert(size, store);
        }
        let store = inner.stores.get(&size).unwrap();
        Ok(f(store)?)
    }
}

/// Decode `src_path`, apply `rotation`, downscale so the longest side is at most
/// `max_side`, and return a JPEG. Prepares a compact image for a local AI model.
pub fn encode_for_ai(src_path: &Path, rotation: i32, max_side: i32) -> Result<Vec<u8>> {
    let src = decode(src_path)?;
    let src = rotate(src, rotation);
    encode(&src, max_side)
}

/// Decode a photo, apply its orientation rotation, and return tightly-packed
/// RGB8 bytes plus the width and height. The long side is capped at `max_side`
/// pixels to keep face detection fast and memory light. Face boxes come back in
/// per-mille, so the downscale does not change the stored coordinates.
///
/// The result is in the same coordinate space the face module expects: after
/// `Photo::orientation`, before any non-destructive edit.
pub fn decode_oriented_rgb(
    src_path: &Path,
    rotation: i32,
    max_side: i32,
) -> Result<(Vec<u8>, u32, u32)> {
    let src = decode(src_path)?;
    let src = rotate(src, rotation);
    let (w, h) = (src.width(), src.height());
    let long = w.max(h);
    let rgba = if max_side > 0 && long > max_side as u32 {
        let scale = max_side as f32 / long as f32;
        let nw = ((w as f32 * scale).round() as u32).max(1);
        let nh = ((h as f32 * scale).round() as u32).max(1);
        image::imageops::resize(&src, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        src
    };
    let (rw, rh) = (rgba.width(), rgba.height());
    // Drop the alpha channel to give the models tight RGB8.
    let mut rgb = Vec::with_capacity((rw as usize) * (rh as usize) * 3);
    for px in rgba.pixels() {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }
    Ok((rgb, rw, rh))
}

/// The thumbnail cache key for a photo. An identity edit uses the bare `hash`,
/// keeping caches made before editing valid; any real edit appends the edit
/// revision so edited thumbnails never collide with the original.
fn cache_key(hash: &str, edit: &crate::model::PhotoEdit) -> String {
    if hash.is_empty() {
        return String::new();
    }
    if edit.is_identity() {
        hash.to_string()
    } else {
        format!("{hash}|{}", edit.edit_rev)
    }
}

/// Read and decode an image file into RGBA8.
fn decode(src_path: &Path) -> Result<RgbaImage> {
    let img = image::ImageReader::open(src_path)
        .map_err(image::ImageError::IoError)?
        .with_guessed_format()
        .map_err(image::ImageError::IoError)?
        .decode()?;
    Ok(img.to_rgba8())
}

/// Resize `src` to `max_side` and JPEG-encode it (quality 85).
fn encode(src: &RgbaImage, max_side: i32) -> Result<Vec<u8>> {
    let dst = resize(src, max_side)?;
    // JPEG has no alpha; convert to RGB.
    let rgb = image::DynamicImage::ImageRgba8(dst).to_rgb8();
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    encoder.write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(buf)
}

/// Render a square face-crop JPEG from a source photo.
///
/// The box is in per-mille (0..1000) of the image after `rotation`. A margin
/// widens the box so the crop shows the whole head, not only the detected face
/// rectangle. The result is a square JPEG whose side is at most `size`.
pub fn render_face_crop(
    src_path: &Path,
    rotation: i32,
    bbox_permille: (i32, i32, i32, i32),
    size: i32,
) -> Result<Vec<u8>> {
    let src = decode(src_path)?;
    let src = rotate(src, rotation);
    let (iw, ih) = (src.width() as f32, src.height() as f32);

    let (px, py, pw, ph) = bbox_permille;
    // Convert per-mille to pixels.
    let mut x = px as f32 / 1000.0 * iw;
    let mut y = py as f32 / 1000.0 * ih;
    let mut w = pw as f32 / 1000.0 * iw;
    let mut h = ph as f32 / 1000.0 * ih;

    // Add a 30% margin and make the crop square around the box center.
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let side = w.max(h) * 1.3;
    x = cx - side / 2.0;
    y = cy - side / 2.0;
    w = side;
    h = side;

    // Clamp to the image bounds.
    let x0 = x.floor().clamp(0.0, iw - 1.0) as u32;
    let y0 = y.floor().clamp(0.0, ih - 1.0) as u32;
    let x1 = (x + w).ceil().clamp(1.0, iw) as u32;
    let y1 = (y + h).ceil().clamp(1.0, ih) as u32;
    let cw = x1.saturating_sub(x0).max(1);
    let ch = y1.saturating_sub(y0).max(1);

    let crop = image::imageops::crop_imm(&src, x0, y0, cw, ch).to_image();
    encode(&crop, size)
}

/// Return `img` rotated clockwise by the given degrees (0/90/180/270).
fn rotate(img: RgbaImage, degrees: i32) -> RgbaImage {
    let degrees = ((degrees % 360) + 360) % 360;
    match degrees {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

/// Scale `img` so its longest side is at most `max_side`, preserving aspect
/// ratio, using a Catmull-Rom convolution filter.
fn resize(img: &RgbaImage, max_side: i32) -> Result<RgbaImage> {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Ok(img.clone());
    }
    let max_side = max_side.max(1) as u32;
    let (mut nw, mut nh) = (w, h);
    if w >= h && w > max_side {
        nw = max_side;
        nh = h * max_side / w;
    } else if h > w && h > max_side {
        nh = max_side;
        nw = w * max_side / h;
    } else if w == h && w > max_side {
        nw = max_side;
        nh = max_side;
    }
    nw = nw.max(1);
    nh = nh.max(1);

    if nw == w && nh == h {
        return Ok(img.clone());
    }

    let src = ImageRef::new(w, h, img.as_raw(), PixelType::U8x4)
        .map_err(|e| Error::Resize(e.to_string()))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8x4);
    let mut resizer = Resizer::new();
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    resizer
        .resize(&src, &mut dst, &opts)
        .map_err(|e| Error::Resize(e.to_string()))?;
    RgbaImage::from_raw(nw, nh, dst.into_vec())
        .ok_or_else(|| Error::Resize("dest buffer size mismatch".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba([200, 100, 50, 255]))
    }

    #[test]
    fn resize_preserves_aspect_and_caps_longest_side() {
        let img = solid(400, 200);
        let out = resize(&img, 100).unwrap();
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn resize_leaves_small_images_untouched() {
        let img = solid(50, 40);
        let out = resize(&img, 320).unwrap();
        assert_eq!((out.width(), out.height()), (50, 40));
    }

    #[test]
    fn rotate_swaps_dimensions_for_90() {
        let img = solid(30, 10);
        let out = rotate(img, 90);
        assert_eq!((out.width(), out.height()), (10, 30));
    }

    #[test]
    fn encode_produces_jpeg() {
        let img = solid(64, 48);
        let jpeg = encode(&img, 32).unwrap();
        // JPEG SOI marker.
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
        // Decodable back to an image at the resized dimensions.
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 24));
    }
}
