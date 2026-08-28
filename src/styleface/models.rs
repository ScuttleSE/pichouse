//! Stylised face model catalog and download.
//!
//! The models are not shipped. They download into the data folder the first
//! time the user picks them. Each entry pins a URL to a specific commit of the
//! source repository and a SHA-256 of the file. The download verifies the hash.
//!
//! The default pair is an anime YOLOv8-nano detector (deepghs, MIT) and CCIP
//! CaFormer (deepghs, OpenRAIL-M). The detector also finds cartoon and furry
//! faces. CCIP is trained for anime character re-identification, so it separates
//! different characters in the same art style. It gives a 768-value feature.

#![allow(dead_code)]

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
pub const DEFAULT_EMBEDDING_ID: &str = "ccip_caformer_24";

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
            label: "CCIP CaFormer-24 (default, 768-D, fp32, 150 MB)",
            kind: ModelKind::Embedding,
            file_name: "styleface_embed_ccip_caformer24_fp32.onnx",
            url: "https://huggingface.co/deepghs/ccip_onnx/resolve/eb2acdd29af1703388d3d0c04221add322bc9110/ccip-caformer-24-randaug-pruned/model_feat.onnx",
            sha256: "4ea118d16496274f4f6e08d3afc768cc592389e8f7f32f8732ce2215c228ac5f",
            embedding_dim: 768,
            license: "OpenRAIL-M",
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
    ensure_model_progress(id, &|_| {})
}

/// Like `ensure_model`, but reports download progress through `on_progress`.
/// The callback gets a fraction 0.0..1.0, or a negative value when the total
/// size is unknown.
pub fn ensure_model_progress(
    id: &str,
    on_progress: &dyn Fn(f64),
) -> Result<PathBuf, String> {
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
    let bytes = read_with_progress(resp, on_progress)?;

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

/// Read a blocking HTTP response body into memory while reporting progress.
/// Uses `Content-Length` when present. Reports a negative fraction when the
/// total size is unknown, so the caller can show an indeterminate state.
pub fn read_with_progress(
    resp: reqwest::blocking::Response,
    on_progress: &dyn Fn(f64),
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let total = resp.content_length();
    let mut reader = resp;
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut buf = [0u8; 64 * 1024];
    let mut read_total: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("read body: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        read_total += n as u64;
        match total {
            Some(t) if t > 0 => on_progress(read_total as f64 / t as f64),
            _ => on_progress(-1.0),
        }
    }
    Ok(bytes)
}

fn verify(path: &std::path::Path, want_hex: &str) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(got.eq_ignore_ascii_case(want_hex))
}
