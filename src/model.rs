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
    /// Unix timestamp (seconds) when this root's first full scan completed;
    /// `0` until then. Files recorded after this are candidates for "new".
    pub first_scan_done_at: i64,
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
    /// 64-bit perceptual hash (dHash) of the oriented image. `0` when not yet
    /// computed. Used by the duplicate finder for near-duplicate matching.
    pub phash: u64,
    pub thumb_ready: bool,
    /// User-applied rotation in degrees clockwise (0, 90, 180, 270). Stored
    /// only in the database, never written to disk.
    pub orientation: i32,
    /// AI tagging state of this photo.
    pub ai_status: AiStatus,
    /// Two-phase import state. `Structured` photos have only cheap stat data
    /// (path/size/mod_time); EXIF, dimensions, and hash are filled in by the
    /// Phase 2 enrichment worker.
    pub scan_state: PhotoScanState,
    /// `true` when the file is gone from disk but the row is kept (soft
    /// "missing") so tags/edits survive a temporary unmount, move, or delete.
    pub missing: bool,
    /// Unix timestamp (seconds) when this photo row was first recorded. Used
    /// with the owning root's `first_scan_done_at` to decide "new".
    pub added_at: i64,
    /// `true` when the user marks this photo unimportant. A skipped photo is
    /// excluded from every future face scan (human and stylised).
    pub skip_face_scan: bool,
}

/// Per-channel color-levels adjustment. Each channel has an input black point,
/// an input white point (both 0..255), and a gamma stored in milli-units
/// (1000 = 1.0). The identity value (via `Default`) maps every input to itself.
///
/// Used both as the levels part of a [`PhotoEdit`] and as the content of a saved
/// levels preset. Integer fields keep the type `Eq`-comparable and cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    pub r_black: i32,
    pub r_white: i32,
    pub r_gamma_mille: i32,
    pub g_black: i32,
    pub g_white: i32,
    pub g_gamma_mille: i32,
    pub b_black: i32,
    pub b_white: i32,
    pub b_gamma_mille: i32,
}

impl Default for Levels {
    fn default() -> Self {
        Levels {
            r_black: 0,
            r_white: 255,
            r_gamma_mille: 1000,
            g_black: 0,
            g_white: 255,
            g_gamma_mille: 1000,
            b_black: 0,
            b_white: 255,
            b_gamma_mille: 1000,
        }
    }
}

impl Levels {
    /// `true` when these levels make no change (all channels are identity).
    pub fn is_identity(&self) -> bool {
        *self == Levels::default()
    }
}

/// A named, reusable color-levels preset (a row in `level_presets`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LevelPreset {
    pub id: i64,
    pub name: String,
    pub levels: Levels,
}

/// The non-destructive edit state for one photo (a row in `photo_edits`).
///
/// Edits are applied at view time and when rendering thumbnails; the original
/// file on disk is never changed. Rotation by 90-degree steps lives separately
/// on [`Photo::orientation`]; these edits are applied *after* that rotation.
///
/// All values are integer-scaled. The [`Default`] value is the identity edit
/// (no change), which is also what a photo with no `photo_edits` row gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoEdit {
    pub photo_id: i64,
    /// Mirror horizontally (after the 90-degree orientation rotation).
    pub flip_h: bool,
    /// Mirror vertically.
    pub flip_v: bool,
    /// Fine straighten angle in milli-degrees clockwise (1000 = 1 degree),
    /// followed by an auto-crop that removes the empty corners.
    pub straighten_mdeg: i32,
    /// Crop rectangle in per-mille (0..1000) of the straightened image. A
    /// `crop_w` or `crop_h` of 0 means "no crop".
    pub crop_x: i32,
    pub crop_y: i32,
    pub crop_w: i32,
    pub crop_h: i32,
    /// Brightness offset, -100..100 (0 = neutral). Applied after levels.
    pub brightness: i32,
    /// Contrast, -100..100 (0 = neutral). Applied after levels.
    pub contrast: i32,
    /// Per-channel color levels.
    pub levels: Levels,
    /// Revision counter, bumped on every change. Part of the thumbnail cache
    /// key so an edited thumbnail never collides with the original.
    pub edit_rev: i64,
}

