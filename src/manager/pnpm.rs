// pnpm scanners: .npmrc (per-project), global rc/yaml, and pnpm-workspace.yaml.

use std::path::Path;

use super::config::{check_flat, check_flat_exact_int, check_yaml, read_flat_config, YamlCheck};
use super::detect::get_delay_days;
use super::types::{mark_unsupported, unsupported_if_configured, Recommendation};
use super::version::version_at_least;

/// Scan pnpm per-project .npmrc (flat INI format).
pub fn scan_project(path: &Path, version: &str) -> Vec<Recommendation> {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let cfg = read_flat_config(path);

    let release_age = check_flat_exact_int(
        path,
        &cfg,
        "minimum-release-age",
        minutes,
        &format!("Delay new versions by {days} days"),
    );
    let ignore_scripts = check_flat(
        path,
        &cfg,
        "ignore-scripts",
        "true",
        "Block malicious install scripts",
    );

    // pnpm >= 11 reads ONLY auth/registry settings from `.npmrc`; pnpm-specific
    // settings written here are silently ignored and must instead live in
    // `pnpm-workspace.yaml` (or the global `config.yaml`). Mark them Unsupported so
    // depsguard neither reports false protection nor writes a fix pnpm would ignore.
    // The redirect names the camelCase YAML key on purpose: pnpm >= 11 ignores the
    // kebab-case `.npmrc` spelling when it appears in a YAML config (verified —
    // `minimum-release-age`/`ignore-scripts` in pnpm-workspace.yaml return undefined
    // on pnpm 11), so reusing the rec's kebab key would just relocate the silent
    // failure. npm still reads `ignore-scripts` from this same file via its own scanner.
    if version_at_least(version, 11, 0) {
        let redirect = |yaml_key: &str| {
            format!("ignored in .npmrc by pnpm \u{2265} 11 — set {yaml_key} in pnpm-workspace.yaml")
        };
        return vec![
            mark_unsupported(release_age, redirect("minimumReleaseAge")),
            mark_unsupported(ignore_scripts, redirect("ignoreScripts")),
        ];
    }

    let release_age = if version_at_least(version, 10, 16) {
        release_age
    } else {
        unsupported_if_configured(release_age, "pnpm", 10, 16, version)
    };

    vec![release_age, ignore_scripts]
}

/// Whether this pnpm version uses `config.yaml` (>= 11) instead of `rc`.
pub fn uses_yaml_config(version: &str) -> bool {
    version_at_least(version, 11, 0)
}

/// Scan the pnpm global config file.
///
/// - pnpm <= 10: reads `<configDir>/rc` (INI, kebab-case) — all settings accepted.
/// - pnpm >= 11: reads `<configDir>/config.yaml` (YAML, camelCase). pnpm filters
///   this file through a global-config allowlist (`pnpmConfigFileKeys`), but every
///   hardening key we set — `minimumReleaseAge`, `blockExoticSubdeps`,
///   `trustPolicy`, `strictDepBuilds`, `ignoreScripts` — is on it and honored
///   globally. (Project-only keys like `nodeLinker`/`hoistPattern` are the ones
///   rejected from the global file.)
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
            YamlCheck::ExactInt(minutes),
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
        // ignoreScripts is a long-standing core flag (no version gate) and is on
        // pnpm's global-config allowlist, so it is honored in config.yaml.
        check_yaml(
            path,
            "ignoreScripts",
            "true",
            "Block malicious install scripts",
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
        g((10, 16)).flat_exact_int(
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
            YamlCheck::ExactInt(minutes),
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
        // ignoreScripts is a long-standing core flag (no version gate); pnpm reads
        // it from pnpm-workspace.yaml, and it is the project-level home for
        // script-blocking now that pnpm >= 11 ignores it in `.npmrc`.
        check_yaml(
            path,
            "ignoreScripts",
            "true",
            "Block malicious install scripts",
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
        let rec = check_yaml(self.path, key, expected, desc, mode);
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            rec
        } else {
            unsupported_if_configured(rec, "pnpm", self.min_ver.0, self.min_ver.1, self.version)
        }
    }

    fn flat_exact(
        &self,
        cfg: &std::collections::HashMap<String, String>,
        key: &str,
        expected: &str,
        desc: &str,
    ) -> Recommendation {
        let rec = check_flat(self.path, cfg, key, expected, desc);
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            rec
        } else {
            unsupported_if_configured(rec, "pnpm", self.min_ver.0, self.min_ver.1, self.version)
        }
    }

    fn flat_exact_int(
        &self,
        cfg: &std::collections::HashMap<String, String>,
        key: &str,
        expected: u64,
        desc: &str,
    ) -> Recommendation {
        let rec = check_flat_exact_int(self.path, cfg, key, expected, desc);
        if version_at_least(self.version, self.min_ver.0, self.min_ver.1) {
            rec
        } else {
            unsupported_if_configured(rec, "pnpm", self.min_ver.0, self.min_ver.1, self.version)
        }
    }
}
