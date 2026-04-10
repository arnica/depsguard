// Core types shared across all package manager scanners.

use std::path::{Path, PathBuf};

/// Result of checking a single security setting against its expected value.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    /// The setting matches the expected value.
    Ok,
    /// The setting is not configured at all.
    Missing,
    /// The config file itself does not exist yet.
    FileMissing,
    /// The setting exists but has an incorrect value.
    WrongValue(String),
    /// The feature is not available (e.g. tool version too old). Not auto-fixable.
    Unsupported(String),
}

impl CheckStatus {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        matches!(self, CheckStatus::Ok)
    }

    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, CheckStatus::Unsupported(_))
    }
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Ok => write!(f, "OK"),
            CheckStatus::Missing => write!(f, "Not set"),
            CheckStatus::FileMissing => write!(f, "file missing"),
            CheckStatus::WrongValue(v) => write!(f, "Current: {v}"),
            CheckStatus::Unsupported(v) => write!(f, "{v}"),
        }
    }
}

/// A single security recommendation for a package manager config file.
#[derive(Debug, Clone)]
pub struct Recommendation {
    pub key: String,
    pub description: String,
    pub expected: String,
    pub status: CheckStatus,
}

impl Recommendation {
    #[must_use]
    pub fn needs_fix(&self) -> bool {
        !self.status.is_ok() && !self.status.is_unsupported()
    }
}

/// Supported package managers that DepsGuard can scan and fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagerKind {
    Npm,
    Pnpm,
    PnpmGlobal,
    PnpmWorkspace,
    Bun,
    Uv,
    Yarn,
    Renovate,
    Dependabot,
}

impl ManagerKind {
    /// Managers scanned via user-level config (version detection + fixed path).
    pub const USER_LEVEL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::PnpmGlobal,
        ManagerKind::Bun,
        ManagerKind::Uv,
        ManagerKind::Yarn,
    ];

    pub const ALL: &[ManagerKind] = &[
        ManagerKind::Npm,
        ManagerKind::Pnpm,
        ManagerKind::PnpmGlobal,
        ManagerKind::PnpmWorkspace,
        ManagerKind::Bun,
        ManagerKind::Uv,
        ManagerKind::Yarn,
        ManagerKind::Renovate,
        ManagerKind::Dependabot,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ManagerKind::Npm => "npm",
            ManagerKind::Pnpm | ManagerKind::PnpmGlobal => "pnpm",
            ManagerKind::PnpmWorkspace => "pnpm-workspace",
            ManagerKind::Bun => "bun",
            ManagerKind::Uv => "uv",
            ManagerKind::Yarn => "yarn",
            ManagerKind::Renovate => "renovate",
            ManagerKind::Dependabot => "dependabot",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ManagerKind::Npm => "📦",
            ManagerKind::Pnpm | ManagerKind::PnpmGlobal | ManagerKind::PnpmWorkspace => "⚡",
            ManagerKind::Bun => "🥟",
            ManagerKind::Uv => "🐍",
            ManagerKind::Yarn => "🧶",
            ManagerKind::Renovate => "🔄",
            ManagerKind::Dependabot => "🤖",
        }
    }

    /// Look up a ManagerKind by its CLI name (case-insensitive).
    pub fn from_name(name: &str) -> Option<ManagerKind> {
        ManagerKind::ALL
            .iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
            .copied()
    }

    /// All valid names for use in `--exclude` (user-facing).
    pub fn valid_names() -> Vec<&'static str> {
        let mut names: Vec<&str> = Vec::new();
        for k in Self::ALL {
            let n = k.name();
            if n != "pnpm-workspace" && !names.contains(&n) {
                names.push(n);
            }
        }
        names
    }

}

/// A detected package manager with its version, config location, and security check results.
#[derive(Debug, Clone)]
pub struct ManagerInfo {
    pub kind: ManagerKind,
    pub version: String,
    pub config_path: PathBuf,
    pub recommendations: Vec<Recommendation>,
    /// True if this entry was found via search (not a user-level global config).
    pub discovered: bool,
}

impl ManagerInfo {
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.recommendations.iter().all(|r| r.status.is_ok())
    }
}

/// Target OS for config path resolution. Allows testing all platforms from any host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetOs {
    Linux,
    MacOs,
    Windows,
}

impl TargetOs {
    /// Detect the current OS at runtime.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            TargetOs::MacOs
        } else if cfg!(target_os = "windows") {
            TargetOs::Windows
        } else {
            TargetOs::Linux
        }
    }
}

/// Types of config files discovered in repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoConfigKind {
    PnpmWorkspace,
    Npmrc,
    YarnRc,
    Renovate,
    Dependabot,
}

/// Build an `Unsupported` recommendation for features gated behind a minimum version.
pub fn unsupported_rec(
    key: &str,
    desc: &str,
    expected: &str,
    manager_name: &str,
    min_major: u64,
    min_minor: u64,
    have_version: &str,
) -> Recommendation {
    Recommendation {
        key: key.into(),
        description: desc.into(),
        expected: expected.into(),
        status: CheckStatus::Unsupported(format!(
            "requires {manager_name} \u{2265} {min_major}.{min_minor} (have {have_version})"
        )),
    }
}

/// Return `Missing` or `FileMissing` based on whether the config file exists on disk.
pub fn missing_status_for_path(path: &Path) -> CheckStatus {
    if path.exists() {
        CheckStatus::Missing
    } else {
        CheckStatus::FileMissing
    }
}
