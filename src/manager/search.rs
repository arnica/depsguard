// Filesystem search for repo-level config files (pnpm-workspace, .npmrc, renovate, etc.).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::paths::home_dir;
use super::types::RepoConfigKind;

/// Directories to skip when searching downward for project files.
const SKIP_DIRS: &[&str] = &[
    "Library",
    "node_modules",
    ".npm",
    ".pnpm-store",
    ".cargo",
    ".rustup",
    ".m2",
    ".gradle",
    ".cache",
    "go",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    "__pycache__",
    ".git",
    ".hg",
    ".svn",
    ".Trash",
    ".pyenv",
    ".rbenv",
    "vendor",
];

/// Max depth for downward search to avoid excessive traversal.
const MAX_SEARCH_DEPTH: usize = 8;

/// Filenames that indicate a Renovate config.
const RENOVATE_FILENAMES: &[&str] = &[
    "renovate.json",
    "renovate.json5",
    ".renovaterc",
    ".renovaterc.json",
    ".renovaterc.json5",
];

/// Search from the current working directory downward for all recognized repo config files.
pub fn find_repo_configs(on_dir: &mut dyn FnMut(&Path)) -> Vec<(PathBuf, RepoConfigKind)> {
    let mut results = Vec::new();
    let home = home_dir();
    let start = env::current_dir().unwrap_or_else(|_| home.clone());
    search_downward(&start, 0, &home, &mut results, on_dir);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by(|a, b| a.0 == b.0);
    results
}

pub(super) fn classify_file(name: &str, parent: &Path, home: &Path) -> Option<RepoConfigKind> {
    match name {
        "pnpm-workspace.yaml" => Some(RepoConfigKind::PnpmWorkspace),
        ".npmrc" => {
            if parent == home {
                None
            } else {
                Some(RepoConfigKind::Npmrc)
            }
        }
        ".yarnrc.yml" => {
            if parent == home {
                None
            } else {
                Some(RepoConfigKind::YarnRc)
            }
        }
        "dependabot.yml" | "dependabot.yaml" => {
            let parent_name = parent.file_name().and_then(|n| n.to_str());
            if parent_name == Some(".github") {
                Some(RepoConfigKind::Dependabot)
            } else {
                None
            }
        }
        _ => {
            if RENOVATE_FILENAMES.contains(&name) {
                Some(RepoConfigKind::Renovate)
            } else {
                None
            }
        }
    }
}

fn search_downward(
    dir: &Path,
    depth: usize,
    home: &Path,
    results: &mut Vec<(PathBuf, RepoConfigKind)>,
    on_dir: &mut dyn FnMut(&Path),
) {
    if depth > MAX_SEARCH_DEPTH {
        return;
    }
    on_dir(dir);
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if !file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(kind) = classify_file(name, dir, home) {
                    results.push((path, kind));
                }
            }
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name == ".github" || (!name.starts_with('.') && !SKIP_DIRS.contains(&name)) {
            search_downward(&path, depth + 1, home, results, on_dir);
        }
    }
}
