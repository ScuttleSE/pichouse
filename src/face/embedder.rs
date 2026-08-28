//! SFace face embedder.
//!
//! SFace turns an aligned 112x112 face crop into an embedding vector. Two
//! crops of the same person give a high cosine similarity. This module loads
//! the model through ONNX Runtime and produces the vector.
//!
//! The real inference lands in Phase 3, with the approved model. The public
//! shape here is stable so the rest of the pipeline can compile and be wired.

/// A loaded SFace embedder session.
pub struct Embedder {
    _model_path: String,
}

impl Embedder {
    /// Load the embedding model from a `.onnx` file. The runtime must be
    /// initialized first (see `runtime::init_runtime`).
    pub fn load(model_path: &str) -> Result<Embedder, String> {
        Ok(Embedder {
            _model_path: model_path.to_string(),
        })
    }

    /// Produce an L2-normalized embedding for one face.
    ///
    /// `rgb` is the full oriented image. The box and landmarks are per-mille of
    /// that image. The embedder aligns the face with the landmarks, then runs
    /// the model.
    ///
    /// Phase 3 fills in the real alignment and inference.
    pub fn embed(
        &self,
        _rgb: &[u8],
        _width: u32,
        _height: u32,
        _landmarks: &[f32],
    ) -> Result<Vec<f32>, String> {
        Err("embedder inference not implemented yet (Phase 3)".into())
    }

    /// The embedding length this model produces. SFace produces 128.
    pub fn embedding_dim(&self) -> i32 {
        128
    }
}
