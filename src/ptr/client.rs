//! Blocking HTTP client for a Hydrus tag repository.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The default PTR server address.
pub const DEFAULT_BASE_URL: &str = "https://ptr.hydrus.network:45871";
/// Where the user gets the PTR access key. pichouse does not ship the key.
pub const ACCESS_KEY_HELP_URL: &str = "https://hydrusnetwork.github.io/hydrus/access_keys.html";

/// One entry of the repository metadata.
#[derive(Debug, Clone)]
pub struct MetaEntry {
    pub index: u64,
    pub hashes: Vec<String>,
    pub begin: i64,
    pub end: i64,
}

/// The repository metadata: the update list and the next due time.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub entries: Vec<MetaEntry>,
    pub next_update_due: i64,
}

pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
    access_key: String,
}

impl Client {
    pub fn new(base_url: &str, access_key: &str) -> Result<Self, String> {
        // The Hydrus server uses a self-signed certificate.
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .user_agent(format!("pichouse/{}", crate::version::VERSION))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            access_key: access_key.to_string(),
        })
    }

    /// GET with retry. The Hydrus client rules are the model:
    /// - 503 (server busy): wait, then retry. The wait starts at 5 minutes.
    /// - 502, 522, and network errors: retry after a short wait.
    /// - 429, 509, 529 (bandwidth used up): stop at once.
    /// - Other errors: stop at once.
    fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        const MAX_TRIES: u32 = 6;
        let mut last_err = String::new();
        for attempt in 0..MAX_TRIES {
            match self.get_once(path) {
                Ok(b) => return Ok(b),
                Err((code, msg)) => {
                    let wait = match code {
                        Some(429 | 509 | 529) => {
                            return Err(format!(
                                "{msg}\nThe server bandwidth limit is reached. Wait a day or more, then run again. The sync continues from the last index."
                            ))
                        }
                        Some(503) => 300 * 2u64.pow(attempt),
                        Some(502 | 522) | None => 10 * 2u64.pow(attempt),
                        Some(_) => return Err(msg),
                    };
                    last_err = msg;
                    if attempt + 1 < MAX_TRIES {
                        eprintln!("ptr: {last_err}; retry in {wait} s");
                        std::thread::sleep(Duration::from_secs(wait));
                    }
                }
            }
        }
        Err(last_err)
    }

    fn get_once(&self, path: &str) -> Result<Vec<u8>, (Option<u16>, String)> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .header("Hydrus-Key", &self.access_key)
            .send()
            .map_err(|e| (None, format!("GET {path}: {e}")))?;
        let status = resp.status();
        let body = resp.bytes().map_err(|e| (None, format!("GET {path}: {e}")))?.to_vec();
        if !status.is_success() {
            let text = String::from_utf8_lossy(&body);
            return Err((
                Some(status.as_u16()),
                format!("GET {path}: HTTP {status}: {}", text.chars().take(200).collect::<String>()),
            ));
        }
        Ok(body)
    }

    /// Fetch the metadata for all update indexes `>= since`.
    pub fn metadata(&self, since: u64) -> Result<Metadata, String> {
        let raw = self.get(&format!("/metadata?since={since}"))?;
        let v = decode(&raw)?;
        parse_metadata(&v)
    }

    /// Fetch one update file. Returns the raw (compressed) bytes after the
    /// SHA-256 check.
    pub fn update_raw(&self, hash_hex: &str) -> Result<Vec<u8>, String> {
        let raw = self.get(&format!("/update?update_hash={hash_hex}"))?;
        let got = hex(&Sha256::digest(&raw));
        if !got.eq_ignore_ascii_case(hash_hex) {
            return Err(format!("update {hash_hex}: hash mismatch ({got})"));
        }
        Ok(raw)
    }
}

/// Decompress zlib bytes and parse the JSON.
pub fn decode(raw: &[u8]) -> Result<Value, String> {
    let mut s = String::new();
    flate2::read::ZlibDecoder::new(raw)
        .read_to_string(&mut s)
        .map_err(|e| format!("zlib: {e}"))?;
    serde_json::from_str(&s).map_err(|e| format!("json: {e}"))
}

/// Parse `{metadata_slice: Metadata}` (a serialisable dictionary, type 21).
fn parse_metadata(v: &Value) -> Result<Metadata, String> {
    let bad = || "metadata: unexpected format".to_string();
    // [21, ver, [[key, value], ...]] where key = [0, "metadata_slice"]
    // and value = [2, [37, ver, [entries, next_due]]].
    let pairs = v.get(2).and_then(Value::as_array).ok_or_else(bad)?;
    let meta = pairs
        .iter()
        .find(|p| p.pointer("/0/1").and_then(Value::as_str) == Some("metadata_slice"))
        .and_then(|p| p.pointer("/1/1"))
        .ok_or_else(bad)?;
    if meta.get(0).and_then(Value::as_u64) != Some(37) {
        return Err(bad());
    }
    let info = meta.get(2).ok_or_else(bad)?;
    let list = info.get(0).and_then(Value::as_array).ok_or_else(bad)?;
    let next_update_due = info.get(1).and_then(Value::as_i64).unwrap_or(0);
    let mut entries = Vec::with_capacity(list.len());
    for e in list {
        entries.push(MetaEntry {
            index: e.get(0).and_then(Value::as_u64).ok_or_else(bad)?,
            hashes: e
                .get(1)
                .and_then(Value::as_array)
                .ok_or_else(bad)?
                .iter()
                .filter_map(|h| h.as_str().map(str::to_string))
                .collect(),
            begin: e.get(2).and_then(Value::as_i64).unwrap_or(0),
            end: e.get(3).and_then(Value::as_i64).unwrap_or(0),
        });
    }
    Ok(Metadata { entries, next_update_due })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
