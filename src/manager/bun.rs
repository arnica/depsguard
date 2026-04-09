// Bun scanner: checks install.minimumReleaseAge in .bunfig.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};

pub fn scan(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let seconds = days.saturating_mul(86400);
    let delay = read_toml_value(path, "install.minimumReleaseAge");
    let delay_status = match &delay {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n >= seconds => CheckStatus::Ok,
            Ok(_) => CheckStatus::WrongValue(v.clone()),
            Err(_) => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };
    vec![Recommendation {
        key: "install.minimumReleaseAge".into(),
        description: format!("Delay new versions by {days} days"),
        expected: seconds.to_string(),
        status: delay_status,
    }]
}
