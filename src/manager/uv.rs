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
    // requires uv >= 0.9.17, so on older uv every state is version-gated
    // (issue #52): a missing setting would be filled with a value the tool
    // can't parse, a configured relative duration is already unusable, and a
    // configured absolute RFC-3339 date — which does work on older uv — must
    // not be offered a fix that replaces it with the unsupported relative
    // form. The absolute-date case keeps a message naming the current value,
    // since the value itself works and only the recommended form needs the
    // upgrade.
    let rec = if supports_relative_duration(version) {
        rec
    } else {
        let configured_non_relative = val
            .as_deref()
            .is_some_and(|v| parse_relative_days(v).is_none());
        let msg = match val.as_deref() {
            Some(v) if configured_non_relative => format!(
                "set to {v} — relative durations require uv \u{2265} \
                 {UV_MIN_MAJOR}.{UV_MIN_MINOR}.{UV_MIN_PATCH} (have {ver})"
            ),
            _ => format!(
                "requires uv \u{2265} {UV_MIN_MAJOR}.{UV_MIN_MINOR}.{UV_MIN_PATCH} (have {ver})"
            ),
        };
        mark_unsupported_with_message(rec, msg)
    };

    vec![rec]
}
