// Package manager detection, config scanning, and recommendation engine.
//
// Each supported package manager has its own submodule with a `scan()` function.
// Shared infrastructure lives in `types`, `config`, `version`, `date`, `paths`,
// `detect`, and `search`.

pub mod bun;
pub mod config;
pub mod date;
pub mod dependabot;
pub mod detect;
pub mod npm;
pub mod paths;
pub mod pnpm;
pub mod renovate;
pub mod search;
pub mod types;
pub mod uv;
pub mod version;
pub mod yarn;

// ── Public re-exports ─────────────────────────────────────────────────
// Keep the existing public API surface so that `main.rs`, `fix.rs`, and
// `ui.rs` continue to compile without changes.

pub use date::days_to_ymd;
pub use detect::{
    detect_version, is_excluded, set_delay_days, set_excluded_managers, set_skip_search,
    skip_search_enabled,
};
pub use paths::{config_path, display_path, home_dir};
pub use types::{CheckStatus, ManagerInfo, ManagerKind, Recommendation, RepoConfigKind, TargetOs};

// Re-exports used only from tests (unit tests + integration tests in tests/)
#[cfg(test)]
pub use version::parse_semver;

use std::collections::HashMap;

use paths::{
    pnpm_global_rc, pnpm_global_rc_from_cli, pnpm_global_yaml, select_scan_paths,
    user_config_candidates,
};
use search::find_repo_configs;

// ── Scanning orchestration ───────────────────────────────────────────

/// Scan a user-level package manager and return one or more config entries.
pub fn scan_manager_infos(kind: ManagerKind) -> Vec<ManagerInfo> {
    let Some(version) = detect_version(kind.name()) else {
        return Vec::new();
    };
    let home = home_dir();
    let appdata = paths::appdata_dir();
    let os = TargetOs::current();

    let scan_paths = match kind {
        ManagerKind::PnpmGlobal => {
            if pnpm::uses_yaml_config(&version) {
                vec![pnpm_global_yaml()]
            } else {
                vec![pnpm_global_rc_from_cli(&version).unwrap_or_else(pnpm_global_rc)]
            }
        }
        _ => {
            let (cands, default_idx) = user_config_candidates(kind, &home, &appdata, os);
            select_scan_paths(&cands, default_idx)
        }
    };

    scan_paths
        .into_iter()
        .map(|path| {
            let recommendations = scan_kind(kind, &path, &version);
            ManagerInfo {
                kind,
                version: version.clone(),
                config_path: path,
                recommendations,
                discovered: false,
            }
        })
        .collect()
}

/// Dispatch to the correct scanner for a given manager kind.
fn scan_kind(kind: ManagerKind, path: &std::path::Path, version: &str) -> Vec<Recommendation> {
    match kind {
        ManagerKind::Npm => npm::scan(path, version),
        ManagerKind::Pnpm => pnpm::scan_project(path, version),
        ManagerKind::PnpmGlobal => pnpm::scan_global(path, version),
        ManagerKind::Bun => bun::scan(path),
        ManagerKind::Uv => uv::scan(path),
        ManagerKind::Yarn => yarn::scan(path, version),
        ManagerKind::PnpmWorkspace | ManagerKind::Renovate | ManagerKind::Dependabot => {
            unreachable!("repo-level managers are scanned via find_repo_configs")
        }
    }
}

/// Scan discovered repo configs and return ManagerInfo entries.
fn scan_repo_configs_with_progress(
    on_progress: &mut dyn FnMut(&str, f32),
    base_frac: f32,
    detected_versions: &HashMap<&'static str, String>,
) -> Vec<ManagerInfo> {
    let configs = find_repo_configs(&mut |dir| {
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("...");
        on_progress(
            &format!("Searching current tree in {dir_name}/..."),
            base_frac,
        );
    });

    let mut results = Vec::new();
    for (path, kind) in configs {
        match kind {
            RepoConfigKind::PnpmWorkspace => {
                if !is_excluded(ManagerKind::PnpmWorkspace) {
                    if let Some(ver) = detected_versions.get("pnpm") {
                        let recs = pnpm::scan_workspace(&path, ver);
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
                            recommendations: npm::scan(&path, ver),
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
                            recommendations: pnpm::scan_project(&path, ver),
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
                            recommendations: yarn::scan(&path, ver),
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
                        recommendations: renovate::scan(&path),
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
                        recommendations: dependabot::scan(&path),
                        discovered: true,
                    });
                }
            }
        }
    }
    results
}

