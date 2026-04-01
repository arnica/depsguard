mod fix;
mod manager;
mod term;
mod ui;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use manager::ManagerInfo;
use term::Key;
use ui::SelectItem;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --no-color flag or auto-detect
    if args.iter().any(|a| a == "--no-color") || !term::should_use_colors() {
        term::disable_colors();
    }
    if args.iter().any(|a| a == "--no-workspaces") {
        manager::set_skip_workspaces(true);
    }
    if let Some(pos) = args.iter().position(|a| a == "--delay-days") {
        if let Some(val) = args.get(pos + 1) {
            match val.parse::<u64>() {
                Ok(d) if d > 0 => manager::set_delay_days(d),
                _ => {
                    eprintln!(
                        "{}{}Error:{} --delay-days requires a positive number",
                        term::RED,
                        term::BOLD,
                        term::RESET
                    );
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!(
                "{}{}Error:{} --delay-days requires a value",
                term::RED,
                term::BOLD,
                term::RESET
            );
            std::process::exit(1);
        }
    }

    if args.iter().any(|a| a == "--scan" || a == "-s") {
        run_scan_only();
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("depsguard {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--restore") {
        run_restore();
        return;
    }

    // Check for unrecognized flags
    let known = &[
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
    let mut skip_next = false;
    for arg in &args[1..] {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--delay-days" {
            skip_next = true;
            continue;
        }
        if arg.starts_with('-') && !known.contains(&arg.as_str()) {
            eprintln!(
                "{}{}Error:{} unrecognized option '{arg}'",
                term::RED,
                term::BOLD,
                term::RESET
            );
            eprintln!();
            print_usage_short();
            std::process::exit(1);
        }
    }

    if let Err(e) = run_interactive() {
        eprintln!("{}{}Error:{} {e}", term::RED, term::BOLD, term::RESET);
        std::process::exit(1);
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
    let managers = manager::scan_all_with_progress(|label, frac| {
        let stdout = io::stdout();
        let mut w = term::ColorWriter::new(stdout.lock());
        ui::print_progress(&mut w, label, frac).ok();
    });
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
        let managers = manager::scan_all_with_progress(|label, frac| {
            let stdout = io::stdout();
            let mut w = term::ColorWriter::new(stdout.lock());
            ui::print_progress(&mut w, label, frac).ok();
        });
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

    writeln!(
        out,
        "  {}Searching for backup files (including pnpm workspaces)...{}",
        term::DIM,
        term::RESET
    )
    .ok();
    out.flush().ok();

    let backups = fix::list_backups();
    // Clear the searching message
    write!(out, "\r\x1b[K").ok();

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
            let display = ui::display_path(original);
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
    let display = ui::display_path(original);
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
