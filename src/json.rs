//! Minimal, zero-dependency JSON serialization (RFC 8259).
//!
//! DepsGuard ships no external crates (see `AGENTS.md`), so structured
//! `--format json` output is built with this hand-rolled emitter rather than
//! `serde_json`. It supports only what the scan report needs: objects (with
//! insertion order preserved), arrays, strings, integers, and booleans.

use crate::manager::{CheckStatus, ManagerInfo, Recommendation};

const SCHEMA_VERSION: &str = "1.2";

/// A JSON value. `Object` preserves insertion order so the emitted document is
/// stable and diff-friendly across runs.
pub enum Json {
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Convenience constructor for a string value.
    fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }
}

/// Build an object from `(key, value)` pairs, preserving insertion order.
fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Json::Bool(b) => write!(f, "{b}"),
            Json::Int(n) => write!(f, "{n}"),
            Json::Str(s) => write_escaped(f, s),
            Json::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Json::Object(pairs) => {
                f.write_str("{")?;
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write_escaped(f, k)?;
                    f.write_str(":")?;
                    write!(f, "{v}")?;
                }
                f.write_str("}")
            }
        }
    }
}

/// Write `s` as a quoted, RFC 8259-escaped JSON string. Control characters
/// below `0x20` without a short escape are emitted as `\u00XX`; all other
/// characters (including non-ASCII UTF-8) pass through unchanged.
fn write_escaped(f: &mut std::fmt::Formatter<'_>, s: &str) -> std::fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            '\u{08}' => f.write_str("\\b")?,
            '\u{0c}' => f.write_str("\\f")?,
            c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
            c => write!(f, "{c}")?,
        }
    }
    f.write_str("\"")
}

/// Render scan results as the schema 1.2 JSON document (the documented contract
/// lives in `README.md`). Returns a complete JSON object with no trailing
/// newline. `scanned_at` is captured once by the caller (see
/// `crate::manager::date::format_timestamp`) so this stays pure/testable.
pub fn render_scan_json(managers: &[ManagerInfo], scanned_at: &str) -> String {
    // Same collapsed counts the text summary uses, from one shared source, so
    // the two reports never drift (see `crate::ui::summary_counts`).
    let counts = crate::ui::summary_counts(managers);
    let needs_fix = crate::has_needs_fix(managers);

    let summary = obj(vec![
        ("needs_fix", Json::Bool(needs_fix)),
        ("unique_configs", Json::Int(counts.unique_configs as i64)),
        ("counts", counts_json(&counts)),
    ]);

    let managers_json = Json::Array(managers.iter().map(manager_json).collect());

    let doc = obj(vec![
        ("schema_version", Json::str(SCHEMA_VERSION)),
        ("tool", Json::str("depsguard")),
        ("tool_version", Json::str(env!("CARGO_PKG_VERSION"))),
        ("scanned_at", Json::str(scanned_at)),
        ("summary", summary),
        ("managers", managers_json),
    ]);

    doc.to_string()
}

fn counts_json(counts: &crate::ui::SummaryCounts) -> Json {
    obj(vec![
        ("ok", Json::Int(counts.ok as i64)),
        ("missing", Json::Int(counts.missing as i64)),
        ("file_missing", Json::Int(counts.file_missing as i64)),
        ("wrong_value", Json::Int(counts.wrong_value as i64)),
        ("manual", Json::Int(counts.manual as i64)),
        ("unsupported", Json::Int(counts.unsupported as i64)),
        ("indeterminate", Json::Int(counts.indeterminate as i64)),
        ("total_errors", Json::Int(counts.total_errors() as i64)),
    ])
}

fn manager_json(mgr: &ManagerInfo) -> Json {
    let recs = Json::Array(
        mgr.recommendations
            .iter()
            .map(recommendation_json)
            .collect(),
    );
    obj(vec![
        ("name", Json::str(mgr.kind.name())),
        ("version", Json::str(mgr.version.clone())),
        // Raw absolute path: the machine field must be unambiguous and resolvable
        // (no `~` collapsing). See KTD7 in the plan.
        (
            "config_path",
            Json::str(mgr.config_path.to_string_lossy().into_owned()),
        ),
        ("discovered", Json::Bool(mgr.discovered)),
        ("recommendations", recs),
    ])
}

