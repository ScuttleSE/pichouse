//! YuNet face detector.
//!
//! YuNet is a light face detector. It outputs boxes, five landmarks, and a
//! score per detected face. This module loads the model through ONNX Runtime,
//! runs it on an RGB image, decodes the raw heads, and returns per-mille boxes
//! and landmarks.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::DetectedFace;

/// The fixed input side of this YuNet build. The model has a static
/// [1,3,640,640] input, so we letterbox into this square.
const INPUT_SIDE: u32 = 640;

/// The NMS IoU threshold.
const NMS_IOU: f32 = 0.3;

/// A loaded YuNet detector session.
pub struct Detector {
    session: Mutex<Session>,
    input_name: String,
}

/// One raw candidate before NMS. Pixel coordinates in the padded input frame.
struct Cand {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    score: f32,
    kps: [f32; 10],
}

impl Detector {
    /// Load the detector model from a `.onnx` file. The runtime must be
    /// initialized first (see `runtime::init_runtime`).
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

    /// Detect faces in an RGB image. `width` and `height` are the image size in
    /// pixels. Returns per-mille boxes and landmarks. Embeddings are empty here
    /// and filled by the embedder.
    pub fn detect(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
        min_score: f32,
    ) -> Result<Vec<DetectedFace>, String> {
        if width == 0 || height == 0 {
            return Err("empty image".into());
        }
        if rgb.len() < (width as usize) * (height as usize) * 3 {
            return Err("rgb buffer too small".into());
        }

        // The model has a fixed square input. Resize the image so its longer
        // side fits INPUT_SIDE, keep aspect ratio, and place it at the top-left
        // of a zero-padded square. One uniform scale maps back to the source.
        let long = width.max(height) as f32;
        let scale = INPUT_SIDE as f32 / long;
        let rw = ((width as f32 * scale).round() as u32).max(1).min(INPUT_SIDE);
        let rh = ((height as f32 * scale).round() as u32).max(1).min(INPUT_SIDE);
        let iw = INPUT_SIDE;
        let ih = INPUT_SIDE;

        // Resize with bilinear sampling, place at the top-left of a padded
        // (ih x iw) BGR NCHW buffer. Padding stays zero.
        let mut chw = vec![0f32; 3 * (ih as usize) * (iw as usize)];
        let plane = (ih as usize) * (iw as usize);
        let sx = width as f32 / rw as f32;
        let sy = height as f32 / rh as f32;
        for dy in 0..rh {
            let syf = (dy as f32 + 0.5) * sy - 0.5;
            for dx in 0..rw {
                let sxf = (dx as f32 + 0.5) * sx - 0.5;
                let (r, g, b) = sample_rgb(rgb, width, height, sxf, syf);
                let idx = (dy as usize) * (iw as usize) + (dx as usize);
                // YuNet wants BGR, raw [0,255] range.
                chw[idx] = b;
                chw[plane + idx] = g;
                chw[2 * plane + idx] = r;
            }
        }

        let input = Tensor::from_array((
            [1usize, 3, ih as usize, iw as usize],
            chw.into_boxed_slice(),
        ))
        .map_err(|e| format!("input tensor: {e}"))?;

        let outputs_map: std::collections::HashMap<String, Vec<f32>> = {
            let mut sess = self.session.lock().map_err(|_| "session lock")?;
            let outputs = sess
                .run(ort::inputs![self.input_name.as_str() => input])
                .map_err(|e| format!("run: {e}"))?;
            let mut m = std::collections::HashMap::new();
            for &s in &[8u32, 16, 32] {
                for pfx in ["cls", "obj", "bbox", "kps"] {
                    let name = format!("{pfx}_{s}");
                    let val = outputs
                        .get(&name)
                        .ok_or_else(|| format!("missing output {name}"))?;
                    let (_, data) = val
                        .try_extract_tensor::<f32>()
                        .map_err(|e| format!("extract {name}: {e}"))?;
                    m.insert(name, data.to_vec());
                }
            }
            m
        };

        // Decode each stride head into candidates.
        let mut cands: Vec<Cand> = Vec::new();
        for &s in &[8u32, 16, 32] {
            let cls = &outputs_map[&format!("cls_{s}")];
            let obj = &outputs_map[&format!("obj_{s}")];
            let bbox = &outputs_map[&format!("bbox_{s}")];
            let kps = &outputs_map[&format!("kps_{s}")];
            let cols = iw / s;
            let rows = ih / s;
            for row in 0..rows {
                for col in 0..cols {
                    let i = (row * cols + col) as usize;
                    let c = cls[i].clamp(0.0, 1.0);
                    let o = obj[i].clamp(0.0, 1.0);
                    let score = (c * o).sqrt();
                    if score < min_score {
                        continue;
                    }
                    let bx = bbox[i * 4];
                    let by = bbox[i * 4 + 1];
                    let bw = bbox[i * 4 + 2];
                    let bh = bbox[i * 4 + 3];
                    let cx = (col as f32 + bx) * s as f32;
                    let cy = (row as f32 + by) * s as f32;
                    let w = bw.exp() * s as f32;
                    let h = bh.exp() * s as f32;
                    let mut lm = [0f32; 10];
                    for k in 0..5 {
                        lm[2 * k] = (col as f32 + kps[i * 10 + 2 * k]) * s as f32;
                        lm[2 * k + 1] = (row as f32 + kps[i * 10 + 2 * k + 1]) * s as f32;
                    }
                    cands.push(Cand {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        w,
                        h,
                        score,
                        kps: lm,
                    });
                }
            }
        }

        let keep = nms(&mut cands, NMS_IOU);

        // Map padded-input pixels back to per-mille of the source image. The
        // face content sits inside the resized (rw x rh) region, which maps to
        // the full source. So per-mille = pixel / rw * 1000 for x.
        let mut faces = Vec::with_capacity(keep.len());
        for idx in keep {
            let c = &cands[idx];
            let pmx = |px: f32| (px / rw as f32 * 1000.0).clamp(0.0, 1000.0);
            let pmy = |py: f32| (py / rh as f32 * 1000.0).clamp(0.0, 1000.0);
            let x0 = pmx(c.x);
            let y0 = pmy(c.y);
            let x1 = pmx(c.x + c.w);
            let y1 = pmy(c.y + c.h);
            let mut lm = Vec::with_capacity(10);
            for k in 0..5 {
                lm.push(pmx(c.kps[2 * k]));
                lm.push(pmy(c.kps[2 * k + 1]));
            }
            faces.push(DetectedFace {
                bbox_x: x0.round() as i32,
                bbox_y: y0.round() as i32,
                bbox_w: (x1 - x0).round() as i32,
                bbox_h: (y1 - y0).round() as i32,
                landmarks: lm,
                embedding: Vec::new(),
                det_score: c.score,
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
    let px = |xx: u32, yy: u32, c: usize| {
        rgb[((yy * w + xx) as usize) * 3 + c] as f32
    };
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
    let ax1 = a.x;
    let ay1 = a.y;
    let ax2 = a.x + a.w;
    let ay2 = a.y + a.h;
    let bx1 = b.x;
    let by1 = b.y;
    let bx2 = b.x + b.w;
    let by2 = b.y + b.h;
    let ix1 = ax1.max(bx1);
    let iy1 = ay1.max(by1);
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
