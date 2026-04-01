// Package manager detection, config scanning, and recommendation engine.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static SKIP_WORKSPACES: AtomicBool = AtomicBool::new(false);
static DELAY_DAYS_SETTING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);

pub fn set_skip_workspaces(skip: bool) {
    SKIP_WORKSPACES.store(skip, Ordering::Relaxed);
}

pub fn skip_workspaces_enabled() -> bool {
    SKIP_WORKSPACES.load(Ordering::Relaxed)
}

pub fn set_delay_days(days: u64) {
    DELAY_DAYS_SETTING.store(days, Ordering::Relaxed);
}

pub fn get_delay_days() -> u64 {
    DELAY_DAYS_SETTING.load(Ordering::Relaxed)
}

// ── Core types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    Ok,
    Missing,
    WrongValue(String),
}

impl CheckStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, CheckStatus::Ok)
    }
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "OK"),
            CheckStatus::Missing => write!(f, "Not set"),
            CheckStatus::WrongValue(v) => write!(f, "Current: {v}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub key: String,
    pub description: String,
    pub expected: String,
    pub status: CheckStatus,
}

impl Recommendation {
    pub fn needs_fix(&self) -> bool {
        !self.status.is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagerKind {
    Npm,
    Pnpm,
    PnpmWorkspace,
    Bun,
    Uv,
}

impl ManagerKind {
    pub const ALL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::PnpmWorkspace,
        ManagerKind::Bun,
        ManagerKind::Uv,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ManagerKind::Npm => "npm",
            ManagerKind::Pnpm => "pnpm",
            ManagerKind::PnpmWorkspace => "pnpm-workspace",
            ManagerKind::Bun => "bun",
            ManagerKind::Uv => "uv",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ManagerKind::Npm => "📦",
            ManagerKind::Pnpm | ManagerKind::PnpmWorkspace => "⚡",
            ManagerKind::Bun => "🥟",
            ManagerKind::Uv => "🐍",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagerInfo {
    pub kind: ManagerKind,
    pub version: String,
    pub config_path: PathBuf,
    pub recommendations: Vec<Recommendation>,
}

impl ManagerInfo {
    pub fn all_ok(&self) -> bool {
        self.recommendations.iter().all(|r| r.status.is_ok())
    }
}

// ── Detection ─────────────────────────────────────────────────────────

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
        // npm/pnpm: ~/.npmrc on all platforms (including Windows %USERPROFILE%\.npmrc)
        ManagerKind::Npm | ManagerKind::Pnpm => home.join(".npmrc"),
        // pnpm-workspace: discovered dynamically, not a fixed path
        ManagerKind::PnpmWorkspace => PathBuf::new(),
        // bun: ~/.bunfig.toml on all platforms
        ManagerKind::Bun => home.join(".bunfig.toml"),
        // uv: OS-specific
        ManagerKind::Uv => match os {
            TargetOs::MacOs => home.join("Library/Application Support/uv/uv.toml"),
            TargetOs::Windows => appdata.join("uv/uv.toml"),
            TargetOs::Linux => home.join(".config/uv/uv.toml"),
        },
    }
}

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
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].trim());
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

// ── Exclude-newer date calculation ────────────────────────────────────

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

#[cfg(test)]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
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
        .unwrap()
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

/// Find workspaces with a progress callback showing each directory being scanned.
/// Used by both scan and restore to show the same live progress UI.
pub fn find_pnpm_workspaces_with_callback(on_dir: &mut dyn FnMut(&Path)) -> Vec<PathBuf> {
    find_pnpm_workspaces_with_progress(on_dir)
}

/// Find pnpm-workspace.yaml files by searching from the user's home directory downward.
/// Returns all unique paths found.
fn find_pnpm_workspaces_with_progress(on_dir: &mut dyn FnMut(&Path)) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let home = home_dir();

    // Search downward from home (skip known large dirs)
    search_downward(&home, 0, &mut results, on_dir);

    // Deduplicate
    results.sort();
    results.dedup();
    results
}