fn recommendation_json(rec: &Recommendation) -> Json {
    let mut fields = vec![
        ("key", Json::str(rec.key.clone())),
        ("description", Json::str(rec.description.clone())),
        ("expected", Json::str(rec.expected.clone())),
        ("needs_fix", Json::Bool(rec.needs_fix())),
        ("auto_fixable", Json::Bool(rec.auto_fixable())),
    ];
    if let Some(message) = rec.remediation_message() {
        fields.push(("remediation_message", Json::str(message)));
    }
    fields.push(("status", status_json(&rec.status)));
    obj(fields)
}

/// Map a `CheckStatus` to its state name plus an optional value/message: `value`
/// accompanies `ok`, `wrong_value`, and `manual`; `message` accompanies
/// `unsupported`, `manual`, and `indeterminate`;
/// `missing`/`file_missing` carry neither. Shared by the blob's nested `status`
/// object and NDJSON's flattened `status_value`/`status_message` fields.
fn status_fields(status: &CheckStatus) -> (&'static str, Option<&str>, Option<&str>) {
    match status {
        CheckStatus::Ok(v) => ("ok", Some(v.as_str()), None),
        CheckStatus::Missing => ("missing", None, None),
        CheckStatus::FileMissing => ("file_missing", None, None),
        CheckStatus::WrongValue(v) => ("wrong_value", Some(v.as_str()), None),
        CheckStatus::Unsupported(m) => ("unsupported", None, Some(m.as_str())),
        CheckStatus::Manual(v, m) => ("manual", Some(v.as_str()), Some(m.as_str())),
        CheckStatus::Indeterminate(m) => ("indeterminate", None, Some(m.as_str())),
    }
}

fn status_json(status: &CheckStatus) -> Json {
    let (state, value, message) = status_fields(status);
    let mut fields = vec![("state", Json::str(state))];
    if let Some(v) = value {
        fields.push(("value", Json::str(v)));
    }
    if let Some(m) = message {
        fields.push(("message", Json::str(m)));
    }
    obj(fields)
}

/// Render scan results as NDJSON: one line per (manager, recommendation)
/// finding, followed by one summary trailer line. Shares the blob format's
/// `schema_version` counter (see README). Every line carries the same
/// envelope and `scanned_at` so it stands alone; finding lines are raw
/// (uncollapsed) even when managers share a `(config_path, key)`, the summary
/// line's counts are what collapse to the most-significant status.
pub fn render_scan_ndjson(managers: &[ManagerInfo], scanned_at: &str) -> String {
    let mut lines: Vec<String> = managers
        .iter()
        .flat_map(|mgr| {
            mgr.recommendations
                .iter()
                .map(move |rec| finding_line(mgr, rec, scanned_at).to_string())
        })
        .collect();
    lines.push(summary_line(managers, scanned_at).to_string());
    lines.join("\n")
}

/// The four fields every NDJSON line carries, identical across a run.
fn envelope(record_type: &str, scanned_at: &str) -> Vec<(&'static str, Json)> {
    vec![
        ("record_type", Json::str(record_type)),
        ("schema_version", Json::str(SCHEMA_VERSION)),
        ("tool", Json::str("depsguard")),
        ("tool_version", Json::str(env!("CARGO_PKG_VERSION"))),
        ("scanned_at", Json::str(scanned_at)),
    ]
}

fn finding_line(mgr: &ManagerInfo, rec: &Recommendation, scanned_at: &str) -> Json {
    let mut fields = envelope("finding", scanned_at);
    fields.push(("manager", Json::str(mgr.kind.name())));
    fields.push(("manager_version", Json::str(mgr.version.clone())));
    fields.push((
        "config_path",
        Json::str(mgr.config_path.to_string_lossy().into_owned()),
    ));
    fields.push(("discovered", Json::Bool(mgr.discovered)));
    fields.push(("key", Json::str(rec.key.clone())));
    fields.push(("description", Json::str(rec.description.clone())));
    fields.push(("expected", Json::str(rec.expected.clone())));
    fields.push(("needs_fix", Json::Bool(rec.needs_fix())));
    fields.push(("auto_fixable", Json::Bool(rec.auto_fixable())));
    if let Some(message) = rec.remediation_message() {
        fields.push(("remediation_message", Json::str(message)));
    }
    let (state, value, message) = status_fields(&rec.status);
    fields.push(("status", Json::str(state)));
    if let Some(v) = value {
        fields.push(("status_value", Json::str(v)));
    }
    if let Some(m) = message {
        fields.push(("status_message", Json::str(m)));
    }
    obj(fields)
}

