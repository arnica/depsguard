// pip scanner: checks `uploaded-prior-to` in the [install] section of pip.conf.

use std::path::Path;

use super::config::read_ini_value;
use super::date::parse_iso8601_days;
use super::detect::get_delay_days;
use super::types::{
    missing_status_for_path, unsupported_if_configured, CheckStatus, Recommendation,
};
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

    // Only relative ISO-8601 durations (e.g. `P7D`) require pip >= 26.1; absolute
    // datetimes are supported on 26.0, so don't relabel a valid absolute-date
    // config as needing an upgrade.
    let configured_relative_duration = val.as_deref().and_then(parse_iso8601_days).is_some();
    let rec =
        if configured_relative_duration && !version_at_least(ver, PIP_MIN_MAJOR, PIP_MIN_MINOR) {
            unsupported_if_configured(rec, "pip", PIP_MIN_MAJOR, PIP_MIN_MINOR, ver)
        } else {
            rec
        };

    vec![rec]
}
