// pip scanner: checks `uploaded-prior-to` in the [install] section of pip.conf.

use std::path::Path;

use super::config::read_ini_value;
use super::date::parse_iso8601_days;
use super::detect::get_delay_days;
use super::types::{
    mark_unsupported, mark_unsupported_with_message, missing_status_for_path, CheckStatus,
    Recommendation,
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

    // The value DepsGuard writes (`P7D`) is a relative ISO-8601 duration, which
    // requires pip >= 26.1, so on older pip every state is version-gated
    // (issue #52): a missing setting would be filled with a value the tool
    // can't parse, a configured relative duration is already unusable, and a
    // configured absolute datetime — which does work on pip 26.0 — must not be
    // offered a fix that replaces it with the unsupported relative form. The
    // absolute-datetime case keeps a message naming the current value, since
    // the value itself works and only the recommended form needs the upgrade.
    let rec = if version_at_least(ver, PIP_MIN_MAJOR, PIP_MIN_MINOR) {
        rec
    } else {
        let configured_non_relative = val
            .as_deref()
            .is_some_and(|v| parse_iso8601_days(v).is_none());
        match val.as_deref() {
            Some(v) if configured_non_relative => mark_unsupported_with_message(
                rec,
                format!(
                    "set to {v} — relative durations require pip \u{2265} \
                     {PIP_MIN_MAJOR}.{PIP_MIN_MINOR} (have {ver})"
                ),
            ),
            _ => mark_unsupported(rec, "pip", PIP_MIN_MAJOR, PIP_MIN_MINOR, ver),
        }
    };

    vec![rec]
}
