// Package manager detection, config scanning, and recommendation engine.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static SKIP_SEARCH: AtomicBool = AtomicBool::new(false);
static DELAY_DAYS_SETTING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);

use std::sync::Mutex;
static EXCLUDED_MANAGERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Enable or disable the `--no-search` flag (skip repo config file discovery).
pub fn set_skip_search(skip: bool) {
    SKIP_SEARCH.store(skip, Ordering::Relaxed);
}

/// Check whether repo config search is disabled.
pub fn skip_search_enabled() -> bool {
    SKIP_SEARCH.load(Ordering::Relaxed)
}

/// Set the minimum release age in days (default: 7).
pub fn set_delay_days(days: u64) {
    DELAY_DAYS_SETTING.store(days, Ordering::Relaxed);
}

/// Get the currently configured release delay in days.
pub fn get_delay_days() -> u64 {
    DELAY_DAYS_SETTING.load(Ordering::Relaxed)
}

/// Set the list of excluded manager names.
pub fn set_excluded_managers(names: Vec<String>) {
    *EXCLUDED_MANAGERS.lock().unwrap_or_else(|e| e.into_inner()) = names;
}

/// Check whether a manager kind is excluded by the user.
pub fn is_excluded(kind: ManagerKind) -> bool {
    let excluded = EXCLUDED_MANAGERS.lock().unwrap_or_else(|e| e.into_inner());
    if excluded.is_empty() {
        return false;
    }
    let name = kind.name();
    excluded.iter().any(|e| {
        e.eq_ignore_ascii_case(name)
            || (kind == ManagerKind::PnpmWorkspace && e.eq_ignore_ascii_case("pnpm"))
    })
}

// ── Core types ────────────────────────────────────────────────────────

/// Result of checking a single security setting against its expected value.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    /// The setting matches the expected value.
    Ok,
    /// The setting is not configured at all.
    Missing,
    /// The setting exists but has an incorrect value.
    WrongValue(String),
    /// The feature is not available (e.g. tool version too old). Not auto-fixable.
    Unsupported(String),
}

impl CheckStatus {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        matches!(self, CheckStatus::Ok)
    }

    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, CheckStatus::Unsupported(_))
    }
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "OK"),
            CheckStatus::Missing => write!(f, "Not set"),
            CheckStatus::WrongValue(v) => write!(f, "Current: {v}"),
            CheckStatus::Unsupported(v) => write!(f, "{v}"),
        }
    }
}

/// A single security recommendation for a package manager config file.
#[derive(Debug, Clone)]
pub struct Recommendation {
    pub key: String,
    pub description: String,
    pub expected: String,
    pub status: CheckStatus,
}

impl Recommendation {
    #[must_use]
    pub fn needs_fix(&self) -> bool {
        !self.status.is_ok() && !self.status.is_unsupported()
    }
}

/// Parse a semantic version string into (major, minor, patch).
pub fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim();
    let mut parts = version.splitn(3, '.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch_str = parts.next().unwrap_or("0");
    // Handle pre-release suffixes like "11.10.0-beta.1"
    let patch: u64 = patch_str
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    Some((major, minor, patch))
}

/// Check if a version is at least (min_major, min_minor).
fn version_at_least(version: &str, min_major: u64, min_minor: u64) -> bool {
    match parse_semver(version) {
        Some((major, minor, _)) => major > min_major || (major == min_major && minor >= min_minor),
        None => false,
    }
}

/// Supported package managers that DepsGuard can scan and fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagerKind {
    Npm,
    Pnpm,
    PnpmWorkspace,
    Bun,
    Uv,
    Yarn,
    Renovate,
    Dependabot,
}

impl ManagerKind {
    /// Managers scanned via user-level config (version detection + fixed path).
    pub const USER_LEVEL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::Bun,
        ManagerKind::Uv,
        ManagerKind::Yarn,
    ];

    pub const ALL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::PnpmWorkspace,
        ManagerKind::Bun,
        ManagerKind::Uv,
        ManagerKind::Yarn,
        ManagerKind::Renovate,
        ManagerKind::Dependabot,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ManagerKind::Npm => "npm",
            ManagerKind::Pnpm => "pnpm",
            ManagerKind::PnpmWorkspace => "pnpm-workspace",
            ManagerKind::Bun => "bun",
            ManagerKind::Uv => "uv",
            ManagerKind::Yarn => "yarn",
            ManagerKind::Renovate => "renovate",
            ManagerKind::Dependabot => "dependabot",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ManagerKind::Npm => "📦",
            ManagerKind::Pnpm | ManagerKind::PnpmWorkspace => "⚡",
            ManagerKind::Bun => "🥟",
            ManagerKind::Uv => "🐍",
            ManagerKind::Yarn => "🧶",
            ManagerKind::Renovate => "🔄",
            ManagerKind::Dependabot => "🤖",
        }
    }

    /// Look up a ManagerKind by its CLI name (case-insensitive).
    pub fn from_name(name: &str) -> Option<ManagerKind> {
        ManagerKind::ALL
            .iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
            .copied()
    }

    /// All valid names for use in `--exclude` (user-facing).
    pub fn valid_names() -> Vec<&'static str> {
        // Deduplicate: show "pnpm" once (covers both Pnpm and PnpmWorkspace)
        let mut names: Vec<&str> = Vec::new();
        for k in Self::ALL {
            let n = k.name();
            if n != "pnpm-workspace" && !names.contains(&n) {
                names.push(n);
            }
        }
        names
    }

    /// Whether this kind is only discovered in repos (not user-level config).
    #[cfg(test)]
    pub fn is_repo_only(self) -> bool {
        matches!(
            self,
            ManagerKind::PnpmWorkspace | ManagerKind::Renovate | ManagerKind::Dependabot
        )
    }
}

/// A detected package manager with its version, config location, and security check results.
#[derive(Debug, Clone)]
pub struct ManagerInfo {
    pub kind: ManagerKind,
    pub version: String,
    pub config_path: PathBuf,
    pub recommendations: Vec<Recommendation>,
    /// True if this entry was found via search (not a user-level global config).
    pub discovered: bool,
}

impl ManagerInfo {
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.recommendations.iter().all(|r| r.status.is_ok())
    }
}

// ── Detection ─────────────────────────────────────────────────────────

