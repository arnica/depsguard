mod fix;
mod manager;
mod term;
mod ui;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use manager::ManagerInfo;
use term::Key;
use ui::SelectItem;

// ── CLI argument parsing ──────────────────────────────────────────────

/// The subcommand to execute.
enum Command {
    Interactive,
    ScanOnly,
    Help,
    Version,
    Restore,
}

/// Parsed CLI configuration.
struct CliConfig {
    command: Command,
    no_color: bool,
    no_workspaces: bool,
    delay_days: u64,
}

/// Errors from argument parsing, with context for display.
enum CliError {
    UnknownFlag(String),
    BadValue(String),
}

/// Parse command-line arguments into a structured configuration.
fn parse_args(args: &[String]) -> Result<CliConfig, CliError> {
    const KNOWN_FLAGS: &[&str] = &[
        "--scan",
        "-s",
        "--help",
        "-h",
        "--version",
        "-V",
        "--restore",
        "--no-color",
        "--no-workspaces",
        "--delay-days",
    ];

    let mut config = CliConfig {
        command: Command::Interactive,
        no_color: false,
        no_workspaces: false,
        delay_days: 7,
    };

    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--no-color" => config.no_color = true,
            "--no-workspaces" => config.no_workspaces = true,
            "--delay-days" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| CliError::BadValue("--delay-days requires a value".into()))?;
                config.delay_days =
                    val.parse::<u64>().ok().filter(|&d| d > 0).ok_or_else(|| {
                        CliError::BadValue("--delay-days requires a positive number".into())
                    })?;
            }
            "--scan" | "-s" => config.command = Command::ScanOnly,
            "--help" | "-h" => config.command = Command::Help,
            "--version" | "-V" => config.command = Command::Version,
            "--restore" => config.command = Command::Restore,
            _ if arg.starts_with('-') && !KNOWN_FLAGS.contains(&arg) => {
                return Err(CliError::UnknownFlag(format!(
                    "unrecognized option '{arg}'"
                )));
            }
            _ => {}
        }
        i += 1;
    }

    Ok(config)
}

fn print_error(msg: &str) {
    eprintln!("{}{}Error:{} {msg}", term::RED, term::BOLD, term::RESET);
}

fn progress_callback(label: &str, _frac: f32) {
    let stdout = io::stdout();
    let mut w = term::ColorWriter::new(stdout.lock());
    ui::print_progress(&mut w, label).ok();
}

// ── Entry point ───────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = match parse_args(&args) {
        Ok(c) => c,
        Err(CliError::UnknownFlag(msg)) => {
            print_error(&msg);
            eprintln!();
            print_usage_short();
            std::process::exit(1);
        }
        Err(CliError::BadValue(msg)) => {
            print_error(&msg);
            std::process::exit(1);
        }
    };

    if config.no_color || !term::should_use_colors() {
        term::disable_colors();
    }
    if config.no_workspaces {
        manager::set_skip_workspaces(true);
    }
    manager::set_delay_days(config.delay_days);

    match config.command {
        Command::ScanOnly => run_scan_only(),
        Command::Help => print_usage(),
        Command::Version => println!("depsguard {}", env!("CARGO_PKG_VERSION")),
        Command::Restore => run_restore(),
        Command::Interactive => {
            if let Err(e) = run_interactive() {
                print_error(&e.to_string());
                std::process::exit(1);
            }
        }
    }
}

fn print_usage() {
    let stdout = io::stdout();
    let mut out = term::ColorWriter::new(stdout.lock());
    ui::print_banner(&mut out).ok();
    print_usage_short();
}

fn print_usage_short() {
    println!("  USAGE:");
    println!("    depsguard                  Interactive mode (TUI)");
    println!("    depsguard --scan           Scan only, no changes");
    println!("    depsguard --help           Show this help");
    println!("    depsguard --version        Show version");
    println!("    depsguard --restore        Restore config files from backup");
    println!("    depsguard --no-color       Disable colored output");
    println!("    depsguard --no-workspaces  Skip pnpm-workspace.yaml search");
    println!("    depsguard --delay-days N   Set release delay (default: 7)");
    println!();
}

fn run_scan_only() {
    let stdout = io::stdout();
    let mut out = term::ColorWriter::new(stdout.lock());
    ui::print_banner(&mut out).ok();
    let managers = manager::scan_all_with_progress(progress_callback);
    ui::clear_progress(&mut out).ok();
    writeln!(out).ok();
    ui::print_scan_results(&mut out, &managers).ok();
}

