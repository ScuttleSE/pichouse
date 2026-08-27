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
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "http: {e}"),
            Error::Status(s) => write!(f, "immich http status {s}"),
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
    pub fn album_assets(&self, album_id: &str) -> Result<Vec<ImmichAsset>> {
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
        struct Album {
            #[serde(default)]
            assets: Vec<Asset>,
        }
        let resp = self
            .http
            .get(format!("{}/albums/{album_id}", self.api_base))
            .header("x-api-key", &self.api_key)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let album: Album = resp.json()?;
        Ok(album
            .assets
            .into_iter()
            .map(|a| {
                let (w, h, taken) = match a.exif_info {
                    Some(e) => (e.width, e.height, parse_taken_at(&e.date_time_original)),
                    None => (0, 0, 0),
                };
                ImmichAsset {
                    id: a.id,
                    filename: a.original_file_name,
                    width: w,
                    height: h,
                    taken_at: taken,
                }
            })
            .collect())
    }

    /// Download the thumbnail JPEG bytes for one asset.
    pub fn asset_thumbnail(&self, asset_id: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(format!(
                "{}/assets/{asset_id}/thumbnail?size={THUMBNAIL_SIZE}",
                self.api_base
            ))
            .header("x-api-key", &self.api_key)
            .send()?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        Ok(resp.bytes()?.to_vec())
    }
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
}