/// Detect the installed version of a package manager by running `<name> --version`.
pub fn detect_version(name: &str) -> Option<String> {
    // On Windows, try the command directly first, then with .cmd extension
    // (npm, pnpm are .cmd shims on Windows)
    let result = Command::new(name).arg("--version").output();
    let output = match result {
        Ok(o) if o.status.success() => o,
        _ if cfg!(target_os = "windows") => Command::new(format!("{name}.cmd"))
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())?,
        _ => return None,
    };
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Return the user's home directory (`$HOME` on Unix, `%USERPROFILE%` on Windows).
pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// On Windows, %APPDATA% (e.g. C:\Users\X\AppData\Roaming) differs from %USERPROFILE%.
fn appdata_dir() -> PathBuf {
    env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join("AppData/Roaming"))
}

/// Display a path relative to the user's home directory (e.g. `~/foo/bar`).
/// Always uses forward slashes for consistency across platforms.
pub fn display_path(path: &Path) -> String {
    let home = home_dir();
    match path.strip_prefix(&home) {
        Ok(rel) => format!("~/{}", rel.display()).replace('\\', "/"),
        Err(_) => path.display().to_string(),
    }
}

/// Target OS for config path resolution. Allows testing all platforms from any host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetOs {
    Linux,
    MacOs,
    Windows,
}

impl TargetOs {
    /// Detect the current OS at runtime.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            TargetOs::MacOs
        } else if cfg!(target_os = "windows") {
            TargetOs::Windows
        } else {
            TargetOs::Linux
        }
    }
}

/// Resolve config path for a given OS and home directory.
/// `home` is %USERPROFILE% on Windows, $HOME on Unix.
/// On Windows, derives %APPDATA% as `home/AppData/Roaming` (used for uv).
#[cfg_attr(not(test), allow(dead_code))]
pub fn config_path_for(kind: ManagerKind, home: &Path, os: TargetOs) -> PathBuf {
    config_path_full(kind, home, &home.join("AppData/Roaming"), os)
}

fn config_path_full(kind: ManagerKind, home: &Path, appdata: &Path, os: TargetOs) -> PathBuf {
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => home.join(".npmrc"),
        ManagerKind::PnpmWorkspace | ManagerKind::Renovate | ManagerKind::Dependabot => {
            PathBuf::new()
        }
        ManagerKind::Bun => home.join(".bunfig.toml"),
        ManagerKind::Yarn => home.join(".yarnrc.yml"),
        ManagerKind::Uv => match os {
            TargetOs::MacOs => home.join("Library/Application Support/uv/uv.toml"),
            TargetOs::Windows => appdata.join("uv/uv.toml"),
            TargetOs::Linux => home.join(".config/uv/uv.toml"),
        },
    }
}

/// Resolve the config file path for a package manager on the current OS.
pub fn config_path(kind: ManagerKind) -> PathBuf {
    let home = home_dir();
    let appdata = appdata_dir();
    config_path_full(kind, &home, &appdata, TargetOs::current())
}

// ── Config reading ────────────────────────────────────────────────────

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
            let v = v.split('#').next().unwrap_or(v); // strip inline comments
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

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
            let v = v.split('#').next().unwrap_or(v).trim();
            // Strip double or single quotes
            let v = v.trim_matches('"').trim_matches('\'');
            if current_section == target_section && k == target_key {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ── Date calculation ──────────────────────────────────────────────────

/// Convert days since Unix epoch to `(year, month, day)`.
///
/// Uses the civil calendar algorithm with signed arithmetic to correctly
/// handle all eras. For modern dates (year > 400), the `u64` cast is safe.
pub(crate) fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mon = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mon <= 2 { y + 1 } else { y };
    (y as u64, mon, d)
}

#[cfg(test)]
fn date_days_ago(days: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let target = now.saturating_sub(days.checked_mul(86400).unwrap_or(u64::MAX));
    epoch_to_date(target)
}

#[cfg(test)]
fn epoch_to_date(epoch: u64) -> String {
    let days_since_epoch = epoch / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

/// Parse a YYYY-MM-DD prefix from a date string (also works with RFC 3339 timestamps).
fn parse_date_to_days(date_str: &str) -> Option<u64> {
    if date_str.len() < 10 {
        return None;
    }
    let b = date_str.as_bytes();
    if b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: u64 = date_str[0..4].parse().ok()?;
    let m: u64 = date_str[5..7].parse().ok()?;
    let d: u64 = date_str[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > 31 {
        return None;
    }
    // Inverse of days_to_ymd (civil_from_days algorithm)
    if y == 0 {
        return None; // year 0 would underflow
    }
    let (adj_y, adj_m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = adj_y / 400;
    let yoe = adj_y - era * 400;
    let doy = (153 * adj_m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn current_epoch_days() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400
}

/// Parse a date (YYYY-MM-DD or RFC 3339) and check if it's at least `min_days` old.
fn is_date_old_enough(date_str: &str, min_days: u64) -> bool {
    let Some(date_days) = parse_date_to_days(date_str) else {
        return false;
    };
    let today = current_epoch_days();
    date_days <= today.saturating_sub(min_days)
}

// ── YAML reading ─────────────────────────────────────────────────────

/// Read a top-level key from a simple YAML file. Returns the value as a string.
/// Handles `key: value`, `key: "value"`, and `key: 'value'`.
pub fn read_yaml_value(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Skip indented lines (nested keys)
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim();
            if k == key {
                let v = v.split('#').next().unwrap_or(v).trim();
                // Strip quotes
                let v = v.trim_matches('"').trim_matches('\'');
                return Some(v.to_string());
            }
        }
    }
    None
}

// ── Duration parsing ──────────────────────────────────────────────────

/// Parse a compact duration string like "7d", "3d", "1440m", "24h" into minutes.
/// Returns minutes to avoid precision loss from integer division.
fn parse_duration_minutes(s: &str) -> Option<u64> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    if s.is_empty() {
        return None;
    }
    let (num_part, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num_part.parse().ok()?;
    match unit {
        "d" => Some(n.saturating_mul(24 * 60)),
        "h" => Some(n.saturating_mul(60)),
        "m" => Some(n),
        _ => None,
    }
}

// ── JSON reading ──────────────────────────────────────────────────────

/// Read a top-level string value from a simple JSON/JSONC file.
/// Matches `"key": "value"` or `"key": number` at the start of a line (after whitespace).
/// Ignores `//` comments and won't false-match keys embedded inside string values.
pub fn read_json_string_value(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let needle = format!("\"{}\"", key);
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        // Only match if the key appears at the very start of the trimmed line
        // (a JSON property), not embedded in a value string.
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

// ── Dependabot YAML reading ──────────────────────────────────────────

/// A parsed update entry from a dependabot.yml file.
#[derive(Debug, Clone)]
pub struct DependabotEntry {
    pub ecosystem: String,
    pub cooldown_default_days: Option<u64>,
}

/// Parse all `updates` entries from a dependabot.yml file, extracting
/// each entry's ecosystem name and optional `cooldown.default-days` value.
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
        // Detect a new update entry by `- package-ecosystem:`
        if trimmed.starts_with("- package-ecosystem:")
            || trimmed.starts_with("- package-ecosystem :")
        {
            let ecosystem = trimmed
                .split_once(':')
                .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_string())
                .unwrap_or_default();
            let _entry_line = i;
            // Determine the base indentation of this entry's properties
            let entry_indent = lines[i].len() - lines[i].trim_start().len();
            let prop_indent = entry_indent + 2; // properties are indented 2 more than the `-`

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
                // A new entry at same or lesser indent means we left this entry
                if line_indent <= entry_indent && line_trimmed.starts_with('-') {
                    break;
                }
                // Check for top-level keys outside updates block
                if line_indent == 0
                    && !line_trimmed.starts_with('-')
                    && !line_trimmed.starts_with('#')
                {
                    break;
                }
                if line_trimmed == "cooldown:" && line_indent >= prop_indent {
                    // Read children of cooldown
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
                cooldown_default_days: cooldown_days,
            });
            i = j;
        } else {
            i += 1;
        }
    }
    entries
}

