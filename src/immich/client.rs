//! Blocking HTTP client for an Immich server.
//!
//! The client sends an `x-api-key` header on every request. It talks to the
//! Immich REST API under the `/api` path. All methods block. Call them on a
//! background thread.

use std::time::Duration;

use serde::Deserialize;

use crate::model::{ImmichAlbum, ImmichAsset};

/// The image size to request for a grid thumbnail.
const THUMBNAIL_SIZE: &str = "thumbnail";

/// The per-request timeout for normal work.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// The shorter timeout for the reachability test.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// An Immich client error.
#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    Status(u16),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "http: {e}"),
            Error::Status(s) => write!(f, "immich http status {s}"),
            Error::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

/// Talks to one Immich server.
pub struct Client {
    /// The API base, for example `http://host:2283/api`. No trailing slash.
    api_base: String,
    api_key: String,
    http: reqwest::blocking::Client,
    ping_http: reqwest::blocking::Client,
}

impl Client {
    /// Build a client for the given base URL and API key.
    ///
    /// `base_url` is the server root, for example `http://host:2283`. The
    /// function removes a trailing slash and adds the `/api` path.
    pub fn new(base_url: &str, api_key: &str) -> Client {
        let trimmed = base_url.trim().trim_end_matches('/');
        let api_base = if trimmed.ends_with("/api") {
            trimmed.to_string()
        } else {
            format!("{trimmed}/api")
        };
        Client {
            api_base,
            api_key: api_key.to_string(),
            http: reqwest::blocking::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .expect("build http client"),
            ping_http: reqwest::blocking::Client::builder()
                .timeout(PING_TIMEOUT)
                .build()
                .expect("build ping http client"),
        }
    }

