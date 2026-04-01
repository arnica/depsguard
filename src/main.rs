mod fix;
mod manager;
mod term;
mod ui;

use std::io::{self, Write};

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

    if let Err(e) = run_interactive() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn print_usage() {
    let stdout = io::stdout();
    let mut out = term::ColorWriter::new(stdout.lock());
    ui::print_banner(&mut out).ok();
    println!("  Harden your package manager configs against supply chain attacks.\n");
    println!("  USAGE:");
    println!("    depsguard            Interactive mode (TUI)");
    println!("    depsguard --scan     Scan only, no changes");
    println!("    depsguard --help     Show this help");
    println!("    depsguard --version  Show version");
    println!("    depsguard --restore  Restore config files from backup");
    println!("    depsguard --no-color      Disable colored output");
    println!("    depsguard --no-workspaces  Skip pnpm-workspace.yaml search\n");
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
            let key = term::read_key()?;
            if matches!(key, Key::Char('q') | Key::Escape) {
                drop(_raw);
                term::show_cursor(&mut out)?;
                out.flush()?;
                return Ok(());
            }

            let go_back = selection_loop(&mut out, &mut items, &managers)?;

            // Raw mode drops here, restoring terminal
            drop(_raw);
            term::show_cursor(&mut out)?;
            out.flush()?;

            if !go_back {
                return Ok(());
            }
        }
        // go_back == true: loop back to scan results
    }
}

/// Returns Ok(true) if user pressed Escape (go back), Ok(false) otherwise.
fn selection_loop(
    out: &mut impl Write,
    items: &mut Vec<SelectItem>,
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
                if cursor > 0 {
                    cursor -= 1;
                }
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
                apply_selected(items, managers);
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

    let backups = fix::list_backups();
    if backups.is_empty() {
        writeln!(
            out,
            "  {}{}No backups found. Nothing to restore.{}",
            term::BOLD,
            term::YELLOW,
            term::RESET
        ).ok();
        return;
    }

    writeln!(
        out,
        "  {}{}Restoring {} file(s) from backup:{}",
        term::BOLD,
        term::CYAN,
        backups.len(),
        term::RESET
    ).ok();
    for (original, _) in &backups {
        writeln!(out, "    {}{}{}", term::DIM, original.display(), term::RESET).ok();
    }
    writeln!(out).ok();

    let results = fix::restore_all();
    for (path, result) in &results {
        match result {
            Ok(()) => writeln!(
                out,
                "  {}✓{} Restored {}",
                term::GREEN,
                term::RESET,
                path.display()
            ).ok(),
            Err(e) => writeln!(
                out,
                "  {}✗{} Failed to restore {}: {}",
                term::RED,
                term::RESET,
                path.display(),
                e
            ).ok(),
        };
    }
    println!();
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
        let _ = std::fs::remove_file(&path);
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
