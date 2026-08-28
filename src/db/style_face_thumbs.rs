//! Stylised-face-crop thumbnail database (`style-face-thumbs.db`).
//!
//! One row per stylised face, keyed by the style-face id. The blob is a small
//! square JPEG of the face cropped from the source photo. The People/Characters
//! UI reads it. This reuses the `FaceThumbs` structure with a separate file.

use super::face_thumbs::FaceThumbs;
use super::Result;

/// The stylised-face-thumbnail database file path.
pub fn style_face_thumbs_path() -> std::io::Result<std::path::PathBuf> {
    Ok(super::config::data_dir()?.join("style-face-thumbs.db"))
}

/// Open (and initialize) the stylised-face-thumbnail database.
pub fn open_style_face_thumbs() -> Result<FaceThumbs> {
    FaceThumbs::open_at(style_face_thumbs_path()?)
}

/// Delete the stylised-face-thumbnail database file. Callers drop open handles
/// first.
pub fn remove_style_face_thumbs_database() -> Result<()> {
    let dir = super::config::data_dir()?;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("style-face-thumbs") && name.contains(".db") {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