/// Directories to skip when searching downward for project files.
const SKIP_DIRS: &[&str] = &[
    // macOS system (enormous, never contains project files)
    "Library",
    // Package manager caches/stores
    "node_modules",
    ".npm",
    ".pnpm-store",
    ".cargo",
    ".rustup",
    ".m2",
    ".gradle",
    ".cache",
    "go",
    // Build output
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    "__pycache__",
    // VCS internals
    ".git",
    ".hg",
    ".svn",
    // Misc large dirs
    ".Trash",
    ".pyenv",
    ".rbenv",
    "vendor",
];

/// Max depth for downward search to avoid excessive traversal.
const MAX_SEARCH_DEPTH: usize = 8;

/// Types of config files discovered in repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoConfigKind {
    PnpmWorkspace,
    Npmrc,
    YarnRc,
    Renovate,
    Dependabot,
}

/// Filenames that indicate a Renovate config.
const RENOVATE_FILENAMES: &[&str] = &[
    "renovate.json",
    "renovate.json5",
    ".renovaterc",
    ".renovaterc.json",
    ".renovaterc.json5",
];

/// Find pnpm-workspace.yaml files (backward-compat wrapper around `find_repo_configs`).
#[cfg(test)]
pub fn find_pnpm_workspaces(on_dir: &mut dyn FnMut(&Path)) -> Vec<PathBuf> {
    find_repo_configs(on_dir)
        .into_iter()
        .filter(|(_, kind)| *kind == RepoConfigKind::PnpmWorkspace)
        .map(|(p, _)| p)
        .collect()
}

/// Search from the user's home directory downward for all recognized repo config files.
///
/// Calls `on_dir` for each directory visited to enable live progress display.
/// Skips files at the HOME level for `.npmrc` and `.yarnrc.yml` (handled by user-level scan).
pub fn find_repo_configs(on_dir: &mut dyn FnMut(&Path)) -> Vec<(PathBuf, RepoConfigKind)> {
    let mut results = Vec::new();
    let home = home_dir();
    search_downward(&home, 0, &home, &mut results, on_dir);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by(|a, b| a.0 == b.0);
    results
}

fn classify_file(name: &str, parent: &Path, home: &Path) -> Option<RepoConfigKind> {
    match name {
        "pnpm-workspace.yaml" => Some(RepoConfigKind::PnpmWorkspace),
        ".npmrc" => {
            if parent == home {
                None // user-level, handled separately
            } else {
                Some(RepoConfigKind::Npmrc)
            }
        }
        ".yarnrc.yml" => {
            if parent == home {
                None
            } else {
                Some(RepoConfigKind::YarnRc)
            }
        }
        "dependabot.yml" | "dependabot.yaml" => {
            let parent_name = parent.file_name().and_then(|n| n.to_str());
            if parent_name == Some(".github") {
                Some(RepoConfigKind::Dependabot)
            } else {
                None
            }
        }
        _ => {
            if RENOVATE_FILENAMES.contains(&name) {
                Some(RepoConfigKind::Renovate)
            } else {
                None
            }
        }
    }
}

fn search_downward(
    dir: &Path,
    depth: usize,
    home: &Path,
    results: &mut Vec<(PathBuf, RepoConfigKind)>,
    on_dir: &mut dyn FnMut(&Path),
) {
    if depth > MAX_SEARCH_DEPTH {
        return;
    }
    on_dir(dir);
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let path = entry.path();
        if !file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(kind) = classify_file(name, dir, home) {
                    results.push((path, kind));
                }
            }
            continue;
        }
        // Skip symlinked directories to avoid loops and leaving $HOME
        if file_type.is_symlink() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Allow .github (for dependabot), skip other hidden dirs and known large dirs
        if name == ".github" || (!name.starts_with('.') && !SKIP_DIRS.contains(&name)) {
            search_downward(&path, depth + 1, home, results, on_dir);
        }
    }
}

// ── Scanning ──────────────────────────────────────────────────────────

// Default delay is 7 days, configurable via --delay-days

fn scan_npm(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let cfg = read_flat_config(path);
    let release_age = if version_at_least(version, 11, 10) {
        check_flat(
            &cfg,
            "min-release-age",
            &days.to_string(),
            &format!("Delay new versions by {days} days"),
        )
    } else {
        Recommendation {
            key: "min-release-age".into(),
            description: format!("Delay new versions by {days} days"),
            expected: days.to_string(),
            status: CheckStatus::Unsupported(format!(
                "requires npm \u{2265} 11.10 (have {version})"
            )),
        }
    };
    vec![
        release_age,
        check_flat(
            &cfg,
            "ignore-scripts",
            "true",
            "Block malicious install scripts",
        ),
    ]
}

fn scan_pnpm(path: &Path) -> Vec<Recommendation> {
    let cfg = read_flat_config(path);
    // ignore-scripts is shared with npm in the same .npmrc — only check
    // pnpm-specific keys here. Release age is handled via pnpm-workspace.yaml.
    vec![check_flat(
        &cfg,
        "ignore-scripts",
        "true",
        "Block malicious install scripts",
    )]
}

