//! Pure formatting helpers for the link-import dialog: timecodes, durations,
//! and yt-dlp's `YYYYMMDD` upload date. Kept apart (with their own tests) so
//! the view modules stay focused on layout and under the AA-06 line cap.

/// Formats a number of seconds as `MM:SS`, or `H:MM:SS` past an hour.
pub fn mmss(secs: f32) -> String {
    let total = secs.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// A rounded "N min" label for a run of seconds (the chapters-header total).
pub fn total_minutes(secs: f32) -> String {
    let mins = (secs.max(0.0) / 60.0).round() as u64;
    format!("{mins} min")
}

/// A coarse "H h M m" (or "M m") label for a long run of seconds (the
/// playlist total).
pub fn long_duration(secs: f32) -> String {
    let total = secs.max(0.0).round() as u64;
    let (h, m) = (total / 3600, (total % 3600) / 60);
    if h > 0 {
        format!("{h} h {m} m")
    } else {
        format!("{m} m")
    }
}

/// Turns yt-dlp's `YYYYMMDD` upload date into a readable `D Mon YYYY`
/// (e.g. `12 Mar 2024`). Anything that doesn't parse is returned unchanged.
pub fn human_date(raw: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if raw.len() != 8 || !raw.chars().all(|c| c.is_ascii_digit()) {
        return raw.to_string();
    }
    let year = &raw[0..4];
    let month: usize = raw[4..6].parse().unwrap_or(0);
    let day: u32 = raw[6..8].parse().unwrap_or(0);
    match MONTHS.get(month.wrapping_sub(1)) {
        Some(mon) if (1..=31).contains(&day) => format!("{day} {mon} {year}"),
        _ => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmss_switches_to_hours_past_an_hour() {
        assert_eq!(mmss(0.0), "00:00");
        assert_eq!(mmss(700.0), "11:40");
        assert_eq!(mmss(4462.0), "1:14:22");
    }

    #[test]
    fn total_minutes_rounds_to_whole_minutes() {
        assert_eq!(total_minutes(3480.0), "58 min");
    }

    #[test]
    fn long_duration_reads_hours_and_minutes() {
        assert_eq!(long_duration(48000.0), "13 h 20 m");
        assert_eq!(long_duration(2700.0), "45 m");
    }

    #[test]
    fn human_date_formats_yyyymmdd() {
        assert_eq!(human_date("20240312"), "12 Mar 2024");
    }

    #[test]
    fn human_date_passes_through_unparseable_input() {
        assert_eq!(human_date("nope"), "nope");
        assert_eq!(human_date("20241399"), "20241399");
    }
}
