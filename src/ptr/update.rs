//! Parser for Hydrus repository update files.

use serde_json::Value;

pub const TYPE_CONTENT_UPDATE: u64 = 34;
pub const TYPE_DEFINITIONS_UPDATE: u64 = 36;

pub const CONTENT_MAPPINGS: u64 = 0;
pub const CONTENT_SIBLINGS: u64 = 1;
pub const CONTENT_PARENTS: u64 = 2;

pub const ACTION_ADD: u64 = 0;
pub const ACTION_DELETE: u64 = 1;

/// The number of hash definitions skipped because they are not SHA-256.
/// The PTR history holds some (for example a 40-character SHA-1 at index
/// 1321 and "0000" at index 1327). They can never match a photo.
pub static SKIPPED_HASHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// One parsed update file.
#[derive(Debug)]
pub enum Update {
    Definitions {
        hashes: Vec<(u64, [u8; 32])>,
        tags: Vec<(u64, String)>,
    },
    Content {
        /// (action, tag_id, hash_ids)
        mappings: Vec<(u64, u64, Vec<u64>)>,
        /// (content_type, action, left_tag_id, right_tag_id)
        pairs: Vec<(u64, u64, u64, u64)>,
    },
}

/// Parse a decoded update (`[type, version, info]`).
pub fn parse(v: &Value) -> Result<Update, String> {
    let bad = |w: &str| format!("update: unexpected format ({w})");
    let ty = v.get(0).and_then(Value::as_u64).ok_or_else(|| bad("type"))?;
    let info = v.get(2).and_then(Value::as_array).ok_or_else(|| bad("info"))?;
    match ty {
        TYPE_DEFINITIONS_UPDATE => {
            let mut hashes = Vec::new();
            let mut tags = Vec::new();
            for part in info {
                let kind = part.get(0).and_then(Value::as_u64).ok_or_else(|| bad("def kind"))?;
                let rows = part.get(1).and_then(Value::as_array).ok_or_else(|| bad("def rows"))?;
                for r in rows {
                    let id = r.get(0).and_then(Value::as_u64).ok_or_else(|| bad("def id"))?;
                    let s = r.get(1).and_then(Value::as_str).ok_or_else(|| bad("def val"))?;
                    match kind {
                        0 => match unhex32(s) {
                            Some(h) => hashes.push((id, h)),
                            None => {
                                SKIPPED_HASHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        },
                        1 => tags.push((id, s.to_string())),
                        _ => return Err(bad("def kind value")),
                    }
                }
            }
            Ok(Update::Definitions { hashes, tags })
        }
        TYPE_CONTENT_UPDATE => {
            let mut mappings = Vec::new();
            let mut pairs = Vec::new();
            for part in info {
                let ct = part.get(0).and_then(Value::as_u64).ok_or_else(|| bad("ctype"))?;
                let actions = part.get(1).and_then(Value::as_array).ok_or_else(|| bad("actions"))?;
                for a in actions {
                    let action = a.get(0).and_then(Value::as_u64).ok_or_else(|| bad("action"))?;
                    let rows = a.get(1).and_then(Value::as_array).ok_or_else(|| bad("rows"))?;
                    for r in rows {
                        let left = r.get(0).and_then(Value::as_u64).ok_or_else(|| bad("row id"))?;
                        if ct == CONTENT_MAPPINGS {
                            let ids = r
                                .get(1)
                                .and_then(Value::as_array)
                                .ok_or_else(|| bad("hash ids"))?
                                .iter()
                                .filter_map(Value::as_u64)
                                .collect();
                            mappings.push((action, left, ids));
                        } else {
                            let right = r.get(1).and_then(Value::as_u64).ok_or_else(|| bad("pair"))?;
                            pairs.push((ct, action, left, right));
                        }
                    }
                }
            }
            Ok(Update::Content { mappings, pairs })
        }
        other => Err(format!("update: unknown type {other}")),
    }
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_non_sha256_hashes() {
        let good = "AB".repeat(32);
        let v = serde_json::json!([36, 1, [[0, [[1, "8714c697021770b263dd23256ab97b3e2093a9cb"], [2, "0000"], [3, good]]]]]);
        let Update::Definitions { hashes, .. } = parse(&v).unwrap() else { panic!() };
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0].0, 3);
        assert!(SKIPPED_HASHES.load(std::sync::atomic::Ordering::Relaxed) >= 2);
    }
}