impl Default for PhotoEdit {
    fn default() -> Self {
        PhotoEdit {
            photo_id: 0,
            flip_h: false,
            flip_v: false,
            straighten_mdeg: 0,
            crop_x: 0,
            crop_y: 0,
            crop_w: 0,
            crop_h: 0,
            brightness: 0,
            contrast: 0,
            levels: Levels::default(),
            edit_rev: 0,
        }
    }
}

impl PhotoEdit {
    /// `true` when this edit makes no visible change to the image.
    pub fn is_identity(&self) -> bool {
        !self.flip_h
            && !self.flip_v
            && self.straighten_mdeg == 0
            && self.crop_w == 0
            && self.crop_h == 0
            && self.brightness == 0
            && self.contrast == 0
            && self.levels.is_identity()
    }
}

/// The two-phase import state of a photo. The integer values are stable and are
/// stored directly in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoScanState {
    /// Phase 1 done: only cheap stat data recorded (path, size, mod_time).
    #[default]
    Structured = 0,
    /// Phase 2 in progress: a worker is enriching this photo.
    Enriching = 1,
    /// Phase 2 done: EXIF taken date, dimensions, and hash are recorded.
    Done = 2,
}

impl PhotoScanState {
    /// Convert an on-disk integer to a `PhotoScanState`. Unknown values map to
    /// `Structured`.
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => PhotoScanState::Enriching,
            2 => PhotoScanState::Done,
            _ => PhotoScanState::Structured,
        }
    }

    /// The integer stored in the database.
    pub fn as_i64(self) -> i64 {
        self as i64
    }
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
    /// The face-recognition kind for this album and (by inheritance) its
    /// sub-albums and folders. See `AlbumKind`.
    pub kind: AlbumKind,
}

/// The face-recognition kind of an album. The integer values are stable and are
/// stored directly in the database.
///
/// `Inherit` takes the parent album's effective kind. A top-level album with
/// `Inherit` resolves to `Photo`. `Photo` routes to the human face system.
/// `Art` routes to the stylised (anime/cartoon/furry) face system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlbumKind {
    /// Take the parent album's kind. The root default is `Photo`.
    #[default]
    Inherit = 0,
    /// Real photographs. Route to the human face system.
    Photo = 1,
    /// Anime, cartoon, or furry art. Route to the stylised face system.
    Art = 2,
}

impl AlbumKind {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => AlbumKind::Photo,
            2 => AlbumKind::Art,
            _ => AlbumKind::Inherit,
        }
    }

    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

/// A virtual album: an organisation of individual *photos* (not folders) drawn
/// from anywhere in the library. Virtual albums may nest and do not touch files
/// on disk. Membership mixes manually pinned photos with rule-matched photos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VirtualAlbum {
    pub id: i64,
    pub name: String,
    /// `0` means top-level.
    pub parent_id: i64,
    pub position: i32,
    /// How this album's rules combine.
    pub rule_match: RuleMatch,
}

/// How multiple rules of a virtual album combine. The integer values are stable
/// and are stored directly in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleMatch {
    /// A photo must match every rule.
    And = 0,
    /// A photo must match any rule.
    #[default]
    Or = 1,
}

impl RuleMatch {
    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => RuleMatch::And,
            _ => RuleMatch::Or,
        }
    }

    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

/// The attribute a virtual-album rule matches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleField {
    /// A tag the photo carries. `value` is the tag name.
    Tag,
    /// Earliest taken date (inclusive). `value` is a Unix timestamp (seconds).
    DateFrom,
    /// Latest taken date (inclusive). `value` is a Unix timestamp (seconds).
    DateTo,
    /// Filename substring (case-insensitive). `value` is the substring.
    Filename,
    /// Full file path substring (case-insensitive). `value` is the substring.
    Path,
    /// Owning folder id. `value` is the folder id.
    Folder,
    /// A named person the photo contains. `value` is the person name.
    Person,
    /// A named character the photo contains. `value` is the character name.
    Character,
}

