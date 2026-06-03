// aube scanner: checks minimumReleaseAge in .npmrc.
//
// aube (a Node.js package manager) reads the cooldown from `.npmrc` using either
// `minimumReleaseAge` (its native camelCase key) or `minimum-release-age` (the
// kebab-case key it shares with pnpm). The value is a number of **minutes**.
// The feature ships as a secure default, so it is not version-gated.

use std::path::Path;

use super::config::read_flat_config;
use super::detect::get_delay_days;
use super::types::{missing_status_for_path, CheckStatus, Recommendation};

/// Key DepsGuard writes when fixing (aube's native camelCase form).
const AUBE_KEY: &str = "minimumReleaseAge";
/// Alias also honoured by aube (and set by pnpm) in the same `.npmrc`.
const AUBE_KEY_KEBAB: &str = "minimum-release-age";

pub fn scan(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let required_minutes = days.saturating_mul(24).saturating_mul(60);
    let cfg = read_flat_config(path);

    // aube accepts either spelling; treat the largest configured value as effective.
    let configured = [AUBE_KEY, AUBE_KEY_KEBAB]
        .iter()
        .filter_map(|k| cfg.get(*k))
        .filter_map(|v| v.parse::<u64>().ok().map(|n| (v.clone(), n)))
        .max_by_key(|(_, n)| *n);

    let status = match configured {
        Some((_, minutes)) if minutes >= required_minutes => CheckStatus::Ok,
        Some((raw, _)) => CheckStatus::WrongValue(raw),
        None => missing_status_for_path(path),
    };

    vec![Recommendation {
        key: AUBE_KEY.into(),
        description: format!("Delay new versions by {days} days"),
        expected: required_minutes.to_string(),
        status,
    }]
}
