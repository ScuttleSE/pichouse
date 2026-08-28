//! Face feature configuration.
//!
//! Values come from the `face.*` settings keys in `library.db`. The UI writes
//! them. `AppState` holds a loaded copy. Face detection is off by default.

use super::cluster::DEFAULT_COSINE_THRESHOLD;

/// The face feature settings.
#[derive(Debug, Clone)]
pub struct FaceConfig {
    /// The master switch. Off by default.
    pub enabled: bool,
    /// Scan newly imported photos automatically. Off by default.
    pub autoscan: bool,
    /// Path to the detector `.onnx` model, or empty when not downloaded.
    pub detector_path: String,
    /// Path to the embedding `.onnx` model, or empty when not downloaded.
    pub embedding_path: String,
    /// The embedding vector length the current model produces. Zero until a
    /// model is chosen. A change means old embeddings need a re-scan.
    pub embedding_dim: i32,
    /// The minimum detector confidence to keep a face, 0.0..1.0.
    pub min_score: f32,
    /// The cosine-similarity threshold for clustering.
    pub cluster_threshold: f32,
    /// The number of worker threads for a scan.
    pub concurrency: usize,
}

impl Default for FaceConfig {
    fn default() -> Self {
        FaceConfig {
            enabled: false,
            autoscan: false,
            detector_path: String::new(),
            embedding_path: String::new(),
            embedding_dim: 0,
            min_score: 0.6,
            cluster_threshold: DEFAULT_COSINE_THRESHOLD,
            concurrency: 2,
        }
    }
}

impl FaceConfig {
    /// Clamp values into safe ranges.
    pub fn normalize(&mut self) {
        if self.min_score < 0.0 {
            self.min_score = 0.0;
        }
        if self.min_score > 1.0 {
            self.min_score = 1.0;
        }
        if self.cluster_threshold < 0.0 {
            self.cluster_threshold = 0.0;
        }
        if self.cluster_threshold > 1.0 {
            self.cluster_threshold = 1.0;
        }
        if self.concurrency == 0 {
            self.concurrency = 1;
        }
        if self.concurrency > 8 {
            self.concurrency = 8;
        }
    }

    /// Report whether both model files are present on disk.
    pub fn models_ready(&self) -> bool {
        !self.detector_path.is_empty()
            && !self.embedding_path.is_empty()
            && std::path::Path::new(&self.detector_path).exists()
            && std::path::Path::new(&self.embedding_path).exists()
    }
}
