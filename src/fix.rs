// Apply fixes to package manager config files.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::manager::{self, ManagerKind, Recommendation};

// ── Backup / Restore ─────────────────────────────────────────────────

/// Generate an ISO-8601–style timestamp from SystemTime, without chrono.
/// Format: `YYYY-MM-DDTHH-MM-SS` (hyphens in time part for filename safety).
fn iso_timestamp() -> String {
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let time = secs % 86400;
    let h = time / 3600;
    let m = (time % 3600) / 60;
    let s = time % 60;

    let (y, mon, d) = manager::days_to_ymd(days);
    format!("{y:04}-{mon:02}-{d:02}T{h:02}-{m:02}-{s:02}")
}

/// Return the depsguard data directory (`~/.depsguard/`).
pub fn data_dir() -> PathBuf {
    manager::home_dir().join(".depsguard")
}

/// Return the backup storage directory (`~/.depsguard/backups/`).
fn backup_dir() -> PathBuf {
    data_dir().join("backups")
}

/// Encode a file path into a safe filename for backup storage.
/// Uses percent-encoding: `/` → `%2F`, `\` → `%5C`, `%` → `%25`.
fn encode_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            '\\' => out.push_str("%5C"),
            ':' => out.push_str("%3A"),
            _ => out.push(c),
        }
    }
    out
}

