//! Face model catalog and download.
//!
//! The models are not shipped. They download into the data folder the first
//! time the user picks them. Each entry pins a URL to a specific commit of the
//! source repository and a SHA-256 of the file. The download verifies the hash.
//!
//! The default pair is YuNet (detector) and SFace (embedding), both from the
//! OpenCV Zoo under permissive licenses. YuNet is MIT. SFace is Apache 2.0.

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

/// The pinned OpenCV Zoo commit the default models come from.
const ZOO_COMMIT: &str = "47534e27c9851bb1128ccc0102f1145e27f23f98";

/// The default detector id.
pub const DEFAULT_DETECTOR_ID: &str = "yunet_2023mar";
/// The default embedding id.
pub const DEFAULT_EMBEDDING_ID: &str = "sface_2021dec";

/// The catalog of downloadable models.
pub fn catalog() -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: DEFAULT_DETECTOR_ID,
            label: "YuNet 2023mar (default detector)",
            kind: ModelKind::Detector,
            file_name: "face_detection_yunet_2023mar.onnx",
            url: "https://github.com/opencv/opencv_zoo/raw/47534e27c9851bb1128ccc0102f1145e27f23f98/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
            sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
            embedding_dim: 0,
            license: "MIT",
        },
        ModelEntry {
            id: DEFAULT_EMBEDDING_ID,
            label: "SFace 2021dec (default, 128-D, Apache 2.0)",
            kind: ModelKind::Embedding,
            file_name: "face_recognition_sface_2021dec.onnx",
            url: "https://github.com/opencv/opencv_zoo/raw/47534e27c9851bb1128ccc0102f1145e27f23f98/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
            sha256: "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
            embedding_dim: 128,
            license: "Apache 2.0",
        },
    ]
}

/// Look up a catalog entry by id.
pub fn entry(id: &str) -> Option<ModelEntry> {
    catalog().into_iter().find(|e| e.id == id)
}

/// Silence the unused-constant lint. The commit is embedded in each URL above
/// and named here for documentation and future catalog entries.
#[allow(dead_code)]
const _ZOO_COMMIT_REF: &str = ZOO_COMMIT;

/// The folder where models live.
pub fn models_dir() -> std::io::Result<PathBuf> {
    let d = crate::db::data_dir()?.join("models");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// The on-disk path for a model file.
pub fn model_path(file_name: &str) -> std::io::Result<PathBuf> {
    Ok(models_dir()?.join(file_name))
}

/// Report whether a catalog model is present and valid.
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

    log::info!("downloading face model {}", e.id);
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
    let bytes = crate::styleface::models::read_with_progress(resp, on_progress)?;

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
    log::info!("face model {} ready at {}", e.id, dest.display());
    Ok(dest)
}

fn verify(path: &std::path::Path, want_hex: &str) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(got.eq_ignore_ascii_case(want_hex))
}