impl RuleField {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleField::Tag => "tag",
            RuleField::DateFrom => "date_from",
            RuleField::DateTo => "date_to",
            RuleField::Filename => "filename",
            RuleField::Path => "path",
            RuleField::Folder => "folder",
            RuleField::Person => "person",
            RuleField::Character => "character",
        }
    }

    /// Parse a database string. Unknown values map to `Tag`.
    pub fn from_str(s: &str) -> Self {
        match s {
            "date_from" => RuleField::DateFrom,
            "date_to" => RuleField::DateTo,
            "filename" => RuleField::Filename,
            "path" => RuleField::Path,
            "folder" => RuleField::Folder,
            "person" => RuleField::Person,
            "character" => RuleField::Character,
            _ => RuleField::Tag,
        }
    }
}

/// The comparison a virtual-album rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOp {
    /// The photo has the tag.
    Has,
    /// `taken_at >= value`.
    Gte,
    /// `taken_at <= value`.
    Lte,
    /// Case-insensitive substring match.
    Contains,
    /// Exact equality (folder id).
    Eq,
}

impl RuleOp {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleOp::Has => "has",
            RuleOp::Gte => "gte",
            RuleOp::Lte => "lte",
            RuleOp::Contains => "contains",
            RuleOp::Eq => "eq",
        }
    }

    /// Parse a database string. Unknown values map to `Has`.
    pub fn from_str(s: &str) -> Self {
        match s {
            "gte" => RuleOp::Gte,
            "lte" => RuleOp::Lte,
            "contains" => RuleOp::Contains,
            "eq" => RuleOp::Eq,
            _ => RuleOp::Has,
        }
    }
}

/// A single condition of a virtual album's smart-membership rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualRule {
    pub id: i64,
    pub album_id: i64,
    pub field: RuleField,
    pub op: RuleOp,
    pub value: String,
}

/// One group of an album's rules, combined by its own AND/OR mode; the group
/// as a whole is a single term in the owning album's top-level `rule_match`.
/// Groups do not nest — a group cannot contain another group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleGroup {
    /// `0` for an unsaved group.
    pub id: i64,
    pub rule_match: RuleMatch,
    pub rules: Vec<VirtualRule>,
}

/// A remote Immich server the user connects to. pichouse reads albums and
/// assets from the server over HTTP with the API key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImmichServer {
    pub id: i64,
    pub name: String,
    /// The base URL, for example `http://host:2283`. No trailing slash.
    pub base_url: String,
    pub api_key: String,
    /// Unix timestamp (seconds) when the server was added.
    pub added_at: i64,
}

/// A link from a local scanned folder to an Immich album. New photos in the
/// folder are auto-uploaded to that album.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImmichFolderLink {
    pub folder_id: i64,
    pub server_id: i64,
    pub immich_album_id: String,
    pub created_at: i64,
}

/// An album on an Immich server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImmichAlbum {
    /// The Immich album id (a UUID string).
    pub id: String,
    pub name: String,
    /// Number of assets in the album, as reported by the server.
    pub asset_count: i64,
}

/// An asset (photo) on an Immich server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImmichAsset {
    /// The Immich asset id (a UUID string).
    pub id: String,
    /// The original file name on the server.
    pub filename: String,
    pub width: i32,
    pub height: i32,
    /// EXIF taken date as a Unix timestamp (seconds); `0` if unknown.
    pub taken_at: i64,
}

/// The scan state of a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // `Error`/`from_str` complete the DB-mapping API.
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
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => ScanStatus::Running,
            "done" => ScanStatus::Done,
            "error" => ScanStatus::Error,
            _ => ScanStatus::Pending,
        }
    }
}

