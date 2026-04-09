// Yarn Berry scanner: checks npmMinimalAgeGate in .yarnrc.yml.

use std::path::Path;

use super::config::read_yaml_value;
use super::date::parse_duration_minutes;
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, unsupported_rec, CheckStatus, Recommendation};
use super::version::version_at_least;

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    if !version_at_least(version, 4, 10) {
        return vec![unsupported_rec(
            "npmMinimalAgeGate",
            &format!("Delay new versions by {days} days"),
            &format!("{days}d"),
            "yarn",
            4,
            10,
            version,
        )];
    }
    let val = read_yaml_value(path, "npmMinimalAgeGate");
    let required_minutes = days.saturating_mul(24).saturating_mul(60);
    let status = match &val {
        Some(v) => {
            if let Some(configured_minutes) = parse_duration_minutes(v) {
                if configured_minutes >= required_minutes {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else if let Ok(raw_minutes) = v.parse::<u64>() {
                if raw_minutes >= required_minutes {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else {
                CheckStatus::WrongValue(v.clone())
            }
        }
        None => missing_status_for_path(path),
    };
    vec![Recommendation {
        key: "npmMinimalAgeGate".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days}d"),
        status,
    }]
}
