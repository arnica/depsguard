// Renovate scanner: checks minimumReleaseAge in renovate.json/renovaterc.

use std::path::Path;

use super::config::read_json_string_value;
use super::date::parse_relative_days;
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};

pub fn scan(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let val = read_json_string_value(path, "minimumReleaseAge");
    let status = match &val {
        Some(v) => {
            if let Some(d) = parse_relative_days(v) {
                if d >= days {
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
        key: "minimumReleaseAge".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days} days"),
        status,
    }]
}
