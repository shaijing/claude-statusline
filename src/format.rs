//! Time and number formatting helpers.

use chrono::{DateTime, NaiveDateTime, Utc};

/// Current Unix epoch in seconds.
pub(crate) fn now_epoch() -> u64 {
    Utc::now().timestamp() as u64
}

/// Parse an ISO-8601 timestamp to Unix epoch seconds.
/// Accepts RFC3339 (with or without fractional seconds, with explicit offset)
/// and falls back to a naive parse for `…Z`-suffixed strings without offset.
pub(crate) fn iso_to_epoch(s: &str) -> Option<u64> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp() as u64);
    }
    let trimmed = s.trim_end_matches('Z');
    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|dt| dt.and_utc().timestamp() as u64)
}

/// Format a duration in ms: "9m49s" when ≥ 1 minute, otherwise "42s".
/// Sub-second remainders are truncated (integer division).
pub(crate) fn fmt_duration_ms(ms: u64) -> String {
    let total = ms / 1000;
    let m = total / 60;
    let s = total % 60;
    if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Compact token count: 999 → "999", 1.5k, 1.5M.
/// Tenths-place precision only ("1.5M", never "1.50M" or "1.55M").
pub(crate) fn fmt_token(n: u64) -> String {
    if n >= 1_000_000 {
        let tenths = n / 100_000;
        let whole = tenths / 10;
        let frac = tenths % 10;
        format!("{whole}.{frac}M")
    } else if n >= 1_000 {
        let tenths = n / 100;
        let whole = tenths / 10;
        let frac = tenths % 10;
        if frac == 0 {
            format!("{whole}k")
        } else {
            format!("{whole}.{frac}k")
        }
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fmt_token ────────────────────────────────────────────────────────────
    #[test]
    fn token_zero() {
        assert_eq!(fmt_token(0), "0");
    }
    #[test]
    fn token_small_under_1k() {
        assert_eq!(fmt_token(1), "1");
        assert_eq!(fmt_token(999), "999");
    }
    #[test]
    fn token_exact_thousands() {
        assert_eq!(fmt_token(1_000), "1k");
        assert_eq!(fmt_token(10_000), "10k");
    }
    #[test]
    fn token_with_decimal() {
        // 1_500 / 100 = 15 → whole=1, frac=5 → "1.5k"
        assert_eq!(fmt_token(1_500), "1.5k");
        // 9_999 / 100 = 99 → whole=9, frac=9 → "9.9k"
        assert_eq!(fmt_token(9_999), "9.9k");
    }
    #[test]
    fn token_millions() {
        assert_eq!(fmt_token(1_000_000), "1.0M");
        assert_eq!(fmt_token(1_500_000), "1.5M");
        assert_eq!(fmt_token(12_300_000), "12.3M");
    }

    // ── fmt_duration_ms ──────────────────────────────────────────────────────
    #[test]
    fn duration_zero() {
        assert_eq!(fmt_duration_ms(0), "0s");
    }
    #[test]
    fn duration_sub_second_truncates() {
        // 500ms → integer division → "0s"
        assert_eq!(fmt_duration_ms(500), "0s");
    }
    #[test]
    fn duration_seconds_only() {
        assert_eq!(fmt_duration_ms(1_000), "1s");
        assert_eq!(fmt_duration_ms(59_000), "59s");
    }
    #[test]
    fn duration_minute_pads_seconds() {
        assert_eq!(fmt_duration_ms(60_000), "1m00s");
        assert_eq!(fmt_duration_ms(65_000), "1m05s");
    }
    #[test]
    fn duration_minute_and_seconds() {
        assert_eq!(fmt_duration_ms(90_000), "1m30s");
        assert_eq!(fmt_duration_ms(600_000), "10m00s");
    }

    // ── iso_to_epoch ─────────────────────────────────────────────────────────
    #[test]
    fn iso_z_form() {
        // 2025-01-15T10:30:00Z = 1736937000
        assert_eq!(iso_to_epoch("2025-01-15T10:30:00Z"), Some(1736937000));
    }
    #[test]
    fn iso_with_explicit_offset() {
        assert_eq!(iso_to_epoch("2025-01-15T10:30:00+00:00"), Some(1736937000));
    }
    #[test]
    fn iso_with_fractional_seconds() {
        // Fractional seconds are truncated, not rounded.
        assert_eq!(iso_to_epoch("2025-01-15T10:30:00.500Z"), Some(1736937000));
    }
    #[test]
    fn iso_invalid_returns_none() {
        assert_eq!(iso_to_epoch("not a date"), None);
        assert_eq!(iso_to_epoch(""), None);
        assert_eq!(iso_to_epoch("2025-13-99T99:99:99Z"), None);
    }
}
