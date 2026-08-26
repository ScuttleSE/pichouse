//! Free-text model output → clean tag list.

use super::config::DEFAULT_MAX_TAGS;

/// Turn a model's free-text keyword response into a clean, deduped, lowercase
/// tag list, capped at `max_tags`. Tolerates commas, newlines, bullets,
/// numbering, and surrounding prose.
pub fn parse_tags(response: &str, max_tags: i32) -> Vec<String> {
    let max_tags = if max_tags <= 0 {
        DEFAULT_MAX_TAGS as usize
    } else {
        max_tags as usize
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for field in response.split(|c| c == ',' || c == '\n' || c == ';') {
        let t = clean_tag(field);
        if t.is_empty() || seen.contains(&t) {
            continue;
        }
        seen.insert(t.clone());
        out.push(t);
        if out.len() >= max_tags {
            break;
        }
    }
    out
}

/// Normalize a single candidate keyword. Returns "" to reject it.
fn clean_tag(s: &str) -> String {
    let mut s = s.trim().to_lowercase();
    // Strip leading junk: whitespace, dashes, digits, dots, parens, #, >.
    let start = s
        .find(|c: char| !(c.is_whitespace() || matches!(c, '-' | '*' | '.' | ')' | '(' | '#' | '>') || c.is_ascii_digit()))
        .unwrap_or(s.len());
    s = s[start..].to_string();
    // Drop quote characters anywhere.
    s.retain(|c| !matches!(c, '"' | '\'' | '`'));
    // Strip a leading "label:" prose prefix (keep text after the last colon).
    if let Some(i) = s.rfind(':') {
        if i < s.len() - 1 {
            s = s[i + 1..].to_string();
        }
    }
    s = s.trim().to_string();
    // Drop trailing punctuation.
    s = s.trim_end_matches(['.', '!', '?', ':', ';']).to_string();
    s = s.trim().to_string();
    if s.is_empty() {
        return String::new();
    }
    // Reject overly long fragments (likely a sentence) and single letters.
    let len = s.chars().count();
    if len < 2 || len > 40 {
        return String::new();
    }
    if s.matches(' ').count() > 3 {
        return String::new();
    }
    // Reject common non-tag prose openers that survived prefix stripping.
    for junk in ["here are", "the image", "this image", "sure", "keywords", "tags"] {
        if s == junk || s.starts_with(&format!("{junk} ")) {
            return String::new();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tags_cases() {
        let cases: &[(&str, &[&str])] = &[
            ("Here are the tags: beach, Sunset, dog", &["beach", "sunset", "dog"]),
            ("1. sand\n2. ocean\n- palm tree", &["sand", "ocean", "palm tree"]),
            ("beach, beach, BEACH", &["beach"]),
            ("\"blue sky\", 'green grass'", &["blue sky", "green grass"]),
            ("a, this image shows a cat, dog", &["dog"]),
        ];
        for (input, want) in cases {
            let got = parse_tags(input, 25);
            assert_eq!(got, *want, "input {input:?}");
        }
    }

    #[test]
    fn parse_tags_cap() {
        assert_eq!(parse_tags("a1,b2,c3,d4,e5", 3).len(), 3);
    }
}