/// Scan all managers, calling `on_progress(step_description, fraction)` after each step.
pub fn scan_all_with_progress(mut on_progress: impl FnMut(&str, f32)) -> Vec<ManagerInfo> {
    let managers: Vec<ManagerKind> = ManagerKind::USER_LEVEL
        .iter()
        .copied()
        .filter(|k| !is_excluded(*k))
        .collect();
    let base_steps = managers.len() + 1;
    let mut results = Vec::new();
    let mut detected_versions: HashMap<&'static str, String> = HashMap::new();

    for (i, &kind) in managers.iter().enumerate() {
        on_progress(
            &format!("Checking {} configuration...", kind.name()),
            i as f32 / base_steps as f32,
        );
        let infos = scan_manager_infos(kind);
        if let Some(first) = infos.first() {
            detected_versions.insert(kind.name(), first.version.clone());
            results.extend(infos);
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
    use super::config::{
        read_dependabot_entries, read_flat_config, read_json_string_value, read_toml_value,
        read_yaml_value,
    };
    use super::detect::{get_delay_days, is_excluded, set_delay_days, set_excluded_managers};
    use super::paths::{
        config_path_for, pnpm_config_dir_for, pnpm_global_rc_for, pnpm_global_rc_from_cli,
        pnpm_global_yaml_for, select_scan_paths, user_config_candidates,
    };
    use super::search::find_repo_configs;
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn tmp_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

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

    // ── Config reading tests ────────────────────────────────────────

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
    fn select_scan_paths_one_exists() {
        let exists = tmp_file("content\n");
        let missing = PathBuf::from("/tmp/does-not-exist-x");
        let paths = select_scan_paths(&[missing, exists.path().to_path_buf()], 0);
        assert_eq!(paths, vec![exists.path().to_path_buf()]);
    }

    #[test]
    fn select_scan_paths_both_exist() {
        let a = tmp_file("a\n");
        let b = tmp_file("b\n");
        let paths = select_scan_paths(&[a.path().to_path_buf(), b.path().to_path_buf()], 0);
        assert_eq!(paths, vec![a.path().to_path_buf(), b.path().to_path_buf()]);
    }

    #[test]
    fn select_scan_paths_none_exist_uses_xdg_default() {
        let xdg = PathBuf::from("/tmp/xdg-not-exist/uv/uv.toml");
        let fallback = PathBuf::from("/tmp/not-exist/.config/uv/uv.toml");
        let paths = select_scan_paths(&[xdg.clone(), fallback], 0);
        assert_eq!(
            paths,
            vec![xdg],
            "should use XDG as default when it's index 0"
        );
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

    // ── Date tests ──────────────────────────────────────────────────

    #[test]
    fn date_days_ago_format() {
        let d = date::date_days_ago(0);
        assert!(d.ends_with("T00:00:00Z"));
        assert_eq!(&d[4..5], "-");
        assert_eq!(&d[7..8], "-");
    }

    #[test]
    fn date_days_ago_is_past() {
        let today = date::date_days_ago(0);
        let week_ago = date::date_days_ago(7);
        assert!(week_ago < today);
    }

    #[test]
    fn epoch_to_date_known() {
        assert_eq!(date::epoch_to_date(1704067200), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn is_date_old_enough_works() {
        let old = date::date_days_ago(30);
        assert!(date::is_date_old_enough(&old, 7));
        let recent = date::date_days_ago(1);
        assert!(!date::is_date_old_enough(&recent, 7));
    }

    // ── CheckStatus tests ───────────────────────────────────────────

    #[test]
    fn check_status_display() {
        assert_eq!(format!("{}", CheckStatus::Ok), "OK");
        assert_eq!(format!("{}", CheckStatus::Missing), "Not set");
        assert_eq!(format!("{}", CheckStatus::FileMissing), "file missing");
        assert_eq!(
            format!("{}", CheckStatus::WrongValue("3".into())),
            "Current: 3"
        );
    }

    #[test]
    fn check_status_is_ok() {
        assert!(CheckStatus::Ok.is_ok());
        assert!(!CheckStatus::Missing.is_ok());
        assert!(!CheckStatus::FileMissing.is_ok());
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
        let missing_file = Recommendation {
            key: "k".into(),
            description: "d".into(),
            expected: "v".into(),
            status: CheckStatus::FileMissing,
        };
        assert!(missing_file.needs_fix());
    }

    #[test]
    fn manager_kind_all_names() {
        for k in ManagerKind::ALL {
            assert!(!k.name().is_empty());
        }
    }

    // ── detect / exclusion tests ────────────────────────────────────

    #[test]
    fn is_excluded_pnpm_cascades_to_variants() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        set_excluded_managers(vec!["pnpm".into()]);
        assert!(is_excluded(ManagerKind::Pnpm));
        assert!(is_excluded(ManagerKind::PnpmGlobal));
        assert!(is_excluded(ManagerKind::PnpmWorkspace));
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(!is_excluded(ManagerKind::Bun));
        set_excluded_managers(vec![]);
    }

    #[test]
    fn is_excluded_case_insensitive() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        set_excluded_managers(vec!["NPM".into()]);
        assert!(is_excluded(ManagerKind::Npm));
        set_excluded_managers(vec![]);
    }

    #[test]
    fn is_excluded_empty_list_excludes_nothing() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        set_excluded_managers(vec![]);
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(!is_excluded(ManagerKind::Pnpm));
    }

    #[test]
    fn delay_days_round_trip() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = get_delay_days();
        set_delay_days(14);
        assert_eq!(get_delay_days(), 14);
        set_delay_days(prev);
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

    // ── npm tests ───────────────────────────────────────────────────

    #[test]
    fn scan_npm_checks() {
        let f = tmp_file("min-release-age=7\nignore-scripts=true\n");
        let recs = npm::scan(f.path(), "11.10.0");
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok());
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_npm_missing() {
        let f = tmp_file("");
        let recs = npm::scan(f.path(), "11.10.0");
        assert!(recs.iter().all(|r| r.needs_fix()));
    }

    #[test]
    fn scan_npm_wrong_values() {
        let f = tmp_file("min-release-age=1\nignore-scripts=false\n");
        let recs = npm::scan(f.path(), "11.10.0");
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
        assert!(matches!(recs[1].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_npm_old_version_unsupported() {
        let f = tmp_file("ignore-scripts=true\n");
        let recs = npm::scan(f.path(), "10.8.0");
        assert!(recs[0].status.is_unsupported());
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_npm_missing_file_is_not_same_as_empty_file() {
        let missing = npm::scan(Path::new("/definitely/not/a/file"), "11.12.1");
        assert_eq!(missing[0].status.to_string(), "file missing");
        assert_eq!(missing[1].status.to_string(), "file missing");

        let empty = tmp_file("");
        let empty_recs = npm::scan(empty.path(), "11.12.1");
        assert_eq!(empty_recs[0].status.to_string(), "Not set");
        assert_eq!(empty_recs[1].status.to_string(), "Not set");
    }

    // ── pnpm tests ──────────────────────────────────────────────────

    #[test]
    fn scan_pnpm_all_ok() {
        let f = tmp_file("minimum-release-age=10080\nignore-scripts=true\n");
        let recs = pnpm::scan_project(f.path(), "10.16.0");
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok(), "minimum-release-age should be Ok");
        assert!(recs[1].status.is_ok(), "ignore-scripts should be Ok");
    }

    #[test]
    fn scan_pnpm_higher_release_age_ok() {
        let f = tmp_file("minimum-release-age=20160\nignore-scripts=true\n");
        let recs = pnpm::scan_project(f.path(), "10.16.0");
        assert!(recs[0].status.is_ok(), "20160 >= 10080 should be Ok");
    }

    #[test]
    fn scan_pnpm_release_age_too_low() {
        let f = tmp_file("minimum-release-age=100\nignore-scripts=true\n");
        let recs = pnpm::scan_project(f.path(), "10.16.0");
        assert!(
            matches!(recs[0].status, CheckStatus::WrongValue(_)),
            "100 < 10080 should be WrongValue"
        );
    }

    #[test]
    fn scan_pnpm_release_age_missing() {
        let f = tmp_file("ignore-scripts=true\n");
        let recs = pnpm::scan_project(f.path(), "10.16.0");
        assert_eq!(recs.len(), 2);
        assert!(
            matches!(recs[0].status, CheckStatus::Missing),
            "minimum-release-age should be Missing"
        );
        assert!(recs[1].status.is_ok());
    }

    #[test]
    fn scan_pnpm_old_version_unsupported() {
        let f = tmp_file("minimum-release-age=10080\nignore-scripts=true\n");
        let recs = pnpm::scan_project(f.path(), "10.15.0");
        assert!(
            recs[0].status.is_unsupported(),
            "pnpm 10.15 should be Unsupported for minimum-release-age"
        );
        assert!(recs[1].status.is_ok(), "ignore-scripts has no version gate");
    }

    #[test]
    fn check_flat_min_int_basic() {
        let mut cfg = HashMap::new();
        cfg.insert("minimum-release-age".into(), "10080".into());
        let r = config::check_flat_min_int(
            Path::new("/tmp/existing"),
            &cfg,
            "minimum-release-age",
            10080,
            "test",
        );
        assert!(r.status.is_ok());

        let r = config::check_flat_min_int(
            Path::new("/tmp/existing"),
            &cfg,
            "minimum-release-age",
            5000,
            "test",
        );
        assert!(r.status.is_ok(), "10080 >= 5000 should be Ok");

        let r = config::check_flat_min_int(
            Path::new("/tmp/existing"),
            &cfg,
            "minimum-release-age",
            20000,
            "test",
        );
        assert!(matches!(r.status, CheckStatus::WrongValue(_)));
    }

    // ── bun tests ───────────────────────────────────────────────────

    #[test]
    fn scan_bun_checks() {
        let f = tmp_file("[install]\nminimumReleaseAge = 604800\n");
        let recs = bun::scan(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_bun_too_low() {
        let f = tmp_file("[install]\nminimumReleaseAge = 100\n");
        let recs = bun::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_bun_missing() {
        let f = tmp_file("");
        let recs = bun::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn scan_bun_invalid_value() {
        let f = tmp_file("[install]\nminimumReleaseAge = abc\n");
        let recs = bun::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    // ── uv tests ────────────────────────────────────────────────────

    #[test]
    fn scan_uv_relative_days() {
        let f = tmp_file("exclude-newer = \"7 days\"\n");
        let recs = uv::scan(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_uv_relative_weeks() {
        let f = tmp_file("exclude-newer = \"2 weeks\"\n");
        let recs = uv::scan(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_uv_relative_too_short() {
        let f = tmp_file("exclude-newer = \"3 days\"\n");
        let recs = uv::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_uv_absolute_date_ok() {
        let old_date = date::date_days_ago(30);
        let content = format!("exclude-newer = \"{old_date}\"\n");
        let f = tmp_file(&content);
        let recs = uv::scan(f.path());
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_uv_too_recent() {
        let content = "exclude-newer = \"2099-01-01\"\n";
        let f = tmp_file(content);
        let recs = uv::scan(f.path());
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_uv_missing() {
        let f = tmp_file("");
        let recs = uv::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn scan_uv_missing_file_is_not_same_as_empty_file() {
        let missing = uv::scan(Path::new("/definitely/not/a/file"));
        assert_eq!(missing[0].status.to_string(), "file missing");

        let empty = tmp_file("");
        let empty_recs = uv::scan(empty.path());
        assert_eq!(empty_recs[0].status.to_string(), "Not set");
    }

    // ── yarn tests ──────────────────────────────────────────────────

    #[test]
    fn scan_yarn_ok() {
        let f = tmp_file("npmMinimalAgeGate: \"7d\"\n");
        let recs = yarn::scan(f.path(), "4.10.0");
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_yarn_too_low() {
        let f = tmp_file("npmMinimalAgeGate: \"3d\"\n");
        let recs = yarn::scan(f.path(), "4.10.0");
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_yarn_missing() {
        let f = tmp_file("");
        let recs = yarn::scan(f.path(), "4.10.0");
        assert!(matches!(recs[0].status, CheckStatus::Missing));
    }

    #[test]
    fn scan_yarn_old_version_unsupported() {
        let f = tmp_file("");
        let recs = yarn::scan(f.path(), "4.9.2");
        assert!(recs[0].status.is_unsupported());
    }

    #[test]
    fn scan_yarn_minutes_format() {
        let f = tmp_file("npmMinimalAgeGate: 10080\n");
        let recs = yarn::scan(f.path(), "4.10.0");
        assert!(recs[0].status.is_ok());
    }

    // ── renovate tests ──────────────────────────────────────────────

    #[test]
    fn scan_renovate_ok() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"7 days\"\n}\n");
        let recs = renovate::scan(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_renovate_missing() {
        let f = tmp_file("{\n  \"extends\": [\"config:recommended\"]\n}\n");
        let recs = renovate::scan(f.path());
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_renovate_too_short() {
        let f = tmp_file("{\n  \"minimumReleaseAge\": \"2 days\"\n}\n");
        let recs = renovate::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    // ── dependabot tests ────────────────────────────────────────────

    #[test]
    fn scan_dependabot_ok() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 7\n",
        );
        let recs = dependabot::scan(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_dependabot_missing_cooldown() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n",
        );
        let recs = dependabot::scan(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].needs_fix());
    }

    #[test]
    fn scan_dependabot_too_low() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 2\n",
        );
        let recs = dependabot::scan(f.path());
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_dependabot_empty() {
        let f = tmp_file("version: 2\n");
        let recs = dependabot::scan(f.path());
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
        let recs = dependabot::scan(f.path());
        assert_eq!(recs.len(), 2);
        assert!(recs[0].status.is_ok());
        assert!(recs[1].needs_fix());
        assert_ne!(recs[0].key, recs[1].key);
        assert!(recs[0].key.contains("npm"));
        assert!(recs[1].key.contains("github-actions"));
    }

    #[test]
    fn scan_dependabot_same_ecosystem_different_dirs() {
        let f = tmp_file(concat!(
            "version: 2\nupdates:\n",
            "  - package-ecosystem: \"npm\"\n    directory: \"/\"\n",
            "    cooldown:\n      default-days: 7\n",
            "  - package-ecosystem: \"npm\"\n    directory: \"/docs\"\n",
            "    schedule:\n      interval: \"weekly\"\n",
        ));
        let recs = dependabot::scan(f.path());
        assert_eq!(recs.len(), 2);
        assert_ne!(recs[0].key, recs[1].key);
        assert!(recs[0].key.contains("/"), "key should include directory");
        assert!(
            recs[1].key.contains("/docs"),
            "key should include directory"
        );
    }

    #[test]
    fn scan_dependabot_single_ecosystem_plain_key() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    cooldown:\n      default-days: 7\n",
        );
        let recs = dependabot::scan(f.path());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].key, "cooldown.default-days");
    }

    // ── config path tests ───────────────────────────────────────────

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

    // ── Cross-platform config path tests ────────────────────────────

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
    fn pnpm_global_rc_linux() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/home/testuser");
        let p = pnpm_global_rc_for(home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(p, PathBuf::from("/home/testuser/.config/pnpm/rc"));
    }

    #[test]
    fn pnpm_global_rc_macos() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/Users/testuser");
        let p = pnpm_global_rc_for(home, TargetOs::MacOs);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(
            p,
            PathBuf::from("/Users/testuser/Library/Preferences/pnpm/rc")
        );
    }

    #[test]
    fn pnpm_global_rc_macos_uses_xdg_when_set() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        }
        let home = Path::new("/Users/testuser");
        let p = pnpm_global_rc_for(home, TargetOs::MacOs);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(p, PathBuf::from("/tmp/xdg-test/pnpm/rc"));
    }

    #[test]
    fn pnpm_config_dir_ignores_empty_xdg() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "") };
        let home = Path::new("/home/testuser");
        let p = pnpm_config_dir_for(home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(
            p,
            PathBuf::from("/home/testuser/.config/pnpm"),
            "empty XDG_CONFIG_HOME should fall back to ~/.config/pnpm"
        );
    }

    #[test]
    fn config_path_linux_uv_ignores_empty_xdg() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "") };
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(p, PathBuf::from("/home/testuser/.config/uv/uv.toml"));
    }

    #[test]
    fn config_path_linux_bun_ignores_empty_xdg() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "") };
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(p, PathBuf::from("/home/testuser/.bunfig.toml"));
    }

    #[test]
    fn pnpm_global_rc_windows() {
        let home = Path::new("C:/Users/testuser");
        let p = pnpm_global_rc_for(home, TargetOs::Windows);
        assert!(
            p.to_str().unwrap().contains("pnpm"),
            "Windows pnpm global rc should contain 'pnpm': {p:?}"
        );
    }

    #[test]
    fn config_path_linux_bun() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(p, PathBuf::from("/home/testuser/.bunfig.toml"));
    }

    #[test]
    fn config_path_linux_uv() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
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
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/Users/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::MacOs);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(p, PathBuf::from("/Users/testuser/.config/uv/uv.toml"));
    }

    #[test]
    fn config_path_macos_uv_uses_xdg_when_set() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        }
        let home = Path::new("/Users/testuser");
        let p = config_path_for(ManagerKind::Uv, home, TargetOs::MacOs);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(p, PathBuf::from("/tmp/xdg-test/uv/uv.toml"));
    }

    #[test]
    fn config_path_windows_npm() {
        let home = Path::new("C:/Users/testuser");
        let p = config_path_for(ManagerKind::Npm, home, TargetOs::Windows);
        assert_eq!(p, PathBuf::from("C:/Users/testuser/.npmrc"));
    }

    #[test]
    fn config_path_windows_uv() {
        let home = Path::new("C:/Users/testuser");
        let appdata = Path::new("C:/Users/testuser/AppData/Roaming");
        let (cands, default_idx) =
            user_config_candidates(ManagerKind::Uv, home, appdata, TargetOs::Windows);
        let paths = select_scan_paths(&cands, default_idx);
        let p = paths.into_iter().next().unwrap_or_default();
        assert_eq!(
            p,
            PathBuf::from("C:/Users/testuser/AppData/Roaming/uv/uv.toml")
        );
    }

    #[test]
    fn config_path_windows_bun() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("C:/Users/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Windows);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(p, PathBuf::from("C:/Users/testuser/.bunfig.toml"));
    }

    #[test]
    fn config_path_linux_bun_uses_xdg_when_set() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        }
        let home = Path::new("/home/testuser");
        let p = config_path_for(ManagerKind::Bun, home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        assert_eq!(p, PathBuf::from("/tmp/xdg-test/.bunfig.toml"));
    }

    #[test]
    fn target_os_current() {
        let os = TargetOs::current();
        assert!(os == TargetOs::Linux || os == TargetOs::MacOs || os == TargetOs::Windows);
    }

    // ── YAML reading tests ──────────────────────────────────────────

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

    // ── pnpm-workspace scanning tests ───────────────────────────────

    #[test]
    fn scan_pnpm_workspace_all_ok() {
        let f = tmp_file(
            "minimumReleaseAge: 10080\nblockExoticSubdeps: true\ntrustPolicy: \"no-downgrade\"\nstrictDepBuilds: true\n",
        );
        let recs = pnpm::scan_workspace(f.path(), "10.26.0");
        assert_eq!(recs.len(), 4);
        assert!(recs.iter().all(|r| r.status.is_ok()));
    }

    #[test]
    fn scan_pnpm_workspace_missing() {
        let f = tmp_file("");
        let recs = pnpm::scan_workspace(f.path(), "10.26.0");
        assert_eq!(recs.len(), 4);
        assert!(
            recs.iter()
                .filter(|r| matches!(r.status, CheckStatus::Missing))
                .count()
                == 4
        );
    }

    #[test]
    fn scan_pnpm_workspace_higher_release_age_ok() {
        let f = tmp_file("minimumReleaseAge: 10080\n");
        let recs = pnpm::scan_workspace(f.path(), "10.26.0");
        assert!(recs[0].status.is_ok());
    }

    #[test]
    fn scan_pnpm_workspace_low_release_age() {
        let f = tmp_file("minimumReleaseAge: 100\n");
        let recs = pnpm::scan_workspace(f.path(), "10.26.0");
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn scan_pnpm_workspace_old_version_block_exotic_unsupported() {
        let f = tmp_file(
            "minimumReleaseAge: 10080\ntrustPolicy: \"no-downgrade\"\nstrictDepBuilds: true\n",
        );
        let recs = pnpm::scan_workspace(f.path(), "10.25.0");
        let exotic = recs.iter().find(|r| r.key == "blockExoticSubdeps").unwrap();
        assert!(matches!(exotic.status, CheckStatus::Unsupported(_)));
        let trust = recs.iter().find(|r| r.key == "trustPolicy").unwrap();
        assert!(trust.status.is_ok());
    }

    #[test]
    fn scan_pnpm_workspace_very_old_version_all_unsupported() {
        let f = tmp_file("");
        let recs = pnpm::scan_workspace(f.path(), "10.2.0");
        assert!(
            recs.iter()
                .all(|r| matches!(r.status, CheckStatus::Unsupported(_))),
            "all settings should be unsupported on pnpm 10.2: {:?}",
            recs.iter().map(|r| (&r.key, &r.status)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn scan_pnpm_workspace_version_10_16_partial_support() {
        let f = tmp_file("minimumReleaseAge: 10080\nstrictDepBuilds: true\n");
        let recs = pnpm::scan_workspace(f.path(), "10.16.0");
        let age = recs.iter().find(|r| r.key == "minimumReleaseAge").unwrap();
        assert!(age.status.is_ok());
        let strict = recs.iter().find(|r| r.key == "strictDepBuilds").unwrap();
        assert!(strict.status.is_ok());
        let trust = recs.iter().find(|r| r.key == "trustPolicy").unwrap();
        assert!(matches!(trust.status, CheckStatus::Unsupported(_)));
        let exotic = recs.iter().find(|r| r.key == "blockExoticSubdeps").unwrap();
        assert!(matches!(exotic.status, CheckStatus::Unsupported(_)));
    }

    // ── parse_semver tests ──────────────────────────────────────────

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
        assert!(version::version_at_least("11.10.0", 11, 10));
        assert!(version::version_at_least("12.0.0", 11, 10));
        assert!(!version::version_at_least("11.9.0", 11, 10));
        assert!(!version::version_at_least("10.0.0", 11, 10));
        assert!(version::version_at_least("4.10.0", 4, 10));
        assert!(!version::version_at_least("4.9.2", 4, 10));
    }

    // ── parse_duration tests ────────────────────────────────────────

    #[test]
    fn parse_duration_days() {
        assert_eq!(date::parse_duration_minutes("7d"), Some(7 * 24 * 60));
        assert_eq!(date::parse_duration_minutes("3d"), Some(3 * 24 * 60));
        assert_eq!(date::parse_duration_minutes("\"7d\""), Some(7 * 24 * 60));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(date::parse_duration_minutes("168h"), Some(168 * 60));
        assert_eq!(date::parse_duration_minutes("48h"), Some(48 * 60));
        assert_eq!(date::parse_duration_minutes("10h"), Some(600));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(date::parse_duration_minutes("").is_none());
        assert!(date::parse_duration_minutes("abc").is_none());
    }

    // ── JSON tests ──────────────────────────────────────────────────

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

    // ── dependabot entries tests ────────────────────────────────────

    #[test]
    fn read_dependabot_single_entry_with_cooldown() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 7\n",
        );
        let entries = read_dependabot_entries(f.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ecosystem, "npm");
        assert_eq!(entries[0].directory, "/");
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
        assert_eq!(entries[0].directory, "/");
        assert_eq!(entries[0].cooldown_default_days, None);
    }

    #[test]
    fn read_dependabot_multiple_entries() {
        let f = tmp_file(
            "version: 2\nupdates:\n  - package-ecosystem: \"npm\"\n    directory: \"/\"\n    schedule:\n      interval: \"weekly\"\n    cooldown:\n      default-days: 5\n  - package-ecosystem: \"pip\"\n    directory: \"/backend\"\n    schedule:\n      interval: \"daily\"\n",
        );
        let entries = read_dependabot_entries(f.path());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].directory, "/");
        assert_eq!(entries[1].directory, "/backend");
        assert_eq!(entries[0].cooldown_default_days, Some(5));
        assert_eq!(entries[1].cooldown_default_days, None);
    }

    // ── pnpm global config path tests ───────────────────────────────

    #[test]
    fn pnpm_global_yaml_linux() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/home/testuser");
        let p = pnpm_global_yaml_for(home, TargetOs::Linux);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(p, PathBuf::from("/home/testuser/.config/pnpm/config.yaml"));
    }

    #[test]
    fn pnpm_global_yaml_macos() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let home = Path::new("/Users/testuser");
        let p = pnpm_global_yaml_for(home, TargetOs::MacOs);
        match prev {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => {}
        }
        assert_eq!(
            p,
            PathBuf::from("/Users/testuser/Library/Preferences/pnpm/config.yaml")
        );
    }

    // ── pnpm global CLI path tests ──────────────────────────────────

    #[test]
    fn pnpm_global_rc_from_cli_rejects_old_version() {
        assert_eq!(pnpm_global_rc_from_cli("10.20.0"), None);
        assert_eq!(pnpm_global_rc_from_cli("9.0.0"), None);
        assert_eq!(pnpm_global_rc_from_cli("10.0.0"), None);
    }

    // ── pnpm global scan tests (v10 rc format) ─────────────────────

    #[test]
    fn scan_pnpm_global_v10_all_ok() {
        let f = tmp_file(
            "minimum-release-age=10080\nblock-exotic-subdeps=true\ntrust-policy=no-downgrade\nstrict-dep-builds=true\nignore-scripts=true\n",
        );
        let recs = pnpm::scan_global(f.path(), "10.33.0");
        assert_eq!(recs.len(), 5);
        assert!(
            recs.iter().all(|r| r.status.is_ok()),
            "all should be Ok: {:?}",
            recs.iter().map(|r| (&r.key, &r.status)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn scan_pnpm_global_v10_missing_all() {
        let f = tmp_file("");
        let recs = pnpm::scan_global(f.path(), "10.33.0");
        assert_eq!(recs.len(), 5);
        let missing_count = recs
            .iter()
            .filter(|r| matches!(r.status, CheckStatus::Missing))
            .count();
        assert_eq!(missing_count, 5);
    }

    #[test]
    fn scan_pnpm_global_v10_old_version_partial() {
        let f = tmp_file("minimum-release-age=10080\nignore-scripts=true\n");
        let recs = pnpm::scan_global(f.path(), "10.16.0");
        let age = recs
            .iter()
            .find(|r| r.key == "minimum-release-age")
            .unwrap();
        assert!(age.status.is_ok());
        let ignore = recs.iter().find(|r| r.key == "ignore-scripts").unwrap();
        assert!(ignore.status.is_ok());
        let trust = recs.iter().find(|r| r.key == "trust-policy").unwrap();
        assert!(trust.status.is_unsupported());
        let exotic = recs
            .iter()
            .find(|r| r.key == "block-exotic-subdeps")
            .unwrap();
        assert!(exotic.status.is_unsupported());
    }

    // ── pnpm global scan tests (v11 config.yaml format) ────────────

    #[test]
    fn scan_pnpm_global_v11_all_ok() {
        let f = tmp_file("minimumReleaseAge: 10080\nblockExoticSubdeps: true\n");
        let recs = pnpm::scan_global(f.path(), "11.0.0");
        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| r.status.is_ok()));
    }

    #[test]
    fn scan_pnpm_global_v11_missing() {
        let f = tmp_file("");
        let recs = pnpm::scan_global(f.path(), "11.0.0");
        assert_eq!(recs.len(), 2, "v11 global should only have 2 settings");
        assert!(recs
            .iter()
            .all(|r| matches!(r.status, CheckStatus::Missing)));
    }

    #[test]
    fn scan_pnpm_global_v11_no_trust_or_strict() {
        let f = tmp_file("");
        let recs = pnpm::scan_global(f.path(), "11.0.0");
        assert!(
            recs.iter()
                .all(|r| r.key != "trustPolicy" && r.key != "strictDepBuilds"),
            "v11 global should not check trustPolicy or strictDepBuilds"
        );
    }

    #[test]
    fn pnpm_uses_yaml_config_versions() {
        assert!(!pnpm::uses_yaml_config("10.33.0"));
        assert!(!pnpm::uses_yaml_config("10.0.0"));
        assert!(pnpm::uses_yaml_config("11.0.0"));
        assert!(pnpm::uses_yaml_config("11.0.0-beta.8"));
        assert!(pnpm::uses_yaml_config("12.0.0"));
    }

    // ── yarn config path tests ──────────────────────────────────────

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

    // ── find_repo_configs tests ─────────────────────────────────────

    #[test]
    fn find_repo_configs_starts_from_current_dir() {
        let root = std::env::temp_dir().join(format!(
            "depsguard_search_root_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cwd = root.join("cwd");
        let outside = root.join("outside");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(cwd.join("pnpm-workspace.yaml"), "packages:\n  - .\n").unwrap();
        std::fs::write(outside.join("pnpm-workspace.yaml"), "packages:\n  - .\n").unwrap();

        let _cwd_lock = TEST_ENV_LOCK.lock().unwrap();
        let prev_cwd = std::env::current_dir().unwrap();
        let prev_home = std::env::var_os("HOME");

        unsafe {
            std::env::set_var("HOME", &root);
        }
        std::env::set_current_dir(&cwd).unwrap();

        let results = find_repo_configs(&mut |_| {});

        std::env::set_current_dir(prev_cwd).unwrap();
        match prev_home {
            Some(val) => unsafe { std::env::set_var("HOME", val) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        let _ = std::fs::remove_dir_all(&root);

        let paths: Vec<PathBuf> = results.into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].file_name().and_then(|n| n.to_str()),
            Some("pnpm-workspace.yaml")
        );
        assert!(
            !paths[0].to_string_lossy().contains("/outside/"),
            "search should stay under current_dir; got {paths:?}"
        );
    }
}
