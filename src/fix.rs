// Apply fixes to package manager config files.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::manager::{self, ManagerKind, Recommendation};

// ── Backup / Restore ─────────────────────────────────────────────────

/// Directory where backups are stored.
fn backup_dir() -> PathBuf {
    manager::home_dir().join(".depsguard/backups")
}

/// Convert a config path to a backup filename (replace / with __ ).
fn backup_name(path: &Path) -> String {
    path.to_string_lossy().replace('/', "__").replace('\\', "__")
}

/// Backup a config file before modifying it. Only backs up once per path per session.
pub fn backup_file(path: &Path, backed_up: &mut HashSet<PathBuf>) -> io::Result<()> {
    if backed_up.contains(path) {
        return Ok(()); // already backed up this session
    }
    if !path.exists() {
        backed_up.insert(path.to_path_buf());
        return Ok(()); // nothing to back up
    }
    let dir = backup_dir();
    fs::create_dir_all(&dir)?;
    let dest = dir.join(backup_name(path));
    fs::copy(path, &dest)?;
    backed_up.insert(path.to_path_buf());
    Ok(())
}

/// List all backed-up files with their original paths.
pub fn list_backups() -> Vec<(PathBuf, PathBuf)> {
    let dir = backup_dir();
    let mut results = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let backup_path = entry.path();
        if backup_path.is_file() {
            let name = backup_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            // Reconstruct original path from __ -separated name
            let original = name.replacen("__", "/", name.matches("__").count());
            results.push((PathBuf::from(original), backup_path));
        }
    }
    results.sort();
    results
}

/// Restore all backed-up files to their original locations.
pub fn restore_all() -> Vec<(PathBuf, Result<(), String>)> {
    let backups = list_backups();
    let mut results = Vec::new();
    for (original, backup) in &backups {
        let res = fs::copy(backup, original)
            .map(|_| ())
            .map_err(|e| e.to_string());
        results.push((original.clone(), res));
    }
    // Clean up backup dir after successful restore
    if results.iter().all(|(_, r)| r.is_ok()) {
        let dir = backup_dir();
        let _ = fs::remove_dir_all(&dir);
    }
    results
}

/// Apply a single recommendation fix to the config file at `path`.
pub fn apply_fix(kind: ManagerKind, path: &Path, rec: &Recommendation) -> io::Result<String> {
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => apply_flat_fix(path, &rec.key, &rec.expected),
        ManagerKind::Bun => apply_toml_fix(path, &rec.key, &rec.expected, false),
        ManagerKind::Uv => apply_toml_fix(path, &rec.key, &rec.expected, true),
        ManagerKind::PnpmWorkspace => {
            let quote = matches!(rec.key.as_str(), "trustPolicy");
            apply_yaml_fix(path, &rec.key, &rec.expected, quote)
        }
    }
}

/// Set `key=value` in a flat (.npmrc-style) config file.
fn apply_flat_fix(path: &Path, key: &str, value: &str) -> io::Result<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let line = format!("{key}={value}");
    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|l| {
            let trimmed = l.trim();
            if trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with('=') {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();

    if !found {
        lines.push(line.clone());
    }

    // Ensure trailing newline
    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &output)?;
    Ok(line)
}

/// Set a key in a simple TOML file. Supports `section.key` notation.
/// If `quote` is true, wraps value in double quotes.
fn apply_toml_fix(path: &Path, dotted_key: &str, value: &str, quote: bool) -> io::Result<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let parts: Vec<&str> = dotted_key.splitn(2, '.').collect();
    let (section, key) = if parts.len() == 2 {
        (Some(parts[0]), parts[1])
    } else {
        (None, parts[0])
    };

    let formatted_val = if quote {
        format!("\"{value}\"")
    } else {
        value.to_string()
    };
    let target_line = format!("{key} = {formatted_val}");

    let mut lines: Vec<String> = Vec::new();
    let mut current_section: Option<String> = None;
    let mut found = false;
    let mut section_exists = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed[1..trimmed.len() - 1].trim().to_string());
            if section.is_some_and(|s| current_section.as_deref() == Some(s)) {
                section_exists = true;
            }
        }

        let in_target = match (&current_section, section) {
            (Some(cur), Some(sec)) => cur == sec,
            (None, None) => true,
            _ => false,
        };

        if in_target && trimmed.starts_with(key) {
            let rest = trimmed[key.len()..].trim_start();
            if rest.starts_with('=') {
                lines.push(target_line.clone());
                found = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }

    if !found {
        if let Some(sec) = section {
            if !section_exists {
                if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                    lines.push(String::new());
                }
                lines.push(format!("[{sec}]"));
            }
            // Find the section and append after it
            if section_exists {
                let mut inserted = false;
                let mut new_lines = Vec::new();
                for line in &lines {
                    new_lines.push(line.clone());
                    if !inserted {
                        let trimmed = line.trim();
                        // Match section header by trimming interior whitespace
                        if trimmed.starts_with('[') && trimmed.ends_with(']') {
                            let inner = trimmed[1..trimmed.len() - 1].trim();
                            if inner == sec {
                                new_lines.push(target_line.clone());
                                inserted = true;
                            }
                        }
                    }
                }
                lines = new_lines;
            } else {
                lines.push(target_line.clone());
            }
        } else {
            // Top-level key, prepend before any sections
            let first_section = lines.iter().position(|l| l.trim().starts_with('['));
            match first_section {
                Some(idx) => lines.insert(idx, target_line.clone()),
                None => lines.push(target_line.clone()),
            }
        }
    }

    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &output)?;
    Ok(target_line)
}