fn summary_line(managers: &[ManagerInfo], scanned_at: &str) -> Json {
    let counts = crate::ui::summary_counts(managers);
    let needs_fix = crate::has_needs_fix(managers);

    let mut fields = envelope("summary", scanned_at);
    fields.push(("needs_fix", Json::Bool(needs_fix)));
    fields.push(("unique_configs", Json::Int(counts.unique_configs as i64)));
    fields.push(("counts", counts_json(&counts)));
    obj(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::ManagerKind;
    use std::path::PathBuf;

    // ── U1: serializer ──────────────────────────────────────────────

    #[test]
    fn serializes_nested_object_and_array() {
        let v = obj(vec![
            ("a", Json::Int(1)),
            ("b", Json::Array(vec![Json::Bool(true), Json::str("x")])),
            ("c", obj(vec![("d", Json::str("e"))])),
        ]);
        assert_eq!(v.to_string(), r#"{"a":1,"b":[true,"x"],"c":{"d":"e"}}"#);
    }

    #[test]
    fn escapes_control_and_special_characters() {
        let v = Json::str("\"\\\n\r\t\u{08}\u{0c}\u{01}");
        assert_eq!(v.to_string(), r#""\"\\\n\r\t\b\f\u0001""#);
    }

    #[test]
    fn empty_object_and_array() {
        assert_eq!(Json::Object(vec![]).to_string(), "{}");
        assert_eq!(Json::Array(vec![]).to_string(), "[]");
    }

    #[test]
    fn non_ascii_passes_through_unescaped() {
        let v = Json::str("café—naïve");
        assert_eq!(v.to_string(), "\"café—naïve\"");
    }

    #[test]
    fn integers_have_no_quotes_or_decimals() {
        assert_eq!(Json::Int(42).to_string(), "42");
        assert_eq!(Json::Int(0).to_string(), "0");
        assert_eq!(Json::Int(-5).to_string(), "-5");
    }

    // ── U2: scan report ─────────────────────────────────────────────

    fn rec(key: &str, status: CheckStatus) -> Recommendation {
        Recommendation {
            key: key.into(),
            description: "d".into(),
            expected: "e".into(),
            status,
        }
    }

    fn mgr(
        kind: ManagerKind,
        path: &str,
        discovered: bool,
        recs: Vec<Recommendation>,
    ) -> ManagerInfo {
        ManagerInfo {
            kind,
            version: "1.0".into(),
            config_path: PathBuf::from(path),
            recommendations: recs,
            discovered,
        }
    }

    #[test]
    fn report_has_schema_envelope() {
        let s = render_scan_json(&[], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""schema_version":"1.2""#));
        assert!(s.contains(r#""tool":"depsguard""#));
        assert!(s.contains(r#""scanned_at":"2024-01-01T00:00:00Z""#));
        assert!(s.contains(r#""managers":[]"#));
    }

    #[test]
    fn empty_managers_is_all_clear() {
        let s = render_scan_json(&[], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""needs_fix":false"#));
        assert!(s.contains(r#""unique_configs":0"#));
        assert!(s.contains(r#""total_errors":0"#));
    }

    #[test]
    fn status_states_map_correctly() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![
                rec("a", CheckStatus::Ok("true".into())),
                rec("b", CheckStatus::Missing),
                rec("c", CheckStatus::FileMissing),
                rec("d", CheckStatus::WrongValue("0".into())),
                rec("e", CheckStatus::Unsupported("requires npm >= 11".into())),
                rec(
                    "f",
                    CheckStatus::Manual("1d".into(), "edit the project source".into()),
                ),
                rec("g", CheckStatus::Indeterminate("dynamic expression".into())),
            ],
        );
        let s = render_scan_json(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#"{"state":"ok","value":"true"}"#));
        assert!(s.contains(r#"{"state":"missing"}"#));
        assert!(s.contains(r#"{"state":"file_missing"}"#));
        assert!(s.contains(r#"{"state":"wrong_value","value":"0"}"#));
        assert!(s.contains(r#"{"state":"unsupported","message":"requires npm >= 11"}"#));
        assert!(
            s.contains(r#"{"state":"manual","value":"1d","message":"edit the project source"}"#)
        );
        assert!(s.contains(r#"{"state":"indeterminate","message":"dynamic expression"}"#));
    }

    #[test]
    fn summary_collapses_shared_key_to_most_significant() {
        // Two managers share one config + key with differing statuses; the
        // summary counts the most-significant (error) once, matching the text
        // report, and reports a single unique config.
        let m1 = mgr(
            ManagerKind::Npm,
            "/shared",
            false,
            vec![rec("k", CheckStatus::Ok("v".into()))],
        );
        let m2 = mgr(
            ManagerKind::Pnpm,
            "/shared",
            false,
            vec![rec("k", CheckStatus::Missing)],
        );
        let s = render_scan_json(&[m1, m2], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""missing":1"#));
        assert!(s.contains(r#""ok":0"#));
        assert!(s.contains(r#""unique_configs":1"#));
        assert!(s.contains(r#""needs_fix":true"#));
    }

    #[test]
    fn escapes_values_in_report_output() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec("k", CheckStatus::WrongValue("a\"b".into()))],
        );
        let s = render_scan_json(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""value":"a\"b""#));
    }

    #[test]
    fn report_marks_manual_findings_as_actionable_but_not_auto_fixable() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec(
                "k",
                CheckStatus::Manual("1d".into(), "edit the project source".into()),
            )],
        );
        let s = render_scan_json(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""needs_fix":true"#));
        assert!(s.contains(r#""auto_fixable":false"#));
        assert!(s.contains(r#""remediation_message":"edit the project source""#));
        assert!(s.contains(r#""manual":1"#));
        assert!(s.contains(r#""total_errors":1"#));
    }

    #[test]
    fn report_counts_indeterminate_findings_without_failing_the_scan() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec(
                "k",
                CheckStatus::Indeterminate("dynamic expression".into()),
            )],
        );
        let s = render_scan_json(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""needs_fix":false"#));
        assert!(s.contains(r#""indeterminate":1"#));
    }

    // ── U2: NDJSON renderer ─────────────────────────────────────────

    fn is_single_json_object(line: &str) -> bool {
        line.starts_with('{') && line.ends_with('}') && !line.contains('\n')
    }

    #[test]
    fn ndjson_emits_one_finding_line_per_recommendation_plus_summary_trailer() {
        let m1 = mgr(
            ManagerKind::Npm,
            "/a",
            false,
            vec![
                rec("k1", CheckStatus::Ok("v".into())),
                rec("k2", CheckStatus::Missing),
            ],
        );
        let m2 = mgr(
            ManagerKind::Pnpm,
            "/b",
            false,
            vec![
                rec("k3", CheckStatus::Ok("v".into())),
                rec("k4", CheckStatus::Missing),
            ],
        );
        let s = render_scan_ndjson(&[m1, m2], "2024-01-01T00:00:00Z");
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 5);
        for line in &lines {
            assert!(
                is_single_json_object(line),
                "not a standalone object: {line}"
            );
        }
        assert_eq!(
            lines[..4]
                .iter()
                .filter(|l| l.contains(r#""record_type":"finding""#))
                .count(),
            4
        );
        assert!(lines[4].contains(r#""record_type":"summary""#));
    }

    #[test]
    fn ndjson_empty_managers_emits_summary_line_only() {
        let s = render_scan_ndjson(&[], "2024-01-01T00:00:00Z");
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains(r#""record_type":"summary""#));
        assert!(lines[0].contains(r#""unique_configs":0"#));
    }

    #[test]
    fn ndjson_manager_with_no_recommendations_has_no_finding_line() {
        let m = mgr(ManagerKind::Renovate, "/repo/renovate.json", true, vec![]);
        let s = render_scan_ndjson(&[m], "2024-01-01T00:00:00Z");
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 1, "no recommendations means no finding lines");
        assert!(lines[0].contains(r#""record_type":"summary""#));
        assert!(
            lines[0].contains(r#""unique_configs":1"#),
            "config_path still counted even with zero recommendations"
        );
    }

    #[test]
    fn ndjson_shared_config_and_key_emits_both_finding_lines_raw() {
        // N3a: finding lines are raw/uncollapsed even when two managers share a
        // (config_path, key); only the summary collapses to most-significant.
        let m1 = mgr(
            ManagerKind::Npm,
            "/shared",
            false,
            vec![rec("k", CheckStatus::Ok("v".into()))],
        );
        let m2 = mgr(
            ManagerKind::Pnpm,
            "/shared",
            false,
            vec![rec("k", CheckStatus::Missing)],
        );
        let s = render_scan_ndjson(&[m1, m2], "2024-01-01T00:00:00Z");
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 3, "both finding lines plus one summary");
        assert!(lines[0].contains(r#""manager":"npm""#) && lines[0].contains(r#""status":"ok""#));
        assert!(
            lines[1].contains(r#""manager":"pnpm""#) && lines[1].contains(r#""status":"missing""#)
        );
        assert!(lines[2].contains(r#""missing":1"#));
        assert!(lines[2].contains(r#""ok":0"#));
        assert!(lines[2].contains(r#""unique_configs":1"#));
    }

    #[test]
    fn ndjson_status_states_map_to_flat_fields() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![
                rec("a", CheckStatus::Ok("true".into())),
                rec("b", CheckStatus::Missing),
                rec("c", CheckStatus::FileMissing),
                rec("d", CheckStatus::WrongValue("0".into())),
                rec("e", CheckStatus::Unsupported("requires npm >= 11".into())),
                rec(
                    "f",
                    CheckStatus::Manual("1d".into(), "edit the project source".into()),
                ),
                rec("g", CheckStatus::Indeterminate("dynamic expression".into())),
            ],
        );
        let s = render_scan_ndjson(&[m], "2024-01-01T00:00:00Z");
        let lines: Vec<&str> = s.split('\n').collect();
        assert!(
            lines[0].contains(r#""status":"ok""#) && lines[0].contains(r#""status_value":"true""#)
        );
        assert!(
            lines[1].contains(r#""status":"missing""#)
                && !lines[1].contains("status_value")
                && !lines[1].contains("status_message")
        );
        assert!(
            lines[2].contains(r#""status":"file_missing""#)
                && !lines[2].contains("status_value")
                && !lines[2].contains("status_message")
        );
        assert!(
            lines[3].contains(r#""status":"wrong_value""#)
                && lines[3].contains(r#""status_value":"0""#)
        );
        assert!(
            lines[4].contains(r#""status":"unsupported""#)
                && lines[4].contains(r#""status_message":"requires npm >= 11""#)
        );
        assert!(
            lines[5].contains(r#""status":"manual""#)
                && lines[5].contains(r#""status_value":"1d""#)
                && lines[5].contains(r#""auto_fixable":false"#)
                && lines[5].contains(r#""remediation_message":"edit the project source""#),
            "manual finding line: {}",
            lines[5]
        );
        assert!(
            lines[6].contains(r#""status":"indeterminate""#)
                && lines[6].contains(r#""status_message":"dynamic expression""#)
        );
    }

    #[test]
    fn ndjson_summary_counts_match_blob_counts() {
        let m1 = mgr(
            ManagerKind::Npm,
            "/a",
            false,
            vec![rec("k1", CheckStatus::Ok("v".into()))],
        );
        let m2 = mgr(
            ManagerKind::Pnpm,
            "/b",
            false,
            vec![rec("k2", CheckStatus::Missing)],
        );
        let managers = [m1, m2];
        let blob = render_scan_json(&managers, "2024-01-01T00:00:00Z");
        let ndjson = render_scan_ndjson(&managers, "2024-01-01T00:00:00Z");
        let summary_line = ndjson.split('\n').last().unwrap();
        for fragment in [
            r#""ok":1"#,
            r#""missing":1"#,
            r#""file_missing":0"#,
            r#""wrong_value":0"#,
            r#""manual":0"#,
            r#""unsupported":0"#,
            r#""total_errors":1"#,
            r#""unique_configs":2"#,
        ] {
            assert!(blob.contains(fragment), "blob missing {fragment}");
            assert!(
                summary_line.contains(fragment),
                "summary missing {fragment}"
            );
        }
    }

    #[test]
    fn ndjson_every_line_shares_identical_envelope() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec("k", CheckStatus::Missing)],
        );
        let s = render_scan_ndjson(&[m], "2026-07-09T14:00:00Z");
        for line in s.split('\n') {
            assert!(line.contains(r#""schema_version":"1.2""#));
            assert!(line.contains(r#""tool":"depsguard""#));
            assert!(line.contains(&format!(
                r#""tool_version":"{}""#,
                env!("CARGO_PKG_VERSION")
            )));
            assert!(line.contains(r#""scanned_at":"2026-07-09T14:00:00Z""#));
        }
    }

    #[test]
    fn ndjson_has_no_trailing_blank_line() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec("k", CheckStatus::Missing)],
        );
        let s = render_scan_ndjson(&[m], "2024-01-01T00:00:00Z");
        assert!(!s.ends_with('\n'));
        assert!(s.split('\n').all(|l| !l.is_empty()));
    }

    #[test]
    fn ndjson_escapes_values() {
        let m = mgr(
            ManagerKind::Npm,
            "/cfg",
            false,
            vec![rec("k", CheckStatus::WrongValue("a\"b".into()))],
        );
        let s = render_scan_ndjson(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#""status_value":"a\"b""#));
    }
}
