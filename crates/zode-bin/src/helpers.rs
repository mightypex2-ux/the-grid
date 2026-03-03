pub(crate) use grid_core::format_bytes;

pub(crate) fn format_timestamp_ms(ms: u64) -> String {
    if ms == 0 {
        return "--:--:--".to_string();
    }
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Day index (days since Unix epoch) for grouping messages by calendar day.
pub(crate) fn day_index(ms: u64) -> u32 {
    (ms / 86_400_000) as u32
}

/// Human-readable day label: "Today", "Yesterday", or "Month Day, Year".
pub(crate) fn format_day_label(ms: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let today = day_index(now_ms);
    let msg_day = day_index(ms);

    if msg_day == today {
        "Today".to_string()
    } else if msg_day + 1 == today {
        "Yesterday".to_string()
    } else {
        let (y, m, d) = civil_from_epoch_ms(ms);
        let month = MONTH_NAMES.get((m - 1) as usize).copied().unwrap_or("???");
        format!("{month} {d}, {y}")
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Convert milliseconds since Unix epoch to (year, month, day) in UTC.
/// Uses the algorithm by Howard Hinnant.
fn civil_from_epoch_ms(ms: u64) -> (i32, usize, u32) {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as usize, d)
}

/// Shorten a long identifier by keeping `prefix_len` leading and `suffix_len`
/// trailing characters, separated by `"..."`.  Returns the original string
/// unchanged when it is short enough.
pub(crate) fn shorten_id(id: &str, prefix_len: usize, suffix_len: usize) -> String {
    let min_len = prefix_len + suffix_len + 3;
    if id.len() > min_len {
        format!("{}...{}", &id[..prefix_len], &id[id.len() - suffix_len..])
    } else {
        id.to_string()
    }
}

/// Shorten a ZODE ID or DID string by stripping common prefixes and showing
/// only the last `tail_chars` unique characters.
pub(crate) fn shorten_zid(id: &str, tail_chars: usize) -> String {
    const ZODE_PREFIX: &str = "Zx12D3KooW";
    const DID_PREFIX: &str = "did:key:z6Mk";
    if let Some(unique) = id.strip_prefix(ZODE_PREFIX) {
        let n = tail_chars.min(unique.len());
        format!("Zx..{}", &unique[unique.len() - n..])
    } else if let Some(unique) = id.strip_prefix(DID_PREFIX) {
        let n = tail_chars.min(unique.len());
        format!("did:..{}", &unique[unique.len() - n..])
    } else {
        shorten_id(id, 4, tail_chars)
    }
}