/// Decode a backup filename back to the original path.
/// Iterates by `char` (not byte) so multi-byte UTF-8 sequences survive intact.
fn decode_path(encoded: &str) -> PathBuf {
    let mut out = String::with_capacity(encoded.len());
    let mut chars = encoded.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hi = chars.next();
            let lo = chars.next();
            match (hi, lo) {
                (Some(h), Some(l)) if h.is_ascii() && l.is_ascii() => {
                    match (hex_val(h as u8), hex_val(l as u8)) {
                        (Some(hv), Some(lv)) => out.push((hv << 4 | lv) as char),
                        _ => {
                            out.push(c);
                            out.push(h);
                            out.push(l);
                        }
                    }
                }
                _ => {
                    out.push(c);
                    if let Some(h) = hi {
                        out.push(h);
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    PathBuf::from(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Backup a config file before modifying it. Only backs up once per path per session.
/// Backups are stored in `~/.depsguard/backups/` with the original path encoded in the filename.
pub fn backup_file(path: &Path, backed_up: &mut HashSet<PathBuf>) -> io::Result<()> {
    if backed_up.contains(path) {
        return Ok(());
    }
    if !path.exists() {
        backed_up.insert(path.to_path_buf());
        return Ok(());
    }
    let dir = backup_dir();
    fs::create_dir_all(&dir)?;
    let ts = iso_timestamp();
    let encoded = encode_path(path);
    let name = format!("{encoded}.{ts}.bak");
    let dest = dir.join(name);
    fs::copy(path, &dest)?;
    backed_up.insert(path.to_path_buf());
    Ok(())
}

/// List all backup files in `~/.depsguard/backups/`.
/// Returns `(backups, stale_count)` where backups is sorted newest-first,
/// and `stale_count` is the number of unrecognized files in the directory.
pub fn list_backups() -> (Vec<(PathBuf, PathBuf)>, usize) {
    let dir = backup_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return (Vec::new(), 0),
    };
    let mut results = Vec::new();
    let mut stale = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with(".bak") {
            stale += 1;
            continue;
        }
        let base = &name[..name.len() - 4];
        if let Some(dot_pos) = base.rfind('.') {
            let ts = &base[dot_pos + 1..];
            let encoded = &base[..dot_pos];
            if !ts.is_empty()
                && ts
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == 'T' || c == '-')
            {
                let original = decode_path(encoded);
                // Agentic Rule (ARNIE_PATH_BOUNDARY_CHECKING): reject paths outside home directory
                if original.starts_with(manager::home_dir()) {
                    results.push((original, p));
                } else {
                    stale += 1;
                }
            } else {
                stale += 1;
            }
        } else {
            stale += 1;
        }
    }
    results.sort_by(|a, b| {
        let ts = |p: &Path| -> String {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let base = name.strip_suffix(".bak").unwrap_or(&name);
            base.rsplit('.').next().unwrap_or("").to_string()
        };
        ts(&b.1).cmp(&ts(&a.1)).then(a.0.cmp(&b.0))
    });
    (results, stale)
}

/// Restore a single backup file to its original location.
pub fn restore_backup(backup: &Path, original: &Path) -> io::Result<()> {
    if let Some(parent) = original.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::copy(backup, original).map(|_| ())
}

/// Read a config file, returning an empty string if the file does not exist.
/// Propagates other errors (e.g. permission denied) instead of silently ignoring them.
fn read_or_create(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Apply a single recommendation fix to the config file at `path`.
pub fn apply_fix(kind: ManagerKind, path: &Path, rec: &Recommendation) -> io::Result<String> {
    match kind {
        ManagerKind::Npm => {
            let user_path = manager::config_path(ManagerKind::Npm);
            if path == user_path {
                apply_npm_config_set(&rec.key, &rec.expected)
            } else {
                apply_flat_fix(path, &rec.key, &rec.expected)
            }
        }
        ManagerKind::Pnpm => apply_flat_fix(path, &rec.key, &rec.expected),
        ManagerKind::Bun => apply_toml_fix(path, &rec.key, &rec.expected, false),
        ManagerKind::Uv => apply_toml_fix(path, &rec.key, &rec.expected, true),
        ManagerKind::PnpmWorkspace => {
            let quote = matches!(rec.key.as_str(), "trustPolicy");
            apply_yaml_fix(path, &rec.key, &rec.expected, quote)
        }
        ManagerKind::Yarn => apply_yaml_fix(path, &rec.key, &rec.expected, true),
        ManagerKind::Renovate => apply_json_fix(path, &rec.key, &rec.expected),
        ManagerKind::Dependabot => apply_dependabot_fix(path, &rec.key, &rec.expected),
    }
}

/// Set `key=value` in a flat (.npmrc-style) config file.
fn apply_flat_fix(path: &Path, key: &str, value: &str) -> io::Result<String> {
    let content = read_or_create(path)?;
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
    atomic_write(path, &output)?;
    Ok(line)
}

/// Set a key in a simple TOML file. Supports `section.key` notation.
/// If `quote` is true, wraps value in double quotes.
fn apply_toml_fix(path: &Path, dotted_key: &str, value: &str, quote: bool) -> io::Result<String> {
    let content = read_or_create(path)?;
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
    let mut section_header_idx: Option<usize> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(inner.trim().to_string());
            if section.is_some_and(|s| current_section.as_deref() == Some(s)) {
                section_header_idx = Some(lines.len());
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
            if let Some(idx) = section_header_idx {
                lines.insert(idx + 1, target_line.clone());
            } else {
                if lines.last().is_some_and(|l| !l.is_empty()) {
                    lines.push(String::new());
                }
                lines.push(format!("[{sec}]"));
                lines.push(target_line.clone());
            }
        } else {
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
    atomic_write(path, &output)?;
    Ok(target_line)
}

/// Set a top-level key in a YAML file (pnpm-workspace.yaml style).
/// If `quote` is true, wraps value in double quotes.
fn apply_yaml_fix(path: &Path, key: &str, value: &str, quote: bool) -> io::Result<String> {
    let content = read_or_create(path)?;
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
        if lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
        lines.push(target_line.clone());
    }

    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &output)?;
    Ok(target_line)
}

/// Set a user-level npm config value via `npm config set --location=user`.
fn apply_npm_config_set(key: &str, value: &str) -> io::Result<String> {
    let setting = format!("{key}={value}");
    let result = std::process::Command::new("npm")
        .args(["config", "set", &setting, "--location=user"])
        .output();
    let output = match result {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            // Try npm.cmd on Windows
            if cfg!(target_os = "windows") {
                let win = std::process::Command::new("npm.cmd")
                    .args(["config", "set", &setting, "--location=user"])
                    .output();
                match win {
                    Ok(o2) if o2.status.success() => o2,
                    _ => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        return Err(io::Error::other(format!("npm config set failed: {stderr}")));
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&o.stderr);
                return Err(io::Error::other(format!("npm config set failed: {stderr}")));
            }
        }
        Err(e) => {
            if cfg!(target_os = "windows") {
                let win = std::process::Command::new("npm.cmd")
                    .args(["config", "set", &setting, "--location=user"])
                    .output()
                    .map_err(|_| e)?;
                if !win.status.success() {
                    let stderr = String::from_utf8_lossy(&win.stderr);
                    return Err(io::Error::other(format!("npm config set failed: {stderr}")));
                }
                win
            } else {
                return Err(e);
            }
        }
    };
    let _ = output;
    Ok(setting)
}

/// Set a top-level key in a JSON/JSONC config file (e.g. renovate.json).
fn apply_json_fix(path: &Path, key: &str, value: &str) -> io::Result<String> {
    let content = read_or_create(path)?;
    let needle = format!("\"{}\"", key);
    let target_value = format!("\"{}\"", value);
    let target_line = format!("  \"{key}\": {target_value}");

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with(&needle) {
            let after = trimmed[needle.len()..].trim();
            if after.starts_with(':') {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                let needs_comma = trimmed.ends_with(',');
                let comma = if needs_comma { "," } else { "" };
                *line = format!("{indent}\"{key}\": {target_value}{comma}");
                found = true;
                break;
            }
        }
    }

    if !found {
        // Find closing `}` and insert before it
        let close_pos = lines.iter().rposition(|l| l.trim() == "}");
        match close_pos {
            Some(close_idx) => {
                // Find the last non-empty, non-comment line before `}` and ensure it has a comma
                for i in (0..close_idx).rev() {
                    let trimmed = lines[i].trim();
                    if trimmed.is_empty() || trimmed.starts_with("//") {
                        continue;
                    }
                    if !trimmed.ends_with(',') && !trimmed.ends_with('{') {
                        lines[i] = format!("{},", lines[i].trim_end());
                    }
                    break;
                }
                lines.insert(close_idx, format!("  \"{key}\": {target_value}"));
            }
            None => {
                lines.clear();
                lines.push("{".into());
                lines.push(format!("  \"{key}\": {target_value}"));
                lines.push("}".into());
            }
        }
    }

    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &output)?;
    Ok(target_line)
}

