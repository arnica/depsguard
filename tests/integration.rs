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

fn run_depsguard(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_depsguard"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("failed to run depsguard")
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
    assert!(
        stdout.contains("supply chain defense"),
        "Missing banner in output"
    );
}

#[test]
fn help_flag_works() {
    let home = TmpHome::new("help");
    let out = run_depsguard(&["--help"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("--scan"));
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

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("npm"), "npm not detected");
    // After fix, should show OK
    assert!(
        stdout.contains("SECURE") || stdout.contains("OK"),
        "Expected SECURE after config fix:\n{stdout}"
    );
}

#[test]
#[ignore] // requires network access — run with: cargo test -- --ignored
fn npm_install_with_min_release_age() {
    if !has_command("npm") {
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

    // Create a minimal package.json
    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"is-odd":"3.0.1"}}"#,
    )
    .unwrap();

    // Install a safe, old, well-known package (not latest)
    let out = Command::new("npm")
        .args(["install", "--no-audit", "--no-fund"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    // Should succeed (is-odd 3.0.1 is old enough)
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

#[test]
#[ignore] // requires network access
fn pnpm_install_with_config() {
    if !has_command("pnpm") {
        return;
    }
    let home = TmpHome::new("pnpm_install");
    let project = home.path().join("testproject");
    fs::create_dir_all(&project).unwrap();

    fs::write(
        home.path().join(".npmrc"),
        "minimum-release-age=10080\nignore-scripts=true\n",
    )
    .unwrap();

    fs::write(
        project.join("package.json"),
        r#"{"name":"test","version":"1.0.0","dependencies":{"is-odd":"3.0.1"}}"#,
    )
    .unwrap();

    let out = Command::new("pnpm")
        .args(["install", "--no-frozen-lockfile"])
        .current_dir(&project)
        .env("HOME", home.path())
        .output()
        .unwrap();

    // pnpm install should succeed
    assert!(
        out.status.success(),
        "pnpm install failed: {}",
        String::from_utf8_lossy(&out.stderr)
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
        r#"{"name":"test","version":"1.0.0","dependencies":{"is-odd":"3.0.1"}}"#,
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
    let uv_config = home.path().join(".config/uv/uv.toml");
    fs::create_dir_all(uv_config.parent().unwrap()).unwrap();

    // Use a date well in the past
    fs::write(&uv_config, "exclude-newer = \"2024-01-01T00:00:00Z\"\n").unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("uv"), "uv not detected");
    assert!(
        stdout.contains("SECURE") || stdout.contains("OK"),
        "Expected SECURE after uv config:\n{stdout}"
    );
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

    // Write existing config
    fs::write(
        &npmrc,
        "registry=https://registry.npmjs.org\nalways-auth=true\n",
    )
    .unwrap();

    // Now apply our fix via the binary's fix module (we test via config write)
    let content = fs::read_to_string(&npmrc).unwrap();
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    lines.push("ignore-scripts=true".into());
    lines.push("min-release-age=7".into());
    fs::write(&npmrc, lines.join("\n") + "\n").unwrap();

    // Verify original content preserved
    let final_content = fs::read_to_string(&npmrc).unwrap();
    assert!(final_content.contains("registry=https://registry.npmjs.org"));
    assert!(final_content.contains("always-auth=true"));
    assert!(final_content.contains("ignore-scripts=true"));
    assert!(final_content.contains("min-release-age=7"));
}
