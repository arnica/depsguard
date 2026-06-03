// npm scanner: checks min-release-age and ignore-scripts in .npmrc.

use std::path::Path;

use super::config::{check_flat, check_flat_min_int, read_flat_config};
use super::detect::get_delay_days;
use super::types::{unsupported_if_configured, Recommendation};
use super::version::version_at_least;

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let cfg = read_flat_config(path);
    // npm's `min-release-age` is a number of days meaning "at least N days old",
    // so any value >= the target satisfies the policy (not an exact match).
    let release_age = check_flat_min_int(
        path,
        &cfg,
        "min-release-age",
        days,
        &format!("Delay new versions by {days} days"),
    );
    let release_age = if version_at_least(version, 11, 10) {
        release_age
    } else {
        unsupported_if_configured(release_age, "npm", 11, 10, version)
    };
    vec![
        release_age,
        check_flat(
            path,
            &cfg,
            "ignore-scripts",
            "true",
            "Block malicious install scripts",
        ),
    ]
}
