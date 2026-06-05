// npm scanner: checks min-release-age and ignore-scripts in .npmrc.

use std::path::Path;

use super::config::{check_flat, check_flat_exact_int, read_flat_config};
use super::detect::get_delay_days;
use super::types::{mark_unsupported, Recommendation};
use super::version::version_at_least;

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let cfg = read_flat_config(path);
    // npm's `min-release-age` is a number of days; DepsGuard enforces the exact
    // configured policy, so the value must equal the requested delay.
    let release_age = check_flat_exact_int(
        path,
        &cfg,
        "min-release-age",
        days,
        &format!("Delay new versions by {days} days"),
    );
    let release_age = if version_at_least(version, 11, 10) {
        release_age
    } else {
        mark_unsupported(release_age, "npm", 11, 10, version)
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
