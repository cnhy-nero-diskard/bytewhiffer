//! Byte-size formatting and small helpers.

use std::time::Duration;

/// Formats an elapsed duration as `Nm SSs` once past a minute, or just `Ns`
/// below it, for a finalized/historical elapsed-time display (the completed-
/// scan summary). Whole-second precision only — sub-second precision on a
/// number nobody is watching tick is noise, not signal.
pub fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins > 0 {
        format!("{mins}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Sub-second precision variant of `format_duration`, for an elapsed-time
/// display that is still live-ticking (a scan or its tree assembly still in
/// progress) — the extra decimal is what makes the number visibly alive
/// between repaints, rather than jumping in whole-second steps.
pub fn format_duration_live(d: Duration) -> String {
    let total_secs = d.as_secs_f64();
    let mins = (total_secs / 60.0) as u64;
    let secs = total_secs - (mins * 60) as f64;
    if mins > 0 {
        format!("{mins}m {secs:04.1}s")
    } else {
        format!("{secs:.1}s")
    }
}

/// Scales `bytes` down to the largest unit that keeps the value >= 1,
/// shared by `format_size` and `format_size_precise` so they only differ in
/// decimal precision, not unit selection.
fn scale_bytes(bytes: u64) -> (f64, usize) {
    const UNIT_COUNT: usize = 6; // B, KB, MB, GB, TB, PB
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNIT_COUNT - 1 {
        value /= 1024.0;
        unit += 1;
    }
    (value, unit)
}

/// Formats a byte count the way SpaceSniffer-style tools do: a small number
/// of significant digits and the largest unit that keeps the value >= 1. Used
/// for finalized/historical byte totals (the completed-scan summary), where
/// coarse precision reads as a tidy historical record rather than noise.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let (value, unit) = scale_bytes(bytes);
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Higher-precision variant of `format_size` (an extra decimal), for a live
/// HUD byte counter or scan-rate readout where real movement between
/// repaints is worth showing rather than rounding away.
pub fn format_size_precise(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let (value, unit) = scale_bytes(bytes);
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Shortens `s` to at most `max_chars` characters by eliding its middle with
/// a `…`, the way Windows Explorer shortens long paths — keeping both the
/// leading context and the (usually most-informative) trailing filename while
/// dropping the middle. Returns `s` unchanged when it already fits. The tail
/// is favoured over the head when the budget is odd, since the end of a path
/// identifies the item more than its ancestry does.
///
/// Character-based, not byte-based, so it never splits a multi-byte UTF-8
/// codepoint. Used to bound the hover tooltip's single-line path readout so it
/// can't wrap into an illegible narrow column when anchored near a screen edge.
pub fn elide_middle(s: &str, max_chars: usize) -> String {
    let len = s.chars().count();
    if len <= max_chars {
        return s.to_owned();
    }
    // Degenerate budgets: nothing sensible to keep around the ellipsis.
    if max_chars <= 1 {
        return "…".to_owned();
    }
    let budget = max_chars - 1; // one char spent on the ellipsis
    let tail = budget / 2 + budget % 2; // tail gets the odd char
    let head = budget - tail;
    let head_str: String = s.chars().take(head).collect();
    let tail_str: String = s.chars().skip(len - tail).collect();
    format!("{head_str}…{tail_str}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_uses_sensible_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn format_duration_switches_to_minutes_past_60s() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_secs(42)), "42s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 05s");
        assert_eq!(format_duration(Duration::from_secs(3725)), "62m 05s");
    }

    #[test]
    fn format_duration_live_shows_tenths() {
        assert_eq!(format_duration_live(Duration::from_millis(0)), "0.0s");
        assert_eq!(format_duration_live(Duration::from_millis(9300)), "9.3s");
        assert_eq!(format_duration_live(Duration::from_millis(65_400)), "1m 05.4s");
        assert_eq!(
            format_duration_live(Duration::from_millis(3_725_600)),
            "62m 05.6s"
        );
    }

    #[test]
    fn format_size_precise_shows_an_extra_decimal() {
        assert_eq!(format_size_precise(0), "0 B");
        assert_eq!(format_size_precise(999), "999 B");
        assert_eq!(format_size_precise(1536), "1.50 KB");
        assert_eq!(format_size_precise(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size_precise(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn elide_middle_leaves_short_strings_untouched() {
        assert_eq!(elide_middle("short/path.txt", 60), "short/path.txt");
        // Exactly at the budget is still untouched.
        assert_eq!(elide_middle("abcdef", 6), "abcdef");
    }

    #[test]
    fn elide_middle_cuts_the_middle_and_respects_the_budget() {
        let long = "SteamLibrary/steamapps/common/Wuthering Waves/Content/pakchunk70.pak";
        let out = elide_middle(long, 40);
        assert_eq!(out.chars().count(), 40);
        assert!(out.contains('…'));
        // Head and tail context are both preserved.
        assert!(out.starts_with("SteamLibrary"));
        assert!(out.ends_with("pakchunk70.pak"));
    }

    #[test]
    fn elide_middle_favours_the_tail_on_an_odd_budget() {
        // max 6 → budget 5 (one char is the ellipsis) → tail 3, head 2, so the
        // odd char lands in the tail.
        assert_eq!(elide_middle("abcdefghij", 6), "ab…hij");
    }

    #[test]
    fn elide_middle_handles_multibyte_without_splitting_codepoints() {
        // Counts characters, not bytes, so a budget cut never lands mid-glyph.
        let s = "ααααα/ββββββββββ/γγγγγ.dat";
        let out = elide_middle(s, 12);
        assert_eq!(out.chars().count(), 12);
        assert!(out.contains('…'));
    }

    #[test]
    fn elide_middle_degenerate_budget() {
        assert_eq!(elide_middle("anything", 1), "…");
        assert_eq!(elide_middle("anything", 0), "…");
    }
}
