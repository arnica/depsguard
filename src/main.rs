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
    if args.len() > 1 && (args[1] == "--scan" || args[1] == "-s") {
        run_scan_only();
        return;
    }
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return;
    }
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("depsguard {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if let Err(e) = run_interactive() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn print_usage() {
    let mut out = io::stdout();
    ui::print_banner(&mut out).ok();
    println!("  Harden your package manager configs against supply chain attacks.\n");
    println!("  USAGE:");
    println!("    depsguard            Interactive mode (TUI)");
    println!("    depsguard --scan     Scan only, no changes");
    println!("    depsguard --help     Show this help");
    println!("    depsguard --version  Show version\n");
}

fn run_scan_only() {
    let mut out = io::stdout();
    ui::print_banner(&mut out).ok();
    let managers = manager::scan_all();
    ui::print_scan_results(&mut out, &managers).ok();
}

fn run_interactive() -> io::Result<()> {
    let mut out = io::stdout();

    // Phase 1: Scan
    ui::print_banner(&mut out)?;
    println!(
        "  {}{}Scanning package managers...{}\n",
        term::BOLD,
        term::CYAN,
        term::RESET
    );
    out.flush()?;

    let managers = manager::scan_all();
    ui::print_scan_results(&mut out, &managers)?;
    out.flush()?;

    // Build fixable items
    let mut items = ui::build_fix_items(&managers);
    if items.is_empty() {
        println!(
            "  {}{}All package managers are properly configured! 🎉{}",
            term::BOLD,
            term::GREEN,
            term::RESET
        );
        return Ok(());
    }

    // Phase 2: Interactive selection
    println!(
        "  {}Press any key to enter selection mode...{}",
        term::DIM,
        term::RESET
    );
    out.flush()?;

    let _raw = term::RawMode::enable()?;
    term::read_key()?; // wait for keypress

    let result = selection_loop(&mut out, &mut items, &managers);

    // Raw mode drops here, restoring terminal
    drop(_raw);
    term::show_cursor(&mut out)?;
    out.flush()?;

    result
}

fn selection_loop(
    out: &mut impl Write,
    items: &mut Vec<SelectItem>,
    managers: &[ManagerInfo],
) -> io::Result<()> {
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
                let results = apply_selected(items, managers);
                term::clear_screen(out)?;
                ui::print_banner(out)?;
                ui::print_fix_results(out, &results)?;

                // Re-scan to show new status
                writeln!(
                    out,
                    "  {}{}Re-scanning...{}\n",
                    term::BOLD,
                    term::CYAN,
                    term::RESET
                )?;
                out.flush()?;
                let new_managers = manager::scan_all();
                ui::print_scan_results(out, &new_managers)?;

                writeln!(
                    out,
                    "  {}Press any key to exit...{}",
                    term::DIM,
                    term::RESET
                )?;
                out.flush()?;
                term::read_key()?;
                return Ok(());
            }
            Key::Escape | Key::Char('q') => {
                return Ok(());
            }
            _ => {}
        }
    }
}

fn apply_selected(
    items: &[SelectItem],
    managers: &[ManagerInfo],
) -> Vec<(String, Result<String, String>)> {
    items
        .iter()
        .filter(|item| item.selected)
        .map(|item| {
            let mgr = &managers[item.manager_idx];
            let rec = &mgr.recommendations[item.rec_idx];
            let result = fix::apply_fix(mgr.kind, &mgr.config_path, rec).map_err(|e| e.to_string());
            (item.label.clone(), result)
        })
        .collect()
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