/// A named person for facial recognition. A person owns one or more faces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Person {
    pub id: i64,
    pub name: String,
    /// A representative face id for the person icon. `0` means none chosen.
    pub cover_face_id: i64,
}

/// A nestable group of named people (e.g. "Disney", "Furry"). A person may
/// belong to any number of groups at once — membership is stored separately
/// in `person_group_members`, not on `Person`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonGroup {
    pub id: i64,
    pub name: String,
    /// `0` means top-level.
    pub parent_id: i64,
    pub position: i32,
    /// A representative face id for the group's tile icon. `0` means none
    /// chosen, so the tile falls back to a folder icon.
    pub cover_face_id: i64,
}

/// One detected face in one photo.
///
/// The bounding box and the landmarks are in per-mille (0..1000) of the photo
/// after `Photo::orientation` rotation and before any non-destructive edit.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Face {
    pub id: i64,
    pub photo_id: i64,
    /// The assigned person id. `0` means unassigned.
    pub person_id: i64,
    /// The automatic similarity cluster id. `0` means not yet clustered.
    pub cluster_id: i64,
    /// Bounding box in per-mille of the oriented image.
    pub bbox_x: i32,
    pub bbox_y: i32,
    pub bbox_w: i32,
    pub bbox_h: i32,
    /// Five landmark points (x,y) in per-mille of the oriented image.
    /// Order: right eye, left eye, nose, right mouth, left mouth.
    pub landmarks: Vec<f32>,
    /// The face embedding vector. Its length is `embedding.len()`.
    pub embedding: Vec<f32>,
    /// Detector confidence, 0.0..1.0.
    pub det_score: f32,
    /// `true` when the user approved the person assignment.
    pub confirmed: bool,
    /// `0` = detector, `1` = user.
    pub source: i32,
}

/// A named stylised character (anime, cartoon, or furry).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Character {
    pub id: i64,
    pub name: String,
    /// A representative face id for the character icon. `0` means none chosen.
    pub cover_face_id: i64,
}

/// A nestable group of named characters. Mirrors `PersonGroup`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterGroup {
    pub id: i64,
    pub name: String,
    /// `0` means top-level.
    pub parent_id: i64,
    pub position: i32,
    /// A representative face id for the group's tile icon. `0` means none
    /// chosen, so the tile falls back to a folder icon.
    pub cover_face_id: i64,
}

/// One detected stylised face in one photo. The box is in per-mille (0..1000)
/// of the photo after `Photo::orientation` rotation. There are no landmarks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleFace {
    pub id: i64,
    pub photo_id: i64,
    /// The assigned character id. `0` means unassigned.
    pub character_id: i64,
    /// The automatic cluster id. `0` means not yet clustered. `-1` is noise.
    pub cluster_id: i64,
    /// Bounding box in per-mille of the oriented image.
    pub bbox_x: i32,
    pub bbox_y: i32,
    pub bbox_w: i32,
    pub bbox_h: i32,
    /// The face embedding vector. Its length is `embedding.len()`.
    pub embedding: Vec<f32>,
    /// Detector confidence, 0.0..1.0.
    pub det_score: f32,
    /// `true` when the user approved the character assignment.
    pub confirmed: bool,
    /// `0` = detector, `1` = user.
    pub source: i32,
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
    fn photo_scan_state_roundtrip() {
        for s in [
            PhotoScanState::Structured,
            PhotoScanState::Enriching,
            PhotoScanState::Done,
        ] {
            assert_eq!(PhotoScanState::from_i64(s.as_i64()), s);
        }
        assert_eq!(PhotoScanState::from_i64(99), PhotoScanState::Structured);
    }

    #[test]
    fn tag_source_roundtrip() {
        assert_eq!(TagSource::from_i64(TagSource::Ai.as_i64()), TagSource::Ai);
        assert_eq!(
            TagSource::from_i64(TagSource::User.as_i64()),
            TagSource::User
        );
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
