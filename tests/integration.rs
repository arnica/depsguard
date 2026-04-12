// Integration tests: verify depsguard works with real package managers.
//
// These tests use isolated HOME directories so they never touch the real
// user config. Each test creates a temporary directory, sets HOME to it,
// runs the scan, applies fixes, and then rescans to verify.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Helpers ───────────────────────────────────────────────────────────

struct TmpHome {
    path: PathBuf,
}

impl TmpHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "depsguard_integ_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TmpHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn has_command(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check that `npm` is at least `major.minor` (e.g. 11.10 for min-release-age).
fn npm_at_least(major: u32, minor: u32) -> bool {
    Command::new("npm")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| {
            let parts: Vec<&str> = v.trim().split('.').collect();
            let m = parts.first()?.parse::<u32>().ok()?;
            let n = parts.get(1)?.parse::<u32>().ok()?;
            Some(m > major || (m == major && n >= minor))
        })
        .unwrap_or(false)
}

fn run_depsguard(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_depsguard"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("failed to run depsguard")
}

fn run_depsguard_with_env(
    args: &[&str],
    home: &Path,
    envs: &[(&str, &std::ffi::OsStr)],
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_depsguard"));
    cmd.args(args).env("HOME", home);
    for (key, val) in envs {
        cmd.env(key, val);
    }
    cmd.output().expect("failed to run depsguard")
}

fn display_under_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.display()).replace('\\', "/"),
        Err(_) => path.display().to_string().replace('\\', "/"),
    }
}

