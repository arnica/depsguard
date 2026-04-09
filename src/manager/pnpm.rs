// pnpm scanners: .npmrc (per-project), global rc/yaml, and pnpm-workspace.yaml.

use std::path::Path;

use super::config::{check_flat, check_flat_min_int, check_yaml, read_flat_config, YamlCheck};
use super::detect::get_delay_days;
use super::types::{unsupported_rec, Recommendation};
use super::version::version_at_least;

/// Scan pnpm per-project .npmrc (flat INI format).
pub fn scan_project(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let cfg = read_flat_config(path);

    let release_age = if version_at_least(version, 10, 16) {
        check_flat_min_int(
            path,
            &cfg,
            "minimum-release-age",
            minutes,
            &format!("Delay new versions by {days} days"),
        )
    } else {
        unsupported_rec(
            "minimum-release-age",
            &format!("Delay new versions by {days} days"),
            &minutes.to_string(),
            "pnpm",
            10,
            16,
            version,
        )
    };

    vec![
        release_age,
        check_flat(
            path,
            &cfg,
            "ignore-scripts",
            "true",
            "Block malicious install scripts",
        ),
    ]
}

/// Whether this pnpm version uses `config.yaml` (>= 11) instead of `rc`.
pub fn uses_yaml_config(version: &str) -> bool {
    version_at_least(version, 11, 0)
}

/// Scan the pnpm global config file.
///
/// - pnpm <= 10: reads `<configDir>/rc` (INI, kebab-case) — all settings accepted.
/// - pnpm >= 11: reads `<configDir>/config.yaml` (YAML, camelCase) — only
///   `minimumReleaseAge` and `blockExoticSubdeps` are valid globally.
pub fn scan_global(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);

    if uses_yaml_config(version) {
        scan_global_yaml(path, version, days, minutes)
    } else {
        scan_global_rc(path, version, days, minutes)
    }
}

fn scan_global_yaml(path: &Path, version: &str, days: u64, minutes: u64) -> Vec<Recommendation> {
    let g = |min_ver| VersionGate {
        path,
        version,
        min_ver,
    };
    vec![
        g((10, 16)).yaml(
            "minimumReleaseAge",
            &minutes.to_string(),
            &format!("Delay new versions by {days} days"),
            YamlCheck::MinInt(minutes),
        ),
        g((10, 26)).yaml(
            "blockExoticSubdeps",
            "true",
            "Block untrusted transitive deps",
            YamlCheck::Exact,
        ),
    ]
}

fn scan_global_rc(path: &Path, version: &str, days: u64, minutes: u64) -> Vec<Recommendation> {
    let cfg = read_flat_config(path);
    let g = |min_ver| VersionGate {
        path,
        version,
        min_ver,
    };
    vec![
        g((10, 16)).flat_min_int(
            &cfg,
            "minimum-release-age",
            minutes,
            &format!("Delay new versions by {days} days"),
        ),
        g((10, 26)).flat_exact(
            &cfg,
            "block-exotic-subdeps",
            "true",
            "Block untrusted transitive deps",
        ),
        g((10, 21)).flat_exact(
            &cfg,
            "trust-policy",
            "no-downgrade",
            "Block provenance downgrades",
        ),
        g((10, 3)).flat_exact(
            &cfg,
            "strict-dep-builds",
            "true",
            "Fail on unreviewed build scripts",
        ),
        check_flat(
            path,
            &cfg,
            "ignore-scripts",
            "true",
            "Block malicious install scripts",
        ),
    ]
}

/// Scan a pnpm-workspace.yaml file (YAML, camelCase).
pub fn scan_workspace(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let g = |min_ver| VersionGate {
        path,
        version,
        min_ver,
    };

    vec![
        g((10, 16)).yaml(
            "minimumReleaseAge",
            &minutes.to_string(),
            &format!("Delay new versions by {days} days"),
            YamlCheck::MinInt(minutes),
        ),
        g((10, 26)).yaml(
            "blockExoticSubdeps",
            "true",
            "Block untrusted transitive deps",
            YamlCheck::Exact,
        ),
        g((10, 21)).yaml(
            "trustPolicy",
            "no-downgrade",
            "Block provenance downgrades",
            YamlCheck::Exact,
        ),
        g((10, 3)).yaml(
            "strictDepBuilds",
            "true",
            "Fail on unreviewed build scripts",
            YamlCheck::Exact,
        ),
    ]
}

// ── DRY helpers for version-gated checks ────────────────────────────

/// Bundles the parameters shared by all version-gated pnpm checks.
struct VersionGate<'a> {
    path: &'a Path,
    version: &'a str,
    min_ver: (u64, u64),
}

impl VersionGate<'_> {
    fn yaml(&self, key: &str, expected: &str, desc: &str, mode: YamlCheck) -> Recommendation {
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            check_yaml(self.path, key, expected, desc, mode)
        } else {
            unsupported_rec(
                key,
                desc,
                expected,
                "pnpm",
                self.min_ver.0,
                self.min_ver.1,
                self.version,
            )
        }
    }

    fn flat_exact(
        &self,
        cfg: &std::collections::HashMap<String, String>,
        key: &str,
        expected: &str,
        desc: &str,
    ) -> Recommendation {
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            check_flat(self.path, cfg, key, expected, desc)
        } else {
            unsupported_rec(
                key,
                desc,
                expected,
                "pnpm",
                self.min_ver.0,
                self.min_ver.1,
                self.version,
            )
        }
    }

    fn flat_min_int(
        &self,
        cfg: &std::collections::HashMap<String, String>,
        key: &str,
        min: u64,
        desc: &str,
    ) -> Recommendation {
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            check_flat_min_int(self.path, cfg, key, min, desc)
        } else {
            unsupported_rec(
                key,
                desc,
                &min.to_string(),
                "pnpm",
                self.min_ver.0,
                self.min_ver.1,
                self.version,
            )
        }
    }
}
