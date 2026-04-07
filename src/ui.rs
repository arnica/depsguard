// TUI rendering — ASCII art banner, status table, interactive selector.

use std::io::{self, Write};
use std::path::Path;

use crate::manager::{self, CheckStatus, ManagerInfo};
use crate::term::{BG_GREEN, BG_RED, BOLD, CYAN, DIM, GREEN, RED, RESET, WHITE, YELLOW};

const BLUE: &str = "\x1b[34m";

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
    manager::display_path(path)
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
    // Art lines inside box (left padded, no centering so glyphs render exactly)
    let art_lines: &[&str] = &[
        "     _                                          _",
        "  __| | ___ _ __  ___  __ _ _   _  __ _ _ __ __| |",
        " / _` |/ _ \\ '_ \\/ __|/ _` | | | |/ _` | '__/ _` |",
        "| (_| |  __/ |_) \\__ \\ (_| | |_| | (_| | | | (_| |",
        " \\__,_|\\___| .__/|___/\\__, |\\__,_|\\__,_|_|  \\__,_|",
        "           |_|        |___/",
    ];
    for line in art_lines {
        let display_len = line.chars().count();
        let left = 1;
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
pub fn print_progress(w: &mut impl Write, label: &str) -> io::Result<()> {
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

/// Clear the progress line.
pub fn clear_progress(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\r\x1b[K")
}

// ── Shared formatting ─────────────────────────────────────────────────

/// Format a group header like `"📦 npm v10.0 · ⚡ pnpm v10.2"`.
fn format_manager_header(managers: &[&ManagerInfo]) -> String {
    managers
        .iter()
        .map(|m| {
            let icon = m.kind.icon();
            let space = if icon.is_empty() { "" } else { " " };
            if m.version.is_empty() {
                format!("{icon}{space}{BOLD}{CYAN}{}{RESET}", m.kind.name())
            } else {
                format!(
                    "{icon}{space}{BOLD}{CYAN}{}{RESET} {DIM}v{}{RESET}",
                    m.kind.name(),
                    m.version
                )
            }
        })
        .collect::<Vec<_>>()
        .join(&format!(" {DIM}·{RESET} "))
}

// ── Status rendering ──────────────────────────────────────────────────

fn status_icon(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ok => "✓",
        CheckStatus::Missing => "✗",
        CheckStatus::WrongValue(_) => "~",
        CheckStatus::Unsupported(_) => "ℹ",
    }
}

fn status_color(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Ok => GREEN,
        CheckStatus::Missing => RED,
        CheckStatus::WrongValue(_) => YELLOW,
        CheckStatus::Unsupported(_) => BLUE,
    }
}

/// Render a summary of scan results grouped by config file, with status badges.
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
    let mut unsupported_count = 0usize;
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
                CheckStatus::Unsupported(_) => unsupported_count += 1,
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
        if unsupported_count > 0 {
            write!(w, "{BLUE}{unsupported_count} unsupported{RESET}  ")?;
        }
        write!(w, "{GREEN}{ok_count} ok{RESET}")?;
        let total = ok_count + missing_count + wrong_count + unsupported_count;
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
        let group_managers: Vec<&ManagerInfo> = group.iter().map(|&idx| &managers[idx]).collect();
        let header = format_manager_header(&group_managers);

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
                let icon = status_icon(&rec.status);
                let color = status_color(&rec.status);
                let detail = match &rec.status {
                    CheckStatus::Ok => format!("{GREEN}{}{RESET}", rec.expected),
                    CheckStatus::Missing => format!("{RED}not set{RESET}"),
                    CheckStatus::WrongValue(v) => {
                        format!("{YELLOW}{v}{RESET} {DIM}(want: {}){RESET}", rec.expected)
                    }
                    CheckStatus::Unsupported(v) => {
                        format!("{BLUE}{v}{RESET}")
                    }
                };
                let prefix = if show_prefix {
                    format!("{DIM}({}){RESET} ", mgr.kind.name())
                } else {
                    String::new()
                };
                writeln!(
                    w,
                    "     {color}{icon}{RESET} {prefix}{color}{}{RESET} {DIM}—{RESET} {detail} {DIM}· {}{RESET}",
                    rec.key, rec.description
                )?;
            }
        }
        writeln!(w)?;
    }
    Ok(())
}