fn run_interactive() -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = term::ColorWriter::new(stdout.lock());

    loop {
        // Phase 1: Scan
        term::clear_screen(&mut out)?;
        ui::print_banner(&mut out)?;
        let managers = manager::scan_all_with_progress(progress_callback);
        ui::clear_progress(&mut out)?;
        writeln!(out)?;
        ui::print_scan_results(&mut out, &managers)?;

        // Build fixable items
        let mut items = ui::build_fix_items(&managers);
        if items.is_empty() {
            writeln!(
                out,
                "  {}{}All package managers are properly configured!{}",
                term::BOLD,
                term::GREEN,
                term::RESET
            )?;
            return Ok(());
        }

        // Phase 2: Interactive selection
        writeln!(
            out,
            "  {}Press any key to enter selection mode (q to quit)...{}",
            term::DIM,
            term::RESET
        )?;
        out.flush()?;

        {
            let _raw = term::RawMode::enable()?;
            let _cursor = term::CursorGuard; // restores cursor on drop (even on error unwind)
            let key = term::read_key()?;
            if matches!(key, Key::Char('q') | Key::Escape) {
                return Ok(());
            }

            let go_back = selection_loop(&mut out, &mut items, &managers)?;

            if !go_back {
                return Ok(());
            }
            // _raw and _cursor drop here, restoring terminal state
        }
        // go_back == true: loop back to scan results
    }
}

/// Returns Ok(true) if user pressed Escape (go back), Ok(false) otherwise.
fn selection_loop(
    out: &mut impl Write,
    items: &mut [SelectItem],
    managers: &[ManagerInfo],
) -> io::Result<bool> {
    let mut cursor: usize = 0;

    loop {
        term::clear_screen(out)?;
        term::hide_cursor(out)?;
        ui::print_banner(out)?;
        ui::print_selector(out, items, cursor)?;
        out.flush()?;

        match term::read_key()? {
            Key::Up => {
                cursor = cursor.saturating_sub(1);
            }
            Key::Down => {
                if cursor + 1 < items.len() {
                    cursor += 1;
                }
            }
            Key::Space => {
                items[cursor].selected = !items[cursor].selected;
            }
            Key::Enter => {
                let results = apply_selected(items, managers);
                // Show errors inline before rescanning
                let errors: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
                if !errors.is_empty() {
                    for (label, result) in &errors {
                        if let Err(e) = result {
                            writeln!(out, "  {}Error:{} {label}: {e}", term::RED, term::RESET)?;
                        }
                    }
                    out.flush()?;
                }
                return Ok(true); // loop back to main scan screen
            }
            Key::Char('d') => {
                // Show diff preview of selected changes
                term::clear_screen(out)?;
                ui::print_banner(out)?;
                ui::print_diff_preview(out, items, managers)?;
                out.flush()?;
                term::read_key()?; // wait for any key to go back
            }
            Key::Char('q') => {
                return Ok(false);
            }
            Key::Escape => {
                return Ok(true);
            }
            _ => {}
        }
    }
}

fn apply_selected(
    items: &[SelectItem],
    managers: &[ManagerInfo],
) -> Vec<(String, Result<String, String>)> {
    let mut backed_up = std::collections::HashSet::new();
    items
        .iter()
        .filter(|item| item.selected)
        .map(|item| {
            let mgr = &managers[item.manager_idx];
            let rec = &mgr.recommendations[item.rec_idx];
            // Backup before first modification to each file
            if let Err(e) = fix::backup_file(&mgr.config_path, &mut backed_up) {
                return (item.label.clone(), Err(format!("backup failed: {e}")));
            }
            let result = fix::apply_fix(mgr.kind, &mgr.config_path, rec).map_err(|e| e.to_string());
            (item.label.clone(), result)
        })
        .collect()
}

