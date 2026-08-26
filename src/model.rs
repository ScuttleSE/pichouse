//! Shared domain types used across pichouse.
//!
//! Times are stored as Unix timestamps in seconds (`i64`), matching how the
//! SQLite schema records them. A value of `0` means "unknown" or "unset".

/// A user-added root folder that gets scanned.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryFolder {
    pub id: i64,
    pub path: String,
    /// Unix timestamp (seconds) when the folder was added.
    pub added_at: i64,
}

/// A scanned directory (a library root or any subfolder) that contains photos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub name: String,
    /// Directory modification time as a Unix timestamp (seconds).
    pub mtime: i64,
    /// Derived from the earliest photo taken date, else folder mtime year.
    pub year: i32,
}

/// A single image file recorded in the library.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Photo {
    pub id: i64,
    pub folder_id: i64,
    pub path: String,
    pub filename: String,
    pub size: i64,
    /// File modification time as a Unix timestamp (seconds).
    pub mod_time: i64,
    /// EXIF taken date as a Unix timestamp (seconds); `0` if unknown.
    pub taken_at: i64,
    pub width: i32,
    pub height: i32,
    /// Content hash, used as the thumbnail cache key.
    pub hash: String,
    pub thumb_ready: bool,
    /// User-applied rotation in degrees clockwise (0, 90, 180, 270). Stored
    /// only in the database, never written to disk.
    pub orientation: i32,
    /// AI tagging state of this photo.
    pub ai_status: AiStatus,
}

/// AI tagging status of a photo. The integer values are stable and are stored
/// directly in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiStatus {
    #[default]
    Untagged = 0,
    Queued = 1,
    Done = 2,
    Error = 3,
    Skipped = 4,
}

impl AiStatus {
    /// Convert an on-disk integer to an `AiStatus`. Unknown values map to
    /// `Untagged`.
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => AiStatus::Queued,
            2 => AiStatus::Done,
            3 => AiStatus::Error,
            4 => AiStatus::Skipped,
            _ => AiStatus::Untagged,
        }
    }

    /// The integer stored in the database.
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

/// Identifies who created a photo-tag link. The integer values are stable and
/// are stored directly in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagSource {
    #[default]
    Ai = 0,
    User = 1,
}

impl TagSource {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => TagSource::User,
            _ => TagSource::Ai,
        }
    }

    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

/// A keyword associated with a photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub source: TagSource,
    pub confirmed: bool,
}

/// A tag together with how many photos carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

/// A virtual organisation of folders. Albums do not affect files on disk and
/// may nest under a parent album.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Album {
    pub id: i64,
    pub name: String,
    /// `0` means top-level.
    pub parent_id: i64,
    pub position: i32,
}

/// The scan state of a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanStatus {
    /// Queued but not yet scanned.
    #[default]
    Pending,
    /// Currently being scanned.
    Running,
    /// Scan completed.
    Done,
    /// Scan failed.
    Error,
}

impl ScanStatus {
    /// The string stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            ScanStatus::Pending => "pending",
            ScanStatus::Running => "running",
            ScanStatus::Done => "done",
            ScanStatus::Error => "error",
        }
    }

    /// Parse a database string. Unknown values map to `Pending`.
    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => ScanStatus::Running,
            "done" => ScanStatus::Done,
            "error" => ScanStatus::Error,
            _ => ScanStatus::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_status_roundtrip() {
        for s in [
            AiStatus::Untagged,
            AiStatus::Queued,
            AiStatus::Done,
            AiStatus::Error,
            AiStatus::Skipped,
        ] {
            assert_eq!(AiStatus::from_i64(s.as_i64()), s);
        }
        assert_eq!(AiStatus::from_i64(99), AiStatus::Untagged);
    }

    #[test]
    fn tag_source_roundtrip() {
        assert_eq!(TagSource::from_i64(TagSource::Ai.as_i64()), TagSource::Ai);
        assert_eq!(TagSource::from_i64(TagSource::User.as_i64()), TagSource::User);
        assert_eq!(TagSource::from_i64(42), TagSource::Ai);
    }

    #[test]
    fn scan_status_roundtrip() {
        for s in [
            ScanStatus::Pending,
            ScanStatus::Running,
            ScanStatus::Done,
            ScanStatus::Error,
        ] {
            assert_eq!(ScanStatus::from_str(s.as_str()), s);
        }
        assert_eq!(ScanStatus::from_str("bogus"), ScanStatus::Pending);
    }
}
