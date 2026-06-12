// bundler scanner: checks `BUNDLE_COOLDOWN` in ~/.bundle/config.

use std::path::Path;

use super::config::read_yaml_value;
use super::detect::get_delay_days;
use super::types::{
    mark_unsupported_with_message, missing_status_for_path, CheckStatus, Recommendation,
};
use super::version::{extract_version_str, parse_semver};

/// Minimum Bundler version that supports the `cooldown` setting (added in 4.0.13).
const BUNDLER_MIN_MAJOR: u64 = 4;
const BUNDLER_MIN_MINOR: u64 = 0;
const BUNDLER_MIN_PATCH: u64 = 13;

/// `bundle config set --global cooldown N` writes `BUNDLE_COOLDOWN` (a
/// non-negative integer number of days) to `~/.bundle/config`.
pub(crate) const BUNDLER_KEY: &str = "BUNDLE_COOLDOWN";

fn supports_cooldown(version: &str) -> bool {
    let ver = extract_version_str(version);
    parse_semver(ver).is_some_and(|(major, minor, patch)| {
        (major, minor, patch) >= (BUNDLER_MIN_MAJOR, BUNDLER_MIN_MINOR, BUNDLER_MIN_PATCH)
    })
}

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_version_str(version);

    let val = read_yaml_value(path, BUNDLER_KEY);
    let status = match &val {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n == days => CheckStatus::Ok(v.clone()),
            _ => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };

    let rec = Recommendation {
        key: BUNDLER_KEY.into(),
        description: format!("Delay new versions by {days} days"),
        expected: days.to_string(),
        status,
    };

    // `cooldown` needs patch-level precision (4.0.13), so the gate condition is
    // computed here and the verdict applied via the low-level helper (see the
    // version-gated settings invariant in AGENTS.md).
    let rec = if supports_cooldown(version) {
        rec
    } else {
        mark_unsupported_with_message(
            rec,
            format!(
                "requires bundler \u{2265} {BUNDLER_MIN_MAJOR}.{BUNDLER_MIN_MINOR}.\
                 {BUNDLER_MIN_PATCH} (have {ver})"
            ),
        )
    };

    vec![rec]
}