fn pnpm_globalconfig_path(home: &Path, envs: &[(&str, &std::ffi::OsStr)]) -> Option<PathBuf> {
    let mut cmd = Command::new("pnpm");
    cmd.args(["config", "get", "globalconfig"])
        .env("HOME", home);
    for (key, val) in envs {
        cmd.env(key, val);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("undefined") {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[test]
fn scan_shows_detected_managers() {
    let home = TmpHome::new("scan_detected");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "depsguard --scan failed: {stdout}");
    // Should detect at least one manager, or report none found gracefully
    let has_any = stdout.contains("npm")
        || stdout.contains("pnpm")
        || stdout.contains("bun")
        || stdout.contains("uv")
        || stdout.contains("No supported package managers found");
    assert!(has_any, "Expected package manager output:\n{stdout}");
}

#[test]
fn scan_shows_banner() {
    let home = TmpHome::new("scan_banner");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("arnica"), "Missing banner in output");
}

#[test]
fn help_flag_works() {
    let home = TmpHome::new("help");
    let out = run_depsguard(&["--help"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("depsguard scan"));
}

#[test]
fn version_flag_works() {
    let home = TmpHome::new("version");
    let out = run_depsguard(&["--version"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("depsguard"));
}

#[test]
fn scan_shows_action_needed_for_fresh_home() {
    let home = TmpHome::new("action_needed");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Fresh home = no configs = should need action, or no managers found
    assert!(
        stdout.contains("ACTION NEEDED")
            || stdout.contains("not configured")
            || stdout.contains("No supported package managers found"),
        "Expected action needed or no managers found:\n{stdout}"
    );
}

// ── npm integration ───────────────────────────────────────────────────

#[test]
fn npm_scan_detects_missing_config() {
    if !has_command("npm") {
        return;
    }
    let home = TmpHome::new("npm_missing");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("npm"), "npm not detected");
    assert!(
        stdout.contains("not configured") || stdout.contains("ACTION NEEDED"),
        "Expected missing config for npm:\n{stdout}"
    );
}

#[test]
fn npm_config_fix_and_rescan() {
    if !has_command("npm") {
        return;
    }
    let home = TmpHome::new("npm_fix");
    let npmrc = home.path().join(".npmrc");

    // Write the expected config manually (simulating what the fix would do)
    fs::write(&npmrc, "min-release-age=7\nignore-scripts=true\n").unwrap();

    let out = run_depsguard(&["--scan", "--no-search"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("npm"), "npm not detected");
    // ignore-scripts should show as configured
    assert!(
        stdout.contains("\u{2713}") && stdout.contains("ignore-scripts"),
        "Expected ignore-scripts OK after config fix:\n{stdout}"
    );
}

#[test]
fn npm_scan_distinguishes_missing_file_from_empty_file() {
    if !has_command("npm") {
        return;
    }

    let missing_home = TmpHome::new("npm_missing_file");
    let missing_out = run_depsguard(
        &[
            "--scan",
            "--no-search",
            "--exclude",
            "pnpm",
            "--exclude",
            "bun",
            "--exclude",
            "uv",
            "--exclude",
            "yarn",
        ],
        missing_home.path(),
    );
    let missing_stdout = String::from_utf8_lossy(&missing_out.stdout);
    assert!(
        missing_stdout.contains("file missing"),
        "expected missing npm config to say file missing:\n{missing_stdout}"
    );

    let empty_home = TmpHome::new("npm_empty_file");
    fs::write(empty_home.path().join(".npmrc"), "").unwrap();
    let empty_out = run_depsguard(
        &[
            "--scan",
            "--no-search",
            "--exclude",
            "pnpm",
            "--exclude",
            "bun",
            "--exclude",
            "uv",
            "--exclude",
            "yarn",
        ],
        empty_home.path(),
    );
    let empty_stdout = String::from_utf8_lossy(&empty_out.stdout);
    assert!(
        empty_stdout.contains("not set"),
        "expected empty npm config to say not set:\n{empty_stdout}"
    );
}

#[test]
#[ignore] // requires network access + npm >= 11.10
fn npm_install_with_min_release_age() {
    if !has_command("npm") || !npm_at_least(11, 10) {
        return;
    }
    let home = TmpHome::new("npm_install");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    // Set up .npmrc with min-release-age
    fs::write(
        home.path().join(".npmrc"),
        "min-release-age=7\nignore-scripts=true\n",
    )
    .unwrap();

    // Create a minimal package.json using a tiny package with no deps/scripts.
    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"picocolors":"1.1.1"}}"#,
    )
    .unwrap();

    // Install a safe, old, well-known package (not latest)
    let out = Command::new("npm")
        .args(["install", "--no-audit", "--no-fund"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    // Should succeed (picocolors 1.1.1 is old enough)
    assert!(
        out.status.success(),
        "npm install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── pnpm integration ─────────────────────────────────────────────────

#[test]
fn pnpm_scan_detects_missing_config() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_missing");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pnpm"), "pnpm not detected");
}

#[test]
fn pnpm_scan_uses_cli_globalconfig_when_npmrc_missing() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_globalconfig_missing");
    let Some(globalconfig) = pnpm_globalconfig_path(home.path(), &[]) else {
        return;
    };

    let out = run_depsguard(&["--scan", "--no-search"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected_display = display_under_home(&globalconfig, home.path());

    assert!(
        stdout.contains(&expected_display),
        "depsguard should use pnpm globalconfig path when ~/.npmrc is missing.\nexpected path: {expected_display}\noutput:\n{stdout}"
    );
    assert!(
        stdout.contains("minimum-release-age — file missing"),
        "expected pnpm minimum-release-age finding:\n{stdout}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn pnpm_scan_uses_cli_globalconfig_xdg_when_npmrc_missing() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_globalconfig_xdg_missing");
    let xdg = home.path().join("xdg");
    fs::create_dir_all(&xdg).unwrap();
    let Some(globalconfig) =
        pnpm_globalconfig_path(home.path(), &[("XDG_CONFIG_HOME", xdg.as_os_str())])
    else {
        return;
    };

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected_display = display_under_home(&globalconfig, home.path());

    assert!(
        stdout.contains(&expected_display),
        "depsguard should use pnpm globalconfig XDG path when ~/.npmrc is missing.\nexpected path: {expected_display}\noutput:\n{stdout}"
    );
    assert!(
        stdout.contains("minimum-release-age — file missing"),
        "expected pnpm minimum-release-age finding:\n{stdout}"
    );
}

#[test]
fn pnpm_config_fix_and_rescan() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_fix");
    let npmrc = home.path().join(".npmrc");

    // Write the pnpm config
    fs::write(
        &npmrc,
        "min-release-age=7\nminimum-release-age=10080\nignore-scripts=true\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pnpm"), "pnpm not detected");
}

