// Safe executable resolution: locate a program on `PATH` *excluding* the
// current working directory and any relative `PATH` entries, to avoid
// binary-planting / "executable hijacking" attacks (most notably on
// Windows, where `CreateProcess` normally searches the application
// directory and the CWD before `PATH`).
//
// Rules:
// - Names containing a path separator, or absolute paths, are only
//   accepted if they resolve to an existing regular file; no lookup is
//   performed.
// - For bare names, we walk `PATH` ourselves, skipping empty and
//   relative entries (including `.`), and return the first absolute
//   path that resolves to a regular, executable file.
// - On Windows we also append each extension from `PATHEXT` (defaulting
//   to the standard set) when the name does not already carry one.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve a bare executable name to an absolute path on `PATH`,
/// *excluding* the current working directory and any relative entries.
///
/// Returns `None` when no matching executable is found.
pub fn resolve_exe(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    let as_path = Path::new(name);
    if as_path.components().count() > 1 || as_path.is_absolute() {
        return if is_executable_file(as_path) {
            Some(as_path.to_path_buf())
        } else {
            None
        };
    }

    let path_env = env::var_os("PATH")?;
    let extensions = windows_pathext();

    for dir in env::split_paths(&path_env) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        // Reject relative PATH entries (this is how we exclude CWD and
        // any other directory the tool might be launched from).
        if !dir.is_absolute() {
            continue;
        }

        let direct = dir.join(name);
        if is_executable_file(&direct) {
            return Some(direct);
        }

        if cfg!(windows) && !has_extension(name) {
            for ext in &extensions {
                let mut file_name = OsString::from(name);
                file_name.push(ext);
                let candidate = dir.join(file_name);
                if is_executable_file(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Build a `Command` rooted at an absolute, PATH-resolved executable path.
/// Returns `None` when the program cannot be located safely.
pub fn safe_command(name: &str) -> Option<Command> {
    resolve_exe(name).map(Command::new)
}

fn has_extension(name: &str) -> bool {
    Path::new(name).extension().is_some()
}

fn windows_pathext() -> Vec<OsString> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let raw = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    let Some(s) = raw.to_str() else {
        return vec![
            OsString::from(".COM"),
            OsString::from(".EXE"),
            OsString::from(".BAT"),
            OsString::from(".CMD"),
        ];
    };
    s.split(';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(OsString::from)
        .collect()
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && (m.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

#[cfg(windows)]
fn is_executable_file(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn is_executable_file(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_name() {
        assert!(resolve_exe("").is_none());
    }

    #[test]
    fn absolute_path_returns_only_if_file() {
        let missing = std::env::temp_dir().join("definitely-not-an-executable-xyz");
        assert!(resolve_exe(missing.to_str().unwrap()).is_none());
    }

    #[test]
    fn rejects_relative_path_components() {
        // A name containing separators bypasses the PATH lookup but is
        // still only accepted when it resolves to a real file.
        assert!(resolve_exe("./nope-not-here").is_none());
        assert!(resolve_exe("nested/nope-not-here").is_none());
    }

    #[test]
    fn does_not_search_cwd_when_path_missing() {
        // Even with no PATH set, we must never pick up an executable
        // from the current directory implicitly.
        let _lock = TEST_LOCK.lock().unwrap();
        let prev = std::env::var_os("PATH");
        unsafe { std::env::remove_var("PATH") };
        let result = resolve_exe("depsguard-not-real");
        match prev {
            Some(v) => unsafe { std::env::set_var("PATH", v) },
            None => {}
        }
        assert!(result.is_none());
    }

    #[test]
    fn ignores_relative_path_entries() {
        let _lock = TEST_LOCK.lock().unwrap();
        let prev = std::env::var_os("PATH");
        // Set PATH to a single entry of "." (relative). We must ignore
        // it and return None rather than executing anything from the CWD.
        unsafe { std::env::set_var("PATH", ".") };
        let result = resolve_exe("sh");
        match prev {
            Some(v) => unsafe { std::env::set_var("PATH", v) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        assert!(
            result.is_none(),
            "resolve_exe must not honour relative PATH entries, got {result:?}"
        );
    }

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
