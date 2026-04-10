// npm scanner: checks min-release-age and ignore-scripts in .npmrc.

use std::path::Path;

use super::config::{check_flat, read_flat_config};
use super::detect::get_delay_days;
use super::types::{unsupported_rec, Recommendation};
use super::version::version_at_least;

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let cfg = read_flat_config(path);
    let release_age = if version_at_least(version, 11, 10) {
        check_flat(
            path,
            &cfg,
            "min-release-age",
            &days.to_string(),
            &format!("Delay new versions by {days} days"),
        )
    } else {
        unsupported_rec(
            "min-release-age",
            &format!("Delay new versions by {days} days"),
            &days.to_string(),
            "npm",
            11,
            10,
            version,
        )
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
