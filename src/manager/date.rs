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

/// Parse a YYYY-MM-DD prefix from a date string (also works with RFC 3339 timestamps).
fn parse_date_to_days(date_str: &str) -> Option<u64> {
    if date_str.len() < 10 {
        return None;
    }
    let b = date_str.as_bytes();
    if b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: u64 = date_str[0..4].parse().ok()?;
    let m: u64 = date_str[5..7].parse().ok()?;
    let d: u64 = date_str[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > 31 {
        return None;
    }
    if y == 0 {
        return None;
    }
    let (adj_y, adj_m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = adj_y / 400;
    let yoe = adj_y - era * 400;
    let doy = (153 * adj_m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn current_epoch_days() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400
}

/// Parse a date (YYYY-MM-DD or RFC 3339) and check if it's at least `min_days` old.
pub fn is_date_old_enough(date_str: &str, min_days: u64) -> bool {
    let Some(date_days) = parse_date_to_days(date_str) else {
        return false;
    };
    let today = current_epoch_days();
    date_days <= today.saturating_sub(min_days)
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

/// Parse a compact duration string like "7d", "3d", "1440m", "24h" into minutes.
pub fn parse_duration_minutes(s: &str) -> Option<u64> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    if s.is_empty() {
        return None;
    }
    let (num_part, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num_part.parse().ok()?;
    match unit {
        "d" => Some(n.saturating_mul(24 * 60)),
        "h" => Some(n.saturating_mul(60)),
        "m" => Some(n),
        _ => None,
    }
}