// ── Interactive selector ──────────────────────────────────────────────

/// A fixable item in the interactive selection list.
#[derive(Debug)]
pub struct SelectItem {
    pub manager_idx: usize,
    pub rec_idx: usize,
    pub label: String,
    pub group_path: String,   // display path for grouping
    pub group_header: String, // e.g. "📦 npm · ⚡ pnpm"
    pub selected: bool,
}

/// Filter mode for the interactive selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectFilter {
    All,
    SelectedOnly,
    UnselectedOnly,
}

impl SelectFilter {
    /// Cycle to the next filter mode.
    pub fn next(self) -> Self {
        match self {
            SelectFilter::All => SelectFilter::SelectedOnly,
            SelectFilter::SelectedOnly => SelectFilter::UnselectedOnly,
            SelectFilter::UnselectedOnly => SelectFilter::All,
        }
    }

    /// What pressing `f` will switch to (shown in the shortcut bar).
    pub fn next_action(self) -> &'static str {
        match self {
            SelectFilter::All => "show selected",
            SelectFilter::SelectedOnly => "show unselected",
            SelectFilter::UnselectedOnly => "show all",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SelectFilter::All => "all",
            SelectFilter::SelectedOnly => "selected",
            SelectFilter::UnselectedOnly => "unselected",
        }
    }
}

/// Build a list of visible item indices based on the current filter.
pub fn filtered_indices(items: &[SelectItem], filter: SelectFilter) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| match filter {
            SelectFilter::All => true,
            SelectFilter::SelectedOnly => item.selected,
            SelectFilter::UnselectedOnly => !item.selected,
        })
        .map(|(i, _)| i)
        .collect()
}

/// A keyboard shortcut for toggling items by config file type.
#[derive(Debug)]
pub struct ToggleKey {
    pub key: char,
    pub label: String,
    pub kind: crate::manager::ManagerKind,
}

/// Config-file label for toggle grouping. Items that share a config file type
/// (e.g. npm and pnpm both use `.npmrc`) are grouped under one toggle.
fn toggle_label(kind: crate::manager::ManagerKind) -> &'static str {
    use crate::manager::ManagerKind;
    match kind {
        ManagerKind::Npm | ManagerKind::Pnpm => ".npmrc",
        ManagerKind::PnpmWorkspace => "pnpm-workspace",
        ManagerKind::Bun => ".bunfig.toml",
        ManagerKind::Uv => "uv.toml",
        ManagerKind::Yarn => ".yarnrc.yml",
        ManagerKind::Renovate => "renovate",
        ManagerKind::Dependabot => "dependabot",
    }
}

/// Canonical kind for toggle grouping (npm + pnpm share `.npmrc`).
fn toggle_canonical(kind: crate::manager::ManagerKind) -> crate::manager::ManagerKind {
    use crate::manager::ManagerKind;
    if kind == ManagerKind::Pnpm {
        ManagerKind::Npm // group under .npmrc
    } else {
        kind
    }
}

/// Build the list of single-key toggle shortcuts for the config types present in `items`.
pub fn build_toggle_keys(
    items: &[SelectItem],
    managers: &[crate::manager::ManagerInfo],
) -> Vec<ToggleKey> {
    let mut seen = std::collections::HashSet::new();
    let mut keys = Vec::new();
    let mut used_chars = std::collections::HashSet::new();
    for c in ['a', 'q', 'd', 'f'] {
        used_chars.insert(c);
    }

    for item in items {
        let kind = managers[item.manager_idx].kind;
        let canonical = toggle_canonical(kind);
        if !seen.insert(canonical) {
            continue;
        }
        let label = toggle_label(canonical);
        let key = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .find(|c| c.is_ascii_lowercase() && !used_chars.contains(c));
        if let Some(k) = key {
            used_chars.insert(k);
            keys.push(ToggleKey {
                key: k,
                label: label.to_string(),
                kind: canonical,
            });
        }
    }
    keys
}

