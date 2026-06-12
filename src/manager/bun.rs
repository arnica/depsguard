// Bun scanner: checks install.minimumReleaseAge in .bunfig.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::detect::get_delay_days;
use super::types::{gate_min_version, missing_status_for_path, CheckStatus, Recommendation};

/// Minimum bun version that supports `install.minimumReleaseAge` (added in 1.3.0).
const BUN_MIN_MAJOR: u64 = 1;
const BUN_MIN_MINOR: u64 = 3;

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let seconds = days.saturating_mul(86400);
    let delay = read_toml_value(path, "install.minimumReleaseAge");
    let delay_status = match &delay {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n == seconds => CheckStatus::Ok(v.clone()),
            Ok(_) => CheckStatus::WrongValue(v.clone()),
            Err(_) => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };
    let rec = Recommendation {
        key: "install.minimumReleaseAge".into(),
        description: format!("Delay new versions by {days} days"),
        expected: seconds.to_string(),
        status: delay_status,
    };
    vec![gate_min_version(
        rec,
        "bun",
        BUN_MIN_MAJOR,
        BUN_MIN_MINOR,
        version,
    )]
}