/// Fix `cooldown.default-days` in a dependabot.yml file.
/// Updates the value if it exists, or inserts a `cooldown:` block if missing.
fn apply_dependabot_fix(path: &Path, _key: &str, value: &str) -> io::Result<String> {
    let content = read_or_create(path)?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let target_line = format!("default-days: {value}");

    // First pass: update existing `default-days:` lines only within `cooldown:` blocks
    let mut in_cooldown = false;
    let mut cooldown_indent = 0usize;
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if trimmed == "cooldown:" {
            in_cooldown = true;
            cooldown_indent = indent;
            continue;
        }
        if in_cooldown {
            if indent <= cooldown_indent {
                in_cooldown = false;
            } else if trimmed.starts_with("default-days:") {
                let indent_str: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                *line = format!("{indent_str}default-days: {value}");
            }
        }
    }

    // Second pass: find update entries that lack cooldown and insert it
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim().to_string();
        if trimmed.starts_with("- package-ecosystem:") {
            let entry_indent = lines[i].len() - lines[i].trim_start().len();
            let prop_indent = entry_indent + 2;
            let indent_str: String = " ".repeat(prop_indent);
            let cooldown_indent: String = " ".repeat(prop_indent + 2);

            // Check if this entry already has a cooldown block
            let mut has_cooldown = false;
            let mut insert_before = i + 1;
            let mut j = i + 1;
            while j < lines.len() {
                let lt = lines[j].trim();
                if lt.is_empty() || lt.starts_with('#') {
                    j += 1;
                    continue;
                }
                let li = lines[j].len() - lines[j].trim_start().len();
                if li <= entry_indent && lt.starts_with('-') {
                    break;
                }
                if li == 0 && !lt.starts_with('-') && !lt.starts_with('#') {
                    break;
                }
                if lt == "cooldown:" {
                    has_cooldown = true;
                }
                insert_before = j + 1;
                j += 1;
            }

            if !has_cooldown {
                lines.insert(
                    insert_before,
                    format!("{cooldown_indent}default-days: {value}"),
                );
                lines.insert(insert_before, format!("{indent_str}cooldown:"));
                i = insert_before + 2;
                continue;
            }
        }
        i += 1;
    }

    let output = lines.join("\n") + "\n";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &output)?;
    Ok(target_line)
}