/// Prove that pnpm READS `minimum-release-age` from `.npmrc`.
///
/// Strategy:
///   1. Set minimum-release-age to an impossibly high value (999_999_999 min ~ 1902 years).
///   2. Try `pnpm install` with a well-known old package pinned to an exact version.
///   3. pnpm must REJECT the install (no version can be that old).
///   4. Then set minimum-release-age=0 and re-run: pnpm must SUCCEED.
///
/// This proves that pnpm reads the setting from `.npmrc` and uses it.
#[test]
#[ignore] // requires network access + pnpm >= 10.16
fn pnpm_minimum_release_age_from_npmrc_blocks_install() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_mra_block");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"picocolors":"1.1.1"}}"#,
    )
    .unwrap();

    // Step 1: impossibly high minimum-release-age -> install should FAIL
    fs::write(
        home.path().join(".npmrc"),
        "minimum-release-age=999999999\nignore-scripts=true\n",
    )
    .unwrap();

    let out_blocked = Command::new("pnpm")
        .args(["install", "--no-frozen-lockfile"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    let stderr_blocked = String::from_utf8_lossy(&out_blocked.stderr);
    assert!(
        !out_blocked.status.success(),
        "pnpm install should FAIL with minimum-release-age=999999999 but succeeded.\n\
         stderr: {stderr_blocked}"
    );

    // Clean up any partial lockfile / node_modules from the failed attempt
    let _ = fs::remove_file(project.join("pnpm-lock.yaml"));
    let _ = fs::remove_dir_all(project.join("node_modules"));

    // Step 2: minimum-release-age=0 -> install should SUCCEED
    fs::write(
        home.path().join(".npmrc"),
        "minimum-release-age=0\nignore-scripts=true\n",
    )
    .unwrap();

    let out_ok = Command::new("pnpm")
        .args(["install", "--no-frozen-lockfile"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    assert!(
        out_ok.status.success(),
        "pnpm install should SUCCEED with minimum-release-age=0.\n\
         stderr: {}",
        String::from_utf8_lossy(&out_ok.stderr)
    );
}

/// Same proof for npm: `min-release-age` in `.npmrc` actually blocks installs.
#[test]
#[ignore] // requires network access + npm >= 11.10
fn npm_min_release_age_from_npmrc_blocks_install() {
    if !has_command("npm") || !npm_at_least(11, 10) {
        return;
    }
    let home = TmpHome::new("npm_mra_block");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"picocolors":"1.1.1"}}"#,
    )
    .unwrap();

    // Step 1: impossibly high min-release-age -> install should FAIL
    fs::write(
        home.path().join(".npmrc"),
        "min-release-age=999999\nignore-scripts=true\n",
    )
    .unwrap();

    let out_blocked = Command::new("npm")
        .args(["install"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    let stderr_blocked = String::from_utf8_lossy(&out_blocked.stderr);
    assert!(
        !out_blocked.status.success(),
        "npm install should FAIL with min-release-age=999999 but succeeded.\n\
         stderr: {stderr_blocked}"
    );

    // Clean up
    let _ = fs::remove_file(project.join("package-lock.json"));
    let _ = fs::remove_dir_all(project.join("node_modules"));

    // Step 2: min-release-age=0 -> install should SUCCEED
    fs::write(
        home.path().join(".npmrc"),
        "min-release-age=0\nignore-scripts=true\n",
    )
    .unwrap();

    let out_ok = Command::new("npm")
        .args(["install"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    assert!(
        out_ok.status.success(),
        "npm install should SUCCEED with min-release-age=0.\n\
         stderr: {}",
        String::from_utf8_lossy(&out_ok.stderr)
    );
}

/// Prove that depsguard detects minimum-release-age in pnpm's .npmrc correctly.
#[test]
fn pnpm_scan_detects_minimum_release_age_in_npmrc() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_mra_scan");
    fs::write(
        home.path().join(".npmrc"),
        "minimum-release-age=10080\nignore-scripts=true\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("minimum-release-age"),
        "depsguard should report minimum-release-age for pnpm:\n{stdout}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn pnpm_scan_detects_global_rc_in_library_preferences() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_global_rc");
    let rc = home.path().join("Library/Preferences/pnpm/rc");
    fs::create_dir_all(rc.parent().unwrap()).unwrap();
    fs::write(&rc, "minimum-release-age=10080\n").unwrap();

    let out = run_depsguard(&["--scan", "--no-search"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("✓ minimum-release-age — 10080")
            || stdout.contains("\u{2713} minimum-release-age — 10080"),
        "depsguard should detect pnpm global rc in Library/Preferences:\n{stdout}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn pnpm_scan_detects_global_rc_in_xdg_config_home() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_global_xdg");
    let xdg = home.path().join("xdg");
    let rc = xdg.join("pnpm/rc");
    fs::create_dir_all(rc.parent().unwrap()).unwrap();
    fs::write(&rc, "minimum-release-age=10080\n").unwrap();

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("✓ minimum-release-age — 10080")
            || stdout.contains("\u{2713} minimum-release-age — 10080"),
        "depsguard should detect pnpm global rc in XDG_CONFIG_HOME:\n{stdout}"
    );
}

// ── bun integration ──────────────────────────────────────────────────

#[test]
fn bun_scan_detects_missing_config() {
    if !has_command("bun") {
        return;
    }
    let home = TmpHome::new("bun_missing");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bun"), "bun not detected");
}

#[test]
fn bun_config_fix_and_rescan() {
    if !has_command("bun") {
        return;
    }
    let home = TmpHome::new("bun_fix");
    let bunfig = home.path().join(".bunfig.toml");

    fs::write(&bunfig, "[install]\nminimumReleaseAge = 604800\n").unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bun"), "bun not detected");
    assert!(
        stdout.contains("SECURE") || stdout.contains("OK"),
        "Expected SECURE after bun config:\n{stdout}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn bun_config_fix_and_rescan_from_xdg() {
    if !has_command("bun") {
        return;
    }
    let home = TmpHome::new("bun_xdg_fix");
    let xdg = home.path().join("xdg");
    let bunfig = xdg.join(".bunfig.toml");
    fs::create_dir_all(&xdg).unwrap();
    fs::write(&bunfig, "[install]\nminimumReleaseAge = 604800\n").unwrap();

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bun"), "bun not detected");
    assert!(
        stdout.contains("\u{2713} install.minimumReleaseAge")
            || stdout.contains("✓ install.minimumReleaseAge"),
        "Expected SECURE after bun XDG config:\n{stdout}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn bun_scan_checks_both_user_configs_when_both_exist() {
    if !has_command("bun") {
        return;
    }
    let home = TmpHome::new("bun_both_configs");
    let xdg = home.path().join("xdg");
    let xdg_bunfig = xdg.join(".bunfig.toml");
    let home_bunfig = home.path().join(".bunfig.toml");
    fs::create_dir_all(&xdg).unwrap();
    fs::write(&xdg_bunfig, "[install]\nminimumReleaseAge = 604800\n").unwrap();
    fs::write(&home_bunfig, "").unwrap();

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("~/xdg/.bunfig.toml"),
        "expected XDG bunfig path:\n{stdout}"
    );
    assert!(
        stdout.contains("~/.bunfig.toml"),
        "expected home bunfig path:\n{stdout}"
    );
    assert!(
        stdout.contains("✓ install.minimumReleaseAge — 604800")
            || stdout.contains("\u{2713} install.minimumReleaseAge — 604800"),
        "expected configured bun entry:\n{stdout}"
    );
    assert!(
        stdout.contains("install.minimumReleaseAge — not set"),
        "expected missing bun entry for the second config:\n{stdout}"
    );
}

#[test]
#[ignore] // requires network access
fn bun_install_with_config() {
    if !has_command("bun") {
        return;
    }
    let home = TmpHome::new("bun_install");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    fs::write(
        home.path().join(".bunfig.toml"),
        "[install]\nminimumReleaseAge = 604800\n",
    )
    .unwrap();

    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"picocolors":"1.1.1"}}"#,
    )
    .unwrap();

    let out = Command::new("bun")
        .args(["install"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "bun install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── uv integration ───────────────────────────────────────────────────

#[test]
fn uv_scan_detects_missing_config() {
    if !has_command("uv") {
        return;
    }
    let home = TmpHome::new("uv_missing");
    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("uv"), "uv not detected");
}

#[test]
fn uv_config_fix_and_rescan() {
    if !has_command("uv") {
        return;
    }
    let home = TmpHome::new("uv_fix");
    // uv config path differs by OS
    let uv_config = if cfg!(target_os = "macos") {
        home.path().join(".config/uv/uv.toml")
    } else if cfg!(target_os = "windows") {
        home.path().join("AppData/Roaming/uv/uv.toml")
    } else {
        home.path().join(".config/uv/uv.toml")
    };
    fs::create_dir_all(uv_config.parent().unwrap()).unwrap();

    fs::write(&uv_config, "exclude-newer = \"2024-01-01T00:00:00Z\"\n").unwrap();

    let out = run_depsguard(&["--scan", "--no-search"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("uv"), "uv not detected");
    assert!(
        stdout.contains("\u{2713}") && stdout.contains("exclude-newer"),
        "Expected exclude-newer OK after uv config:\n{stdout}"
    );
}

#[test]
fn uv_scan_distinguishes_missing_file_from_empty_file() {
    if !has_command("uv") {
        return;
    }

    let missing_home = TmpHome::new("uv_missing_file");
    let missing_out = run_depsguard(
        &[
            "--scan",
            "--no-search",
            "--exclude",
            "npm",
            "--exclude",
            "pnpm",
            "--exclude",
            "bun",
            "--exclude",
            "yarn",
        ],
        missing_home.path(),
    );
    let missing_stdout = String::from_utf8_lossy(&missing_out.stdout);
    assert!(
        missing_stdout.contains("file missing"),
        "expected missing uv config to say file missing:\n{missing_stdout}"
    );

    let empty_home = TmpHome::new("uv_empty_file");
    let uv_config = empty_home.path().join(".config/uv/uv.toml");
    fs::create_dir_all(uv_config.parent().unwrap()).unwrap();
    fs::write(&uv_config, "").unwrap();
    let empty_out = run_depsguard(
        &[
            "--scan",
            "--no-search",
            "--exclude",
            "npm",
            "--exclude",
            "pnpm",
            "--exclude",
            "bun",
            "--exclude",
            "yarn",
        ],
        empty_home.path(),
    );
    let empty_stdout = String::from_utf8_lossy(&empty_out.stdout);
    assert!(
        empty_stdout.contains("not set"),
        "expected empty uv config to say not set:\n{empty_stdout}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn uv_config_fix_and_rescan_from_xdg() {
    if !has_command("uv") {
        return;
    }
    let home = TmpHome::new("uv_xdg_fix");
    let xdg = home.path().join("xdg");
    let uv_config = xdg.join("uv/uv.toml");
    fs::create_dir_all(uv_config.parent().unwrap()).unwrap();
    fs::write(&uv_config, "exclude-newer = \"2024-01-01T00:00:00Z\"\n").unwrap();

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("uv"), "uv not detected");
    assert!(
        stdout.contains("\u{2713}") && stdout.contains("exclude-newer"),
        "Expected exclude-newer OK after uv XDG config:\n{stdout}"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn uv_scan_checks_both_user_configs_when_both_exist() {
    if !has_command("uv") {
        return;
    }
    let home = TmpHome::new("uv_both_configs");
    let xdg = home.path().join("xdg");
    let xdg_uv = xdg.join("uv/uv.toml");
    let home_uv = home.path().join(".config/uv/uv.toml");
    fs::create_dir_all(xdg_uv.parent().unwrap()).unwrap();
    fs::create_dir_all(home_uv.parent().unwrap()).unwrap();
    fs::write(&xdg_uv, "exclude-newer = \"2024-01-01T00:00:00Z\"\n").unwrap();
    fs::write(&home_uv, "").unwrap();

    let out = run_depsguard_with_env(
        &["--scan", "--no-search"],
        home.path(),
        &[("XDG_CONFIG_HOME", xdg.as_os_str())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("~/xdg/uv/uv.toml"),
        "expected XDG uv path:\n{stdout}"
    );
    assert!(
        stdout.contains("~/.config/uv/uv.toml"),
        "expected home uv path:\n{stdout}"
    );
    let is_unsupported =
        stdout.contains("ℹ exclude-newer") || stdout.contains("\u{2139} exclude-newer");
    if is_unsupported {
        assert!(
            stdout.contains("requires uv"),
            "unsupported uv should explain version requirement:\n{stdout}"
        );
    } else {
        assert!(
            stdout.contains("✓ exclude-newer —") || stdout.contains("\u{2713} exclude-newer —"),
            "expected configured uv entry:\n{stdout}"
        );
        assert!(
            stdout.contains("exclude-newer — not set"),
            "expected missing uv entry for the second config:\n{stdout}"
        );
    }
}

#[test]
#[ignore] // requires network access
fn uv_install_with_config() {
    if !has_command("uv") {
        return;
    }
    let home = TmpHome::new("uv_install");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    let uv_config = home.path().join(".config/uv/uv.toml");
    fs::create_dir_all(uv_config.parent().unwrap()).unwrap();
    fs::write(&uv_config, "exclude-newer = \"2025-01-01T00:00:00Z\"\n").unwrap();

    // Create a pyproject.toml with a safe, old dependency
    fs::write(
        project.join("pyproject.toml"),
        r#"[project]
name = "test"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = ["six==1.16.0"]
"#,
    )
    .unwrap();

    // Use uv pip install with --target to avoid needing a venv
    let out = Command::new("uv")
        .args([
            "pip",
            "install",
            "six==1.16.0",
            "--target",
            project.join("deps").to_str().unwrap(),
            "--exclude-newer",
            "2025-01-01T00:00:00Z",
        ])
        .current_dir(&project)
        .env("HOME", home.path())
        .env("UV_PYTHON_PREFERENCE", "system")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "uv pip install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── Cross-cutting integration ────────────────────────────────────────

#[test]
fn scan_all_managers_no_panic() {
    let home = TmpHome::new("all_no_panic");
    let out = run_depsguard(&["--scan"], home.path());
    assert!(out.status.success(), "depsguard should not panic");
}

#[test]
fn scan_output_is_valid_utf8() {
    let home = TmpHome::new("utf8");
    let out = run_depsguard(&["--scan"], home.path());
    // Ensure output is valid UTF-8
    String::from_utf8(out.stdout).expect("stdout should be valid UTF-8");
    String::from_utf8(out.stderr).expect("stderr should be valid UTF-8");
}

#[test]
fn multiple_scans_are_idempotent() {
    let home = TmpHome::new("idempotent");
    let out1 = run_depsguard(&["--scan"], home.path());
    let out2 = run_depsguard(&["--scan"], home.path());
    let s1 = String::from_utf8_lossy(&out1.stdout);
    let s2 = String::from_utf8_lossy(&out2.stdout);
    // Normalize date-dependent output (uv exclude-newer shows rolling date)
    // that could differ across a UTC day boundary.
    let normalize = |s: &str| -> String {
        s.lines()
            .map(|l| {
                if l.contains("currently 20") {
                    // Strip the date portion which may change at midnight
                    l.split("currently").next().unwrap_or(l).to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        normalize(&s1),
        normalize(&s2),
        "Consecutive scans should produce identical output (ignoring rolling dates)"
    );
}

#[test]
fn config_with_existing_content_is_preserved() {
    if !has_command("npm") {
        return;
    }
    let home = TmpHome::new("preserve_content");
    let npmrc = home.path().join(".npmrc");

    // Write existing config with user settings + security keys
    fs::write(
        &npmrc,
        "registry=https://registry.npmjs.org\nalways-auth=true\nignore-scripts=true\nmin-release-age=7\n",
    )
    .unwrap();

    // Scan: depsguard should see both npm-managed keys as OK while preserving
    // the user's existing registry/auth settings
    let out = run_depsguard(&["--scan", "--no-search"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // ignore-scripts should show as configured; min-release-age may be OK or unsupported
    assert!(
        stdout.contains("\u{2713}") && stdout.contains("ignore-scripts"),
        "Expected ignore-scripts OK with all keys set:\n{stdout}"
    );
    // Verify existing content was preserved
    let content = fs::read_to_string(&npmrc).unwrap();
    assert!(content.contains("registry=https://registry.npmjs.org"));
    assert!(content.contains("always-auth=true"));
}
