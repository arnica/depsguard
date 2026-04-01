// TUI rendering — ASCII art banner, status table, interactive selector.

use std::io::{self, Write};

use crate::manager::{CheckStatus, ManagerInfo};
use crate::term::*;

// ── Banner ────────────────────────────────────────────────────────────

const BANNER: &str = r#"
     ╔═══════════════════════════════════════════════════════╗
     ║   ____                  ____                     _    ║
     ║  |  _ \  ___ _ __  ___ / ___|_   _  __ _ _ __ __| |   ║
     ║  | | | |/ _ \ '_ \/ __| |  _| | | |/ _` | '__/ _` |   ║
     ║  | |_| |  __/ |_) \__ \ |_| | |_| | (_| | | | (_| |   ║
     ║  |____/ \___| .__/|___/\____|\__,_|\__,_|_|  \__,_|   ║
     ║             |_|    supply chain defense               ║
     ║                                                       ║
     ║        Made with love by Arnica in Atlanta            ║
     ╚═══════════════════════════════════════════════════════╝
"#;

pub fn print_banner(w: &mut impl Write) -> io::Result<()> {
    write!(w, "{CYAN}{BOLD}{BANNER}{RESET}\n")
}

// ── Status rendering ──────────────────────────────────────────────────

fn status_icon(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ok => "✅",
        CheckStatus::Missing => "❌",
        CheckStatus::WrongValue(_) => "⚠️ ",
    }
}

fn status_color(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ok => GREEN,
        CheckStatus::Missing => RED,
        CheckStatus::WrongValue(_) => YELLOW,
    }
}

pub fn print_scan_results(w: &mut impl Write, managers: &[ManagerInfo]) -> io::Result<()> {
    if managers.is_empty() {
        writeln!(
            w,
            "\n  {YELLOW}{BOLD}No supported package managers found.{RESET}"
        )?;
        writeln!(
            w,
            "  {DIM}Install npm, pnpm, bun, or uv to get started.{RESET}\n"
        )?;
        return Ok(());
    }

    writeln!(w, "  {BOLD}{WHITE}Detected Package Managers:{RESET}\n")?;

    for mgr in managers {
        let icon = mgr.kind.icon();
        let name = mgr.kind.name();
        let ver = &mgr.version;
        let all_ok = mgr.all_ok();
        let badge = if all_ok {
            format!("{BG_GREEN}{BOLD} SECURE {RESET}")
        } else {
            format!("{BG_RED}{BOLD} ACTION NEEDED {RESET}")
        };

        writeln!(
            w,
            "  {icon} {BOLD}{CYAN}{name}{RESET} {DIM}v{ver}{RESET}  {badge}"
        )?;
        writeln!(w, "    {DIM}Config: {}{RESET}", mgr.config_path.display())?;

        for rec in &mgr.recommendations {
            let si = status_icon(&rec.status);
            let sc = status_color(&rec.status);
            let detail = match &rec.status {
                CheckStatus::Ok => format!("{GREEN}= {}{RESET}", rec.expected),
                CheckStatus::Missing => format!("{RED}not configured{RESET}"),
                CheckStatus::WrongValue(v) => {
                    format!(
                        "{YELLOW}{v}{RESET} {DIM}(expected: {}){RESET}",
                        rec.expected
                    )
                }
            };
            writeln!(w, "    {si} {sc}{}{RESET}", rec.key)?;
            writeln!(w, "       {DIM}{}{RESET}", rec.description)?;
            writeln!(w, "       Status: {detail}")?;
        }
        writeln!(w)?;
    }
    Ok(())
}

// ── Interactive selector ──────────────────────────────────────────────

#[derive(Debug)]
pub struct SelectItem {
    pub manager_idx: usize,
    pub rec_idx: usize,
    pub label: String,
    pub selected: bool,
}

pub fn build_fix_items(managers: &[ManagerInfo]) -> Vec<SelectItem> {
    let mut items = Vec::new();
    for (mi, mgr) in managers.iter().enumerate() {
        for (ri, rec) in mgr.recommendations.iter().enumerate() {
            if rec.needs_fix() {
                items.push(SelectItem {
                    manager_idx: mi,
                    rec_idx: ri,
                    label: format!(
                        "{} {} → {} = {}",
                        mgr.kind.icon(),
                        mgr.kind.name(),
                        rec.key,
                        rec.expected
                    ),
                    selected: true, // default: select all
                });
            }
        }
    }
    items
}

pub fn print_selector(w: &mut impl Write, items: &[SelectItem], cursor: usize) -> io::Result<()> {
    writeln!(
        w,
        "  {BOLD}{WHITE}Select fixes to apply:{RESET}  {DIM}(↑↓ move, space toggle, enter apply, q quit){RESET}\n"
    )?;

    for (i, item) in items.iter().enumerate() {
        let arrow = if i == cursor {
            format!("{CYAN}{BOLD}▸{RESET}")
        } else {
            " ".to_string()
        };
        let check = if item.selected {
            format!("{GREEN}[✓]{RESET}")
        } else {
            format!("{DIM}[ ]{RESET}")
        };
        let highlight = if i == cursor { BOLD } else { "" };
        writeln!(w, "  {arrow} {check} {highlight}{}{RESET}", item.label)?;
    }
    writeln!(w)?;

    // Summary
    let count = items.iter().filter(|i| i.selected).count();
    writeln!(w, "  {DIM}{count} fix(es) selected{RESET}\n")
}

pub fn print_fix_results(
    w: &mut impl Write,
    results: &[(String, Result<String, String>)],
) -> io::Result<()> {
    writeln!(w, "  {BOLD}{WHITE}Fix Results:{RESET}\n")?;
    for (label, result) in results {
        match result {
            Ok(line) => {
                writeln!(w, "  {GREEN}✓{RESET} {label}")?;
                writeln!(w, "    {DIM}Set: {line}{RESET}")?;
            }
            Err(e) => {
                writeln!(w, "  {RED}✗{RESET} {label}")?;
                writeln!(w, "    {RED}Error: {e}{RESET}")?;
            }
        }
    }
    writeln!(w)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{ManagerKind, Recommendation};
    use std::path::PathBuf;

    fn make_manager(recs: Vec<Recommendation>) -> ManagerInfo {
        ManagerInfo {
            kind: ManagerKind::Npm,
            version: "10.0.0".into(),
            config_path: PathBuf::from("/home/test/.npmrc"),
            recommendations: recs,
        }
    }

    fn make_rec(key: &str, status: CheckStatus) -> Recommendation {
        Recommendation {
            key: key.into(),
            description: "test desc".into(),
            expected: "expected_val".into(),
            status,
        }
    }

    #[test]
    fn banner_renders() {
        let mut buf = Vec::new();
        print_banner(&mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("supply chain defense"));
        assert!(s.contains("Arnica"));
    }

    #[test]
    fn scan_results_empty() {
        let mut buf = Vec::new();
        print_scan_results(&mut buf, &[]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("No supported package managers found"));
    }

    #[test]
    fn scan_results_all_ok() {
        let mgr = make_manager(vec![make_rec("key", CheckStatus::Ok)]);
        let mut buf = Vec::new();
        print_scan_results(&mut buf, &[mgr]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("SECURE"));
        assert!(s.contains("npm"));
    }

    #[test]
    fn scan_results_needs_fix() {
        let mgr = make_manager(vec![make_rec("key", CheckStatus::Missing)]);
        let mut buf = Vec::new();
        print_scan_results(&mut buf, &[mgr]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("ACTION NEEDED"));
        assert!(s.contains("not configured"));
    }

    #[test]
    fn scan_results_wrong_value() {
        let mgr = make_manager(vec![make_rec("key", CheckStatus::WrongValue("bad".into()))]);
        let mut buf = Vec::new();
        print_scan_results(&mut buf, &[mgr]).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("bad"));
        assert!(s.contains("expected"));
    }

    #[test]
    fn build_fix_items_skips_ok() {
        let mgr = make_manager(vec![
            make_rec("ok_key", CheckStatus::Ok),
            make_rec("bad_key", CheckStatus::Missing),
        ]);
        let items = build_fix_items(&[mgr]);
        assert_eq!(items.len(), 1);
        assert!(items[0].label.contains("bad_key"));
        assert!(items[0].selected);
    }

    #[test]
    fn build_fix_items_empty_when_all_ok() {
        let mgr = make_manager(vec![make_rec("ok", CheckStatus::Ok)]);
        let items = build_fix_items(&[mgr]);
        assert!(items.is_empty());
    }

    #[test]
    fn selector_renders_cursor() {
        let items = vec![
            SelectItem {
                manager_idx: 0,
                rec_idx: 0,
                label: "fix A".into(),
                selected: true,
            },
            SelectItem {
                manager_idx: 0,
                rec_idx: 1,
                label: "fix B".into(),
                selected: false,
            },
        ];
        let mut buf = Vec::new();
        print_selector(&mut buf, &items, 0).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("▸")); // cursor
        assert!(s.contains("[✓]"));
        assert!(s.contains("[ ]"));
        assert!(s.contains("1 fix(es) selected"));
    }

    #[test]
    fn selector_cursor_second_item() {
        let items = vec![
            SelectItem {
                manager_idx: 0,
                rec_idx: 0,
                label: "fix A".into(),
                selected: true,
            },
            SelectItem {
                manager_idx: 0,
                rec_idx: 1,
                label: "fix B".into(),
                selected: true,
            },
        ];
        let mut buf = Vec::new();
        print_selector(&mut buf, &items, 1).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("2 fix(es) selected"));
    }

    #[test]
    fn fix_results_ok() {
        let results = vec![("fix X".into(), Ok("key=val".into()))];
        let mut buf = Vec::new();
        print_fix_results(&mut buf, &results).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("✓"));
        assert!(s.contains("key=val"));
    }

    #[test]
    fn fix_results_error() {
        let results = vec![("fix Y".into(), Err("permission denied".into()))];
        let mut buf = Vec::new();
        print_fix_results(&mut buf, &results).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("✗"));
        assert!(s.contains("permission denied"));
    }
}