fn scan_pnpm_workspace(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    vec![
        check_yaml(
            path,
            "minimumReleaseAge",
            &minutes.to_string(),
            &format!("Delay new versions by {days} days"),
            YamlCheck::MinInt(minutes),
        ),
        check_yaml(
            path,
            "blockExoticSubdeps",
            "true",
            "Block untrusted transitive deps",
            YamlCheck::Exact,
        ),
        check_yaml(
            path,
            "trustPolicy",
            "no-downgrade",
            "Block provenance downgrades",
            YamlCheck::Exact,
        ),
        check_yaml(
            path,
            "strictDepBuilds",
            "true",
            "Fail on unreviewed build scripts",
            YamlCheck::Exact,
        ),
    ]
}

enum YamlCheck {
    Exact,
    MinInt(u64),
}

fn check_yaml(
    path: &Path,
    key: &str,
    expected: &str,
    desc: &str,
    check: YamlCheck,
) -> Recommendation {
    let val = read_yaml_value(path, key);
    let status = match (&val, &check) {
        (None, _) => CheckStatus::Missing,
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

fn scan_bun(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let seconds = days.saturating_mul(86400);
    let delay = read_toml_value(path, "install.minimumReleaseAge");
    let delay_status = match &delay {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n >= seconds => CheckStatus::Ok,
            Ok(_) => CheckStatus::WrongValue(v.clone()),
            Err(_) => CheckStatus::WrongValue(v.clone()),
        },
        None => CheckStatus::Missing,
    };
    vec![Recommendation {
        key: "install.minimumReleaseAge".into(),
        description: format!("Delay new versions by {days} days"),
        expected: seconds.to_string(),
        status: delay_status,
    }]
}

/// Parse a relative duration string like "7 days", "2 weeks" into days.
fn parse_relative_days(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 {
        let n: u64 = parts[0].parse().ok()?;
        match parts[1].trim_end_matches('s') {
            "day" => Some(n),
            "week" => n.checked_mul(7),
            _ => None,
        }
    } else {
        None
    }
}

fn scan_uv(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let val = read_toml_value(path, "exclude-newer");
    let status = match &val {
        Some(v) => {
            // Accept relative durations (e.g. "7 days") or absolute dates old enough
            if let Some(d) = parse_relative_days(v) {
                if d >= days {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else if is_date_old_enough(v, days) {
                CheckStatus::Ok
            } else {
                CheckStatus::WrongValue(v.clone())
            }
        }
        None => CheckStatus::Missing,
    };
    vec![Recommendation {
        key: "exclude-newer".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days} days"),
        status,
    }]
}

fn scan_yarn(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    if !version_at_least(version, 4, 10) {
        return vec![Recommendation {
            key: "npmMinimalAgeGate".into(),
            description: format!("Delay new versions by {days} days"),
            expected: format!("{days}d"),
            status: CheckStatus::Unsupported(format!(
                "requires yarn \u{2265} 4.10 (have {version})"
            )),
        }];
    }
    let val = read_yaml_value(path, "npmMinimalAgeGate");
    let required_minutes = days.saturating_mul(24).saturating_mul(60);
    let status = match &val {
        Some(v) => {
            if let Some(configured_minutes) = parse_duration_minutes(v) {
                if configured_minutes >= required_minutes {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else if let Ok(raw_minutes) = v.parse::<u64>() {
                if raw_minutes >= required_minutes {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else {
                CheckStatus::WrongValue(v.clone())
            }
        }
        None => CheckStatus::Missing,
    };
    vec![Recommendation {
        key: "npmMinimalAgeGate".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days}d"),
        status,
    }]
}

fn scan_renovate(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let val = read_json_string_value(path, "minimumReleaseAge");
    let status = match &val {
        Some(v) => {
            if let Some(d) = parse_relative_days(v) {
                if d >= days {
                    CheckStatus::Ok
                } else {
                    CheckStatus::WrongValue(v.clone())
                }
            } else {
                CheckStatus::WrongValue(v.clone())
            }
        }
        None => CheckStatus::Missing,
    };
    vec![Recommendation {
        key: "minimumReleaseAge".into(),
        description: format!("Delay new versions by {days} days"),
        expected: format!("{days} days"),
        status,
    }]
}

fn scan_dependabot(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let entries = read_dependabot_entries(path);
    if entries.is_empty() {
        return Vec::new();
    }
    let single = entries.len() == 1;
    let mut recs = Vec::new();
    for entry in &entries {
        let status = match entry.cooldown_default_days {
            Some(d) if d >= days => CheckStatus::Ok,
            Some(d) => CheckStatus::WrongValue(d.to_string()),
            None => CheckStatus::Missing,
        };
        let key = if single {
            "cooldown.default-days".into()
        } else {
            format!("cooldown.default-days ({})", entry.ecosystem)
        };
        recs.push(Recommendation {
            key,
            description: format!("Delay updates by {days} days"),
            expected: days.to_string(),
            status,
        });
    }
    recs
}

fn check_flat(
    cfg: &HashMap<String, String>,
    key: &str,
    expected: &str,
    desc: &str,
) -> Recommendation {
    let status = match cfg.get(key) {
        Some(v) if v == expected => CheckStatus::Ok,
        Some(v) => CheckStatus::WrongValue(v.clone()),
        None => CheckStatus::Missing,
    };
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: expected.into(),
        status,
    }
}

/// Scan a single package manager: detect version, read config, and return recommendations.
pub fn scan_manager(kind: ManagerKind) -> Option<ManagerInfo> {
    let version = detect_version(kind.name())?;
    let path = config_path(kind);
    let recommendations = match kind {
        ManagerKind::Npm => scan_npm(&path, &version),
        ManagerKind::Pnpm => scan_pnpm(&path),
        ManagerKind::Bun => scan_bun(&path),
        ManagerKind::Uv => scan_uv(&path),
        ManagerKind::Yarn => scan_yarn(&path, &version),
        ManagerKind::PnpmWorkspace | ManagerKind::Renovate | ManagerKind::Dependabot => {
            unreachable!("repo-level managers are scanned via find_repo_configs")
        }
    };
    Some(ManagerInfo {
        kind,
        version,
        config_path: path,
        recommendations,
        discovered: false,
    })
}

/// Scan discovered repo configs and return ManagerInfo entries.
/// Uses cached detected versions to avoid re-running `--version`.
fn scan_repo_configs_with_progress(
    on_progress: &mut dyn FnMut(&str, f32),
    base_frac: f32,
    detected_versions: &HashMap<&'static str, String>,
) -> Vec<ManagerInfo> {
    let configs = find_repo_configs(&mut |dir| {
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("...");
        on_progress(
            &format!("Searching for configs in ~/{}...", dir_name),
            base_frac,
        );
    });

    let mut results = Vec::new();
    for (path, kind) in configs {
        match kind {
            RepoConfigKind::PnpmWorkspace => {
                if !is_excluded(ManagerKind::PnpmWorkspace) {
                    if let Some(ver) = detected_versions.get("pnpm") {
                        let recs = scan_pnpm_workspace(&path);
                        results.push(ManagerInfo {
                            kind: ManagerKind::PnpmWorkspace,
                            version: ver.clone(),
                            config_path: path,
                            recommendations: recs,
                            discovered: true,
                        });
                    }
                }
            }
            RepoConfigKind::Npmrc => {
                if !is_excluded(ManagerKind::Npm) {
                    if let Some(ver) = detected_versions.get("npm") {
                        results.push(ManagerInfo {
                            kind: ManagerKind::Npm,
                            version: ver.clone(),
                            config_path: path.clone(),
                            recommendations: scan_npm(&path, ver),
                            discovered: true,
                        });
                    }
                }
                if !is_excluded(ManagerKind::Pnpm) {
                    if let Some(ver) = detected_versions.get("pnpm") {
                        results.push(ManagerInfo {
                            kind: ManagerKind::Pnpm,
                            version: ver.clone(),
                            config_path: path.clone(),
                            recommendations: scan_pnpm(&path),
                            discovered: true,
                        });
                    }
                }
            }
            RepoConfigKind::YarnRc => {
                if !is_excluded(ManagerKind::Yarn) {
                    if let Some(ver) = detected_versions.get("yarn") {
                        results.push(ManagerInfo {
                            kind: ManagerKind::Yarn,
                            version: ver.clone(),
                            config_path: path.clone(),
                            recommendations: scan_yarn(&path, ver),
                            discovered: true,
                        });
                    }
                }
            }
            RepoConfigKind::Renovate => {
                if !is_excluded(ManagerKind::Renovate) {
                    results.push(ManagerInfo {
                        kind: ManagerKind::Renovate,
                        version: String::new(),
                        config_path: path.clone(),
                        recommendations: scan_renovate(&path),
                        discovered: true,
                    });
                }
            }
            RepoConfigKind::Dependabot => {
                if !is_excluded(ManagerKind::Dependabot) {
                    results.push(ManagerInfo {
                        kind: ManagerKind::Dependabot,
                        version: String::new(),
                        config_path: path.clone(),
                        recommendations: scan_dependabot(&path),
                        discovered: true,
                    });
                }
            }
        }
    }
    results
}

#[cfg(test)]
pub fn scan_all() -> Vec<ManagerInfo> {
    scan_all_with_progress(|_, _| {})
}

/// Scan all managers, calling `on_progress(step_description, fraction)` after each step.
/// `fraction` is a value from 0.0 to 1.0.
pub fn scan_all_with_progress(mut on_progress: impl FnMut(&str, f32)) -> Vec<ManagerInfo> {
    let managers: Vec<ManagerKind> = ManagerKind::USER_LEVEL
        .iter()
        .copied()
        .filter(|k| !is_excluded(*k))
        .collect();
    let base_steps = managers.len() + 1; // +1 for repo search
    let mut results = Vec::new();
    let mut detected_versions: HashMap<&'static str, String> = HashMap::new();

    for (i, &kind) in managers.iter().enumerate() {
        on_progress(
            &format!("Checking {} configuration...", kind.name()),
            i as f32 / base_steps as f32,
        );
        if let Some(info) = scan_manager(kind) {
            detected_versions.insert(kind.name(), info.version.clone());
            results.push(info);
        }
    }

    if !skip_search_enabled() {
        let base_frac = managers.len() as f32 / base_steps as f32;
        let repo_infos =
            scan_repo_configs_with_progress(&mut on_progress, base_frac, &detected_versions);
        results.extend(repo_infos);
    }

    on_progress("Done", 1.0);
    results
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    // We use a tiny inline tempfile helper since we have zero deps.
    mod tempfile {
        use std::fs;
        use std::io::{self, Write};
        use std::path::PathBuf;

        pub struct NamedTempFile {
            pub path: PathBuf,
            file: fs::File,
        }

        impl NamedTempFile {
            pub fn new() -> io::Result<Self> {
                use std::sync::atomic::{AtomicU64, Ordering};
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let n = COUNTER.fetch_add(1, Ordering::Relaxed);
                let id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir()
                    .join(format!("depsguard_test_{id}_{}_{n}", std::process::id()));
                let file = fs::File::create(&path)?;
                Ok(Self { path, file })
            }

            pub fn path(&self) -> &std::path::Path {
                &self.path
            }
        }

        impl Write for NamedTempFile {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.file.write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.file.flush()
            }
        }

        impl Drop for NamedTempFile {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    #[test]
    fn read_flat_config_basic() {
        let f = tmp_file("key1=value1\nkey2 = value2\n# comment\nkey3=val3 # inline\n");
        let cfg = read_flat_config(f.path());
        assert_eq!(cfg.get("key1").unwrap(), "value1");
        assert_eq!(cfg.get("key2").unwrap(), "value2");
        assert_eq!(cfg.get("key3").unwrap(), "val3");
    }

    #[test]
    fn read_flat_config_empty_and_missing() {
        let cfg = read_flat_config(Path::new("/nonexistent/path"));
        assert!(cfg.is_empty());

        let f = tmp_file("");
        let cfg = read_flat_config(f.path());
        assert!(cfg.is_empty());
    }

    #[test]
    fn read_flat_config_semicolon_comments() {
        let f = tmp_file("; this is a comment\nkey=val\n");
        let cfg = read_flat_config(f.path());
        assert_eq!(cfg.len(), 1);
        assert_eq!(cfg.get("key").unwrap(), "val");
    }

    #[test]
    fn read_toml_value_basic() {
        let f = tmp_file("foo = \"bar\"\n\n[install]\nminimumReleaseAge = 604800\n");
        assert_eq!(read_toml_value(f.path(), "foo"), Some("bar".into()));
        assert_eq!(
            read_toml_value(f.path(), "install.minimumReleaseAge"),
            Some("604800".into())
        );
    }

    #[test]
    fn read_toml_value_missing() {
        let f = tmp_file("[section]\nkey = 1\n");
        assert_eq!(read_toml_value(f.path(), "nonexistent"), None);
        assert_eq!(read_toml_value(f.path(), "section.missing"), None);
        assert_eq!(read_toml_value(Path::new("/no/file"), "key"), None);
    }

    #[test]
    fn read_toml_value_inline_comment() {
        let f = tmp_file("key = 42 # a comment\n");
        assert_eq!(read_toml_value(f.path(), "key"), Some("42".into()));
    }

    #[test]
    fn date_days_ago_format() {
        let d = date_days_ago(0);
        assert!(d.ends_with("T00:00:00Z")); // RFC 3339
        assert_eq!(&d[4..5], "-");
        assert_eq!(&d[7..8], "-");
    }

    #[test]
    fn date_days_ago_is_past() {
        let today = date_days_ago(0);
        let week_ago = date_days_ago(7);
        assert!(week_ago < today);
    }

    #[test]
    fn epoch_to_date_known() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(epoch_to_date(1704067200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn is_date_old_enough_works() {
        let old = date_days_ago(30);
        assert!(is_date_old_enough(&old, 7));
        let recent = date_days_ago(1);
        assert!(!is_date_old_enough(&recent, 7));
    }

    #[test]
    fn check_status_display() {
        assert_eq!(format!("{}", CheckStatus::Ok), "OK");
        assert_eq!(format!("{}", CheckStatus::Missing), "Not set");
        assert_eq!(
            format!("{}", CheckStatus::WrongValue("3".into())),
            "Current: 3"
        );
    }

    #[test]
    fn check_status_is_ok() {
        assert!(CheckStatus::Ok.is_ok());
        assert!(!CheckStatus::Missing.is_ok());
        assert!(!CheckStatus::WrongValue("x".into()).is_ok());
    }

    #[test]
    fn recommendation_needs_fix() {
        let ok = Recommendation {
            key: "k".into(),
            description: "d".into(),
            expected: "v".into(),
            status: CheckStatus::Ok,
        };
        assert!(!ok.needs_fix());
        let bad = Recommendation {
            key: "k".into(),
            description: "d".into(),
            expected: "v".into(),
            status: CheckStatus::Missing,
        };
        assert!(bad.needs_fix());
    }

    #[test]
    fn manager_kind_all_names() {
        for k in ManagerKind::ALL {
            assert!(!k.name().is_empty());
        }
    }

    #[test]
    fn manager_info_all_ok() {
        let info = ManagerInfo {
            kind: ManagerKind::Npm,
            version: "1.0".into(),
            config_path: PathBuf::from("/tmp"),
            recommendations: vec![Recommendation {
                key: "k".into(),
                description: "d".into(),
                expected: "v".into(),
                status: CheckStatus::Ok,
            }],
            discovered: false,
        };
        assert!(info.all_ok());
    }

    #[test]
    fn scan_npm_checks() {
        let f = tmp_file("min-release-age=7\nignore-scripts=true\n");
        let recs = scan_npm(f.path(), "11.10.0");
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok());
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_npm_missing() {
        let f = tmp_file("");
        let recs = scan_npm(f.path(), "11.10.0");
        assert!(recs.iter().all(|r| r.needs_fix()));
    }

    #[test]
    fn scan_npm_wrong_values() {
        let f = tmp_file("min-release-age=1\nignore-scripts=false\n");
        let recs = scan_npm(f.path(), "11.10.0");
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
        assert!(matches!(recs[1].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_npm_old_version_unsupported() {
        let f = tmp_file("ignore-scripts=true\n");
        let recs = scan_npm(f.path(), "10.8.0");
        assert!(recs[0].status.is_unsupported());
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_pnpm_checks() {
        let f = tmp_file("ignore-scripts=true\n");
        let recs = scan_pnpm(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_bun_checks() {
        let f = tmp_file("[install]\nminimumReleaseAge = 604800\n");
        let recs = scan_bun(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_bun_too_low() {
        let f = tmp_file("[install]\nminimumReleaseAge = 100\n");
        let recs = scan_bun(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_bun_missing() {
        let f = tmp_file("");
        let recs = scan_bun(f.path());
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn scan_bun_invalid_value() {
        let f = tmp_file("[install]\nminimumReleaseAge = abc\n");
        let recs = scan_bun(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_uv_relative_days() {
        let f = tmp_file("exclude-newer = \"7 days\"\n");
        let recs = scan_uv(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_uv_relative_weeks() {
        let f = tmp_file("exclude-newer = \"2 weeks\"\n");
        let recs = scan_uv(f.path());
        assert!(recs[0].status.is_ok()); // 14 days >= 7
    }

    #[test]
    fn scan_uv_relative_too_short() {
        let f = tmp_file("exclude-newer = \"3 days\"\n");
        let recs = scan_uv(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_uv_absolute_date_ok() {
        let old_date = date_days_ago(30);
        let content = format!("exclude-newer = \"{old_date}\"\n");
        let f = tmp_file(&content);
        let recs = scan_uv(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_uv_too_recent() {
        let content = "exclude-newer = \"2099-01-01\"\n";
        let f = tmp_file(content);
        let recs = scan_uv(f.path());
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_uv_missing() {
        let f = tmp_file("");
        let recs = scan_uv(f.path());
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn config_path_npm() {
        let p = config_path(ManagerKind::Npm);
        assert!(p.to_str().unwrap().contains(".npmrc"));
    }

    #[test]
    fn config_path_bun() {
        let p = config_path(ManagerKind::Bun);
        assert!(p.to_str().unwrap().contains(".bunfig.toml"));
    }

    #[test]
    fn config_path_uv() {
        let p = config_path(ManagerKind::Uv);
        assert!(p.to_str().unwrap().contains("uv"));
    }

    #[test]
    fn home_dir_returns_path() {
        let h = home_dir();
        assert!(!h.as_os_str().is_empty());
    }

    // ── Cross-platform config path tests ──────────────────────────────

    #[test]
    fn config_path_linux_npm() {
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Npm, home, TargetOs::Linux);
        assert_eq!(p, PathBuf::from("/home/testuser/.npmrc"));
    }

    #[test]
    fn config_path_linux_pnpm() {
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Pnpm, home, TargetOs::Linux);
        assert_eq!(p, PathBuf::from("/home/testuser/.npmrc"));
    }

    #[test]
    fn config_path_linux_bun() {
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Linux);
        assert_eq!(p, PathBuf::from("/home/testuser/.bunfig.toml"));
    }

    #[test]
    fn config_path_linux_uv() {
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::Linux);
        assert_eq!(p, PathBuf::from("/home/testuser/.config/uv/uv.toml"));
    }

    #[test]
    fn config_path_macos_npm() {
        let home = Path::new("/Users/testuser");
        let p = config_path_for(ManagerKind::Npm, home, TargetOs::MacOs);
        assert_eq!(p, PathBuf::from("/Users/testuser/.npmrc"));
    }

    #[test]
    fn config_path_macos_uv() {
        let home = Path::new("/Users/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::MacOs);
        assert_eq!(
            p,
            PathBuf::from("/Users/testuser/Library/Application Support/uv/uv.toml")
        );
    }

    #[test]
    fn config_path_windows_npm() {
        // npm user config is %USERPROFILE%\.npmrc on Windows
        let home = Path::new("C:/Users/testuser");
        let p = config_path_for(ManagerKind::Npm, home, TargetOs::Windows);
        assert_eq!(p, PathBuf::from("C:/Users/testuser/.npmrc"));
    }

    #[test]
    fn config_path_windows_uv() {
        // uv uses %APPDATA%\uv\uv.toml on Windows
        let home = Path::new("C:/Users/testuser");
        let appdata = Path::new("C:/Users/testuser/AppData/Roaming");
        let p = config_path_full(ManagerKind::Uv, home, appdata, TargetOs::Windows);
        assert_eq!(
            p,
            PathBuf::from("C:/Users/testuser/AppData/Roaming/uv/uv.toml")
        );
    }

    #[test]
    fn config_path_windows_bun() {
        let home = Path::new("C:/Users/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Windows);
        assert_eq!(p, PathBuf::from("C:/Users/testuser/.bunfig.toml"));
    }

    #[test]
    fn target_os_current() {
        let os = TargetOs::current();
        // Just ensure it returns something valid
        assert!(os == TargetOs::Linux || os == TargetOs::MacOs || os == TargetOs::Windows);
    }

    // ── YAML reading tests ───────────────────────────────────────────

    #[test]
    fn read_yaml_value_basic() {
        let f = tmp_file("minimumReleaseAge: 4320\nblockExoticSubdeps: true\n");
        assert_eq!(
            read_yaml_value(f.path(), "minimumReleaseAge"),
            Some("4320".into())
        );
        assert_eq!(
            read_yaml_value(f.path(), "blockExoticSubdeps"),
            Some("true".into())
        );
    }

    #[test]
    fn read_yaml_value_quoted() {
        let f = tmp_file("trustPolicy: \"no-downgrade\"\n");
        assert_eq!(
            read_yaml_value(f.path(), "trustPolicy"),
            Some("no-downgrade".into())
        );
    }

    #[test]
    fn read_yaml_value_missing() {
        let f = tmp_file("foo: bar\n");
        assert_eq!(read_yaml_value(f.path(), "nonexistent"), None);
    }

    #[test]
    fn read_yaml_value_skips_nested() {
        // Should not match indented keys
        let f = tmp_file("packages:\n  - 'src/*'\nminimumReleaseAge: 4320\n");
        assert_eq!(
            read_yaml_value(f.path(), "minimumReleaseAge"),
            Some("4320".into())
        );
        assert_eq!(read_yaml_value(f.path(), "- 'src/*'"), None);
    }

    #[test]
    fn read_yaml_value_with_comment() {
        let f = tmp_file("minimumReleaseAge: 4320 # 3 days\n");
        assert_eq!(
            read_yaml_value(f.path(), "minimumReleaseAge"),
            Some("4320".into())
        );
    }

    // ── pnpm-workspace scanning tests ────────────────────────────────

    #[test]
    fn scan_pnpm_workspace_all_ok() {
        let f = tmp_file(
            "minimumReleaseAge: 10080\nblockExoticSubdeps: true\ntrustPolicy: \"no-downgrade\"\nstrictDepBuilds: true\n",
        );
        let recs = scan_pnpm_workspace(f.path());
        assert_eq!(recs.len(), 4);
        assert!(recs.iter().all(|r| r.status.is_ok()));
    }

    #[test]
    fn scan_pnpm_workspace_missing() {
        let f = tmp_file("");
        let recs = scan_pnpm_workspace(f.path());
        assert_eq!(recs.len(), 4);
        assert!(recs
            .iter()
            .all(|r| matches!(r.status, CheckStatus::Missing)));
    }

    #[test]
    fn scan_pnpm_workspace_higher_release_age_ok() {
        let f = tmp_file("minimumReleaseAge: 10080\n");
        let recs = scan_pnpm_workspace(f.path());
        assert!(recs[0].status.is_ok()); // 10080 >= 4320
    }

    #[test]
    fn scan_pnpm_workspace_low_release_age() {
        let f = tmp_file("minimumReleaseAge: 100\n");
        let recs = scan_pnpm_workspace(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    // ── parse_semver tests ───────────────────────────────────────────

    #[test]
    fn parse_semver_basic() {
        assert_eq!(parse_semver("11.10.0"), Some((11, 10, 0)));
        assert_eq!(parse_semver("4.10.2"), Some((4, 10, 2)));
        assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
    }

    #[test]
    fn parse_semver_with_prerelease() {
        assert_eq!(parse_semver("11.10.0-beta.1"), Some((11, 10, 0)));
    }

    #[test]
    fn parse_semver_invalid() {
        assert!(parse_semver("").is_none());
        assert!(parse_semver("abc").is_none());
        assert!(parse_semver("1").is_none());
    }

    #[test]
    fn version_at_least_checks() {
        assert!(version_at_least("11.10.0", 11, 10));
        assert!(version_at_least("12.0.0", 11, 10));
        assert!(!version_at_least("11.9.0", 11, 10));
        assert!(!version_at_least("10.0.0", 11, 10));
        assert!(version_at_least("4.10.0", 4, 10));
        assert!(!version_at_least("4.9.2", 4, 10));
    }

    // ── parse_duration_string tests ──────────────────────────────────

    #[test]
    fn parse_duration_days() {
        assert_eq!(parse_duration_minutes("7d"), Some(7 * 24 * 60));
        assert_eq!(parse_duration_minutes("3d"), Some(3 * 24 * 60));
        assert_eq!(parse_duration_minutes("\"7d\""), Some(7 * 24 * 60));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration_minutes("168h"), Some(168 * 60));
        assert_eq!(parse_duration_minutes("48h"), Some(48 * 60));
        assert_eq!(parse_duration_minutes("10h"), Some(600));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration_minutes("").is_none());
        assert!(parse_duration_minutes("abc").is_none());
    }

    // ── read_json_string_value tests ─────────────────────────────────

    #[test]
    fn read_json_basic() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"7 days\"\n}\n");
        assert_eq!(
            read_json_string_value(f.path(), "minimumReleaseAge"),
            Some("7 days".into())
        );
    }

    #[test]
    fn read_json_with_comments() {
        let f = tmp_file("{\n  // some comment\n  \"minimumReleaseAge\": \"3 days\"\n}\n");
        assert_eq!(
            read_json_string_value(f.path(), "minimumReleaseAge"),
            Some("3 days".into())
        );
    }

    #[test]
    fn read_json_missing_key() {
        let f = tmp_file("{\n  \"other\": \"value\"\n}\n");
        assert_eq!(read_json_string_value(f.path(), "minimumReleaseAge"), None);
    }

    // ── read_dependabot_entries tests ────────────────────────────────

    #[test]
    fn read_dependabot_single_entry_with_cooldown() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 7\n",
        );
        let entries = read_dependabot_entries(f.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ecosystem, "npm");
        assert_eq!(entries[0].cooldown_default_days, Some(7));
    }

    #[test]
    fn read_dependabot_no_cooldown() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"pip\"\n    directory: \"/\"\n    schedule:\n      interval: \"daily\"\n",
        );
        let entries = read_dependabot_entries(f.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ecosystem, "pip");
        assert_eq!(entries[0].cooldown_default_days, None);
    }

    #[test]
    fn read_dependabot_multiple_entries() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 5\n  - package-ecosystem: \"pip\"\n    directory: \"/\"\n    schedule:\n      interval: \"daily\"\n",
        );
        let entries = read_dependabot_entries(f.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].cooldown_default_days, Some(5));
        assert_eq!(entries[1].cooldown_default_days, None);
    }

    // ── scan_yarn tests ──────────────────────────────────────────────

    #[test]
    fn scan_yarn_ok() {
        let f = tmp_file("npmMinimalAgeGate: \"7d\"\n");
        let recs = scan_yarn(f.path(), "4.10.0");
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_yarn_too_low() {
        let f = tmp_file("npmMinimalAgeGate: \"3d\"\n");
        let recs = scan_yarn(f.path(), "4.10.0");
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_yarn_missing() {
        let f = tmp_file("");
        let recs = scan_yarn(f.path(), "4.10.0");
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn scan_yarn_old_version_unsupported() {
        let f = tmp_file("");
        let recs = scan_yarn(f.path(), "4.9.2");
        assert!(recs[0].status.is_unsupported());
    }

    #[test]
    fn scan_yarn_minutes_format() {
        let f = tmp_file("npmMinimalAgeGate: 10080\n");
        let recs = scan_yarn(f.path(), "4.10.0");
        assert!(recs[0].status.is_ok()); // 10080 minutes = 7 days
    }

    // ── scan_renovate tests ──────────────────────────────────────────

    #[test]
    fn scan_renovate_ok() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"7 days\"\n}\n");
        let recs = scan_renovate(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_renovate_missing() {
        let f = tmp_file("{\n  \"extends\": [\"config:recommended\"]\n}\n");
        let recs = scan_renovate(f.path());
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_renovate_too_short() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"2 days\"\n}\n");
        let recs = scan_renovate(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    // ── scan_dependabot tests ────────────────────────────────────────

    #[test]
    fn scan_dependabot_ok() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 7\n",
        );
        let recs = scan_dependabot(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_dependabot_missing_cooldown() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n",
        );
        let recs = scan_dependabot(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_dependabot_too_low() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 2\n",
        );
        let recs = scan_dependabot(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_dependabot_empty() {
        let f = tmp_file("version: 2\n");
        let recs = scan_dependabot(f.path());
        assert!(recs.is_empty());
    }

    #[test]
    fn scan_dependabot_multi_ecosystem_unique_keys() {
        let f = tmp_file(concat!(
            "version: 2\nupdates:\n",
            "  - package-ecosystem: \"npm\"\n    directory: \"/\"\n",
            "    cooldown:\n      default-days: 7\n",
            "  - package-ecosystem: \"github-actions\"\n    directory: \"/\"\n",
            "    schedule:\n      interval: \"weekly\"\n",
        ));
        let recs = scan_dependabot(f.path());
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok());
        assert!(recs[1].needs_fix());
        assert_ne!(recs[0].key, recs[1].key);
        assert!(recs[0].key.contains("npm"));
        assert!(recs[1].key.contains("github-actions"));
    }

    #[test]
    fn scan_dependabot_single_ecosystem_plain_key() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    cooldown:\n      default-days: 7\n",
        );
        let recs = scan_dependabot(f.path());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].key, "cooldown.default-days");
    }

    // ── config_path_yarn tests ───────────────────────────────────────

    #[test]
    fn config_path_yarn_linux() {
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Yarn, home, TargetOs::Linux);
        assert_eq!(p, PathBuf::from("/home/testuser/.yarnrc.yml"));
    }

    #[test]
    fn config_path_yarn_macos() {
        let home = Path::new("/Users/testuser");
        let p = config_path_for(ManagerKind::Yarn, home, TargetOs::MacOs);
        assert_eq!(p, PathBuf::from("/Users/testuser/.yarnrc.yml"));
    }

    #[test]
    fn read_json_no_false_positive_in_value() {
        let f = tmp_file(
            "{\n  \"description\": \"set minimumReleaseAge to 7 days\",\n  \"minimumReleaseAge\": \"3 days\"\n}\n",
        );
        let val = read_json_string_value(f.path(), "minimumReleaseAge");
        assert_eq!(val, Some("3 days".into()));
    }

    #[test]
    fn read_json_no_match_in_nested_value() {
        let f = tmp_file("{\n  \"note\": \"minimumReleaseAge is important\"\n}\n");
        let val = read_json_string_value(f.path(), "minimumReleaseAge");
        assert_eq!(val, None);
    }
}
