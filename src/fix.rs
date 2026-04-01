// Apply fixes to package manager config files.

use std::fs;
use std::io;
use std::path::Path;

use crate::manager::{ManagerKind, Recommendation};

/// Apply a single recommendation fix to the config file at `path`.
pub fn apply_fix(kind: ManagerKind, path: &Path, rec: &Recommendation) -> io::Result<String> {
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => apply_flat_fix(path, &rec.key, &rec.expected),
        ManagerKind::Bun => apply_toml_fix(path, &rec.key, &rec.expected, false),
        ManagerKind::Uv => apply_toml_fix(path, &rec.key, &rec.expected, true),
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
            expected: "2024-01-01".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Uv, f.path(), &rec).unwrap();
        assert!(f.read().contains("exclude-newer = \"2024-01-01\""));
    }
}
