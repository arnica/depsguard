// Config path resolution for all supported package managers.

use std::env;
use std::path::{Path, PathBuf};

use super::types::{ManagerKind, TargetOs};
use super::version::version_at_least;
use crate::exec::safe_command;

/// Return the user's home directory (`$HOME` on Unix, `%USERPROFILE%` on Windows).
pub fn home_dir() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// On Windows, %APPDATA% (e.g. C:\Users\X\AppData\Roaming) differs from %USERPROFILE%.
pub(super) fn appdata_dir() -> PathBuf {
    env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join("AppData/Roaming"))
}

/// Display a path relative to the user's home directory (e.g. `~/foo/bar`).
/// Always uses forward slashes for consistency across platforms.
pub fn display_path(path: &Path) -> String {
    let home = home_dir();
    let raw = match path.strip_prefix(&home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    };
    if std::path::MAIN_SEPARATOR == '\\' {
        raw.replace('\\', "/")
    } else {
        raw
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

/// Select the user-level config paths to scan.
///
/// Rules:
/// - If exactly one candidate exists, scan it and ignore the others.
/// - If multiple candidates exist, scan all existing candidates independently.
/// - If none exist, scan only the default candidate (reported as missing).
pub fn select_scan_paths(candidates: &[PathBuf], default_idx: usize) -> Vec<PathBuf> {
    let existing: Vec<PathBuf> = candidates.iter().filter(|p| p.exists()).cloned().collect();
    if existing.is_empty() {
        vec![candidates[default_idx].clone()]
    } else {
        existing
    }
}

/// Return (candidates, default_index) for a manager's user-level config.
pub fn user_config_candidates(
    kind: ManagerKind,
    home: &Path,
    appdata: &Path,
    os: TargetOs,
) -> (Vec<PathBuf>, usize) {
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => (vec![home.join(".npmrc")], 0),
        ManagerKind::Yarn => (vec![home.join(".yarnrc.yml")], 0),
        ManagerKind::PnpmGlobal
        | ManagerKind::PnpmWorkspace
        | ManagerKind::Renovate
        | ManagerKind::Dependabot => (vec![PathBuf::new()], 0),
        ManagerKind::Bun => {
            let mut cands = Vec::new();
            let default_idx;
            if let Some(xdg) = xdg_config_home() {
                cands.push(xdg.join(".bunfig.toml"));
                cands.push(home.join(".bunfig.toml"));
                default_idx = 0;
            } else {
                cands.push(home.join(".bunfig.toml"));
                default_idx = 0;
            }
            (cands, default_idx)
        }
        ManagerKind::Uv => match os {
            TargetOs::Windows => (vec![appdata.join("uv/uv.toml")], 0),
            TargetOs::MacOs | TargetOs::Linux => {
                let mut cands = Vec::new();
                let default_idx;
                if let Some(xdg) = xdg_config_home() {
                    cands.push(xdg.join("uv/uv.toml"));
                    cands.push(home.join(".config/uv/uv.toml"));
                    default_idx = 0;
                } else {
                    cands.push(home.join(".config/uv/uv.toml"));
                    default_idx = 0;
                }
                (cands, default_idx)
            }
        },
    }
}

fn config_path_full(kind: ManagerKind, home: &Path, appdata: &Path, os: TargetOs) -> PathBuf {
    let (cands, default_idx) = user_config_candidates(kind, home, appdata, os);
    select_scan_paths(&cands, default_idx)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Resolve config path for a given OS and home directory.
#[cfg_attr(not(test), allow(dead_code))]
pub fn config_path_for(kind: ManagerKind, home: &Path, os: TargetOs) -> PathBuf {
    config_path_full(kind, home, &home.join("AppData/Roaming"), os)
}

/// Resolve the config file path for a package manager on the current OS.
pub fn config_path(kind: ManagerKind) -> PathBuf {
    let home = home_dir();
    let appdata = appdata_dir();
    config_path_full(kind, &home, &appdata, TargetOs::current())
}

// ── pnpm global config paths ────────────────────────────────────────

/// Resolve the pnpm global config directory for a given OS.
#[cfg_attr(not(test), allow(dead_code))]
pub fn pnpm_config_dir_for(home: &Path, os: TargetOs) -> PathBuf {
    if let Some(xdg) = xdg_config_home() {
        xdg.join("pnpm")
    } else {
        match os {
            TargetOs::MacOs => home.join("Library/Preferences/pnpm"),
            TargetOs::Windows => {
                if let Ok(local) = env::var("LOCALAPPDATA") {
                    PathBuf::from(local).join("pnpm/config")
                } else {
                    home.join(".config/pnpm")
                }
            }
            TargetOs::Linux => home.join(".config/pnpm"),
        }
    }
}

/// Resolve the pnpm global `rc` file (pnpm <= 10) for a given OS.
#[cfg_attr(not(test), allow(dead_code))]
pub fn pnpm_global_rc_for(home: &Path, os: TargetOs) -> PathBuf {
    pnpm_config_dir_for(home, os).join("rc")
}

/// Resolve the pnpm global `config.yaml` file (pnpm >= 11) for a given OS.
#[cfg_attr(not(test), allow(dead_code))]
pub fn pnpm_global_yaml_for(home: &Path, os: TargetOs) -> PathBuf {
    pnpm_config_dir_for(home, os).join("config.yaml")
}

/// Resolve the pnpm global `rc` file on the current OS.
pub fn pnpm_global_rc() -> PathBuf {
    pnpm_global_rc_for(&home_dir(), TargetOs::current())
}

/// Resolve the pnpm global `config.yaml` file on the current OS.
pub fn pnpm_global_yaml() -> PathBuf {
    pnpm_global_yaml_for(&home_dir(), TargetOs::current())
}

fn parse_command_path(output: &[u8]) -> Option<PathBuf> {
    let value = String::from_utf8(output.to_vec()).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("undefined") {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

pub fn pnpm_global_rc_from_cli(version: &str) -> Option<PathBuf> {
    if !version_at_least(version, 10, 21) {
        return None;
    }
    let args = ["config", "get", "globalconfig"];
    let mut cmd = safe_command("pnpm")?;
    let output = cmd
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    parse_command_path(&output.stdout)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_path_trims_and_parses() {
        assert_eq!(
            parse_command_path(b"/tmp/pnpm/rc\n"),
            Some(PathBuf::from("/tmp/pnpm/rc"))
        );
        assert_eq!(parse_command_path(b"undefined\n"), None);
        assert_eq!(parse_command_path(b"\n"), None);
    }

    #[test]
    fn select_scan_paths_none_exist_uses_default() {
        let a = PathBuf::from("/tmp/does-not-exist-a");
        let b = PathBuf::from("/tmp/does-not-exist-b");
        let paths = select_scan_paths(&[a.clone(), b], 0);
        assert_eq!(paths, vec![a]);
    }

    #[test]
    fn home_dir_returns_path() {
        let h = home_dir();
        assert!(!h.as_os_str().is_empty());
    }
}