fn search_downward(
    dir: &Path,
    depth: usize,
    results: &mut Vec<PathBuf>,
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
        let path = entry.path();
        if !path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("pnpm-workspace.yaml") {
                results.push(path);
            }
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Skip hidden dirs (except those we explicitly handle) and known large dirs
        if name.starts_with('.') || SKIP_DIRS.contains(&name) {
            continue;
        }
        search_downward(&path, depth + 1, results, on_dir);
    }
}

// ── Scanning ──────────────────────────────────────────────────────────

// Default delay is 7 days, configurable via --delay-days

fn scan_npm(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let cfg = read_flat_config(path);
    vec![
        check_flat(
            &cfg,
            "min-release-age",
            &days.to_string(),
            &format!("Delay new versions by {days} days"),
        ),
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

pub fn scan_manager(kind: ManagerKind) -> Option<ManagerInfo> {
    let version = detect_version(kind.name())?;
    let path = config_path(kind);
    let recommendations = match kind {
        ManagerKind::Npm => scan_npm(&path),
        ManagerKind::Pnpm => scan_pnpm(&path),
        ManagerKind::Bun => scan_bun(&path),
        ManagerKind::Uv => scan_uv(&path),
        ManagerKind::PnpmWorkspace => unreachable!("use scan_pnpm_workspaces instead"),
    };
    Some(ManagerInfo {
        kind,
        version,
        config_path: path,
        recommendations,
    })
}

/// Scan all discovered pnpm-workspace.yaml files (requires pnpm installed).
fn scan_pnpm_workspaces_with_progress(
    on_progress: &mut dyn FnMut(&str, f32),
    base_frac: f32,
) -> Vec<ManagerInfo> {
    let version = match detect_version("pnpm") {
        Some(v) => v,
        None => return Vec::new(),
    };
    let paths = find_pnpm_workspaces_with_progress(&mut |dir| {
        // Show a pulsing progress in the workspace-search fraction range (base_frac..1.0)
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("...");
        on_progress(
            &format!("Searching for workspace configs in ~/{}...", dir_name),
            base_frac,
        );
    });
    paths
        .into_iter()
        .map(|path| {
            let recommendations = scan_pnpm_workspace(&path);
            ManagerInfo {
                kind: ManagerKind::PnpmWorkspace,
                version: version.clone(),
                config_path: path,
                recommendations,
            }
        })
        .collect()
}

#[cfg(test)]
pub fn scan_all() -> Vec<ManagerInfo> {
    scan_all_with_progress(|_, _| {})
}

/// Scan all managers, calling `on_progress(step_description, fraction)` after each step.
/// `fraction` is a value from 0.0 to 1.0.
pub fn scan_all_with_progress(mut on_progress: impl FnMut(&str, f32)) -> Vec<ManagerInfo> {
    let managers: Vec<ManagerKind> = ManagerKind::ALL
        .iter()
        .copied()
        .filter(|&k| k != ManagerKind::PnpmWorkspace)
        .collect();
    let base_steps = managers.len() + 1; // +1 for workspace search
    let mut results = Vec::new();

    for (i, &kind) in managers.iter().enumerate() {
        on_progress(
            &format!("Checking {} configuration...", kind.name()),
            i as f32 / base_steps as f32,
        );
        if let Some(info) = scan_manager(kind) {
            results.push(info);
        }
    }

    if !SKIP_WORKSPACES.load(Ordering::Relaxed) {
        let base_frac = managers.len() as f32 / base_steps as f32;
        let workspace_infos = scan_pnpm_workspaces_with_progress(&mut on_progress, base_frac);
        results.extend(workspace_infos);
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
                let id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir()
                    .join(format!("depsguard_test_{id}_{}", std::process::id()));
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
        };
        assert!(info.all_ok());
    }

    #[test]
    fn scan_npm_checks() {
        let f = tmp_file("min-release-age=7\nignore-scripts=true\n");
        let recs = scan_npm(f.path());
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok());
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_npm_missing() {
        let f = tmp_file("");
        let recs = scan_npm(f.path());
        assert!(recs.iter().all(|r| r.needs_fix()));
    }

    #[test]
    fn scan_npm_wrong_values() {
        let f = tmp_file("min-release-age=1\nignore-scripts=false\n");
        let recs = scan_npm(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
        assert!(matches!(recs[1].status, CheckStatus::WrongValue(_)));
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
}
