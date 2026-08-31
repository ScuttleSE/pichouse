//! Small UI helpers.

use gtk4::gdk;
use gtk4::gdk_pixbuf::PixbufLoader;
use gtk4::prelude::*;

/// Decode an image blob into a `gdk::Texture`. Tries the pixbuf loader first,
/// then the `image` crate. Returns `None` on failure.
pub fn texture_from_bytes(blob: &[u8]) -> Option<gdk::Texture> {
    let loader = PixbufLoader::new();
    if loader.write(blob).is_ok() && loader.close().is_ok() {
        if let Some(pixbuf) = loader.pixbuf() {
            return Some(gdk::Texture::for_pixbuf(&pixbuf));
        }
    }
    let img = image::load_from_memory(blob).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    let bytes = gtk4::glib::Bytes::from_owned(rgba.into_raw());
    let texture =
        gdk::MemoryTexture::new(w, h, gdk::MemoryFormat::R8g8b8a8, &bytes, (w * 4) as usize);
    Some(texture.upcast())
}

/// Escape text for safe inclusion in Pango markup.
pub fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Format a byte count as a human-readable string.
pub fn human_size(n: i64) -> String {
    const UNIT: i64 = 1024;
    if n < UNIT {
        return format!("{n} B");
    }
    let mut div = UNIT;
    let mut exp = 0usize;
    let mut m = n / UNIT;
    while m >= UNIT {
        div *= UNIT;
        exp += 1;
        m /= UNIT;
    }
    let suffix = ['K', 'M', 'G', 'T', 'P', 'E'][exp];
    format!("{:.1} {}B", n as f64 / div as f64, suffix)
}

/// Format a Unix timestamp (seconds, UTC) as "YYYY-MM-DD HH:MM:SS".
pub fn format_unix(unix: i64) -> String {
    if unix <= 0 {
        return "—".to_string();
    }
    let days = unix.div_euclid(86400);
    let secs_of_day = unix.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

/// Howard Hinnant's civil-from-days algorithm (UTC).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn format_unix_known() {
        // 2020-06-15 12:30:00 UTC = 1592224200
        assert_eq!(format_unix(1592224200), "2020-06-15 12:30:00");
        assert_eq!(format_unix(0), "—");
    }

    #[test]
    fn escape_markup_specials() {
        assert_eq!(escape_markup("a<b>&'\""), "a&lt;b&gt;&amp;&#39;&quot;");
    }
}
