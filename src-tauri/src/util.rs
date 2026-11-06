//! Tiny helpers shared by the whole app (no dependencies).

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as an ISO 8601 UTC string, e.g. `2026-11-06T21:30:00Z`.
pub fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    iso_from_unix(secs)
}

pub fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Percent-encodes a string for use in a URL query value.
pub fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// `Spotify.exe` -> `Spotify`, `notepad.exe` -> `Notepad`.
pub fn label_from_exe(exe: &str) -> String {
    let stem = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let stem = stem.strip_suffix(".exe").or_else(|| stem.strip_suffix(".EXE")).unwrap_or(stem);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::from("?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_epoch_and_leap_day() {
        assert_eq!(iso_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_from_unix(1_793_999_323), "2026-11-06T21:08:43Z");
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(url_encode("Par Toutatis !"), "Par%20Toutatis%20%21");
        assert_eq!(url_encode("é"), "%C3%A9");
    }

    #[test]
    fn labels() {
        assert_eq!(label_from_exe("Spotify.exe"), "Spotify");
        assert_eq!(label_from_exe("C:\\Windows\\notepad.exe"), "Notepad");
    }
}
