// TUI rendering — ASCII art banner, status table, interactive selector.

use std::io::{self, Write};
use std::path::Path;

use crate::manager::{CheckStatus, ManagerInfo};
use crate::term::*;

/// Return "1 config" or "3 configs" — simple singular/plural.
fn plural(n: usize, singular: &str, plural_form: &str) -> String {
    if n == 1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {plural_form}")
    }
}

/// Display a path relative to the user's home directory (~/...).
pub fn display_path(path: &Path) -> String {
    let home = crate::manager::home_dir();
    match path.strip_prefix(&home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

// ── Banner ────────────────────────────────────────────────────────────

/// Arnica brand color — ANSI 256-color 73 (closest to #44bea4)
const ARNICA: &str = "\x1b[38;5;73m";
/// DepsGuard brand color #ebb838 — ANSI 256-color 178 (closest match)
const GOLD: &str = "\x1b[38;5;178m";

const BOX_WIDTH: usize = 66; // inner width between │ chars

pub fn print_banner(w: &mut impl Write) -> io::Result<()> {
    let version = env!("CARGO_PKG_VERSION");

    let indent = "  ";

    // Top border with version
    let title = format!(" v{version} ");
    let remaining = BOX_WIDTH.saturating_sub(title.len() + 1);
    writeln!(
        w,
        "{indent}{ARNICA}╭─{RESET}{BOLD}{WHITE}{title}{RESET}{ARNICA}{}{RESET}{ARNICA}╮{RESET}",
        "─".repeat(remaining)
    )?;

    // Art lines inside box (centered)
    let art_lines: &[&str] = &[
        "",
        "╶┬┐┌─╴┌─┐┌─┐┌─╴╷ ╷┌─┐┌─┐╶┬┐",
        " ││├╴ ├─┘└─┐│╶┐│ │├─┤├┬┘ ││",
        "╶┴┘└─╴╵  └─┘└─┘└─┘╵ ╵╵└╴╶┴┘",
        "",
    ];
    for line in art_lines {
        let display_len = line.chars().count();
        let left = (BOX_WIDTH.saturating_sub(display_len)) / 2;
        let right = BOX_WIDTH.saturating_sub(display_len + left);
        writeln!(
            w,
            "{indent}{ARNICA}│{RESET}{}{GOLD}{BOLD}{line}{RESET}{}{ARNICA}│{RESET}",
            " ".repeat(left),
            " ".repeat(right)
        )?;
    }

    // Bottom border with branding right-aligned
    let brand_cols = 13;
    let left_dashes = BOX_WIDTH.saturating_sub(brand_cols + 1);
    write!(w, "{indent}{ARNICA}╰{}─{RESET}", "─".repeat(left_dashes))?;
    write!(
        w,
        "{DIM} by {RESET}{ARNICA}[{RESET}{WHITE}{BOLD}arnica{RESET}{ARNICA}]{RESET}{DIM} {RESET}"
    )?;
    writeln!(w, "{ARNICA}╯{RESET}")?;
    writeln!(w)
}

// ── Progress bar ─────────────────────────────────────────────────────

/// Render a status line in-place (overwrites current line).
pub fn print_progress(w: &mut impl Write, label: &str, _fraction: f32) -> io::Result<()> {
    let term_width = crate::term::terminal_size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    let prefix_cols = 4; // "  · "
    let max_label = term_width.saturating_sub(prefix_cols);
    let truncated = if label.len() > max_label && max_label > 3 {
        // UTF-8 safe truncation: find a valid char boundary
        let limit = max_label - 3;
        let cut = label
            .char_indices()
            .map(|(i, _)| i)
            .nth(limit)
            .unwrap_or(label.len());
        format!("{}...", &label[..cut])
    } else {
        label.to_string()
    };
    write!(w, "\r\x1b[K  {DIM}· {truncated}{RESET}")?;
    w.flush()
}

/// Clear the progress bar line.
pub fn clear_progress(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\r\x1b[K")
}

// ── Status rendering ──────────────────────────────────────────────────

fn status_icon(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ok => "✓",
        CheckStatus::Missing => "✗",
        CheckStatus::WrongValue(_) => "~",
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

    // Count issues (deduplicate by config_path + key, matching display logic)
    let mut ok_count = 0usize;
    let mut missing_count = 0usize;
    let mut wrong_count = 0usize;
    let mut seen_keys = std::collections::HashSet::new();
    for mgr in managers {
        for rec in &mgr.recommendations {
            if !seen_keys.insert((&mgr.config_path, &rec.key)) {
                continue;
            }
            match &rec.status {
                CheckStatus::Ok => ok_count += 1,
                CheckStatus::Missing => missing_count += 1,
                CheckStatus::WrongValue(_) => wrong_count += 1,
            }
        }
    }
    let total_issues = missing_count + wrong_count;
    let unique_configs = {
        let s: std::collections::HashSet<_> = managers.iter().map(|m| &m.config_path).collect();
        s.len()
    };

    if total_issues == 0 {
        writeln!(
            w,
            "  {GREEN}{BOLD}All {ok_count} checks passed{RESET} {DIM}across {}{RESET}\n",
            plural(unique_configs, "config", "configs")
        )?;
    } else {
        write!(w, "  ")?;
        if missing_count > 0 {
            write!(w, "{RED}{BOLD}{missing_count} not set{RESET}  ")?;
        }
        if wrong_count > 0 {
            write!(w, "{YELLOW}{BOLD}{wrong_count} misconfigured{RESET}  ")?;
        }
        write!(w, "{GREEN}{ok_count} ok{RESET}")?;
        let total = ok_count + missing_count + wrong_count;
        writeln!(
            w,
            "  {DIM}({} total across {}){RESET}\n",
            plural(total, "check", "checks"),
            plural(unique_configs, "config", "configs"),
        )?;
    }

    writeln!(w, "  {BOLD}{WHITE}Detected Package Managers:{RESET}\n")?;

    // Group managers that share the same config path
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut assigned = vec![false; managers.len()];

    for i in 0..managers.len() {
        if assigned[i] {
            continue;
        }
        let mut group = vec![i];
        assigned[i] = true;
        for j in (i + 1)..managers.len() {
            if !assigned[j] && managers[j].config_path == managers[i].config_path {
                group.push(j);
                assigned[j] = true;
            }
        }
        groups.push(group);
    }

    for group in &groups {
        // Build header: "📦 npm v10.0 · pnpm v10.20"
        let header_parts: Vec<String> = group
            .iter()
            .map(|&idx| {
                let mgr = &managers[idx];
                let icon = mgr.kind.icon();
                let space = if icon.is_empty() { "" } else { " " };
                format!(
                    "{icon}{space}{BOLD}{CYAN}{}{RESET} {DIM}v{}{RESET}",
                    mgr.kind.name(),
                    mgr.version
                )
            })
            .collect();
        let header = header_parts.join(&format!(" {DIM}·{RESET} "));

        let all_ok = group.iter().all(|&idx| managers[idx].all_ok());
        let badge = if all_ok {
            format!("{BG_GREEN}{BOLD} SECURE {RESET}")
        } else {
            format!("{BG_RED}{BOLD} ACTION NEEDED {RESET}")
        };

        writeln!(w, "  {header}  {badge}")?;
        writeln!(
            w,
            "     {DIM}Config: {}{RESET}",
            display_path(&managers[group[0]].config_path)
        )?;

        let mut seen_keys = std::collections::HashSet::new();
        for &idx in group {
            let mgr = &managers[idx];
            let show_prefix = group.len() > 1;
            for rec in &mgr.recommendations {
                // Deduplicate identical keys within a grouped config file
                if !seen_keys.insert(rec.key.clone()) {
                    continue;
                }
                let si = status_icon(&rec.status);
                let sc = status_color(&rec.status);
                let detail = match &rec.status {
                    CheckStatus::Ok => format!("{GREEN}{}{RESET}", rec.expected),
                    CheckStatus::Missing => format!("{RED}not set{RESET}"),
                    CheckStatus::WrongValue(v) => {
                        format!("{YELLOW}{v}{RESET} {DIM}(want: {}){RESET}", rec.expected)
                    }
                };
                let prefix = if show_prefix {
                    format!("{DIM}({}){RESET} ", mgr.kind.name())
                } else {
                    String::new()
                };
                writeln!(
                    w,
                    "     {sc}{si}{RESET} {prefix}{sc}{}{RESET} {DIM}—{RESET} {detail} {DIM}· {}{RESET}",
                    rec.key, rec.description
                )?;
            }
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
    pub group_path: String,   // display path for grouping
    pub group_header: String, // e.g. "📦 npm · ⚡ pnpm"
    pub selected: bool,
}

pub fn build_fix_items(managers: &[ManagerInfo]) -> Vec<SelectItem> {
    let mut items = Vec::new();
    // Track (config_path, key) to avoid duplicate fix items when managers share a config
    let mut seen = std::collections::HashSet::new();

    // Pre-compute which managers share each config path
    let mut path_managers: std::collections::HashMap<&std::path::Path, Vec<&ManagerInfo>> =
        std::collections::HashMap::new();
    for mgr in managers {
        path_managers
            .entry(mgr.config_path.as_path())
            .or_default()
            .push(mgr);
    }

    for (mi, mgr) in managers.iter().enumerate() {
        for (ri, rec) in mgr.recommendations.iter().enumerate() {
            if rec.needs_fix() {
                let dedup_key = (mgr.config_path.clone(), rec.key.clone());
                if !seen.insert(dedup_key) {
                    continue;
                }
                let siblings = &path_managers[mgr.config_path.as_path()];
                let group_header = siblings
                    .iter()
                    .map(|m| {
                        let icon = m.kind.icon();
                        let space = if icon.is_empty() { "" } else { " " };
                        format!(
                            "{icon}{space}{BOLD}{CYAN}{}{RESET} {DIM}v{}{RESET}",
                            m.kind.name(),
                            m.version
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(&format!(" {DIM}·{RESET} "));
                items.push(SelectItem {
                    manager_idx: mi,
                    rec_idx: ri,
                    label: format!("{} = {}", rec.key, rec.expected),
                    group_path: display_path(&mgr.config_path),
                    group_header,
                    selected: true,
                });
            }
        }
    }
    items
}

pub fn print_selector(w: &mut impl Write, items: &[SelectItem], cursor: usize) -> io::Result<()> {
    writeln!(
        w,
        "  {BOLD}{WHITE}Select fixes to apply:{RESET}  {DIM}(↑↓ move, space toggle, enter apply, d diff, q quit){RESET}\n"
    )?;

    let mut last_group: Option<&str> = None;
    for (i, item) in items.iter().enumerate() {
        // Print group header when path changes
        if last_group != Some(&item.group_path) {
            if last_group.is_some() {
                writeln!(w)?;
            }
            writeln!(w, "  {}", item.group_header)?;
            writeln!(w, "     {DIM}Config: {}{RESET}", item.group_path)?;
            last_group = Some(&item.group_path);
        }
        let arrow = if i == cursor {
            format!("{CYAN}{BOLD}▸{RESET}")
        } else {
            " ".to_string()
        };
        let check = if item.selected {
            format!("{GREEN}●{RESET}")
        } else {
            format!("{DIM}○{RESET}")
        };
        let highlight = if i == cursor { BOLD } else { "" };
        writeln!(w, "     {arrow} {check} {highlight}{}{RESET}", item.label)?;
    }
    writeln!(w)?;

    // Summary
    let count = items.iter().filter(|i| i.selected).count();
    writeln!(
        w,
        "  {DIM}{} selected{RESET}\n",
        plural(count, "fix", "fixes")
    )
}

// ── Diff preview ─────────────────────────────────────────────────────

/// Render a unified-diff style preview of what selected fixes will change.
pub fn print_diff_preview(
    w: &mut impl Write,
    items: &[SelectItem],
    managers: &[crate::manager::ManagerInfo],
) -> io::Result<()> {
    writeln!(
        w,
        "  {BOLD}{WHITE}Preview of changes:{RESET}  {DIM}(press any key to go back){RESET}\n"
    )?;

    // Group selected items by config path
    let mut by_path: std::collections::BTreeMap<
        &std::path::Path,
        Vec<(&SelectItem, &crate::manager::Recommendation)>,
    > = std::collections::BTreeMap::new();
    for item in items.iter().filter(|i| i.selected) {
        let mgr = &managers[item.manager_idx];
        let rec = &mgr.recommendations[item.rec_idx];
        by_path
            .entry(&mgr.config_path)
            .or_default()
            .push((item, rec));
    }

    if by_path.is_empty() {
        writeln!(w, "  {DIM}No fixes selected.{RESET}\n")?;
        return Ok(());
    }

    for (path, fixes) in &by_path {
        let dp = display_path(path);
        writeln!(w, "  {BOLD}{CYAN}--- {dp}{RESET}")?;
        writeln!(w, "  {BOLD}{CYAN}+++ {dp}{RESET} {DIM}(after fix){RESET}")?;

        // Determine format based on file type
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let is_npmrc = fname == ".npmrc";
        let is_yaml = ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml");

        for (_item, rec) in fixes {
            // For TOML dotted keys like "install.minimumReleaseAge", look up the
            // leaf key within its section using the real TOML reader
            let current_val = if rec.key.contains('.') {
                crate::manager::read_toml_value(path, &rec.key)
            } else if is_yaml {
                crate::manager::read_yaml_value(path, &rec.key)
            } else {
                let flat = crate::manager::read_flat_config(path);
                flat.get(&rec.key).cloned()
            };

            // Format to match what apply_fix actually writes (including quoting)
            let is_toml = ext.eq_ignore_ascii_case("toml");
            let fmt = |k: &str, v: &str| -> String {
                if is_npmrc {
                    format!("{k}={v}")
                } else if is_yaml {
                    // trustPolicy gets quoted in YAML
                    let needs_quote = k == "trustPolicy";
                    if needs_quote {
                        format!("{k}: \"{v}\"")
                    } else {
                        format!("{k}: {v}")
                    }
                } else if is_toml {
                    // uv exclude-newer and other string TOML values get quoted
                    let needs_quote = v.contains(' ') || v.contains('-') || v.contains('T');
                    if needs_quote {
                        format!("{k} = \"{v}\"")
                    } else {
                        format!("{k} = {v}")
                    }
                } else {
                    format!("{k} = {v}")
                }
            };

            match current_val {
                Some(ref cv) if cv != &rec.expected => {
                    writeln!(w, "  {RED}-  {}{RESET}", fmt(&rec.key, cv))?;
                    writeln!(w, "  {GREEN}+  {}{RESET}", fmt(&rec.key, &rec.expected))?;
                }
                Some(_) => {
                    // Already correct, skip
                }
                None => {
                    writeln!(w, "  {GREEN}+  {}{RESET}", fmt(&rec.key, &rec.expected))?;
                }
            }
        }
        writeln!(w)?;
    }
    Ok(())
}

#[cfg(test)]
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
        let expected_ver = format!("v{}", env!("CARGO_PKG_VERSION"));
        assert!(s.contains(&expected_ver));
        assert!(s.contains("arnica"));
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
        assert!(s.contains("not set"));
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
                group_path: "~/.npmrc".into(),
                group_header: "npm".into(),
                selected: true,
            },
            SelectItem {
                manager_idx: 0,
                rec_idx: 1,
                label: "fix B".into(),
                group_path: "~/.npmrc".into(),
                group_header: "npm".into(),
                selected: false,
            },
        ];
        let mut buf = Vec::new();
        print_selector(&mut buf, &items, 0).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("▸")); // cursor
        assert!(s.contains("●"));
        assert!(s.contains("○"));
        assert!(s.contains("1 fix selected"));
    }

    #[test]
    fn selector_cursor_second_item() {
        let items = vec![
            SelectItem {
                manager_idx: 0,
                rec_idx: 0,
                label: "fix A".into(),
                group_path: "~/.npmrc".into(),
                group_header: "npm".into(),
                selected: true,
            },
            SelectItem {
                manager_idx: 0,
                rec_idx: 1,
                label: "fix B".into(),
                group_path: "~/.npmrc".into(),
                group_header: "npm".into(),
                selected: true,
            },
        ];
        let mut buf = Vec::new();
        print_selector(&mut buf, &items, 1).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("2 fixes selected"));
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