/// Toggle selection state for all items matching a config file type.
pub fn toggle_manager(
    items: &mut [SelectItem],
    managers: &[crate::manager::ManagerInfo],
    target: crate::manager::ManagerKind,
) -> bool {
    let matching: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let kind = managers[item.manager_idx].kind;
            toggle_canonical(kind) == target
        })
        .map(|(i, _)| i)
        .collect();
    if matching.is_empty() {
        return false;
    }
    let any_selected = matching.iter().any(|&i| items[i].selected);
    for &i in &matching {
        items[i].selected = !any_selected;
    }
    true
}

/// Build the list of fixable items from scan results, deduplicating shared configs.
/// Items are sorted by manager kind (user-level first, then discovered), then by path.
pub fn build_fix_items(managers: &[ManagerInfo]) -> Vec<SelectItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

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
                let group_header = format_manager_header(siblings);
                items.push(SelectItem {
                    manager_idx: mi,
                    rec_idx: ri,
                    label: format!("{} = {}", rec.key, rec.expected),
                    group_path: display_path(&mgr.config_path),
                    group_header,
                    selected: !mgr.discovered,
                });
            }
        }
    }

    // Sort: user-level before discovered, then by manager kind name, then by path
    items.sort_by(|a, b| {
        let a_mgr = &managers[a.manager_idx];
        let b_mgr = &managers[b.manager_idx];
        a_mgr
            .discovered
            .cmp(&b_mgr.discovered)
            .then_with(|| a_mgr.kind.name().cmp(b_mgr.kind.name()))
            .then_with(|| a.group_path.cmp(&b.group_path))
    });

    items
}

/// Compute the number of terminal rows used by fixed chrome (header, footer).
/// `has_toggle_keys` adds a line for the toggle shortcut bar.
pub fn selector_chrome_lines(has_toggle_keys: bool) -> usize {
    let title = 1;
    let nav_line = 1;
    let toggle_line = if has_toggle_keys { 1 } else { 0 };
    let blank_after_header = 1;
    let footer = 2; // blank + status/page line
    title + nav_line + toggle_line + blank_after_header + footer
}

/// Return the maximum number of item-area lines available for the current terminal.
pub fn max_item_lines_for(has_toggle_keys: bool) -> usize {
    let term_rows = crate::term::terminal_size()
        .map(|(_, h)| h as usize)
        .unwrap_or(24);
    term_rows.saturating_sub(selector_chrome_lines(has_toggle_keys))
}

