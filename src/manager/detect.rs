// Version detection and global settings (skip-search, delay-days, exclusions).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::types::ManagerKind;

static SKIP_SEARCH: AtomicBool = AtomicBool::new(false);
static DELAY_DAYS_SETTING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);
static EXCLUDED_MANAGERS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ONLY_MANAGERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

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

/// Set the allow-list of manager names for `--only` (scan mode only).
/// When non-empty, only the named managers are scanned.
pub fn set_only_managers(names: Vec<String>) {
    *ONLY_MANAGERS.lock().unwrap_or_else(|e| e.into_inner()) = names;
}

/// Returns `true` when `needle` matches the manager's canonical name, accounting
/// for the pnpm variant alias: "pnpm" matches `PnpmGlobal` and `PnpmWorkspace`.
fn name_matches(needle: &str, kind: ManagerKind) -> bool {
    needle.eq_ignore_ascii_case(kind.name())
        || ((kind == ManagerKind::PnpmWorkspace || kind == ManagerKind::PnpmGlobal)
            && needle.eq_ignore_ascii_case("pnpm"))
}

/// Check whether a manager kind should be skipped.
///
/// A manager is skipped when either:
/// - it is in the explicit `--exclude` list, OR
/// - an `--only` allow-list is set and the manager is not in it.
pub fn is_excluded(kind: ManagerKind) -> bool {
    let excluded = EXCLUDED_MANAGERS.lock().unwrap_or_else(|e| e.into_inner());
    if excluded.iter().any(|e| name_matches(e, kind)) {
        return true;
    }
    drop(excluded);

    let only = ONLY_MANAGERS.lock().unwrap_or_else(|e| e.into_inner());
    if !only.is_empty() && !only.iter().any(|o| name_matches(o, kind)) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::types::ManagerKind;

    // ── --only tests ─────────────────────────────────────────────────

    #[test]
    fn only_list_restricts_to_listed_managers() {
        set_only_managers(vec!["npm".into()]);
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(is_excluded(ManagerKind::Pnpm));
        assert!(is_excluded(ManagerKind::Bun));
        set_only_managers(vec![]);
    }

    #[test]
    fn only_list_pnpm_cascades_to_variants() {
        set_only_managers(vec!["pnpm".into()]);
        assert!(!is_excluded(ManagerKind::Pnpm));
        assert!(!is_excluded(ManagerKind::PnpmGlobal));
        assert!(!is_excluded(ManagerKind::PnpmWorkspace));
        assert!(is_excluded(ManagerKind::Npm));
        set_only_managers(vec![]);
    }

    #[test]
    fn only_list_case_insensitive() {
        set_only_managers(vec!["NPM".into()]);
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(is_excluded(ManagerKind::Pnpm));
        set_only_managers(vec![]);
    }

    #[test]
    fn empty_only_list_excludes_nothing() {
        set_only_managers(vec![]);
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(!is_excluded(ManagerKind::Pnpm));
        assert!(!is_excluded(ManagerKind::Bun));
    }

    #[test]
    fn only_and_exclude_interaction_exclude_wins() {
        // --only npm --exclude npm → excluded (exclude takes priority)
        set_only_managers(vec!["npm".into()]);
        set_excluded_managers(vec!["npm".into()]);
        assert!(is_excluded(ManagerKind::Npm));
        set_only_managers(vec![]);
        set_excluded_managers(vec![]);
    }

    #[test]
    fn only_multiple_managers() {
        set_only_managers(vec!["npm".into(), "bun".into()]);
        assert!(!is_excluded(ManagerKind::Npm));
        assert!(!is_excluded(ManagerKind::Bun));
        assert!(is_excluded(ManagerKind::Pnpm));
        assert!(is_excluded(ManagerKind::Uv));
        set_only_managers(vec![]);
    }
}

/// Detect the installed version of a package manager by running `<name> --version`.
pub fn detect_version(name: &str) -> Option<String> {
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
