//! Minimal, zero-dependency JSON serialization (RFC 8259).
//!
//! DepsGuard ships no external crates (see `AGENTS.md`), so structured
//! `--format json` output is built with this hand-rolled emitter rather than
//! `serde_json`. It supports only what the scan report needs: objects (with
//! insertion order preserved), arrays, strings, integers, and booleans.

use crate::manager::{CheckStatus, ManagerInfo, Recommendation};

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

/// Render scan results as the schema 1.1 JSON document (the documented contract
/// lives in `README.md`). Returns a complete JSON object with no trailing
/// newline. `scanned_at` is captured once by the caller (see
/// `crate::manager::date::format_timestamp`) so this stays pure/testable.
pub fn render_scan_json(managers: &[ManagerInfo], scanned_at: &str) -> String {
    // Same collapsed counts the text summary uses, from one shared source, so
    // the two reports never drift (see `crate::ui::summary_counts`).
    let counts = crate::ui::summary_counts(managers);
    let needs_fix = managers
        .iter()
        .any(|m| m.recommendations.iter().any(|r| r.needs_fix()));

    let summary = obj(vec![
        ("needs_fix", Json::Bool(needs_fix)),
        ("unique_configs", Json::Int(counts.unique_configs as i64)),
        (
            "counts",
            obj(vec![
                ("ok", Json::Int(counts.ok as i64)),
                ("missing", Json::Int(counts.missing as i64)),
                ("file_missing", Json::Int(counts.file_missing as i64)),
                ("wrong_value", Json::Int(counts.wrong_value as i64)),
                ("unsupported", Json::Int(counts.unsupported as i64)),
                ("total_errors", Json::Int(counts.total_errors() as i64)),
            ]),
        ),
    ]);

    let managers_json = Json::Array(managers.iter().map(manager_json).collect());

    let doc = obj(vec![
        ("schema_version", Json::str("1.1")),
        ("tool", Json::str("depsguard")),
        ("tool_version", Json::str(env!("CARGO_PKG_VERSION"))),
        ("scanned_at", Json::str(scanned_at)),
        ("summary", summary),
        ("managers", managers_json),
    ]);

    doc.to_string()
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
    obj(vec![
        ("key", Json::str(rec.key.clone())),
        ("description", Json::str(rec.description.clone())),
        ("expected", Json::str(rec.expected.clone())),
        ("needs_fix", Json::Bool(rec.needs_fix())),
        ("status", status_json(&rec.status)),
    ])
}

/// Map a `CheckStatus` to its JSON object. `value` accompanies `ok`/`wrong_value`;
/// `message` accompanies `unsupported`; `missing`/`file_missing` carry neither.
fn status_json(status: &CheckStatus) -> Json {
    match status {
        CheckStatus::Ok(v) => obj(vec![
            ("state", Json::str("ok")),
            ("value", Json::str(v.clone())),
        ]),
        CheckStatus::Missing => obj(vec![("state", Json::str("missing"))]),
        CheckStatus::FileMissing => obj(vec![("state", Json::str("file_missing"))]),
        CheckStatus::WrongValue(v) => obj(vec![
            ("state", Json::str("wrong_value")),
            ("value", Json::str(v.clone())),
        ]),
        CheckStatus::Unsupported(m) => obj(vec![
            ("state", Json::str("unsupported")),
            ("message", Json::str(m.clone())),
        ]),
    }
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
        assert!(s.contains(r#""schema_version":"1.1""#));
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
            ],
        );
        let s = render_scan_json(&[m], "2024-01-01T00:00:00Z");
        assert!(s.contains(r#"{"state":"ok","value":"true"}"#));
        assert!(s.contains(r#"{"state":"missing"}"#));
        assert!(s.contains(r#"{"state":"file_missing"}"#));
        assert!(s.contains(r#"{"state":"wrong_value","value":"0"}"#));
        assert!(s.contains(r#"{"state":"unsupported","message":"requires npm >= 11"}"#));
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
}