/// Render the interactive fix selector with page-based pagination.
///
/// `visible` is the list of item indices to display (after filtering).
/// `vis_cursor` is the cursor position within `visible`.
/// `vis_page_start` is the page-start position within `visible`.
pub fn print_selector(
    w: &mut impl Write,
    items: &[SelectItem],
    visible: &[usize],
    vis_cursor: usize,
    vis_page_start: usize,
    toggle_keys: &[ToggleKey],
    filter: SelectFilter,
) -> io::Result<()> {
    let view: Vec<&SelectItem> = visible.iter().map(|&i| &items[i]).collect();
    let max_lines = max_item_lines_for(!toggle_keys.is_empty());

    let view_page_end = page_end(&view, vis_page_start, max_lines);

    let total_vis = view.len();
    let (total_pages, current_page) = {
        let mut pages = 0;
        let mut cur = 1;
        let mut s = 0;
        while s < total_vis {
            let e = page_end(&view, s, max_lines);
            pages += 1;
            if vis_page_start >= s && vis_page_start < e {
                cur = pages;
            }
            s = e;
        }
        (pages.max(1), cur)
    };
    let paginated = total_pages > 1;

    let filter_label = if filter != SelectFilter::All {
        format!("  {DIM}showing{RESET} {YELLOW}{}{RESET}", filter.label())
    } else {
        String::new()
    };

    // Items first (top of screen)
    if total_vis == 0 {
        writeln!(
            w,
            "\n  {DIM}No items match the current filter ({}).{RESET}\n",
            filter.label()
        )?;
    } else {
        let mut last_group: Option<&str> = None;
        for (vi, &real_idx) in visible
            .iter()
            .enumerate()
            .take(view_page_end)
            .skip(vis_page_start)
        {
            let item = &items[real_idx];
            if last_group != Some(item.group_path.as_str()) {
                if last_group.is_some() {
                    writeln!(w)?;
                }
                writeln!(w, "  {}", item.group_header)?;
                writeln!(w, "     {DIM}Config: {}{RESET}", item.group_path)?;
                last_group = Some(&item.group_path);
            }
            let arrow = if vi == vis_cursor {
                format!("{CYAN}{BOLD}▸{RESET}")
            } else {
                " ".to_string()
            };
            let check = if item.selected {
                format!("{GREEN}●{RESET}")
            } else {
                format!("{DIM}○{RESET}")
            };
            let highlight = if vi == vis_cursor { BOLD } else { "" };
            writeln!(w, "     {arrow} {check} {highlight}{}{RESET}", item.label)?;
        }
    }

    writeln!(w)?;

    // Status line
    let selected_count = items.iter().filter(|i| i.selected).count();
    if paginated {
        writeln!(
            w,
            "  {DIM}items {}-{} of {} (page {}/{}) \u{2014} {} selected{RESET}{filter_label}",
            vis_page_start + 1,
            view_page_end,
            total_vis,
            current_page,
            total_pages,
            selected_count,
        )?;
    } else {
        writeln!(
            w,
            "  {DIM}{} selected{RESET}{filter_label}",
            plural(selected_count, "fix", "fixes")
        )?;
    }

    // Shortcuts at the bottom
    writeln!(
        w,
        "  {YELLOW}↑↓{RESET} {DIM}navigate{RESET}  \
         {YELLOW}^u ^d{RESET} {DIM}page{RESET}  \
         {YELLOW}space{RESET} {DIM}toggle{RESET}  \
         {YELLOW}enter{RESET} {DIM}apply{RESET}  \
         {YELLOW}d{RESET} {DIM}diff{RESET}  \
         {YELLOW}f{RESET} {DIM}{}{RESET}  \
         {YELLOW}q{RESET} {DIM}quit{RESET}",
        filter.next_action()
    )?;
    if !toggle_keys.is_empty() {
        let toggles: String = toggle_keys
            .iter()
            .map(|t| format!("{YELLOW}{}{RESET} {DIM}{}{RESET}", t.key, t.label))
            .collect::<Vec<_>>()
            .join("  ");
        writeln!(w, "  {YELLOW}a{RESET} {DIM}all{RESET}  {toggles}")?;
    }

    Ok(())
}

// ── Page navigation helpers ──────────────────────────────────────────
//
// These functions share a consistent definition of "page": a maximal
// contiguous slice of items whose rendered line count fits within
// `max_lines`.  They are used by both `print_selector` (rendering) and
// the selection loop (keyboard navigation).

/// Return the exclusive end index of the page starting at `start`.
///
/// A page is the longest prefix of `view[start..]` that fits in
/// `max_lines` rendered terminal rows. Always returns at least
/// `start + 1` (a page always contains at least one item).
pub fn page_end(view: &[&SelectItem], start: usize, max_lines: usize) -> usize {
    let mut end = start;
    let mut lines = 0;
    let mut last_group: Option<&str> = None;
    while end < view.len() {
        let item = view[end];
        let mut add = 0;
        if last_group != Some(item.group_path.as_str()) {
            if last_group.is_some() {
                add += 1;
            }
            add += 2;
            last_group = Some(&item.group_path);
        }
        add += 1;
        if lines + add > max_lines && end > start {
            break;
        }
        lines += add;
        end += 1;
    }
    end.max(start + 1).min(view.len())
}

/// Return the page-start index for the page that contains `target`.
pub fn find_page_start(view: &[&SelectItem], target: usize, max_lines: usize) -> usize {
    let mut s = 0;
    while s < view.len() {
        let e = page_end(view, s, max_lines);
        if target < e {
            return s;
        }
        s = e;
    }
    0
}

