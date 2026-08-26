// Config file readers: flat (INI), TOML, YAML, JSON/JSONC, and Dependabot YAML.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::types::{missing_status_for_path, CheckStatus, Recommendation};

// ── Flat (INI-style) config ──────────────────────────────────────────

/// Read a flat key=value config file (.npmrc style). Ignores comments (#/;).
pub fn read_flat_config(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            // Only strip comments where # is preceded by whitespace, to preserve
            // URL fragments and other values containing #.
            let v = if let Some(pos) = v.find(" #").or_else(|| v.find("\t#")) {
                &v[..pos]
            } else {
                v
            };
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Check a flat config key for an exact match.
pub fn check_flat(
    path: &Path,
    cfg: &HashMap<String, String>,
    key: &str,
    expected: &str,
    desc: &str,
) -> Recommendation {
    let status = match cfg.get(key) {
        Some(v) if v == expected => CheckStatus::Ok(v.clone()),
        Some(v) => CheckStatus::WrongValue(v.clone()),
        None => missing_status_for_path(path),
    };
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: expected.into(),
        status,
    }
}

/// Check a flat config key as an integer equal to the exact target.
pub fn check_flat_exact_int(
    path: &Path,
    cfg: &HashMap<String, String>,
    key: &str,
    expected: u64,
    desc: &str,
) -> Recommendation {
    let status = match cfg.get(key) {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n == expected => CheckStatus::Ok(v.clone()),
            _ => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: expected.to_string(),
        status,
    }
}

// ── TOML config ──────────────────────────────────────────────────────

/// Split a TOML key in `section.key` notation into its section and leaf key.
/// The final dot separates the leaf so nested sections such as `tool.pdm` work.
pub fn split_toml_key(dotted_key: &str) -> (Option<&str>, &str) {
    if let Some((section, key)) = dotted_key.rsplit_once('.') {
        (Some(section), key)
    } else {
        (None, dotted_key)
    }
}