    /// Report whether the server is reachable and the API key works.
    ///
    /// The function first calls the public ping endpoint. It then lists albums
    /// to confirm that the API key is valid.
    pub fn test(&self) -> Result<bool> {
        let resp = match self
            .ping_http
            .get(format!("{}/server/ping", self.api_base))
            .send()
        {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        if !resp.status().is_success() {
            return Ok(false);
        }
        // The ping endpoint needs no key. Confirm the key with an albums call.
        let resp = self
            .ping_http
            .get(format!("{}/albums", self.api_base))
            .header("x-api-key", &self.api_key)
            .send()?;
        Ok(resp.status().is_success())
    }

    /// List every album on the server.
    pub fn albums(&self) -> Result<Vec<ImmichAlbum>> {
        #[derive(Deserialize)]
        struct Row {
            id: String,
            #[serde(rename = "albumName")]
            album_name: String,
            #[serde(rename = "assetCount", default)]
            asset_count: i64,
        }
        let resp = self
            .http
            .get(format!("{}/albums", self.api_base))
            .header("x-api-key", &self.api_key)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let rows: Vec<Row> = resp.json()?;
        Ok(rows
            .into_iter()
            .map(|r| ImmichAlbum {
                id: r.id,
                name: r.album_name,
                asset_count: r.asset_count,
            })
            .collect())
    }

    /// List the assets in one album.
    ///
    /// Recent Immich servers do not return assets from `GET /albums/{id}`. This
    /// method uses `POST /search/metadata` with an `albumIds` filter instead.
    /// It reads pages of `page_size` until the server reports no next page.
    pub fn album_assets(&self, album_id: &str, page_size: i32) -> Result<Vec<ImmichAsset>> {
        self.search_assets(Some(album_id), page_size)
    }

    /// List every asset on the server, newest first, as a timeline. Uses
    /// `POST /search/metadata` with no `albumIds` filter and pages through the
    /// whole library, then sorts by capture time descending.
    pub fn timeline_assets(&self, page_size: i32) -> Result<Vec<ImmichAsset>> {
        let mut out = self.search_assets(None, page_size)?;
        // Newest first. Assets with no EXIF date (taken_at == 0) sort last.
        out.sort_by(|a, b| b.taken_at.cmp(&a.taken_at));
        Ok(out)
    }

    /// Paged `POST /search/metadata`. With `album_id`, filters to that album;
    /// without it, returns every asset in the library.
    fn search_assets(&self, album_id: Option<&str>, page_size: i32) -> Result<Vec<ImmichAsset>> {
        #[derive(Deserialize)]
        struct Exif {
            #[serde(rename = "exifImageWidth", default)]
            width: i32,
            #[serde(rename = "exifImageHeight", default)]
            height: i32,
            #[serde(rename = "dateTimeOriginal", default)]
            date_time_original: Option<String>,
        }
        #[derive(Deserialize)]
        struct Asset {
            id: String,
            #[serde(rename = "originalFileName", default)]
            original_file_name: String,
            #[serde(rename = "exifInfo", default)]
            exif_info: Option<Exif>,
        }
        #[derive(Deserialize)]
        struct Bucket {
            #[serde(default)]
            items: Vec<Asset>,
            #[serde(rename = "nextPage", default)]
            next_page: Option<String>,
        }
        #[derive(Deserialize)]
        struct SearchResponse {
            assets: Bucket,
        }

        let size = if page_size <= 0 { 100 } else { page_size };
        let mut out: Vec<ImmichAsset> = Vec::new();
        let mut page = 1;
        loop {
            let mut body = serde_json::Map::new();
            if let Some(id) = album_id {
                body.insert("albumIds".into(), serde_json::json!([id]));
            }
            body.insert("size".into(), serde_json::json!(size));
            body.insert("page".into(), serde_json::json!(page));
            let resp = self
                .http
                .post(format!("{}/search/metadata", self.api_base))
                .header("x-api-key", &self.api_key)
                .json(&serde_json::Value::Object(body))
                .send()?;
            if !resp.status().is_success() {
                return Err(Error::Status(resp.status().as_u16()));
            }
            let sr: SearchResponse = resp.json()?;
            for a in sr.assets.items {
                let (w, h, taken) = match a.exif_info {
                    Some(e) => (e.width, e.height, parse_taken_at(&e.date_time_original)),
                    None => (0, 0, 0),
                };
                out.push(ImmichAsset {
                    id: a.id,
                    filename: a.original_file_name,
                    width: w,
                    height: h,
                    taken_at: taken,
                });
            }
            // `nextPage` is a string page number, or null when done.
            match sr.assets.next_page.and_then(|s| s.parse::<i32>().ok()) {
                Some(n) if n > page => page = n,
                _ => break,
            }
        }
        Ok(out)
    }

    /// Download the thumbnail JPEG bytes for one asset.
    pub fn asset_thumbnail(&self, asset_id: &str) -> Result<Vec<u8>> {
        self.asset_image(asset_id, THUMBNAIL_SIZE)
    }

    /// Download the larger "preview" image bytes for one asset, for the viewer.
    pub fn asset_preview(&self, asset_id: &str) -> Result<Vec<u8>> {
        self.asset_image(asset_id, "preview")
    }

    /// Download the original file bytes for one asset. Used by reverse sync to
    /// bring an Immich-only asset into a local folder.
    pub fn asset_original(&self, asset_id: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(format!("{}/assets/{asset_id}/original", self.api_base))
            .header("x-api-key", &self.api_key)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        Ok(resp.bytes()?.to_vec())
    }

    /// Download an asset image at the given size (`thumbnail` or `preview`).
    fn asset_image(&self, asset_id: &str, size: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(format!(
                "{}/assets/{asset_id}/thumbnail?size={size}",
                self.api_base
            ))
            .header("x-api-key", &self.api_key)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        Ok(resp.bytes()?.to_vec())
    }

    /// Upload one asset file to the server.
    ///
    /// Immich detects duplicates by checksum. The returned `UploadOutcome`
    /// reports the server asset id and whether the asset was newly `created` or
    /// was already present (`duplicate`).
    pub fn upload_asset(
        &self,
        path: &std::path::Path,
        filename: &str,
        created_at: i64,
        modified_at: i64,
    ) -> Result<UploadOutcome> {
        let bytes = std::fs::read(path).map_err(Error::Io)?;
        let device_asset_id = format!("pichouse-{filename}-{modified_at}");
        let part = reqwest::blocking::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("deviceAssetId", device_asset_id)
            .text("deviceId", "pichouse")
            .text("fileCreatedAt", unix_to_rfc3339(created_at))
            .text("fileModifiedAt", unix_to_rfc3339(modified_at))
            .text("filename", filename.to_string())
            .part("assetData", part);

        let resp = self
            .http
            .post(format!("{}/assets", self.api_base))
            .header("x-api-key", &self.api_key)
            .multipart(form)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        #[derive(Deserialize)]
        struct Out {
            id: String,
            #[serde(default)]
            status: String,
        }
        let out: Out = resp.json()?;
        Ok(UploadOutcome {
            asset_id: out.id,
            duplicate: out.status == "duplicate",
        })
    }

    /// Create a new album, optionally with an initial set of asset ids. Returns
    /// the new album's id.
    pub fn create_album(&self, name: &str, asset_ids: &[String]) -> Result<String> {
        let body = serde_json::json!({
            "albumName": name,
            "assetIds": asset_ids,
        });
        let resp = self
            .http
            .post(format!("{}/albums", self.api_base))
            .header("x-api-key", &self.api_key)
            .json(&body)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        #[derive(Deserialize)]
        struct Out {
            id: String,
        }
        let out: Out = resp.json()?;
        Ok(out.id)
    }

    /// Add asset ids to an existing album. Assets already in the album are
    /// ignored by the server.
    pub fn add_assets_to_album(&self, album_id: &str, asset_ids: &[String]) -> Result<()> {
        if asset_ids.is_empty() {
            return Ok(());
        }
        let body = serde_json::json!({ "ids": asset_ids });
        let resp = self
            .http
            .put(format!("{}/albums/{album_id}/assets", self.api_base))
            .header("x-api-key", &self.api_key)
            .json(&body)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        Ok(())
    }
}

/// The result of uploading one asset.
#[derive(Debug, Clone)]
pub struct UploadOutcome {
    /// The asset id on the server (new or existing).
    pub asset_id: String,
    /// `true` when the server already had this asset (upload was a no-op).
    pub duplicate: bool,
}

/// Format a Unix timestamp (seconds) as an RFC 3339 UTC string, for the upload
/// `fileCreatedAt` / `fileModifiedAt` fields. A non-positive value uses the
/// Unix epoch.
fn unix_to_rfc3339(secs: i64) -> String {
    let secs = if secs > 0 { secs } else { 0 };
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.000Z")
}

/// Convert a day count since the Unix epoch to a civil (year, month, day) in
/// UTC. Inverse of `civil_to_unix`'s date part (days-from-civil algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse an Immich ISO 8601 date string into a Unix timestamp in seconds.
/// Returns `0` when the value is missing or cannot be parsed.
fn parse_taken_at(value: &Option<String>) -> i64 {
    let s = match value {
        Some(s) if !s.is_empty() => s,
        _ => return 0,
    };
    // Immich sends RFC 3339, for example "2021-05-01T12:00:00.000Z". Parse the
    // date and time fields directly. This avoids a new date dependency.
    parse_rfc3339_seconds(s).unwrap_or(0)
}

/// Convert an RFC 3339 timestamp to Unix seconds. This is a small parser for
/// the fixed shape Immich sends. It ignores the fractional part and the zone
/// offset, treating the value as UTC.
fn parse_rfc3339_seconds(s: &str) -> Option<i64> {
    // Expect "YYYY-MM-DDTHH:MM:SS" as the first 19 characters.
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse::<i64>().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    Some(civil_to_unix(year, month, day, hour, min, sec))
}

/// Convert a UTC civil date and time to Unix seconds. Uses the standard
/// days-from-civil algorithm.
fn civil_to_unix(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + hh * 3600 + mm * 60 + ss
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_normalizes() {
        assert_eq!(Client::new("http://h:2283", "k").api_base, "http://h:2283/api");
        assert_eq!(Client::new("http://h:2283/", "k").api_base, "http://h:2283/api");
        assert_eq!(
            Client::new("http://h:2283/api", "k").api_base,
            "http://h:2283/api"
        );
    }

    #[test]
    fn rfc3339_parses() {
        // 2021-01-01T00:00:00Z is 1609459200.
        assert_eq!(parse_rfc3339_seconds("2021-01-01T00:00:00.000Z"), Some(1609459200));
        assert_eq!(parse_rfc3339_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_seconds("bad"), None);
    }

    #[test]
    fn unix_to_rfc3339_roundtrips() {
        for secs in [0i64, 1609459200, 1_700_000_000, 946684800, 1234567890] {
            let s = unix_to_rfc3339(secs);
            assert_eq!(parse_rfc3339_seconds(&s), Some(secs), "for {secs} got {s}");
        }
        assert!(unix_to_rfc3339(1609459200).starts_with("2021-01-01T00:00:00"));
    }

    #[test]
    fn search_response_deserializes() {        // The exact shape POST /search/metadata returns. Confirms the nested
        // `assets.items` and `assets.nextPage` parse into our structs.
        #[derive(serde::Deserialize)]
        struct Exif {
            #[serde(rename = "exifImageWidth", default)]
            width: i32,
        }
        #[derive(serde::Deserialize)]
        struct Asset {
            id: String,
            #[serde(rename = "originalFileName", default)]
            original_file_name: String,
            #[serde(rename = "exifInfo", default)]
            exif_info: Option<Exif>,
        }
        #[derive(serde::Deserialize)]
        struct Bucket {
            #[serde(default)]
            items: Vec<Asset>,
            #[serde(rename = "nextPage", default)]
            next_page: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct SearchResponse {
            assets: Bucket,
        }
        let json = r#"{
            "albums": {"items": [], "total": 0, "count": 0},
            "assets": {
                "total": 1, "count": 1, "nextPage": "2",
                "items": [
                    {"id": "abc", "originalFileName": "IMG_1.jpg",
                     "exifInfo": {"exifImageWidth": 4000, "exifImageHeight": 3000}}
                ]
            }
        }"#;
        let sr: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(sr.assets.items.len(), 1);
        assert_eq!(sr.assets.items[0].id, "abc");
        assert_eq!(sr.assets.items[0].original_file_name, "IMG_1.jpg");
        assert_eq!(sr.assets.items[0].exif_info.as_ref().unwrap().width, 4000);
        assert_eq!(sr.assets.next_page.as_deref(), Some("2"));
    }
}
