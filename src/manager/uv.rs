// uv scanner: checks exclude-newer in uv.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::date::{is_date_old_enough, parse_relative_days};
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};

/// Minimum uv version that supports relative durations for `exclude-newer`.
const UV_MIN_MAJOR: u64 = 0;
const UV_MIN_MINOR: u64 = 9;
const UV_MIN_PATCH: u64 = 17;

/// Extract the semver portion from a uv version string like
/// `"uv 0.11.6 (65950801c 2026-04-09 aarch64-apple-darwin)"`.
fn extract_uv_version(version: &str) -> &str {
    let s = version.trim();
    let Some(numeric_start) = s.find(|c: char| c.is_ascii_digit()) else {
        return s;
    };
    let rest = &s[numeric_start..];
    rest.split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or(rest)
}

fn supports_relative_duration(version: &str) -> bool {
    let ver = extract_uv_version(version);
    super::version::parse_semver(ver).is_some_and(|(major, minor, patch)| {
        (major, minor, patch) >= (UV_MIN_MAJOR, UV_MIN_MINOR, UV_MIN_PATCH)
    })
}

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_uv_version(version);

    if !supports_relative_duration(version) {
        return vec![Recommendation {
            key: "exclude-newer".into(),
            description: format!("Delay new versions by {days} days"),
            expected: format!("{days} days"),
            status: CheckStatus::Unsupported(format!(
                "requires uv \u{2265} {UV_MIN_MAJOR}.{UV_MIN_MINOR}.{UV_MIN_PATCH} (have {ver})"
            )),
        }];
    }

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