/// Read a TOML value from a simple TOML file. Supports `key = value` and
/// `[nested.section]`. For nested keys use fully dotted `section.key` notation.
pub fn read_toml_value(path: &Path, dotted_key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let (target_section, target_key) = split_toml_key(dotted_key);

    let mut current_section: Option<&str> = None;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(inner.trim());
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            // Only strip comments where # is preceded by whitespace, to preserve
            // URL fragments and other values containing #.
            let v = if let Some(pos) = v.find(" #").or_else(|| v.find("\t#")) {
                &v[..pos]
            } else {
                v
            };
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if current_section == target_section && k == target_key {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Read a value from an INI / Python-`configparser` file (pip.conf / pip.ini).
///
/// Matches configparser's default semantics, which differ from TOML:
/// - both `=` and `:` are key/value delimiters (first one wins);
/// - only whole-line comments are recognized (`#` or `;` at the start of a
///   line); inline comments are NOT stripped, the value is the full remainder
///   of the line, so `key = P7D ; note` has the literal value `P7D ; note`;
/// - values are not unquoted.
///
/// Use `section.key` notation for sectioned keys (e.g. `install.uploaded-prior-to`).
pub fn read_ini_value(path: &Path, dotted_key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let parts: Vec<&str> = dotted_key.splitn(2, '.').collect();
    let (target_section, target_key) = if parts.len() == 2 {
        (Some(parts[0]), parts[1])
    } else {
        (None, parts[0])
    };

    let mut current_section: Option<&str> = None;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(inner.trim());
            continue;
        }
        // configparser accepts either `=` or `:`; the earliest delimiter wins.
        if let Some(pos) = line.find(['=', ':']) {
            let k = line[..pos].trim();
            let v = line[pos + 1..].trim();
            if current_section == target_section && k == target_key {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ── YAML config ──────────────────────────────────────────────────────

/// Read a top-level key from a simple YAML file.
pub fn read_yaml_value(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim();
            if k == key {
                // Only strip comments where # is preceded by whitespace, to preserve
                // URL fragments and other values containing #.
                let v = if let Some(pos) = v.find(" #").or_else(|| v.find("\t#")) {
                    &v[..pos]
                } else {
                    v
                };
                let v = v.trim().trim_matches('"').trim_matches('\'');
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Check mode for YAML values.
pub enum YamlCheck {
    Exact,
    ExactInt(u64),
}

/// Check a YAML config key using the given mode.
pub fn check_yaml(
    path: &Path,
    key: &str,
    expected: &str,
    desc: &str,
    check: YamlCheck,
) -> Recommendation {
    let val = read_yaml_value(path, key);
    let status = match (&val, &check) {
        (None, _) => missing_status_for_path(path),
        (Some(v), YamlCheck::Exact) if v == expected => CheckStatus::Ok(v.clone()),
        (Some(v), YamlCheck::ExactInt(want)) => match v.parse::<u64>() {
            Ok(n) if n == *want => CheckStatus::Ok(v.clone()),
            _ => CheckStatus::WrongValue(v.clone()),
        },
        (Some(v), _) => CheckStatus::WrongValue(v.clone()),
    };
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: expected.into(),
        status,
    }
}

// ── JSON/JSONC config ────────────────────────────────────────────────

/// Read a top-level string value from a simple JSON/JSONC file.
pub fn read_json_string_value(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let needle = format!("\"{}\"", key);
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if !trimmed.starts_with(&needle) {
            continue;
        }
        let after = trimmed[needle.len()..].trim();
        let after = after.strip_prefix(':')?;
        let after = after.trim().trim_end_matches(',');
        let val = after.trim().trim_matches('"');
        return Some(val.to_string());
    }
    None
}

/// A JSONC policy value that is either a scalar string or an object field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum JsonPolicyValue {
    /// The policy uses its short scalar form.
    Scalar(String),
    /// The policy uses an object form. `None` means the expected field is absent.
    ObjectField(Option<String>),
}

/// Read a JSONC policy that accepts either a string value or an object with a
/// string field. This deliberately keeps comments and trailing commas outside
/// the value model so callers can inspect Deno-style configuration without
/// normalizing the source file.
#[allow(dead_code)]
pub fn read_jsonc_scalar_or_object(
    path: &Path,
    key: &str,
    object_field: &str,
) -> Option<JsonPolicyValue> {
    let content = fs::read_to_string(path).ok()?;
    let key_start = find_jsonc_key(&content, key)?;
    let after_key = skip_jsonc_space_and_comments(key_start, 0)?;
    let value_start = after_key.strip_prefix(':')?;
    let value_start = skip_jsonc_space_and_comments(value_start, 0)?;

    if value_start.starts_with('"') {
        return parse_json_string(value_start).map(JsonPolicyValue::Scalar);
    }
    if !value_start.starts_with('{') {
        return None;
    }
    let object = jsonc_object(value_start)?;
    Some(JsonPolicyValue::ObjectField(
        find_jsonc_member(object, object_field).and_then(parse_json_string),
    ))
}

#[allow(dead_code)]
fn find_jsonc_key<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let mut offset = 0usize;
    let mut depth = 0usize;
    while offset < content.len() {
        let rest = skip_jsonc_space_and_comments(content, offset)?;
        offset = content.len().saturating_sub(rest.len());
        if rest.starts_with('"') {
            let string_len = json_string_len(rest)?;
            if depth == 1 && parse_json_string(rest.get(..string_len)?)? == key {
                let after_key = rest.get(string_len..)?;
                if skip_jsonc_space_and_comments(after_key, 0)?.starts_with(':') {
                    return Some(after_key);
                }
            }
            offset += string_len;
        } else {
            let ch = rest.chars().next()?;
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.checked_sub(1)?,
                _ => {}
            }
            offset += ch.len_utf8();
        }
    }
    None
}

#[allow(dead_code)]
fn find_jsonc_member<'a>(object: &'a str, key: &str) -> Option<&'a str> {
    find_jsonc_key(object, key).and_then(|after_key| {
        let after_key = skip_jsonc_space_and_comments(after_key, 0)?;
        skip_jsonc_space_and_comments(after_key.strip_prefix(':')?, 0)
    })
}

