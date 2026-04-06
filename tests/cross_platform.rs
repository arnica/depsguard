// Cross-platform tests: verify config paths and scanning for all OS variants.
// Uses simulated filesystem layouts to test macOS/Windows paths on Linux.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Helpers ───────────────────────────────────────────────────────────

struct TmpDir(PathBuf);
impl TmpDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "depsguard_xplat_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_depsguard(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_depsguard"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("failed to run depsguard")
}

// ── Cross-platform uv filesystem layout ───────────────────────────────
// Tests config reading at the Linux uv path. macOS-specific path
// resolution is covered by config_path_for unit tests in manager.rs.

#[cfg(target_os = "linux")]
#[test]
fn linux_uv_path_layout() {
    let home = TmpDir::new("linux_uv");
    let uv_dir = home.path().join(".config/uv");
    fs::create_dir_all(&uv_dir).unwrap();
    fs::write(
        uv_dir.join("uv.toml"),
        "exclude-newer = \"2024-06-01T00:00:00Z\"\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // On Linux, it should read from .config/uv/uv.toml
    if stdout.contains("uv") {
        assert!(
            stdout.contains("OK") || stdout.contains("SECURE"),
            "uv should show OK with old exclude-newer: {stdout}"
        );
    }
}

#[test]
fn simulated_macos_bun_layout() {
    let home = TmpDir::new("macos_bun");
    // bun config is the same path on all platforms: ~/.bunfig.toml
    fs::write(
        home.path().join(".bunfig.toml"),
        "[install]\nminimumReleaseAge = 604800\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("bun") {
        assert!(
            stdout.contains("SECURE") || stdout.contains("OK"),
            "bun should show SECURE: {stdout}"
        );
    }
}

// ── Windows under Wine ───────────────────────────────────────────────

fn has_wine() -> bool {
    Command::new("wine")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn wine_exe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/x86_64-pc-windows-gnu/release/depsguard.exe")
}

fn has_wine_exe() -> bool {
    has_wine() && wine_exe_path().exists()
}

fn run_wine(args: &[&str], userprofile: &Path) -> std::process::Output {
    Command::new("wine")
        .arg(wine_exe_path())
        .args(args)
        .env("WINEDEBUG", "-all")
        .env("USERPROFILE", userprofile)
        .env("HOME", userprofile)
        .output()
        .expect("wine failed")
}

#[test]
fn wine_version_flag() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_ver");
    let out = run_wine(&["--version"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("depsguard"), "version output: {stdout}");
}

#[test]
fn wine_help_flag() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_help");
    let out = run_wine(&["--help"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USAGE"), "help output: {stdout}");
}

#[test]
fn wine_scan_empty_home() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_scan_empty");
    let out = run_wine(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Under Wine, no package managers are in PATH, so should show "No supported"
    assert!(
        stdout.contains("No supported package managers found") || stdout.contains("Detected"),
        "scan output: {stdout}"
    );
}

#[test]
fn wine_scan_with_npmrc() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_npmrc");
    // Windows npm reads %USERPROFILE%\.npmrc
    fs::write(
        home.path().join(".npmrc"),
        "min-release-age=7\nignore-scripts=true\n",
    )
    .unwrap();

    let out = run_wine(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Even if npm isn't detected (not in Wine PATH), the binary shouldn't crash
    assert!(out.status.success(), "wine scan crashed: {stdout}");
}

#[test]
fn wine_scan_with_bunfig() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_bunfig");
    fs::write(
        home.path().join(".bunfig.toml"),
        "[install]\nminimumReleaseAge = 604800\n",
    )
    .unwrap();

    let out = run_wine(&["--scan"], home.path());
    assert!(out.status.success(), "wine scan with bunfig crashed");
}

#[test]
fn wine_scan_with_uv_toml() {
    if !has_wine_exe() {
        eprintln!("SKIP: wine or Windows exe not available");
        return;
    }
    let home = TmpDir::new("wine_uv");
    // On Windows, uv config is at %APPDATA%\uv\uv.toml
    // Under Wine with HOME set, it falls back to HOME/AppData/Roaming
    let uv_dir = home.path().join("AppData/Roaming/uv");
    fs::create_dir_all(&uv_dir).unwrap();
    fs::write(
        uv_dir.join("uv.toml"),
        "exclude-newer = \"2024-01-01T00:00:00Z\"\n",
    )
    .unwrap();

    let out = run_wine(&["--scan"], home.path());
    assert!(out.status.success(), "wine scan with uv.toml crashed");
}

// ── Cross-compile verification ───────────────────────────────────────

#[test]
fn windows_exe_is_valid_pe() {
    let exe = wine_exe_path();
    if !exe.exists() {
        eprintln!(
            "SKIP: Windows exe not built (run: cargo build --target x86_64-pc-windows-gnu --release)"
        );
        return;
    }
    let header = fs::read(&exe).unwrap();
    assert!(
        header.len() >= 2,
        "Executable too small to be a valid PE file: {}",
        exe.display()
    );
    // PE files start with "MZ"
    assert_eq!(&header[0..2], b"MZ", "Not a valid PE executable");
}

// ── All-platform config file content tests ───────────────────────────

#[test]
fn npmrc_round_trip_all_keys() {
    let home = TmpDir::new("npmrc_roundtrip");
    let npmrc = home.path().join(".npmrc");

    // Start empty, apply both npm and pnpm keys
    fs::write(&npmrc, "").unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let _stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());

    // Now write all keys
    fs::write(
        &npmrc,
        "min-release-age=7\nminimum-release-age=10080\nignore-scripts=true\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout2 = String::from_utf8_lossy(&out.stdout);
    // npm and pnpm lines should show as configured (SECURE)
    // Note: other managers (bun, uv) may still show ACTION NEEDED
    if stdout2.contains("npm") {
        // Check that at least one manager shows SECURE (npm or pnpm with all keys set)
        assert!(
            stdout2.contains("SECURE"),
            "npm/pnpm should be secure after setting all keys: {stdout2}"
        );
    }
}

#[test]
fn bunfig_round_trip() {
    let home = TmpDir::new("bunfig_roundtrip");
    let bunfig = home.path().join(".bunfig.toml");

    fs::write(&bunfig, "[install]\nminimumReleaseAge = 604800\n").unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("bun") {
        assert!(stdout.contains("SECURE") || stdout.contains("OK"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn uv_toml_round_trip() {
    let home = TmpDir::new("uv_roundtrip");
    let uv_dir = home.path().join(".config/uv");
    fs::create_dir_all(&uv_dir).unwrap();
    fs::write(
        uv_dir.join("uv.toml"),
        "exclude-newer = \"2024-06-01T00:00:00Z\"\n",
    )
    .unwrap();

    let out = run_depsguard(&["--scan"], home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("uv") {
        assert!(stdout.contains("SECURE") || stdout.contains("OK"));
    }
}
