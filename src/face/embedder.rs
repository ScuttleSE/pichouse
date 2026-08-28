//! SFace face embedder.
//!
//! SFace turns an aligned 112x112 face crop into a 128-D embedding vector. Two
//! crops of the same person give a high cosine similarity. This module aligns
//! the face with a similarity transform, warps it, then runs the model.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

/// The aligned crop side SFace expects.
const CROP: usize = 112;

/// The ArcFace/SFace 5-point reference template for a 112x112 crop.
const TEMPLATE: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];

/// A loaded SFace embedder session.
pub struct Embedder {
    session: Mutex<Session>,
    input_name: String,
}

impl Embedder {
    /// Load the embedding model from a `.onnx` file. The runtime must be
    /// initialized first (see `runtime::init_runtime`).
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

    /// Produce an L2-normalized embedding for one face.
    ///
    /// `rgb` is the full oriented image. `landmarks` holds 10 per-mille values
    /// (x,y for 5 points). The embedder aligns the face with the landmarks,
    /// warps it to 112x112, then runs the model.
    pub fn embed(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
        landmarks: &[f32],
    ) -> Result<Vec<f32>, String> {
        if landmarks.len() < 10 {
            return Err("need 5 landmark points".into());
        }
        if rgb.len() < (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer too small".into());
        }

        // Convert per-mille landmarks to pixel coordinates.
        let mut src = [(0f32, 0f32); 5];
        for k in 0..5 {
            src[k].0 = landmarks[2 * k] / 1000.0 * width as f32;
            src[k].1 = landmarks[2 * k + 1] / 1000.0 * height as f32;
        }

        // Solve the similarity transform src -> template. Then invert it to map
        // destination crop pixels back into the source image for sampling.
        let (a, b, tx, ty) = umeyama_similarity(&src, &TEMPLATE)?;
        // Forward: dst = M * src. Inverse maps dst -> src.
        let det = a * a + b * b;
        if det.abs() < 1e-12 {
            return Err("degenerate transform".into());
        }
        // Inverse of [[a,-b],[b,a]] is (1/det)[[a,b],[-b,a]].
        let ia = a / det;
        let ib = b / det;

        // Warp with bilinear sampling into a BGR NCHW buffer.
        let plane = CROP * CROP;
        let mut chw = vec![0f32; 3 * plane];
        for dy in 0..CROP {
            for dx in 0..CROP {
                let ox = dx as f32 - tx;
                let oy = dy as f32 - ty;
                // src = Minv * (dst - t)
                let sxf = ia * ox + ib * oy;
                let syf = -ib * ox + ia * oy;
                let (r, g, bl) = sample_rgb(rgb, width, height, sxf, syf);
                let idx = dy * CROP + dx;
                // SFace/OpenCV uses BGR, raw [0,255].
                chw[idx] = bl;
                chw[plane + idx] = g;
                chw[2 * plane + idx] = r;
            }
        }

        let input = Tensor::from_array(([1usize, 3, CROP, CROP], chw.into_boxed_slice()))
            .map_err(|e| format!("input tensor: {e}"))?;

        let mut emb = {
            let mut sess = self.session.lock().map_err(|_| "session lock")?;
            let outputs = sess
                .run(ort::inputs![self.input_name.as_str() => input])
                .map_err(|e| format!("run: {e}"))?;
            // The model has one output. Take the first.
            let val = outputs
                .into_iter()
                .next()
                .ok_or("model produced no output")?
                .1;
            let (_, data) = val
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("extract embedding: {e}"))?;
            data.to_vec()
        };

        // L2-normalize.
        let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in emb.iter_mut() {
                *v /= norm;
            }
        }
        Ok(emb)
    }

    /// The embedding length this model produces. SFace produces 128.
    pub fn embedding_dim(&self) -> i32 {
        128
    }
}

/// Compute a similarity transform (rotation+scale+translation) that maps `src`
/// points to `dst` points. Returns (a, b, tx, ty) where the forward map is
/// [x',y'] = [[a,-b],[b,a]] [x,y] + [tx,ty]. This is the Umeyama solution.
fn umeyama_similarity(
    src: &[(f32, f32); 5],
    dst: &[(f32, f32); 5],
) -> Result<(f32, f32, f32, f32), String> {
    let n = src.len() as f32;
    let mut mean_s = (0f32, 0f32);
    let mut mean_d = (0f32, 0f32);
    for k in 0..src.len() {
        mean_s.0 += src[k].0;
        mean_s.1 += src[k].1;
        mean_d.0 += dst[k].0;
        mean_d.1 += dst[k].1;
    }
    mean_s.0 /= n;
    mean_s.1 /= n;
    mean_d.0 /= n;
    mean_d.1 /= n;

    let mut var_s = 0f32;
    // Covariance matrix components between centered src and dst.
    let mut cxx = 0f32;
    let mut cxy = 0f32;
    let mut cyx = 0f32;
    let mut cyy = 0f32;
    for k in 0..src.len() {
        let sx = src[k].0 - mean_s.0;
        let sy = src[k].1 - mean_s.1;
        let dx = dst[k].0 - mean_d.0;
        let dy = dst[k].1 - mean_d.1;
        var_s += sx * sx + sy * sy;
        cxx += dx * sx;
        cxy += dx * sy;
        cyx += dy * sx;
        cyy += dy * sy;
    }
    var_s /= n;
    cxx /= n;
    cxy /= n;
    cyx /= n;
    cyy /= n;

    // For a pure similarity in 2D with rotation matrix R = [[a,-b],[b,a]], the
    // closed form uses the covariance C = dst_centered * src_centered^T. The
    // rotation angle comes from atan2 of the anti-symmetric and symmetric sums.
    // a = s*cos(theta), b = s*sin(theta).
    // C = [[cxx,cxy],[cyx,cyy]]. For R minimizing error with scale:
    //   num = cyx - cxy   (sin term)
    //   den = cxx + cyy   (cos term)
    let num = cyx - cxy;
    let den = cxx + cyy;
    let theta = num.atan2(den);
    // Scale = (den*cos + num*sin) / var_s.
    let scale = (den * theta.cos() + num * theta.sin()) / var_s;
    let a = scale * theta.cos();
    let b = scale * theta.sin();
    let tx = mean_d.0 - (a * mean_s.0 - b * mean_s.1);
    let ty = mean_d.1 - (b * mean_s.0 + a * mean_s.1);
    Ok((a, b, tx, ty))
}

/// Bilinear sample the packed RGB image at floating point (x,y). Out-of-range
/// samples return black. Returns (r,g,b) as f32 in [0,255].
fn sample_rgb(rgb: &[u8], w: u32, h: u32, x: f32, y: f32) -> (f32, f32, f32) {
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return (0.0, 0.0, 0.0);
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
