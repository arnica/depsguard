// uv scanner: checks exclude-newer in uv.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::date::{is_date_old_enough, parse_relative_days};
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};

pub fn scan(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let val = read_toml_value(path, "exclude-newer");
    let status = match &val {
        Some(v) => {
            if let Some(d) = parse_relative_days(v) {
                if d >= days {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else if is_date_old_enough(v, days) {
                CheckStatus::Ok
            } else {
                CheckStatus::WrongValue(v.clone())
            }
        }
        None => missing_status_for_path(path),
    };
    vec![Recommendation {
        key: "exclude-newer".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days} days"),
        status,
    }]
}