fn run_restore() {
    let stdout = io::stdout();
    let mut out = term::ColorWriter::new(stdout.lock());
    ui::print_banner(&mut out).ok();

    let backups = fix::list_backups_with_progress(&mut |label| {
        let stdout = io::stdout();
        let mut w = term::ColorWriter::new(stdout.lock());
        ui::print_progress(&mut w, label).ok();
    });
    ui::clear_progress(&mut out).ok();

    if backups.is_empty() {
        writeln!(
            out,
            "  {}{}No backups found. Nothing to restore.{}",
            term::BOLD,
            term::YELLOW,
            term::RESET
        )
        .ok();
        return;
    }

    writeln!(
        out,
        "  {}{}Available backups:{} {}(newest first){}",
        term::BOLD,
        term::CYAN,
        term::RESET,
        term::DIM,
        term::RESET,
    )
    .ok();

    // Group backups by original path for cleaner display
    let mut last_original: Option<&std::path::Path> = None;
    for (i, (original, backup)) in backups.iter().enumerate() {
        if last_original != Some(original.as_path()) {
            let display = manager::display_path(original);
            writeln!(out, "\n    {}{}{}:", term::BOLD, display, term::RESET).ok();
            last_original = Some(original.as_path());
        }
        let bak_name = backup.file_name().unwrap_or_default().to_string_lossy();
        // Extract timestamp from backup name
        let ts = bak_name
            .rsplit('.')
            .nth(1) // second from end (before "bak")
            .unwrap_or("?");
        writeln!(
            out,
            "      {}[{}]{} {}{}{}",
            term::BOLD,
            i + 1,
            term::RESET,
            term::DIM,
            ts,
            term::RESET,
        )
        .ok();
    }
    writeln!(out).ok();

    // Prompt with "latest" shortcut
    write!(
        out,
        "  Select (1-{}, '{}latest{}' to restore all newest, '{}q{}' to cancel): ",
        backups.len(),
        term::BOLD,
        term::RESET,
        term::BOLD,
        term::RESET,
    )
    .ok();
    out.flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return;
    }
    let input = input.trim();
    if input.eq_ignore_ascii_case("q") || input.is_empty() {
        writeln!(out, "  Cancelled.").ok();
        return;
    }

    if input.eq_ignore_ascii_case("latest") || input == "l" {
        // Restore the newest backup for each unique original path
        restore_latest(&backups, &mut out);
        return;
    }

    let idx: usize = match input.parse::<usize>() {
        Ok(n) if n >= 1 && n <= backups.len() => n - 1,
        _ => {
            writeln!(
                out,
                "  {}{}Invalid selection.{}",
                term::RED,
                term::BOLD,
                term::RESET
            )
            .ok();
            return;
        }
    };

    restore_one(&backups[idx].1, &backups[idx].0, &mut out);
    println!();
}

/// Restore the newest backup for each unique original path.
fn restore_latest(backups: &[(PathBuf, PathBuf)], out: &mut impl Write) {
    // Backups are sorted newest-first, so first occurrence per original is the latest
    let mut restored = std::collections::HashSet::new();
    for (original, backup) in backups {
        if restored.insert(original.clone()) {
            restore_one(backup, original, out);
        }
    }
    writeln!(out).ok();
}

fn restore_one(backup: &Path, original: &Path, out: &mut impl Write) {
    let display = manager::display_path(original);
    match fix::restore_backup(backup, original) {
        Ok(()) => {
            writeln!(out, "  {}✓{} Restored {display}", term::GREEN, term::RESET).ok();
            let _ = std::fs::remove_file(backup);
        }
        Err(e) => {
            writeln!(
                out,
                "  {}✗{} Failed to restore {display}: {e}",
                term::RED,
                term::RESET,
            )
            .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{CheckStatus, ManagerKind, Recommendation};
    use std::path::PathBuf;

    #[test]
    fn apply_selected_filters_unselected() {
        let managers = vec![ManagerInfo {
            kind: ManagerKind::Npm,
            version: "10.0".into(),
            config_path: PathBuf::from("/tmp/test_npmrc"),
            recommendations: vec![Recommendation {
                key: "ignore-scripts".into(),
                description: "test".into(),
                expected: "true".into(),
                status: CheckStatus::Missing,
            }],
        }];
        let items = vec![SelectItem {
            manager_idx: 0,
            rec_idx: 0,
            label: "fix A".into(),
            group_path: "~/.npmrc".into(),
            group_header: "npm".into(),
            selected: false,
        }];
        let results = apply_selected(&items, &managers);
        assert!(results.is_empty());
    }

    #[test]
    fn apply_selected_applies_selected() {
        let path = std::env::temp_dir().join(format!("depsguard_main_test_{}", std::process::id()));
        std::fs::write(&path, "").unwrap();

        let managers = vec![ManagerInfo {
            kind: ManagerKind::Npm,
            version: "10.0".into(),
            config_path: path.clone(),
            recommendations: vec![Recommendation {
                key: "ignore-scripts".into(),
                description: "test".into(),
                expected: "true".into(),
                status: CheckStatus::Missing,
            }],
        }];
        let items = vec![SelectItem {
            manager_idx: 0,
            rec_idx: 0,
            label: "fix".into(),
            group_path: "~/.npmrc".into(),
            group_header: "npm".into(),
            selected: true,
        }];
        let results = apply_selected(&items, &managers);
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
        // Clean up: remove original and any .bak files
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                let prefix = path.file_name().unwrap().to_string_lossy().to_string();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&prefix) && name.ends_with(".bak") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    #[test]
    fn selection_loop_quit() {
        // We can't easily test the interactive loop without a real terminal,
        // but we test the helper functions it uses.
        let managers = vec![ManagerInfo {
            kind: ManagerKind::Npm,
            version: "10.0".into(),
            config_path: PathBuf::from("/tmp/test"),
            recommendations: vec![Recommendation {
                key: "k".into(),
                description: "d".into(),
                expected: "v".into(),
                status: CheckStatus::Missing,
            }],
        }];
        let items = ui::build_fix_items(&managers);
        assert_eq!(items.len(), 1);
    }
}
