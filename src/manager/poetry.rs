// poetry scanner: checks `solver.min-release-age` in poetry's config.toml.

use std::path::Path;

use super::config::read_toml_value;
use super::detect::get_delay_days;
use super::types::{
    missing_status_for_path, unsupported_if_configured, CheckStatus, Recommendation,
};
use super::version::{extract_version_str, version_at_least};

/// Minimum poetry version that supports `solver.min-release-age` (added in 2.4.0).
const POETRY_MIN_MAJOR: u64 = 2;
const POETRY_MIN_MINOR: u64 = 4;

/// poetry stores the cooldown under `[solver]` as `min-release-age` (integer days).
pub(crate) const POETRY_KEY: &str = "solver.min-release-age";

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_version_str(version);
    let expected = days.to_string();
    let description = format!("Delay new versions by {days} days");

    let val = read_toml_value(path, POETRY_KEY);
    let status = match &val {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n >= days => CheckStatus::Ok,
            _ => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };

    let rec = Recommendation {
        key: POETRY_KEY.into(),
        description,
        expected,
        status,
    };

    let rec = if version_at_least(ver, POETRY_MIN_MAJOR, POETRY_MIN_MINOR) {
        rec
    } else {
        unsupported_if_configured(rec, "poetry", POETRY_MIN_MAJOR, POETRY_MIN_MINOR, ver)
    };

    vec![rec]
}