/// Return the page-start index for the previous page (before `current_start`).
pub fn prev_page_start(view: &[&SelectItem], current_start: usize, max_lines: usize) -> usize {
    if current_start == 0 {
        return 0;
    }
    find_page_start(view, current_start - 1, max_lines)
}

/// Return the page-start index for the last page of the view.
pub fn last_page_start(view: &[&SelectItem], max_lines: usize) -> usize {
    if view.is_empty() {
        return 0;
    }
    find_page_start(view, view.len() - 1, max_lines)
}

// ── Diff preview ─────────────────────────────────────────────────────

/// Render a unified-diff style preview of what selected fixes will change.
// ── Myers diff ────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum DiffOp {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Myers diff algorithm — produces the shortest edit script (SES).
/// Same algorithm as `git diff`. O(ND) time where D is the edit distance.
fn myers_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffOp> {
    let n = old.len();
    let m = new.len();
    let max = n + m;
    if max == 0 {
        return Vec::new();
    }

    // v[k] stores the furthest-reaching x on diagonal k.
    // k ranges from -max to +max; we index with offset `max`.
    let mut v = vec![0usize; 2 * max + 1];
    // Record the v-array snapshot at each step d so we can backtrack.
    let mut trace: Vec<Vec<usize>> = Vec::new();

    let mut found = false;
    for d in 0..=(max as isize) {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let ki = (k + max as isize) as usize;
            let mut x = if k == -d || (k != d && v[ki.wrapping_sub(1)] < v[ki.wrapping_add(1)]) {
                v[ki + 1] // move down (insert)
            } else {
                v[ki - 1] + 1 // move right (delete)
            };
            let mut y = (x as isize - k) as usize;
            // Follow the diagonal (equal lines)
            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
            }
            v[ki] = x;
            if x >= n && y >= m {
                found = true;
                break;
            }
            k += 2;
        }
        if found {
            break;
        }
    }

    // Backtrack to recover the edit script
    let mut ops = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (0..trace.len()).rev() {
        let v_prev = &trace[d];
        let k = x as isize - y as isize;
        let ki = (k + max as isize) as usize;

        let prev_k = if k == -(d as isize)
            || (k != d as isize && v_prev[ki.wrapping_sub(1)] < v_prev[ki.wrapping_add(1)])
        {
            k + 1 // came from insert
        } else {
            k - 1 // came from delete
        };
        let prev_ki = (prev_k + max as isize) as usize;
        let prev_x = v_prev[prev_ki];
        let prev_y = (prev_x as isize - prev_k) as usize;

        // Diagonal moves (equal lines) traced backward
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push(DiffOp::Equal(x, y));
        }
        if d > 0 {
            if prev_k == k + 1 {
                y -= 1;
                ops.push(DiffOp::Insert(y));
            } else {
                x -= 1;
                ops.push(DiffOp::Delete(x));
            }
        }
    }
    ops.reverse();
    ops
}

