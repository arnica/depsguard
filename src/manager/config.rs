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
        Some(v) if v == expected => CheckStatus::Ok,
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

/// Check a flat config key as an integer `>= min`.
pub fn check_flat_min_int(
    path: &Path,
    cfg: &HashMap<String, String>,
    key: &str,
    min: u64,
    desc: &str,
) -> Recommendation {
    let status = match cfg.get(key) {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n >= min => CheckStatus::Ok,
            _ => CheckStatus::WrongValue(v.clone()),
        },
        None => missing_status_for_path(path),
    };
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: min.to_string(),
        status,
    }
}

// ── TOML config ──────────────────────────────────────────────────────

/// Read a TOML value from a simple TOML file. Supports `key = value` and `[section]`.
/// For nested keys use "section.key" notation.
pub fn read_toml_value(path: &Path, dotted_key: &str) -> Option<String> {
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
    MinInt(u64),
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
        (Some(v), YamlCheck::Exact) if v == expected => CheckStatus::Ok,
        (Some(v), YamlCheck::MinInt(min)) => match v.parse::<u64>() {
            Ok(n) if n >= *min => CheckStatus::Ok,
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
