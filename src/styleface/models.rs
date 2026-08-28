//! Stylised face model catalog and download.
//!
//! The models are not shipped. They download into the data folder the first
//! time the user picks them. Each entry pins a URL to a specific commit of the
//! source repository and a SHA-256 of the file. The download verifies the hash.
//!
//! The default pair is an anime YOLOv8-nano detector (deepghs, MIT) and DINOv2
//! ViT-S/14 (onnx-community export, Apache 2.0). The detector also finds cartoon
//! and furry faces. DINOv2 needs no training data and separates characters in
//! stylised art well.

use std::io::Write;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// The kind of model a catalog entry provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Detector,
    Embedding,
}

/// One downloadable model.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    /// A stable id used in settings.
    pub id: &'static str,
    /// A human label for the settings dropdown.
    pub label: &'static str,
    pub kind: ModelKind,
    /// The file name written into the models folder.
    pub file_name: &'static str,
    /// The pinned download URL.
    pub url: &'static str,
    /// The SHA-256 of the file.
    pub sha256: &'static str,
    /// The embedding length for an embedding model, else 0.
    pub embedding_dim: i32,
    /// A short license note for the UI.
    pub license: &'static str,
}

/// The default detector id.
pub const DEFAULT_DETECTOR_ID: &str = "anime_yolov8n_v1_4";
/// The default embedding id.
pub const DEFAULT_EMBEDDING_ID: &str = "dinov2_small";

/// The catalog of downloadable models.
pub fn catalog() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: DEFAULT_DETECTOR_ID,
            label: "Anime YOLOv8-nano v1.4 (default detector, 12 MB)",
            kind: ModelKind::Detector,
            file_name: "styleface_detect_anime_yolov8n_v1.4.onnx",
            url: "https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_n/model.onnx",
            sha256: "fd860b650a4377046842c3cd80d01b0b408bdfbdb4acee5759630f82c6ef04a9",
            embedding_dim: 0,
            license: "MIT",
        },
        ModelEntry {
            id: "anime_yolov8s_v1_4",
            label: "Anime YOLOv8-small v1.4 (larger detector, 44 MB)",
            kind: ModelKind::Detector,
            file_name: "styleface_detect_anime_yolov8s_v1.4.onnx",
            url: "https://huggingface.co/deepghs/anime_face_detection/resolve/784dc4c0bb692351ddcdbe6131a050b17d3025d5/face_detect_v1.4_s/model.onnx",
            sha256: "403b5bc93b6ff789b7d183418df4a1364049bac00c24acd927604a7ff6891483",
            embedding_dim: 0,
            license: "MIT",
        },
        ModelEntry {
            id: DEFAULT_EMBEDDING_ID,
            label: "DINOv2 ViT-S/14 (default, 384-D, fp32, 88 MB)",
            kind: ModelKind::Embedding,
            file_name: "styleface_embed_dinov2_small_fp32.onnx",
            url: "https://huggingface.co/onnx-community/dinov2-small/resolve/8b1f705a3a7f6f062f6bdd21986c1583d3ef105d/onnx/model.onnx",
            sha256: "f22797eabf810a75e41de68d378541ebea372122b25c4ce3ef25ff618250c20a",
            embedding_dim: 384,
            license: "Apache 2.0",
        },
        ModelEntry {
            id: "dinov2_small_fp16",
            label: "DINOv2 ViT-S/14 (smaller, 384-D, fp16, 44 MB)",
            kind: ModelKind::Embedding,
            file_name: "styleface_embed_dinov2_small_fp16.onnx",
            url: "https://huggingface.co/onnx-community/dinov2-small/resolve/8b1f705a3a7f6f062f6bdd21986c1583d3ef105d/onnx/model_fp16.onnx",
            sha256: "16845e153bbaf3fd1ef8a2154c454940f901f6fe8a80dd4c6c319eaecdb4d2ee",
            embedding_dim: 384,
            license: "Apache 2.0",
        },
    ]
}

/// Look up a catalog entry by id.
pub fn entry(id: &str) -> Option<ModelEntry> {
    catalog().into_iter().find(|e| e.id == id)
}

/// The folder where models live. Shared with the human face models folder.
pub fn models_dir() -> std::io::Result<PathBuf> {
    let d = crate::db::data_dir()?.join("models");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// The on-disk path for a model file.
pub fn model_path(file_name: &str) -> std::io::Result<PathBuf> {
    Ok(models_dir()?.join(file_name))
}

/// Report whether a catalog model is present.
pub fn model_present(id: &str) -> bool {
    let Some(e) = entry(id) else { return false };
    let Ok(path) = model_path(e.file_name) else {
        return false;
    };
    path.exists()
}

/// Download a catalog model into the models folder if absent or if its hash
/// does not match. Returns the file path. This does blocking network work.
/// Call it off the GTK main thread.
pub fn ensure_model(id: &str) -> Result<PathBuf, String> {
    let e = entry(id).ok_or_else(|| format!("unknown model id {id}"))?;
    let dest = model_path(e.file_name).map_err(|err| format!("models dir: {err}"))?;
    if dest.exists() && verify(&dest, e.sha256)? {
        return Ok(dest);
    }

    log::info!("downloading stylised face model {}", e.id);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|err| format!("http client: {err}"))?;
    let resp = client
        .get(e.url)
        .send()
        .map_err(|err| format!("download: {err}"))?;
    if !resp.status().is_success() {
        return Err(format!("download status {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|err| format!("read body: {err}"))?;

    let mut h = Sha256::new();
    h.update(&bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !got.eq_ignore_ascii_case(e.sha256) {
        return Err(format!(
            "model {} hash mismatch: expected {}, got {got}",
            e.id, e.sha256
        ));
    }

    let tmp = dest.with_extension("part");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|err| format!("write: {err}"))?;
        f.write_all(&bytes).map_err(|err| format!("write: {err}"))?;
    }
    std::fs::rename(&tmp, &dest).map_err(|err| format!("finalize: {err}"))?;
    log::info!("stylised face model {} ready at {}", e.id, dest.display());
    Ok(dest)
}

fn verify(path: &std::path::Path, want_hex: &str) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(got.eq_ignore_ascii_case(want_hex))
}
