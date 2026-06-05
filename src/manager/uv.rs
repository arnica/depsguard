// uv scanner: checks exclude-newer in uv.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::date::parse_relative_days;
use super::detect::get_delay_days;
use super::types::{
    mark_unsupported_with_message, missing_status_for_path, CheckStatus, Recommendation,
};
use super::version::{extract_version_str, parse_semver};

/// uv stores the cooldown as the top-level `exclude-newer` key.
pub(crate) const UV_KEY: &str = "exclude-newer";

/// Minimum uv version that supports relative durations for `exclude-newer`.
const UV_MIN_MAJOR: u64 = 0;
const UV_MIN_MINOR: u64 = 9;
const UV_MIN_PATCH: u64 = 17;

fn supports_relative_duration(version: &str) -> bool {
    let ver = extract_version_str(version);
    parse_semver(ver).is_some_and(|(major, minor, patch)| {
        (major, minor, patch) >= (UV_MIN_MAJOR, UV_MIN_MINOR, UV_MIN_PATCH)
    })
}

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_version_str(version);

    let val = read_toml_value(path, UV_KEY);
    let status = match &val {
        Some(v) => {
            // Exact policy: only the requested rolling duration is OK. An absolute
            // date (or any other form) is a different kind of value and is flagged.
            match parse_relative_days(v) {
                Some(d) if d == days => CheckStatus::Ok(v.clone()),
                _ => CheckStatus::WrongValue(v.clone()),
            }
        }
        None => missing_status_for_path(path),
    };

    let rec = Recommendation {
        key: UV_KEY.into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days} days"),
        status,
    };

    // The value DepsGuard writes (`7 days`) is a relative duration, which
    // requires uv >= 0.9.17. On older uv, a relative duration — or a missing
    // setting we would fill with one — is unusable, so it's reported as
    // `Unsupported` rather than an actionable fix (issue #52). A configured
    // absolute RFC-3339 date works on older uv, so it stays out of the gate and
    // is evaluated on its own merits (flagged as a wrong value, never an upgrade
    // prompt).
    let configured_relative_duration = val.as_deref().and_then(parse_relative_days).is_some();
    let would_recommend_relative = val.is_none() || configured_relative_duration;
    let rec = if would_recommend_relative && !supports_relative_duration(version) {
        mark_unsupported_with_message(
            rec,
            format!(
                "requires uv \u{2265} {UV_MIN_MAJOR}.{UV_MIN_MINOR}.{UV_MIN_PATCH} (have {ver})"
            ),
        )
    } else {
        rec
    };

    vec![rec]
}
