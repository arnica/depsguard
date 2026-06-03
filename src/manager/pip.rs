// pip scanner: checks `uploaded-prior-to` in the [install] section of pip.conf.

use std::path::Path;

use super::config::read_toml_value;
use super::date::{is_date_old_enough, parse_iso8601_days};
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};
use super::version::{extract_version_str, parse_semver};

/// Minimum pip version that supports relative ISO 8601 durations for
/// `--uploaded-prior-to`. pip 26.0 added the flag with absolute datetimes only;
/// pip 26.1 added relative durations (e.g. `P7D`), which is the self-maintaining
/// value DepsGuard writes.
const PIP_MIN_MAJOR: u64 = 26;
const PIP_MIN_MINOR: u64 = 1;

/// pip stores cooldowns in the `[install]` section as `uploaded-prior-to`.
const PIP_KEY: &str = "install.uploaded-prior-to";

fn supports_relative_duration(version: &str) -> bool {
    let ver = extract_version_str(version);
    parse_semver(ver)
        .is_some_and(|(major, minor, _patch)| (major, minor) >= (PIP_MIN_MAJOR, PIP_MIN_MINOR))
}

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_version_str(version);
    let expected = format!("P{days}D");
    let description = format!("Delay new versions by {days} days");

    if !supports_relative_duration(version) {
        return vec![Recommendation {
            key: PIP_KEY.into(),
            description,
            expected,
            status: CheckStatus::Unsupported(format!(
                "requires pip \u{2265} {PIP_MIN_MAJOR}.{PIP_MIN_MINOR} (have {ver})"
            )),
        }];
    }

    let val = read_toml_value(path, PIP_KEY);
    let status = match &val {
        Some(v) => {
            if let Some(d) = parse_iso8601_days(v) {
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
        key: PIP_KEY.into(),
        description,
        expected,
        status,
    }]
}
