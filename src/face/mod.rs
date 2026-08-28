//! Facial detection and recognition.
//!
//! The pipeline has three parts. A detector finds face boxes and 5 landmarks.
//! An embedder turns each aligned face into a vector. A clustering step groups
//! vectors of the same person. All parts run locally through ONNX Runtime.
//!
//! The ONNX Runtime library and the models are not shipped. They download into
//! the data folder the first time the user enables faces. See `runtime.rs`.

pub mod cluster;
pub mod config;
pub mod detector;
pub mod embedder;
pub mod runtime;

pub use config::FaceConfig;

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