/// Render a unified diff (patch-compatible) with `@@ ... @@` hunk headers.
fn render_unified_diff(
    w: &mut impl Write,
    old: &[&str],
    new: &[&str],
    context: usize,
) -> io::Result<()> {
    let ops = myers_diff(old, new);
    if ops.is_empty() {
        return Ok(());
    }

    // Tag each op
    struct TaggedLine<'a> {
        tag: char, // ' ', '-', '+'
        text: &'a str,
        old_line: Option<usize>, // 0-based
        new_line: Option<usize>,
    }

    let mut lines: Vec<TaggedLine> = Vec::new();
    for op in &ops {
        match op {
            DiffOp::Equal(oi, ni) => lines.push(TaggedLine {
                tag: ' ',
                text: old[*oi],
                old_line: Some(*oi),
                new_line: Some(*ni),
            }),
            DiffOp::Delete(oi) => lines.push(TaggedLine {
                tag: '-',
                text: old[*oi],
                old_line: Some(*oi),
                new_line: None,
            }),
            DiffOp::Insert(ni) => lines.push(TaggedLine {
                tag: '+',
                text: new[*ni],
                old_line: None,
                new_line: Some(*ni),
            }),
        }
    }

    // Build hunks with context
    let changes: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.tag != ' ')
        .map(|(i, _)| i)
        .collect();
    if changes.is_empty() {
        return Ok(());
    }

    // Group changes into hunks (merge if context lines overlap)
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    let mut start = changes[0].saturating_sub(context);
    let mut end = (changes[0] + context + 1).min(lines.len());
    for &ci in &changes[1..] {
        let cs = ci.saturating_sub(context);
        let ce = (ci + context + 1).min(lines.len());
        if cs <= end {
            end = ce;
        } else {
            hunks.push((start, end));
            start = cs;
            end = ce;
        }
    }
    hunks.push((start, end));

    for (hs, he) in hunks {
        // Compute @@ header line numbers
        let old_start = lines[hs]
            .old_line
            .unwrap_or_else(|| lines[hs..].iter().find_map(|l| l.old_line).unwrap_or(0));
        let new_start = lines[hs]
            .new_line
            .unwrap_or_else(|| lines[hs..].iter().find_map(|l| l.new_line).unwrap_or(0));
        let old_count = lines[hs..he].iter().filter(|l| l.tag != '+').count();
        let new_count = lines[hs..he].iter().filter(|l| l.tag != '-').count();

        writeln!(
            w,
            "  {CYAN}@@ -{},{} +{},{} @@{RESET}",
            old_start + 1,
            old_count,
            new_start + 1,
            new_count,
        )?;
        for line in &lines[hs..he] {
            match line.tag {
                '-' => writeln!(w, "  {RED}-{}{RESET}", line.text)?,
                '+' => writeln!(w, "  {GREEN}+{}{RESET}", line.text)?,
                _ => writeln!(w, "  {DIM} {}{RESET}", line.text)?,
            }
        }
    }
    Ok(())
}

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

        // Compute real diff: apply fixes to a temp copy, then diff line-by-line
        let original = std::fs::read_to_string(path).unwrap_or_default();
        let mgr = &managers[fixes[0].0.manager_idx];

        // Apply all fixes for this path to a temp copy with unpredictable name
        let tmp_dir = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_path = tmp_dir.join(format!(".depsguard-preview-{}-{nonce}", std::process::id()));
        std::fs::write(&tmp_path, &original)?;
        for (_item, rec) in fixes {
            let _ = crate::fix::apply_fix(mgr.kind, &tmp_path, rec);
        }
        let modified = std::fs::read_to_string(&tmp_path).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp_path);

        // Unified diff with context
        let old_lines: Vec<&str> = original.lines().collect();
        let new_lines: Vec<&str> = modified.lines().collect();
        render_unified_diff(w, &old_lines, &new_lines, 1)?;

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
            discovered: false,
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
        let vis: Vec<usize> = (0..items.len()).collect();
        print_selector(&mut buf, &items, &vis, 0, 0, &[], SelectFilter::All).unwrap();
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
        let vis: Vec<usize> = (0..items.len()).collect();
        print_selector(&mut buf, &items, &vis, 1, 0, &[], SelectFilter::All).unwrap();
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

    #[test]
    fn toggle_manager_selects_and_deselects() {
        use crate::manager::{CheckStatus, ManagerKind, Recommendation};

        let managers = vec![
            ManagerInfo {
                kind: ManagerKind::Npm,
                version: "11.12.0".into(),
                config_path: std::path::PathBuf::from("/tmp/.npmrc"),
                recommendations: vec![Recommendation {
                    key: "k".into(),
                    description: "d".into(),
                    expected: "v".into(),
                    status: CheckStatus::Missing,
                }],
                discovered: false,
            },
            ManagerInfo {
                kind: ManagerKind::Uv,
                version: "0.9.0".into(),
                config_path: std::path::PathBuf::from("/tmp/uv.toml"),
                recommendations: vec![Recommendation {
                    key: "k2".into(),
                    description: "d2".into(),
                    expected: "v2".into(),
                    status: CheckStatus::Missing,
                }],
                discovered: false,
            },
        ];
        let mut items = build_fix_items(&managers);
        assert_eq!(items.len(), 2);
        assert!(items[0].selected);
        assert!(items[1].selected);

        toggle_manager(&mut items, &managers, ManagerKind::Npm);
        assert!(!items[0].selected);
        assert!(items[1].selected);

        toggle_manager(&mut items, &managers, ManagerKind::Npm);
        assert!(items[0].selected);
        assert!(items[1].selected);
    }

    #[test]
    fn build_toggle_keys_assigns_unique_chars() {
        use crate::manager::{CheckStatus, ManagerKind, Recommendation};

        let managers = vec![
            ManagerInfo {
                kind: ManagerKind::Npm,
                version: "11.12.0".into(),
                config_path: std::path::PathBuf::from("/tmp/.npmrc"),
                recommendations: vec![Recommendation {
                    key: "k".into(),
                    description: "d".into(),
                    expected: "v".into(),
                    status: CheckStatus::Missing,
                }],
                discovered: false,
            },
            ManagerInfo {
                kind: ManagerKind::Uv,
                version: "0.9.0".into(),
                config_path: std::path::PathBuf::from("/tmp/uv.toml"),
                recommendations: vec![Recommendation {
                    key: "k2".into(),
                    description: "d2".into(),
                    expected: "v2".into(),
                    status: CheckStatus::Missing,
                }],
                discovered: false,
            },
        ];
        let items = build_fix_items(&managers);
        let keys = build_toggle_keys(&items, &managers);
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0].key, keys[1].key);
        assert_eq!(keys[0].label, ".npmrc");
        assert_eq!(keys[1].label, "uv.toml");
    }

    // ── SelectFilter tests ──────────────────────────────────────────

    #[test]
    fn filter_cycles_through_all_states() {
        let f = SelectFilter::All;
        assert_eq!(f.next(), SelectFilter::SelectedOnly);
        assert_eq!(f.next().next(), SelectFilter::UnselectedOnly);
        assert_eq!(f.next().next().next(), SelectFilter::All);
    }

    #[test]
    fn filtered_indices_all() {
        let items = vec![
            SelectItem {
                manager_idx: 0,
                rec_idx: 0,
                label: "a".into(),
                group_path: "p".into(),
                group_header: "h".into(),
                selected: true,
            },
            SelectItem {
                manager_idx: 0,
                rec_idx: 1,
                label: "b".into(),
                group_path: "p".into(),
                group_header: "h".into(),
                selected: false,
            },
        ];
        assert_eq!(filtered_indices(&items, SelectFilter::All), vec![0, 1]);
        assert_eq!(
            filtered_indices(&items, SelectFilter::SelectedOnly),
            vec![0]
        );
        assert_eq!(
            filtered_indices(&items, SelectFilter::UnselectedOnly),
            vec![1]
        );
    }

    // ── Myers diff tests ────────────────────────────────────────────

    #[test]
    fn myers_diff_identical() {
        let a = vec!["foo", "bar"];
        let ops = super::myers_diff(&a, &a);
        assert!(ops
            .iter()
            .all(|op| matches!(op, super::DiffOp::Equal(_, _))));
    }

    #[test]
    fn myers_diff_empty() {
        let ops = super::myers_diff(&[], &[]);
        assert!(ops.is_empty());
    }

    #[test]
    fn myers_diff_add_line() {
        let old = vec!["a", "b"];
        let new = vec!["a", "x", "b"];
        let ops = super::myers_diff(&old, &new);
        let inserts: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, super::DiffOp::Insert(_)))
            .collect();
        assert_eq!(inserts.len(), 1);
    }

    #[test]
    fn myers_diff_remove_line() {
        let old = vec!["a", "x", "b"];
        let new = vec!["a", "b"];
        let ops = super::myers_diff(&old, &new);
        let deletes: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, super::DiffOp::Delete(_)))
            .collect();
        assert_eq!(deletes.len(), 1);
    }

    #[test]
    fn myers_diff_completely_different() {
        let old = vec!["a", "b"];
        let new = vec!["x", "y"];
        let ops = super::myers_diff(&old, &new);
        let changes: Vec<_> = ops
            .iter()
            .filter(|op| !matches!(op, super::DiffOp::Equal(_, _)))
            .collect();
        assert!(!changes.is_empty());
    }
}