#[allow(dead_code)]
fn skip_jsonc_space_and_comments(content: &str, mut offset: usize) -> Option<&str> {
    loop {
        let rest = content.get(offset..)?;
        let trimmed = rest.trim_start();
        offset += rest.len().saturating_sub(trimmed.len());
        if let Some(after_comment) = trimmed.strip_prefix("//") {
            let newline = after_comment.find('\n')?;
            offset += 2 + newline + 1;
            continue;
        }
        if let Some(after_comment) = trimmed.strip_prefix("/*") {
            let end = after_comment.find("*/")?;
            offset += 2 + end + 2;
            continue;
        }
        return Some(trimmed);
    }
}

#[allow(dead_code)]
fn parse_json_string(content: &str) -> Option<String> {
    let mut chars = content.strip_prefix('"')?.chars();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(result),
            '\\' => match chars.next()? {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                '/' => result.push('/'),
                'b' => result.push('\u{08}'),
                'f' => result.push('\u{0c}'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                'u' => {
                    let digits: String = chars.by_ref().take(4).collect();
                    result.push(char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?);
                }
                _ => return None,
            },
            other => result.push(other),
        }
    }
    None
}

#[allow(dead_code)]
fn json_string_len(content: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in content.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(index + ch.len_utf8());
        }
    }
    None
}

#[allow(dead_code)]
fn jsonc_object(content: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut offset = 0usize;
    while offset < content.len() {
        let rest = content.get(offset..)?;
        if !in_string && rest.starts_with("//") {
            offset += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if !in_string && rest.starts_with("/*") {
            offset += 2 + rest.strip_prefix("/*")?.find("*/")? + 2;
            continue;
        }
        let ch = rest.chars().next()?;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            offset += ch.len_utf8();
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return content.get(..offset + ch.len_utf8());
                }
            }
            _ => {}
        }
        offset += ch.len_utf8();
    }
    None
}

// ── Dependabot YAML ──────────────────────────────────────────────────

/// A parsed update entry from a dependabot.yml file.
#[derive(Debug, Clone)]
pub struct DependabotEntry {
    pub ecosystem: String,
    pub directory: String,
    pub cooldown_default_days: Option<u64>,
}

/// Parse all `updates` entries from a dependabot.yml file.
pub fn read_dependabot_entries(path: &Path) -> Vec<DependabotEntry> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut entries = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("- package-ecosystem:")
            || trimmed.starts_with("- package-ecosystem :")
        {
            let ecosystem = trimmed
                .split_once(':')
                .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_string())
                .unwrap_or_default();
            let entry_indent = lines[i].len() - lines[i].trim_start().len();
            let prop_indent = entry_indent + 2;

            let mut directory = String::from("/");
            let mut cooldown_days: Option<u64> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let line = lines[j];
                let line_trimmed = line.trim();
                if line_trimmed.is_empty() || line_trimmed.starts_with('#') {
                    j += 1;
                    continue;
                }
                let line_indent = line.len() - line.trim_start().len();
                if line_indent <= entry_indent && line_trimmed.starts_with('-') {
                    break;
                }
                if line_indent == 0
                    && !line_trimmed.starts_with('-')
                    && !line_trimmed.starts_with('#')
                {
                    break;
                }
                if line_trimmed.starts_with("directory:") && line_indent == prop_indent {
                    if let Some((_, v)) = line_trimmed.split_once(':') {
                        directory = v.trim().trim_matches('"').trim_matches('\'').to_string();
                    }
                }
                if line_trimmed == "cooldown:" && line_indent >= prop_indent {
                    let cooldown_indent = line_indent;
                    let mut k = j + 1;
                    while k < lines.len() {
                        let cl = lines[k];
                        let cl_trimmed = cl.trim();
                        if cl_trimmed.is_empty() || cl_trimmed.starts_with('#') {
                            k += 1;
                            continue;
                        }
                        let cl_indent = cl.len() - cl.trim_start().len();
                        if cl_indent <= cooldown_indent {
                            break;
                        }
                        if cl_trimmed.starts_with("default-days:") {
                            if let Some(val) = cl_trimmed.split_once(':').map(|(_, v)| v.trim()) {
                                cooldown_days = val.parse().ok();
                            }
                        }
                        k += 1;
                    }
                }
                j += 1;
            }
            entries.push(DependabotEntry {
                ecosystem,
                directory,
                cooldown_default_days: cooldown_days,
            });
            i = j;
        } else {
            i += 1;
        }
    }
    entries
}
