//! Stylised face detection and clustering.
//!
//! This system finds faces in anime art, cartoon art, and furry art. It groups
//! the faces by character. The design mirrors the human face system in
//! `src/face/`, but it uses different models. A YOLOv8-nano detector with anime
//! training finds the boxes. CCIP CaFormer makes a 768-value embedding per face.
//! HDBSCAN groups the embeddings.
//!
//! The two systems run separate clustering passes. The human system and the
//! stylised system do not mix. A named group is a "character" here, not a
//! "person".
//!
//! The ONNX Runtime library is shared with the human face system. See
//! `crate::face::runtime`. The detector and embedder models are separate and
//! download on first use. See `models.rs`.

#![allow(dead_code)]

pub mod cluster;
pub mod config;
pub mod detector;
pub mod embedder;
pub mod models;

pub use config::StyleFaceConfig;

use detector::Detector;
use embedder::Embedder;

/// One detected stylised face before it becomes a database row. Coordinates are
/// in per-mille (0..1000) of the oriented source image. There are no landmarks.
/// The embedder uses the box only.
#[derive(Debug, Clone)]
pub struct DetectedStyleFace {
    pub bbox_x: i32,
    pub bbox_y: i32,
    pub bbox_w: i32,
    pub bbox_h: i32,
    /// The embedding vector, filled by the embedder.
    pub embedding: Vec<f32>,
    /// Detector confidence, 0.0..1.0.
    pub det_score: f32,
}

/// The loaded detect-and-embed pipeline. It owns a detector and an embedder.
/// Build one per scan run, after the runtime and the models are ready.
pub struct StyleFacePipeline {
    detector: Detector,
    embedder: Embedder,
    min_score: f32,
}

impl StyleFacePipeline {
    /// Load both models. The runtime must be initialized first (see
    /// `crate::face::runtime::init_runtime`).
    pub fn load(
        detector_path: &str,
        embedding_path: &str,
        min_score: f32,
    ) -> Result<StyleFacePipeline, String> {
        Ok(StyleFacePipeline {
            detector: Detector::load(detector_path)?,
            embedder: Embedder::load(embedding_path)?,
            min_score,
        })
    }

    /// The embedding length the loaded model produces.
    pub fn embedding_dim(&self) -> i32 {
        self.embedder.embedding_dim()
    }

    /// Detect stylised faces in one oriented RGB image and embed each one.
    /// `rgb` is tightly-packed RGB8 of the image after orientation rotation.
    pub fn detect_and_embed(
        &self,
        rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<DetectedStyleFace>, String> {
        let mut faces = self.detector.detect(rgb, width, height, self.min_score)?;
        for f in faces.iter_mut() {
            let bbox = (f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h);
            match self.embedder.embed(rgb, width, height, bbox) {
                Ok(emb) => f.embedding = emb,
                Err(e) => log::warn!("embed stylised face: {e}"),
            }
        }
        // Keep only faces that got an embedding, so clustering has a vector.
        faces.retain(|f| !f.embedding.is_empty());
        Ok(faces)
    }
}