/// Write content to a temp file then rename into place (atomic on same filesystem).
fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or(Path::new("."));
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".depsguard-tmp-{}-{n}", std::process::id()));
    fs::write(&tmp, content)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        #[cfg(windows)]
        Err(e)
            if e.kind() == io::ErrorKind::PermissionDenied
                || e.raw_os_error() == Some(183) /* ERROR_ALREADY_EXISTS */ =>
        {
            // Windows rename fails if dest exists; remove then retry
            if let Err(re) = fs::remove_file(path) {
                if re.kind() != io::ErrorKind::NotFound {
                    let _ = fs::remove_file(&tmp);
                    return Err(e);
                }
            }
            fs::rename(&tmp, path).map_err(|e2| {
                let _ = fs::remove_file(&tmp);
                e2
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(content: &str) -> TmpFile {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "depsguard_fix_test_{id}_{}_{n}",
            std::process::id()
        ));
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
            key: "ignore-scripts".into(),
            description: "test".into(),
            expected: "true".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Pnpm, f.path(), &rec).unwrap();
        assert!(f.read().contains("ignore-scripts=true"));
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

    // ── Yarn fix tests ──────────────────────────────────────────────

    #[test]
    fn apply_fix_yarn() {
        let f = tmp_file("");
        let rec = Recommendation {
            key: "npmMinimalAgeGate".into(),
            description: "test".into(),
            expected: "7d".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Yarn, f.path(), &rec).unwrap();
        assert!(f.read().contains("npmMinimalAgeGate: \"7d\""));
    }

    // ── JSON fix tests ──────────────────────────────────────────────

    #[test]
    fn json_fix_adds_missing_key() {
        let f = tmp_file("{\n  \"extends\": [\"config:recommended\"]\n}\n");
        apply_json_fix(f.path(), "minimumReleaseAge", "7 days").unwrap();
        let content = f.read();
        assert!(content.contains("\"minimumReleaseAge\": \"7 days\""));
        // Previous last line should have a comma added
        assert!(
            content.contains("[\"config:recommended\"],"),
            "preceding line should have comma: {content}"
        );
    }

    #[test]
    fn json_fix_updates_existing_key() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"3 days\"\n}\n");
        apply_json_fix(f.path(), "minimumReleaseAge", "7 days").unwrap();
        let content = f.read();
        assert!(content.contains("\"minimumReleaseAge\": \"7 days\""));
        assert!(!content.contains("3 days"));
    }

    #[test]
    fn json_fix_creates_empty_file() {
        let f = tmp_file("");
        apply_json_fix(f.path(), "minimumReleaseAge", "7 days").unwrap();
        let content = f.read();
        assert!(content.contains("{"));
        assert!(content.contains("\"minimumReleaseAge\": \"7 days\""));
        assert!(content.contains("}"));
    }

    #[test]
    fn apply_fix_renovate() {
        let f = tmp_file("{\n  \"extends\": [\"config:recommended\"]\n}\n");
        let rec = Recommendation {
            key: "minimumReleaseAge".into(),
            description: "test".into(),
            expected: "7 days".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Renovate, f.path(), &rec).unwrap();
        assert!(f.read().contains("\"minimumReleaseAge\": \"7 days\""));
    }

    // ── Dependabot fix tests ────────────────────────────────────────

    #[test]
    fn dependabot_fix_updates_existing_default_days() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    cooldown:\n      default-days: 2\n",
        );
        apply_dependabot_fix(f.path(), "cooldown.default-days", "7").unwrap();
        let content = f.read();
        assert!(content.contains("default-days: 7"));
        assert!(!content.contains("default-days: 2"));
    }

    #[test]
    fn dependabot_fix_adds_cooldown_block() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n",
        );
        apply_dependabot_fix(f.path(), "cooldown.default-days", "7").unwrap();
        let content = f.read();
        assert!(content.contains("cooldown:"));
        assert!(content.contains("default-days: 7"));
    }

    #[test]
    fn dependabot_fix_mixed_entries() {
        let f = tmp_file(concat!(
            "version: 2\nupdates:\n",
            "  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n",
            "    cooldown:\n      default-days: 7\n",
            "  - package-ecosystem: \"npm\"\n    directory: \"/docs\"\n",
            "    schedule:\n      interval: \"weekly\"\n",
        ));
        apply_dependabot_fix(f.path(), "cooldown.default-days (npm)", "7").unwrap();
        let content = f.read();
        assert_eq!(
            content.matches("cooldown:").count(),
            2,
            "both entries should have cooldown blocks: {content}"
        );
        assert_eq!(
            content.matches("default-days: 7").count(),
            2,
            "both entries should have default-days: 7: {content}"
        );
    }

    #[test]
    fn apply_fix_dependabot() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n",
        );
        let rec = Recommendation {
            key: "cooldown.default-days".into(),
            description: "test".into(),
            expected: "7".into(),
            status: crate::manager::CheckStatus::Missing,
        };
        apply_fix(ManagerKind::Dependabot, f.path(), &rec).unwrap();
        assert!(f.read().contains("default-days: 7"));
    }

    // ── encode/decode path tests ────────────────────────────────────

    #[test]
    fn encode_decode_round_trip_unix() {
        let path = Path::new("/Users/test/.npmrc");
        let encoded = super::encode_path(path);
        assert!(!encoded.contains('/'));
        let decoded = super::decode_path(&encoded);
        assert_eq!(decoded, path);
    }

    #[test]
    fn encode_decode_round_trip_with_special_chars() {
        let path = Path::new("/home/user/my__project/.npmrc");
        let encoded = super::encode_path(path);
        let decoded = super::decode_path(&encoded);
        assert_eq!(decoded, path);
    }

    #[test]
    fn encode_decode_round_trip_with_percent() {
        let path = Path::new("/home/user/100%done/.npmrc");
        let encoded = super::encode_path(path);
        let decoded = super::decode_path(&encoded);
        assert_eq!(decoded, path);
    }

    #[test]
    fn encode_decode_round_trip_non_ascii() {
        let path = Path::new("/home/user/プロジェクト/.npmrc");
        let encoded = super::encode_path(path);
        let decoded = super::decode_path(&encoded);
        assert_eq!(decoded, path);
    }

    #[test]
    fn encode_decode_windows_path() {
        let path = Path::new("C:\\Users\\test\\.npmrc");
        let encoded = super::encode_path(path);
        assert!(!encoded.contains('\\'));
        let decoded = super::decode_path(&encoded);
        assert_eq!(decoded, path);
    }
}