/// Set a top-level key in a YAML file (pnpm-workspace.yaml style).
/// If `quote` is true, wraps value in double quotes.
fn apply_yaml_fix(path: &Path, key: &str, value: &str, quote: bool) -> io::Result<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let formatted_val = if quote {
        format!("\"{value}\"")
    } else {
        value.to_string()
    };
    let target_line = format!("{key}: {formatted_val}");

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Only match top-level keys (not indented)
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some((k, _)) = trimmed.split_once(':') {
            if k.trim() == key {
                *line = target_line.clone();
                found = true;
                break;
            }
        }
    }

    if !found {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(target_line.clone());
    }

    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &output)?;
    Ok(target_line)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(content: &str) -> TmpFile {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("depsguard_fix_test_{id}_{}", std::process::id()));
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        TmpFile(path)
    }

    struct TmpFile(std::path::PathBuf);
    impl TmpFile {
        fn path(&self) -> &Path {
            &self.0
        }
        fn read(&self) -> String {
            fs::read_to_string(&self.0).unwrap()
        }
    }
    impl Drop for TmpFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn flat_fix_adds_missing_key() {
        let f = tmp_file("");
        apply_flat_fix(f.path(), "ignore-scripts", "true").unwrap();
        assert!(f.read().contains("ignore-scripts=true"));
    }

    #[test]
    fn flat_fix_updates_existing_key() {
        let f = tmp_file("ignore-scripts=false\n");
        apply_flat_fix(f.path(), "ignore-scripts", "true").unwrap();
        let content = f.read();
        assert!(content.contains("ignore-scripts=true"));
        assert!(!content.contains("false"));
    }

    #[test]
    fn flat_fix_preserves_other_keys() {
        let f = tmp_file("registry=https://registry.npmjs.org\nignore-scripts=false\n");
        apply_flat_fix(f.path(), "ignore-scripts", "true").unwrap();
        let content = f.read();
        assert!(content.contains("registry=https://registry.npmjs.org"));
        assert!(content.contains("ignore-scripts=true"));
    }

    #[test]
    fn flat_fix_creates_parent_dirs() {
        let dir = std::env::temp_dir().join(format!("depsguard_nested_{}", std::process::id()));
        let path = dir.join("sub/config");
        apply_flat_fix(&path, "key", "val").unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("key=val"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn toml_fix_adds_section_and_key() {
        let f = tmp_file("");
        apply_toml_fix(f.path(), "install.minimumReleaseAge", "604800", false).unwrap();
        let content = f.read();
        assert!(content.contains("[install]"));
        assert!(content.contains("minimumReleaseAge = 604800"));
    }

    #[test]
    fn toml_fix_updates_existing_key_in_section() {
        let f = tmp_file("[install]\nminimumReleaseAge = 100\n");
        apply_toml_fix(f.path(), "install.minimumReleaseAge", "604800", false).unwrap();
        let content = f.read();
        assert!(content.contains("minimumReleaseAge = 604800"));
        assert!(!content.contains("100"));
    }

    #[test]
    fn toml_fix_adds_key_to_existing_section() {
        let f = tmp_file("[install]\nother = 1\n");
        apply_toml_fix(f.path(), "install.minimumReleaseAge", "604800", false).unwrap();
        let content = f.read();
        assert!(content.contains("[install]"));
        assert!(content.contains("minimumReleaseAge = 604800"));
    }

    #[test]
    fn toml_fix_quoted_value() {
        let f = tmp_file("");
        apply_toml_fix(f.path(), "exclude-newer", "2024-01-01", true).unwrap();
        let content = f.read();
        assert!(content.contains("exclude-newer = \"2024-01-01\""));
    }

    #[test]
    fn toml_fix_top_level_key() {
        let f = tmp_file("[other]\nfoo = 1\n");
        apply_toml_fix(f.path(), "exclude-newer", "2024-01-01", true).unwrap();
        let content = f.read();
        assert!(content.contains("exclude-newer = \"2024-01-01\""));
        assert!(content.contains("[other]"));
    }

    #[test]
    fn toml_fix_updates_top_level_key() {
        let f = tmp_file("exclude-newer = \"2020-01-01\"\n");
        apply_toml_fix(f.path(), "exclude-newer", "2024-06-01", true).unwrap();
        let content = f.read();
        assert!(content.contains("exclude-newer = \"2024-06-01\""));
        assert!(!content.contains("2020-01-01"));
    }

    #[test]
    fn toml_fix_creates_parent_dirs() {
        let dir = std::env::temp_dir().join(format!("depsguard_toml_{}", std::process::id()));
        let path = dir.join("sub/config.toml");
        apply_toml_fix(&path, "key", "val", false).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("key = val"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_fix_npm() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "ignore-scripts".into(),
            description: "test".into(),
            expected: "true".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Npm, f.path(), &rec).unwrap();
        assert!(f.read().contains("ignore-scripts=true"));
    }

    #[test]
    fn apply_fix_pnpm() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "minimum-release-age".into(),
            description: "test".into(),
            expected: "10080".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Pnpm, f.path(), &rec).unwrap();
        assert!(f.read().contains("minimum-release-age=10080"));
    }

    #[test]
    fn apply_fix_bun() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "install.minimumReleaseAge".into(),
            description: "test".into(),
            expected: "604800".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Bun, f.path(), &rec).unwrap();
        let content = f.read();
        assert!(content.contains("minimumReleaseAge = 604800"));
    }

    #[test]
    fn apply_fix_uv() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "exclude-newer".into(),
            description: "test".into(),
            expected: "7 days".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Uv, f.path(), &rec).unwrap();
        assert!(f.read().contains("exclude-newer = \"7 days\""));
    }

    // ── YAML fix tests ──────────────────────────────────────────────

    #[test]
    fn yaml_fix_adds_missing_key() {
        let f = tmp_file("packages:\n  - 'src/*'\n");
        apply_yaml_fix(f.path(), "strictDepBuilds", "true", false).unwrap();
        let content = f.read();
        assert!(content.contains("strictDepBuilds: true"));
        assert!(content.contains("packages:")); // preserves existing
    }

    #[test]
    fn yaml_fix_updates_existing_key() {
        let f = tmp_file("minimumReleaseAge: 100\n");
        apply_yaml_fix(f.path(), "minimumReleaseAge", "4320", false).unwrap();
        let content = f.read();
        assert!(content.contains("minimumReleaseAge: 4320"));
        assert!(!content.contains("100"));
    }

    #[test]
    fn yaml_fix_quoted_value() {
        let f = tmp_file("");
        apply_yaml_fix(f.path(), "trustPolicy", "no-downgrade", true).unwrap();
        let content = f.read();
        assert!(content.contains("trustPolicy: \"no-downgrade\""));
    }

    #[test]
    fn yaml_fix_preserves_other_keys() {
        let f = tmp_file("packages:\n  - 'src/*'\nblockExoticSubdeps: true\n");
        apply_yaml_fix(f.path(), "minimumReleaseAge", "4320", false).unwrap();
        let content = f.read();
        assert!(content.contains("blockExoticSubdeps: true"));
        assert!(content.contains("minimumReleaseAge: 4320"));
        assert!(content.contains("packages:"));
    }

    #[test]
    fn apply_fix_pnpm_workspace() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "trustPolicy".into(),
            description: "test".into(),
            expected: "no-downgrade".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::PnpmWorkspace, f.path(), &rec).unwrap();
        assert!(f.read().contains("trustPolicy: \"no-downgrade\""));
    }

    #[test]
    fn apply_fix_pnpm_workspace_unquoted() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "strictDepBuilds".into(),
            description: "test".into(),
            expected: "true".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::PnpmWorkspace, f.path(), &rec).unwrap();
        assert!(f.read().contains("strictDepBuilds: true"));
    }
}
