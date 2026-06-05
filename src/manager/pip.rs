// pip scanner: checks `uploaded-prior-to` in the [install] section of pip.conf.

use std::path::Path;

use super::config::read_ini_value;
use super::date::parse_iso8601_days;
use super::detect::get_delay_days;
use super::types::{mark_unsupported, missing_status_for_path, CheckStatus, Recommendation};
use super::version::{extract_version_str, version_at_least};

/// Minimum pip version that supports relative ISO 8601 durations for
/// `--uploaded-prior-to`. pip 26.0 added the flag with absolute datetimes only;
/// pip 26.1 added relative durations (e.g. `P7D`), which is the self-maintaining
/// value DepsGuard writes.
const PIP_MIN_MAJOR: u64 = 26;
const PIP_MIN_MINOR: u64 = 1;

/// pip stores cooldowns in the `[install]` section as `uploaded-prior-to`.
pub(crate) const PIP_KEY: &str = "install.uploaded-prior-to";

pub fn scan(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let ver = extract_version_str(version);
    let expected = format!("P{days}D");
    let description = format!("Delay new versions by {days} days");

    let val = read_ini_value(path, PIP_KEY);
    let status = match &val {
        Some(v) => {
            // Exact policy: only the requested rolling duration is OK. An absolute
            // datetime (or any other form) is a different kind of value and is flagged.
            match parse_iso8601_days(v) {
                Some(d) if d == days => CheckStatus::Ok(v.clone()),
                _ => CheckStatus::WrongValue(v.clone()),
            }
        }
        None => missing_status_for_path(path),
    };

    let rec = Recommendation {
        key: PIP_KEY.into(),
        description,
        expected,
        status,
    };

    // The value DepsGuard writes (`P7D`) is a relative ISO-8601 duration, which
    // requires pip >= 26.1. On older pip, a relative duration — or a missing
    // setting we would fill with one — is unusable, so it's reported as
    // `Unsupported` rather than an actionable fix (issue #52). A configured
    // absolute datetime works on pip 26.0, so it stays out of the gate and is
    // evaluated on its own merits (flagged as a wrong value, never an upgrade
    // prompt).
    let configured_relative_duration = val.as_deref().and_then(parse_iso8601_days).is_some();
    let would_recommend_relative = val.is_none() || configured_relative_duration;
    let rec = if would_recommend_relative && !version_at_least(ver, PIP_MIN_MAJOR, PIP_MIN_MINOR) {
        mark_unsupported(rec, "pip", PIP_MIN_MAJOR, PIP_MIN_MINOR, ver)
    } else {
        rec
    };

    vec![rec]
}
