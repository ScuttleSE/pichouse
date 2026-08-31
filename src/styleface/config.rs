//! Stylised face feature configuration.
//!
//! Values come from the `styleface.*` settings keys in `library.db`. The UI
//! writes them. `AppState` holds a loaded copy. The feature is off by default.

use super::cluster::DEFAULT_EPSILON;

/// The stylised face feature settings.
#[derive(Debug, Clone)]
pub struct StyleFaceConfig {
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
    /// The HDBSCAN cluster-selection epsilon. A larger value makes fewer, larger
    /// groups. Zero uses pure HDBSCAN selection.
    pub cluster_epsilon: f32,
    /// The number of worker threads for a scan.
    pub concurrency: usize,
}

impl Default for StyleFaceConfig {
    fn default() -> Self {
        StyleFaceConfig {
            enabled: false,
            autoscan: false,
            detector_path: String::new(),
            embedding_path: String::new(),
            embedding_dim: 0,
            min_score: 0.5,
            cluster_epsilon: DEFAULT_EPSILON,
            concurrency: 2,
        }
    }
}

impl StyleFaceConfig {
    /// Clamp values into safe ranges.
    pub fn normalize(&mut self) {
        if self.min_score < 0.0 {
            self.min_score = 0.0;
        }
        if self.min_score > 1.0 {
            self.min_score = 1.0;
        }
        if self.cluster_epsilon < 0.0 {
            self.cluster_epsilon = 0.0;
        }
        if self.cluster_epsilon > 2.0 {
            self.cluster_epsilon = 2.0;
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
