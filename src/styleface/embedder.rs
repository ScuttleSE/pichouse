//! CCIP CaFormer stylised face embedder.
//!
//! CCIP turns a 384x384 crop into a 768-value embedding. The model is trained
//! for anime character re-identification, so two crops of one character match
//! and two characters differ, even in the same art style. This module enlarges
//! the detector box by 10 percent on each side, crops a square, resizes to
//! 384x384, normalises with the CCIP mean and standard deviation, and runs the
//! model. It takes the output vector, then L2-normalises it.

#![allow(dead_code)]

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

/// The crop side CCIP expects.
const CROP: usize = 384;

/// The embedding length CCIP CaFormer produces.
const EMBED_DIM: i32 = 768;

/// CCIP channel means (RGB), 0..1.
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
/// CCIP channel standard deviations (RGB).
const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

/// The box enlargement on each side. 0.10 makes the box 10 percent larger.
const MARGIN: f32 = 0.10;

/// A loaded CCIP embedder session.
pub struct Embedder {
    session: Mutex<Session>,
    input_name: String,
}

impl Embedder {
    /// Load the embedding model from a `.onnx` file. The runtime must be
    /// initialized first (see `crate::face::runtime::init_runtime`).
    pub fn load(model_path: &str) -> Result<Embedder, String> {
        let session = Session::builder()
            .map_err(|e| format!("session builder: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("load model: {e}"))?;
        let input_name = session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .ok_or("model has no input")?;
        Ok(Embedder {
            session: Mutex::new(session),
            input_name,
        })
    }

    /// Produce an L2-normalised embedding for one face box.
    ///
    /// `rgb` is the full oriented image. `bbox` is (x, y, w, h) in per-mille of
    /// the oriented image. The embedder enlarges the box, crops a square, resizes
    /// to 384x384, and runs the model.
    pub fn embed(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
        bbox: (i32, i32, i32, i32),
    ) -> Result<Vec<f32>, String> {
        if rgb.len() < (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer too small".into());
        }

        // Per-mille box to pixels.
        let (px, py, pw, ph) = bbox;
        let x0 = px as f32 / 1000.0 * width as f32;
        let y0 = py as f32 / 1000.0 * height as f32;
        let w0 = pw as f32 / 1000.0 * width as f32;
        let h0 = ph as f32 / 1000.0 * height as f32;
        if w0 < 1.0 || h0 < 1.0 {
            return Err("degenerate box".into());
        }

        // Enlarge and make a square around the center.
        let cx = x0 + w0 / 2.0;
        let cy = y0 + h0 / 2.0;
        let side = w0.max(h0) * (1.0 + 2.0 * MARGIN);
        let x = cx - side / 2.0;
        let y = cy - side / 2.0;

        // Sample the square into a 224x224 RGB NCHW buffer with ImageNet norm.
        let plane = CROP * CROP;
        let mut chw = vec![0f32; 3 * plane];
        let step = side / CROP as f32;
        for dy in 0..CROP {
            let syf = y + (dy as f32 + 0.5) * step;
            for dx in 0..CROP {
                let sxf = x + (dx as f32 + 0.5) * step;
                let (r, g, b) = sample_rgb(rgb, width, height, sxf, syf);
                let idx = dy * CROP + dx;
                chw[idx] = (r / 255.0 - MEAN[0]) / STD[0];
                chw[plane + idx] = (g / 255.0 - MEAN[1]) / STD[1];
                chw[2 * plane + idx] = (b / 255.0 - MEAN[2]) / STD[2];
            }
        }

        let input = Tensor::from_array(([1usize, 3, CROP, CROP], chw.into_boxed_slice()))
            .map_err(|e| format!("input tensor: {e}"))?;

        // Run. Output is the feature vector [1, 768]. Take all of it.
        let mut emb = {
            let mut sess = self.session.lock().map_err(|_| "session lock")?;
            let outputs = sess
                .run(ort::inputs![self.input_name.as_str() => input])
                .map_err(|e| format!("run: {e}"))?;
            let val = outputs
                .into_iter()
                .next()
                .ok_or("model produced no output")?
                .1;
            let (shape, data) = val
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("extract embedding: {e}"))?;
            // Shape [1, dim]. Take the last dimension as the feature length.
            let dim = *shape.last().ok_or("empty output shape")? as usize;
            if data.len() < dim {
                return Err("output smaller than one feature vector".into());
            }
            data[..dim].to_vec()
        };

        // L2-normalise.
        let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in emb.iter_mut() {
                *v /= norm;
            }
        }
        Ok(emb)
    }

    /// The embedding length this model produces.
    pub fn embedding_dim(&self) -> i32 {
        EMBED_DIM
    }
}

/// Bilinear sample the packed RGB image at floating point (x,y). Out-of-range
/// samples return gray (114). Returns (r,g,b) as f32 in [0,255].
fn sample_rgb(rgb: &[u8], w: u32, h: u32, x: f32, y: f32) -> (f32, f32, f32) {
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return (114.0, 114.0, 114.0);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let px = |xx: u32, yy: u32, c: usize| rgb[((yy * w + xx) as usize) * 3 + c] as f32;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let mut out = [0f32; 3];
    for c in 0..3 {
        let top = lerp(px(x0, y0, c), px(x1, y0, c), fx);
        let bot = lerp(px(x0, y1, c), px(x1, y1, c), fx);
        out[c] = lerp(top, bot, fy);
    }
    (out[0], out[1], out[2])
}
