//! Stylised face feature configuration.
//!
//! Values come from the `styleface.*` settings keys in `library.db`. The UI
//! writes them. `AppState` holds a loaded copy. The feature is off by default.

use super::cluster::{ClusterParams, DEFAULT_EPSILON, DEFAULT_MAX_DIST, DEFAULT_MIN_SAMPLES};

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
    /// The cosine-distance limit for grouping, 0.0..2.0. Smaller is stricter.
    pub cluster_max_dist: f32,
    /// The HDBSCAN `min_samples`, 1..10. Larger stops chains of faces.
    pub min_samples: usize,
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
            cluster_max_dist: DEFAULT_MAX_DIST,
            min_samples: DEFAULT_MIN_SAMPLES,
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
        self.cluster_max_dist = self.cluster_max_dist.clamp(0.01, 2.0);
        self.min_samples = self.min_samples.clamp(1, 10);
        if self.concurrency == 0 {
            self.concurrency = 1;
        }
        if self.concurrency > 8 {
            self.concurrency = 8;
        }
    }

    /// The grouping parameters for `cluster::cluster`.
    pub fn cluster_params(&self) -> ClusterParams {
        ClusterParams {
            epsilon: self.cluster_epsilon,
            max_dist: self.cluster_max_dist,
            min_samples: self.min_samples,
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
