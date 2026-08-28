//! YuNet face detector.
//!
//! YuNet is a light face detector. It outputs one box, five landmarks, and a
//! score per detected face. This module loads the model through ONNX Runtime,
//! runs it on an RGB image, and returns per-mille boxes and landmarks.
//!
//! The real inference lands in Phase 3, with the approved model. The public
//! shape here is stable so the rest of the pipeline can compile and be wired.

use super::DetectedFace;

/// A loaded YuNet detector session.
pub struct Detector {
    // The ONNX session lands in Phase 3. It is not held yet to avoid a partial
    // init before the model download is wired.
    _model_path: String,
}

impl Detector {
    /// Load the detector model from a `.onnx` file. The runtime must be
    /// initialized first (see `runtime::init_runtime`).
    pub fn load(model_path: &str) -> Result<Detector, String> {
        Ok(Detector {
            _model_path: model_path.to_string(),
        })
    }

    /// Detect faces in an RGB image. `width` and `height` are the image size in
    /// pixels. Returns per-mille boxes and landmarks. Embeddings are empty here
    /// and filled by the embedder.
    ///
    /// Phase 3 fills in the real inference.
    pub fn detect(
        &self,
        _rgb: &[u8],
        _width: u32,
        _height: u32,
        _min_score: f32,
    ) -> Result<Vec<DetectedFace>, String> {
        Err("detector inference not implemented yet (Phase 3)".into())
    }
}
