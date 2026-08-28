//! Facial detection and recognition.
//!
//! The pipeline has three parts. A detector finds face boxes and 5 landmarks.
//! An embedder turns each aligned face into a vector. A clustering step groups
//! vectors of the same person. All parts run locally through ONNX Runtime.
//!
//! The ONNX Runtime library and the models are not shipped. They download into
//! the data folder the first time the user enables faces. See `runtime.rs`.

#[cfg(test)]
mod inference_test;

pub mod cluster;
pub mod config;
pub mod detector;
pub mod embedder;
pub mod models;
pub mod runtime;

pub use config::FaceConfig;

use detector::Detector;
use embedder::Embedder;

/// One detected face before it becomes a database row. Coordinates are in
/// per-mille (0..1000) of the oriented source image.
#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub bbox_x: i32,
    pub bbox_y: i32,
    pub bbox_w: i32,
    pub bbox_h: i32,
    /// Five landmark points (x,y) in per-mille of the oriented image.
    pub landmarks: Vec<f32>,
    /// The embedding vector, filled by the embedder.
    pub embedding: Vec<f32>,
    /// Detector confidence, 0.0..1.0.
    pub det_score: f32,
}

/// The loaded detect-and-embed pipeline. It owns a detector and an embedder.
/// Build one per scan run, after the runtime and the models are ready.
pub struct FacePipeline {
    detector: Detector,
    embedder: Embedder,
    min_score: f32,
}

impl FacePipeline {
    /// Load both models. The runtime must be initialized first (see
    /// `runtime::init_runtime`).
    pub fn load(
        detector_path: &str,
        embedding_path: &str,
        min_score: f32,
    ) -> Result<FacePipeline, String> {
        Ok(FacePipeline {
            detector: Detector::load(detector_path)?,
            embedder: Embedder::load(embedding_path)?,
            min_score,
        })
    }

    /// The embedding length the loaded model produces.
    pub fn embedding_dim(&self) -> i32 {
        self.embedder.embedding_dim()
    }

    /// Detect faces in one oriented RGB image and embed each one. `rgb` is
    /// tightly-packed RGB8 of the image after `Photo::orientation` rotation.
    pub fn detect_and_embed(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<DetectedFace>, String> {
        let mut faces = self.detector.detect(rgb, width, height, self.min_score)?;
        for f in faces.iter_mut() {
            match self.embedder.embed(rgb, width, height, &f.landmarks) {
                Ok(emb) => f.embedding = emb,
                Err(e) => log::warn!("embed face: {e}"),
            }
        }
        // Keep only faces that got an embedding, so clustering has a vector.
        faces.retain(|f| !f.embedding.is_empty());
        Ok(faces)
    }
}
