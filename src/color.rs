//! Color palette, ANSI helpers, and the context-window progress bar.

use owo_colors::OwoColorize;
use std::fmt::Write;

// ─── Catppuccin Mocha palette ─────────────────────────────────────────────────

pub(crate) const CYAN: (u8, u8, u8) = (137, 220, 235);
pub(crate) const YELLOW: (u8, u8, u8) = (249, 226, 175);
pub(crate) const GREEN: (u8, u8, u8) = (166, 227, 161);
pub(crate) const RED: (u8, u8, u8) = (243, 139, 168);
pub(crate) const GRAY: (u8, u8, u8) = (108, 112, 134);
pub(crate) const WHITE: (u8, u8, u8) = (205, 214, 244);
pub(crate) const DIM: (u8, u8, u8) = (110, 110, 110);
// Progress bar — darker variants for transparent-bg readability
const BAR_GREEN: (u8, u8, u8) = (74, 153, 90);
const BAR_YELLOW: (u8, u8, u8) = (180, 140, 40);
const BAR_RED: (u8, u8, u8) = (180, 60, 60);
const BAR_BG: (u8, u8, u8) = (66, 84, 42);

// ─── Color helpers ────────────────────────────────────────────────────────────

/// Write colored string to buffer. Accepts any `Display` — pass `&str`, `String`,
/// or `format_args!(...)` to avoid the temporary String from `format!`.
pub(crate) fn col_to(buf: &mut String, s: impl std::fmt::Display, (r, g, b): (u8, u8, u8)) {
    let _ = write!(buf, "{}", s.truecolor(r, g, b));
}

/// Write bold colored string to buffer.
pub(crate) fn col_bold_to(buf: &mut String, s: impl std::fmt::Display, (r, g, b): (u8, u8, u8)) {
    let _ = write!(buf, "{}", s.truecolor(r, g, b).bold());
}

/// Returns a colored string (for simple cases where a buffer isn't worth it).
pub(crate) fn col(s: impl std::fmt::Display, rgb: (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(rgb.0, rgb.1, rgb.2))
}

/// Map a percentage to a color: <50 green, 50–74 yellow, ≥75 red.
pub(crate) fn usage_color(pct: u32) -> (u8, u8, u8) {
    if pct < 50 {
        GREEN
    } else if pct < 75 {
        YELLOW
    } else {
        RED
    }
}

// ─── Progress bar ─────────────────────────────────────────────────────────────

/// Build the context-window progress bar.
///
/// Hand-rolled ANSI to avoid per-character owo-colors Display allocations:
/// one combined SGR escape per color group, one reset at the end.
/// Output is byte-identical to the previous owo-colors version.
pub(crate) fn make_bar(used_pct: u32, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let pct = used_pct.min(100);
    let filled = (pct as usize * width) / 100;
    let empty = width - filled;

    let fg = if used_pct < 50 {
        BAR_GREEN
    } else if used_pct < 75 {
        BAR_YELLOW
    } else {
        BAR_RED
    };

    let (br, bg, bb) = BAR_BG;
    let (dr, dg, db) = DIM;

    // Pre-size: 2 ANSI sequences (~22 bytes) + 1 reset (~6 bytes) + width chars.
    let mut s = String::with_capacity(width + 32);
    let _ = write!(
        s,
        "\x1b[38;2;{};{};{};48;2;{};{};{}m",
        fg.0, fg.1, fg.2, br, bg, bb
    );
    for _ in 0..filled {
        s.push('█');
    }
    if empty > 0 {
        s.push_str("\x1b[39;49m");
        let _ = write!(
            s,
            "\x1b[38;2;{};{};{};48;2;{};{};{}m",
            dr, dg, db, br, bg, bb
        );
        for _ in 0..empty {
            s.push('░');
        }
    }
    s.push_str("\x1b[39;49m");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_zero_returns_empty() {
        assert_eq!(make_bar(50, 0), "");
    }

    #[test]
    fn full_bar_all_filled() {
        let bar = make_bar(100, 10);
        assert_eq!(bar.matches('█').count(), 10);
        assert_eq!(bar.matches('░').count(), 0);
        // No mid-bar reset when 100% filled (only the final one).
        assert_eq!(bar.matches("\x1b[39;49m").count(), 1);
    }

    #[test]
    fn empty_bar_all_unfilled() {
        let bar = make_bar(0, 10);
        assert_eq!(bar.matches('█').count(), 0);
        assert_eq!(bar.matches('░').count(), 10);
    }

    #[test]
    fn half_bar_split_50_50() {
        let bar = make_bar(50, 10);
        assert_eq!(bar.matches('█').count(), 5);
        assert_eq!(bar.matches('░').count(), 5);
    }

    #[test]
    fn over_100_clamps_to_full() {
        let bar = make_bar(150, 10);
        assert_eq!(bar.matches('█').count(), 10);
        assert_eq!(bar.matches('░').count(), 0);
    }

    #[test]
    fn color_thresholds() {
        // < 50 → BAR_GREEN (74, 153, 90)
        assert!(make_bar(30, 10).contains("\x1b[38;2;74;153;90"));
        // 50–74 → BAR_YELLOW (180, 140, 40)
        assert!(make_bar(60, 10).contains("\x1b[38;2;180;140;40"));
        // ≥ 75 → BAR_RED (180, 60, 60)
        assert!(make_bar(80, 10).contains("\x1b[38;2;180;60;60"));
    }

    #[test]
    fn ends_with_reset() {
        let bar = make_bar(50, 10);
        assert!(bar.ends_with("\x1b[39;49m"));
    }
}
