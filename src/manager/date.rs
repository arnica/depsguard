// Date calculation and comparison for release-age checks.

/// Convert days since Unix epoch to `(year, month, day)`.
///
/// Uses the civil calendar algorithm with signed arithmetic to correctly
/// handle all eras. For modern dates (year > 400), the `u64` cast is safe.
pub fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mon = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mon <= 2 { y + 1 } else { y };
    (y as u64, mon, d)
}

#[cfg(test)]
pub fn date_days_ago(days: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let target = now.saturating_sub(days.saturating_mul(86400));
    epoch_to_date(target)
}

#[cfg(test)]
pub fn epoch_to_date(epoch: u64) -> String {
    let days_since_epoch = epoch / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

/// Parse a relative duration string like "7 days", "2 weeks" into days.
pub fn parse_relative_days(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 {
        let n: u64 = parts[0].parse().ok()?;
        match parts[1].trim_end_matches('s') {
            "day" => Some(n),
            "week" => n.checked_mul(7),
            _ => None,
        }
    } else {
        None
    }
}

/// Parse a simple ISO 8601 duration like "P7D" (7 days) or "P2W" (2 weeks) into days.
///
/// pip's `--uploaded-prior-to` accepts these relative durations (e.g. `P3D`).
/// Only whole-day and whole-week periods are supported; durations with a time
/// component (e.g. `P1DT12H`) return `None`.
pub fn parse_iso8601_days(s: &str) -> Option<u64> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    let rest = s.strip_prefix(['P', 'p'])?;
    // Split off the trailing unit character safely. A byte-index split would
    // panic on multibyte input (e.g. "P7é").
    let mut chars = rest.chars();
    let unit = chars.next_back()?;
    let num_part = chars.as_str();
    if num_part.is_empty() {
        return None;
    }
    let n: u64 = num_part.parse().ok()?;
    match unit {
        'D' | 'd' => Some(n),
        'W' | 'w' => n.checked_mul(7),
        _ => None,
    }
}

/// Parse a compact duration string like "7d", "3d", "1440m", "24h" into minutes.
pub fn parse_duration_minutes(s: &str) -> Option<u64> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    // Split off the trailing unit character safely (avoid a byte-index split,
    // which panics on multibyte input).
    let mut chars = s.chars();
    let unit = chars.next_back()?;
    let num_part = chars.as_str();
    if num_part.is_empty() {
        return None;
    }
    let n: u64 = num_part.parse().ok()?;
    match unit {
        'd' => Some(n.saturating_mul(24 * 60)),
        'h' => Some(n.saturating_mul(60)),
        'm' => Some(n),
        _ => None,
    }
}
