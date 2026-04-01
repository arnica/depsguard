// Package manager detection, config scanning, and recommendation engine.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    Bun,
    Uv,
}

impl ManagerKind {
    pub const ALL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::Bun,
        ManagerKind::Uv,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ManagerKind::Npm => "npm",
            ManagerKind::Pnpm => "pnpm",
            ManagerKind::Bun => "bun",
            ManagerKind::Uv => "uv",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ManagerKind::Npm => "📦",
            ManagerKind::Pnpm => "⚡",
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
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn config_path(kind: ManagerKind) -> PathBuf {
    let home = home_dir();
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => home.join(".npmrc"),
        ManagerKind::Bun => home.join(".bunfig.toml"),
        ManagerKind::Uv => {
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/uv/uv.toml")
            } else {
                // Linux / default
                home.join(".config/uv/uv.toml")
            }
        }
    }
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
        if line.starts_with('[') && line.ends_with(']') {
            current_section = Some(line[1..line.len() - 1].trim());
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.split('#').next().unwrap_or(v).trim();
            // Strip quotes
            let v = v.trim_matches('"');
            if current_section == target_section && k == target_key {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ── Exclude-newer date calculation ────────────────────────────────────

/// Returns a date string N days ago in YYYY-MM-DD format (for uv exclude-newer).
pub fn date_days_ago(days: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let target = now - (days * 86400);
    // Convert epoch to YYYY-MM-DD
    epoch_to_date(target)
}

fn epoch_to_date(epoch: u64) -> String {
    // Simple date calculation from epoch — returns RFC 3339 with T00:00:00Z
    let days_since_epoch = epoch / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
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

/// Parse a YYYY-MM-DD date and check if it's at least `min_days` old from today.
pub fn is_date_old_enough(date_str: &str, min_days: u64) -> bool {
    let today = date_days_ago(0);
    let threshold = date_days_ago(min_days);
    // date_str should be <= threshold (i.e., at least min_days ago)
    date_str <= &threshold && date_str <= &today
}

// ── Scanning ──────────────────────────────────────────────────────────

const DELAY_DAYS: u64 = 7;

fn scan_npm(path: &Path) -> Vec<Recommendation> {
    let cfg = read_flat_config(path);
    vec![
        check_flat(&cfg, "min-release-age", "7", "Minimum release age (days) - delays new package versions by 7 days"),
        check_flat(&cfg, "ignore-scripts", "true", "Disable post-install scripts - prevents malicious install scripts"),
    ]
}

fn scan_pnpm(path: &Path) -> Vec<Recommendation> {
    let cfg = read_flat_config(path);
    vec![
        check_flat(&cfg, "minimum-release-age", "10080", "Minimum release age (minutes) - delays new package versions by 7 days"),
        check_flat(&cfg, "ignore-scripts", "true", "Disable post-install scripts - prevents malicious install scripts"),
    ]
}

fn scan_bun(path: &Path) -> Vec<Recommendation> {
    let delay = read_toml_value(path, "install.minimumReleaseAge");
    let delay_status = match &delay {
        Some(v) => match v.parse::<u64>() {
            Ok(n) if n >= 604800 => CheckStatus::Ok,
            Ok(_) => CheckStatus::WrongValue(v.clone()),
            Err(_) => CheckStatus::WrongValue(v.clone()),
        },
        None => CheckStatus::Missing,
    };
    // Bun uses a trusted-dependencies allow-list by default, so scripts are
    // already restricted. We still recommend awareness but mark it OK.
    vec![
        Recommendation {
            key: "install.minimumReleaseAge".into(),
            description: "Minimum release age (seconds) - delays new versions by 7 days".into(),
            expected: "604800".into(),
            status: delay_status,

        },
    ]
}

fn scan_uv(path: &Path) -> Vec<Recommendation> {
    let val = read_toml_value(path, "exclude-newer");
    let threshold = date_days_ago(DELAY_DAYS);
    let status = match &val {
        Some(v) if is_date_old_enough(v, DELAY_DAYS) => CheckStatus::Ok,
        Some(v) => CheckStatus::WrongValue(v.clone()),
        None => CheckStatus::Missing,
    };
    vec![Recommendation {
        key: "exclude-newer".into(),
        description: format!(
            "Exclude packages newer than 7 days - set to rolling date (currently {threshold})"
        ),
        expected: threshold,
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
    };
    Some(ManagerInfo {
        kind,
        version,
        config_path: path,
        recommendations,
    })
}

pub fn scan_all() -> Vec<ManagerInfo> {
    ManagerKind::ALL
        .iter()
        .filter_map(|&k| scan_manager(k))
        .collect()
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
                let path =
                    std::env::temp_dir().join(format!("depsguard_test_{id}_{}", std::process::id()));
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
        assert_eq!(
            read_toml_value(f.path(), "foo"),
            Some("bar".into())
        );
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
            assert!(!k.icon().is_empty());
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
        let f = tmp_file("minimum-release-age=10080\nignore-scripts=true\n");
        let recs = scan_pnpm(f.path());
        assert!(recs.iter().all(|r| r.status.is_ok()));
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
    fn scan_uv_checks() {
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
}
