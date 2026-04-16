// Version detection and global settings (skip-search, delay-days, exclusions).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::types::ManagerKind;
use crate::exec::safe_command;

static SKIP_SEARCH: AtomicBool = AtomicBool::new(false);
static DELAY_DAYS_SETTING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);
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
            || ((kind == ManagerKind::PnpmWorkspace || kind == ManagerKind::PnpmGlobal)
                && e.eq_ignore_ascii_case("pnpm"))
    })
}

/// Detect the installed version of a package manager by running `<name> --version`.
///
/// Resolves `name` via an explicit `PATH` walk that rejects the current
/// working directory and any relative `PATH` entries, so a repo
/// containing e.g. `npm.cmd` cannot hijack execution.
pub fn detect_version(name: &str) -> Option<String> {
    let mut cmd = safe_command(name)?;
    let result = cmd.arg("--version").output();
    let output = match result {
        Ok(o) if o.status.success() => o,
        _ => return None,
    };
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}
