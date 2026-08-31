//! Anime YOLOv8-nano face detector.
//!
//! This detector finds faces in anime, cartoon, and furry art. It has one class
//! (face). The model input is `images [1,3,H,W]` in RGB, values 0..1. The model
//! output is `output0 [1, 5, num_anchors]`: rows are cx, cy, w, h, score, in
//! input-pixel units. This module letterboxes the image into a square, runs the
//! model, decodes the output, applies NMS, and returns per-mille boxes.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::DetectedStyleFace;

/// The fixed square input side. YOLOv8 accepts dynamic sizes, but a fixed
/// letterboxed square keeps the mapping simple. 640 is the training size.
const INPUT_SIDE: u32 = 640;

/// The NMS IoU threshold.
const NMS_IOU: f32 = 0.45;

/// A loaded detector session.
pub struct Detector {
    session: Mutex<Session>,
    input_name: String,
}

/// One raw candidate before NMS. Pixel coordinates in the letterboxed input.
struct Cand {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    score: f32,
}

impl Detector {
    /// Load the detector model from a `.onnx` file. The runtime must be
    /// initialized first (see `crate::face::runtime::init_runtime`).
    pub fn load(model_path: &str) -> Result<Detector, String> {
        let session = Session::builder()
            .map_err(|e| format!("session builder: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("load model: {e}"))?;
        let input_name = session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .ok_or("model has no input")?;
        Ok(Detector {
            session: Mutex::new(session),
            input_name,
        })
    }

    /// Detect stylised faces in an RGB image. Returns per-mille boxes.
    pub fn detect(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
        min_score: f32,
    ) -> Result<Vec<DetectedStyleFace>, String> {
        if width == 0 || height == 0 {
            return Err("empty image".into());
        }
        if rgb.len() < (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer too small".into());
        }

        // Letterbox: scale so the longer side fits INPUT_SIDE, center in a
        // square with gray padding (value 114/255, the YOLO convention).
        let long = width.max(height) as f32;
        let scale = INPUT_SIDE as f32 / long;
        let rw = ((width as f32 * scale).round() as u32).max(1).min(INPUT_SIDE);
        let rh = ((height as f32 * scale).round() as u32).max(1).min(INPUT_SIDE);
        let pad_x = (INPUT_SIDE - rw) / 2;
        let pad_y = (INPUT_SIDE - rh) / 2;
        let side = INPUT_SIDE as usize;

        // Build an RGB NCHW buffer, values 0..1, gray padding.
        let plane = side * side;
        let mut chw = vec![114f32 / 255.0; 3 * plane];
        let sx = width as f32 / rw as f32;
        let sy = height as f32 / rh as f32;
        for dy in 0..rh {
            let syf = (dy as f32 + 0.5) * sy - 0.5;
            let oy = (dy + pad_y) as usize;
            for dx in 0..rw {
                let sxf = (dx as f32 + 0.5) * sx - 0.5;
                let (r, g, b) = sample_rgb(rgb, width, height, sxf, syf);
                let ox = (dx + pad_x) as usize;
                let idx = oy * side + ox;
                chw[idx] = r / 255.0;
                chw[plane + idx] = g / 255.0;
                chw[2 * plane + idx] = b / 255.0;
            }
        }

        let input = Tensor::from_array(([1usize, 3, side, side], chw.into_boxed_slice()))
            .map_err(|e| format!("input tensor: {e}"))?;

        // Run and read output0 with its shape.
        let (shape, data): (Vec<i64>, Vec<f32>) = {
            let mut sess = self.session.lock().map_err(|_| "session lock")?;
            let outputs = sess
                .run(ort::inputs![self.input_name.as_str() => input])
                .map_err(|e| format!("run: {e}"))?;
            let val = outputs
                .into_iter()
                .next()
                .ok_or("model produced no output")?
                .1;
            let (shp, d) = val
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("extract output: {e}"))?;
            (shp.to_vec(), d.to_vec())
        };

        // Expect [1, C, N] with C >= 5 (cx,cy,w,h,score...). Decode.
        if shape.len() != 3 {
            return Err(format!("unexpected output rank {}", shape.len()));
        }
        let c = shape[1] as usize;
        let n = shape[2] as usize;
        if c < 5 {
            return Err(format!("output channels {c} < 5"));
        }
        // Data is row-major [C, N]. Element (ch, i) is data[ch*n + i].
        let mut cands: Vec<Cand> = Vec::new();
        for i in 0..n {
            let cx = data[i];
            let cy = data[n + i];
            let w = data[2 * n + i];
            let h = data[3 * n + i];
            // One class: take channel 4. If more classes exist, take the max.
            let mut score = data[4 * n + i];
            for ch in 5..c {
                let s = data[ch * n + i];
                if s > score {
                    score = s;
                }
            }
            if score < min_score {
                continue;
            }
            cands.push(Cand {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                w,
                h,
                score,
            });
        }

        let keep = nms(&mut cands, NMS_IOU);

        // Map letterboxed-input pixels back to per-mille of the source. Remove
        // the padding, divide by the resized region, times 1000.
        let mut faces = Vec::with_capacity(keep.len());
        for idx in keep {
            let cnd = &cands[idx];
            let pmx = |px: f32| ((px - pad_x as f32) / rw as f32 * 1000.0).clamp(0.0, 1000.0);
            let pmy = |py: f32| ((py - pad_y as f32) / rh as f32 * 1000.0).clamp(0.0, 1000.0);
            let x0 = pmx(cnd.x);
            let y0 = pmy(cnd.y);
            let x1 = pmx(cnd.x + cnd.w);
            let y1 = pmy(cnd.y + cnd.h);
            faces.push(DetectedStyleFace {
                bbox_x: x0.round() as i32,
                bbox_y: y0.round() as i32,
                bbox_w: (x1 - x0).round() as i32,
                bbox_h: (y1 - y0).round() as i32,
                embedding: Vec::new(),
                det_score: cnd.score,
            });
        }
        Ok(faces)
    }
}

/// Bilinear sample the packed RGB image at floating point (x,y). Clamps to the
/// image edge. Returns (r,g,b) as f32 in [0,255].
fn sample_rgb(rgb: &[u8], w: u32, h: u32, x: f32, y: f32) -> (f32, f32, f32) {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
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

/// Greedy NMS. Returns the kept indices sorted by score.
fn nms(cands: &mut [Cand], iou_thresh: f32) -> Vec<usize> {
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by(|&a, &b| cands[b].score.partial_cmp(&cands[a].score).unwrap());
    let mut keep = Vec::new();
    let mut removed = vec![false; cands.len()];
    for i in 0..order.len() {
        let a = order[i];
        if removed[a] {
            continue;
        }
        keep.push(a);
        for j in (i + 1)..order.len() {
            let b = order[j];
            if removed[b] {
                continue;
            }
            if iou(&cands[a], &cands[b]) > iou_thresh {
                removed[b] = true;
            }
        }
    }
    keep
}

/// Intersection over union of two boxes.
fn iou(a: &Cand, b: &Cand) -> f32 {
    let ax2 = a.x + a.w;
    let ay2 = a.y + a.h;
    let bx2 = b.x + b.w;
    let by2 = b.y + b.h;
    let ix1 = a.x.max(b.x);
    let iy1 = a.y.max(b.y);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let ua = a.w * a.h + b.w * b.h - inter;
    if ua <= 0.0 {
        0.0
    } else {
        inter / ua
    }
}
