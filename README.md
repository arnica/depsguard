# depsguard

[![CI](https://github.com/arnica/depsguard/actions/workflows/ci.yml/badge.svg)](https://github.com/arnica/depsguard/actions/workflows/ci.yml)
[![Security Audit](https://github.com/arnica/depsguard/actions/workflows/audit.yml/badge.svg)](https://github.com/arnica/depsguard/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/depsguard.svg)](https://crates.io/crates/depsguard)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.74-orange.svg)](https://blog.rust-lang.org/2023/11/16/Rust-1.74.0.html)

```text
     _                                          _
  __| | ___ _ __  ___  __ _ _   _  __ _ _ __ __| |
 / _` |/ _ \ '_ \/ __|/ _` | | | |/ _` | '__/ _` |
| (_| |  __/ |_) \__ \ (_| | |_| | (_| | | | (_| |
 \__,_|\___| .__/|___/\__, |\__,_|\__,_|_|  \__,_|
           |_|        |___/
```

Guard your dependencies against supply chain attacks. **Single static binary, zero Rust crate dependencies.**

By **[[arnica](https://arnica.io?utm_source=depsguard&utm_medium=referral&utm_campaign=community)]**

## Table of contents

- [Overview](#overview)
- [Install](#install)
- [Usage](#usage)
- [What gets checked](#what-gets-checked)
- [Config file locations](#config-file-locations)
- [Urgent security fix](#urgent-security-fix)
- [Backups and restore](#backups-and-restore)
- [How it works](#how-it-works)
- [Troubleshooting](#troubleshooting)
- [Help & feedback](#help--feedback)
- [See also](#see-also)
- [License](#license)

## Overview

DepsGuard looks for **npm**, **pnpm**, **yarn**, **bun**, **uv**, **pip**, **poetry**, and **aube** on your machine, reads their config files, compares them to recommended supply-chain settings, and can **apply fixes interactively**. It also scans for **Renovate**, **Dependabot**, and **Docker** configs in your repos. It never runs package installs; it only edits config files you approve, and it writes **backups** before any change.

### Key features

- Interactive TUI: scan, review, toggle fixes, apply
- `scan` subcommand for read-only reporting
- `restore` subcommand to pick a backup and roll back a file
- Report-only Docker checks for floating images and Dockerfile package-manager hardening
- Cross-platform: Linux, macOS, Windows
- No bundled third-party Rust crates (stdlib + small amount of platform FFI for the terminal)

### Tech stack

| Area | Details |
|------|---------|
| Language | Rust (MSRV **1.74**, see `Cargo.toml`) |
| CLI / TUI | `src/main.rs`, `src/ui.rs`, `src/term.rs` |
| Config logic | `src/manager.rs`, `src/fix.rs` |
| Website | Static site under `docs/` (separate from the binary) |

## Install

### Prebuilt binaries

Each [GitHub Release](https://github.com/arnica/depsguard/releases) includes archives for:

- Linux: `x86_64` (glibc), `x86_64` (musl), `aarch64` (glibc)
- macOS: Intel and Apple Silicon
- Windows: `x86_64` ZIP containing `depsguard.exe`

Download the archive for your platform, unpack it, and put the binary on your `PATH`.

Verify integrity using the matching `.sha256` file next to each asset on the release page.

### Install by platform

#### Linux (Debian/Ubuntu via APT)

```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://depsguard.com/apt/gpg.key | sudo gpg --dearmor -o /etc/apt/keyrings/depsguard.gpg
echo "deb [arch=amd64,arm64 signed-by=/etc/apt/keyrings/depsguard.gpg] https://depsguard.com/apt stable main" | sudo tee /etc/apt/sources.list.d/depsguard.list >/dev/null
sudo apt update
sudo apt install depsguard
```

#### macOS / Linux (Homebrew)

```bash
# Homebrew
brew install depsguard
```

DepsGuard is in [homebrew-core](https://github.com/Homebrew/homebrew-core/blob/HEAD/Formula/d/depsguard.rb), so no custom tap is required.

> **Migrating from the old `arnica/depsguard` tap?** Switch to the core formula once:
>
> ```bash
> brew uninstall depsguard
> brew untap arnica/depsguard
> brew update
> brew install depsguard
> ```

#### Windows

```powershell
# WinGet
winget install Arnica.DepsGuard

# Scoop
scoop bucket add depsguard https://github.com/arnica/depsguard
scoop install depsguard
```

Or download manually via PowerShell:

```powershell
$zip = "$env:TEMP\\depsguard.zip"
Invoke-WebRequest -Uri "https://github.com/arnica/depsguard/releases/latest/download/depsguard-x86_64-pc-windows-msvc.zip" -OutFile $zip
Expand-Archive -LiteralPath $zip -DestinationPath "$env:TEMP\\depsguard" -Force
Copy-Item "$env:TEMP\\depsguard\\depsguard.exe" "$HOME\\AppData\\Local\\Microsoft\\WindowsApps\\depsguard.exe" -Force
depsguard.exe --help
```

### crates.io

```bash
cargo install depsguard
```

Requires a [Rust toolchain](https://rustup.rs/) with `cargo`.

### Package managers (when published by your vendor)

If your organization ships DepsGuard via Homebrew, Scoop, or WinGet, use their instructions. **Setting up or automating those channels** (Homebrew core PRs, buckets, WinGet PRs, CI secrets) is maintainer documentation — see [`AGENTS.md`](AGENTS.md) under *Release & distribution*.

#### App stores / package managers

| Channel | Linux | macOS | Windows | Install command |
|---------|-------|-------|---------|-----------------|
| APT (custom repo) | yes | no | no | `sudo apt install depsguard` (after repo setup above) |
| crates.io | yes | yes | yes | `cargo install depsguard` |
| Homebrew (homebrew-core) | yes | yes | no | `brew install depsguard` |
| Scoop (custom bucket) | no | no | yes | `scoop bucket add depsguard https://github.com/arnica/depsguard ; scoop install depsguard` |
| WinGet | no | no | yes | `winget install Arnica.DepsGuard` |

### Update to the latest version

Use whichever channel you installed with:

| Channel | Upgrade command |
|---------|-----------------|
| Homebrew | `brew update && brew upgrade depsguard` |
| APT (custom repo) | `sudo apt update && sudo apt install --only-upgrade depsguard` |
| crates.io | `cargo install --force depsguard` (reinstalls the latest release) |
| Scoop | `scoop update && scoop update depsguard` |
| WinGet | `winget upgrade Arnica.DepsGuard` |

Check your installed version any time with `depsguard --version`, and see the [releases page](https://github.com/arnica/depsguard/releases) for the newest version.

### Build from source

```bash
git clone https://github.com/arnica/depsguard.git
cd depsguard
cargo build --release
```

The binary is `target/release/depsguard` (`.exe` on Windows). Rust **1.74+** is required.

## Usage

```bash
depsguard              # interactive: scan, choose fixes, apply
depsguard scan         # report only; no writes (exits 1 if action is needed)
depsguard --no-search  # skip recursive file search, check local configs only
depsguard restore      # restore from a previous backup
depsguard --help       # CLI help
```

### How to use

1. **Install** – pick your platform [above](#install).
2. **Run** `depsguard` to launch the interactive TUI. It scans your system and shows a table of findings. Press any key to continue to the fix selector. Repo-level config discovery starts from the current directory and searches downward. Use `depsguard scan` for a read-only report, or `depsguard --no-search` to skip the recursive file search and only check user-level configs.
   > **Note:** some settings require a minimum version. If your version is too old you'll see:
   > `ℹ min-release-age – requires npm ≥ 11.10 (have 10.2.0)`.
   > Upgrade with `npm install -g npm@latest` and re-run.
3. **Navigate & select** – use `↑` `↓` to move through the list (`^u` `^d` to page). Press `Space` to toggle a fix on or off. Use quick-filter keys to bulk-select by file: `a` all, `n` .npmrc, `u` uv.toml, etc. – press once to select, again to deselect, a third time to clear the filter. Press `f` to show only currently selected fixes.
4. **Preview** – press `d` to see a diff of what will change before you commit to anything.
5. **Apply** – press `Enter` to apply the selected fixes. A timestamped backup is created before any file is written.
6. **Rescan** – DepsGuard automatically reruns the scan after applying, so you can verify everything is green.
7. **Restore** – run `depsguard restore` at any time to roll back from the backup list. Press `q` or `Esc` to quit.

## What gets checked

| Manager | Config | Setting | Target | Why |
|---------|--------|---------|--------|-----|
| npm | `~/.npmrc` | `min-release-age` | `7` (days) | Delay brand-new releases (requires npm >= 11.10) |
| npm/pnpm | `~/.npmrc` | `ignore-scripts` | `true` | Reduce install-script risk (npm honors this in `.npmrc`; pnpm >= 11 reads it from `pnpm-workspace.yaml` / global `config.yaml`, not `.npmrc`) |
| pnpm | `~/.npmrc` | `minimum-release-age` | `10080` (minutes) | Delay new versions by 7 days (pnpm 10.16–10.x only; pnpm >= 11 ignores `.npmrc` — use `pnpm-workspace.yaml`) |
| pnpm | global `rc` (pnpm <= 10) | `minimum-release-age` | `10080` (minutes) | Delay new versions by 7 days (requires pnpm >= 10.16) |
| pnpm | global `rc` (pnpm <= 10) | `block-exotic-subdeps` | `true` | Block untrusted transitive deps (requires pnpm >= 10.26) |
| pnpm | global `rc` (pnpm <= 10) | `trust-policy` | `no-downgrade` | Block provenance downgrades (requires pnpm >= 10.21) |
| pnpm | global `rc` (pnpm <= 10) | `strict-dep-builds` | `true` | Fail on unreviewed build scripts (requires pnpm >= 10.3) |
| pnpm | global `rc` (pnpm <= 10) | `ignore-scripts` | `true` | Block malicious install scripts |
| pnpm | global `config.yaml` (pnpm >= 11) | `minimumReleaseAge` | `10080` (minutes) | Delay new versions by 7 days |
| pnpm | global `config.yaml` (pnpm >= 11) | `blockExoticSubdeps` | `true` | Block untrusted transitive deps |
| pnpm | global `config.yaml` (pnpm >= 11) | `trustPolicy` | `no-downgrade` | Block provenance downgrades |
| pnpm | global `config.yaml` (pnpm >= 11) | `strictDepBuilds` | `true` | Fail on unreviewed build scripts |
| pnpm | global `config.yaml` (pnpm >= 11) | `ignoreScripts` | `true` | Block malicious install scripts |
| yarn | `.yarnrc.yml` | `npmMinimalAgeGate` | `7d` | Delay new versions by 7 days (requires yarn >= 4.10) |
| pnpm | `pnpm-workspace.yaml` | `minimumReleaseAge` | `10080` (minutes) | Delay new versions by 7 days (requires pnpm >= 10.16) |
| pnpm | `pnpm-workspace.yaml` | `strictDepBuilds` | `true` | Fail on unreviewed build scripts (requires pnpm >= 10.3) |
| pnpm | `pnpm-workspace.yaml` | `trustPolicy` | `no-downgrade` | Block provenance downgrades (requires pnpm >= 10.21) |
| pnpm | `pnpm-workspace.yaml` | `blockExoticSubdeps` | `true` | Block untrusted transitive deps (requires pnpm >= 10.26) |
| pnpm | `pnpm-workspace.yaml` | `ignoreScripts` | `true` | Block malicious install scripts (requires pnpm >= 10.16) |
| bun | `~/.bunfig.toml` | `install.minimumReleaseAge` | `604800` (seconds) | ~7 day delay (requires bun >= 1.3) |
| aube | `~/.npmrc` | `minimumReleaseAge` | `10080` (minutes) | Delay new versions by 7 days |
| uv | `uv.toml` | `exclude-newer` | `7 days` | Delay new publishes (requires uv >= 0.9.17) |
| pip | `pip.conf` (`[install]`) | `uploaded-prior-to` | `P7D` (7 days) | Delay new publishes (requires pip >= 26.1) |
| poetry | `config.toml` (`[solver]`) | `min-release-age` | `7` (days) | Delay new publishes (requires poetry >= 2.4) |
| renovate | `renovate.json` etc. | `minimumReleaseAge` | `7 days` | Delay dependency update PRs by 7 days |
| dependabot | `.github/dependabot.yml` | `cooldown.default-days` | `7` | Delay dependency update PRs by 7 days |
| docker | `Dockerfile`, `Dockerfile.*` | image references | version tag / digest | Flag `latest`, missing tags, and warn on missing digest pins |
| docker | `docker-compose.yml`, `docker-compose.yaml`, `compose.yml`, `compose.yaml` | service images | version tag / digest | Flag `latest`, missing tags, and warn on missing digest pins |
| docker | `Dockerfile`, `Dockerfile.*` | package-manager install commands | prior hardening config | Require npm/pnpm/pip/uv/poetry hardening before Docker build installs packages |

## Config file locations

| Manager | Linux | macOS | Windows |
|---------|-------|-------|---------|
| npm/pnpm/aube | `~/.npmrc` | `~/.npmrc` | `%USERPROFILE%\.npmrc` |
| pnpm global (pnpm <= 10) | `$XDG_CONFIG_HOME/pnpm/rc` or `~/.config/pnpm/rc` | `$XDG_CONFIG_HOME/pnpm/rc` or `~/Library/Preferences/pnpm/rc` | `%LOCALAPPDATA%\pnpm\config\rc` |
| pnpm global (pnpm >= 11) | `$XDG_CONFIG_HOME/pnpm/config.yaml` or `~/.config/pnpm/config.yaml` | `$XDG_CONFIG_HOME/pnpm/config.yaml` or `~/Library/Preferences/pnpm/config.yaml` | `%LOCALAPPDATA%\pnpm\config\config.yaml` |
| yarn | `~/.yarnrc.yml` | `~/.yarnrc.yml` | `%USERPROFILE%\.yarnrc.yml` |
| pnpm | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` |
| bun | `$XDG_CONFIG_HOME/.bunfig.toml` or `~/.bunfig.toml` | `$XDG_CONFIG_HOME/.bunfig.toml` or `~/.bunfig.toml` | `%USERPROFILE%\.bunfig.toml` |
| uv | `$XDG_CONFIG_HOME/uv/uv.toml` or `~/.config/uv/uv.toml` | `$XDG_CONFIG_HOME/uv/uv.toml` or `~/.config/uv/uv.toml` | `%APPDATA%\uv\uv.toml` |
| pip | `$XDG_CONFIG_HOME/pip/pip.conf` or `~/.config/pip/pip.conf` | `~/Library/Application Support/pip/pip.conf` or `~/.config/pip/pip.conf` (or `$XDG_CONFIG_HOME/pip/pip.conf` when set) | `%APPDATA%\pip\pip.ini` |
| poetry | `$XDG_CONFIG_HOME/pypoetry/config.toml` or `~/.config/pypoetry/config.toml` | `$XDG_CONFIG_HOME/pypoetry/config.toml` (when set) or `~/Library/Application Support/pypoetry/config.toml` | `%APPDATA%\pypoetry\config.toml` |
| renovate | `renovate.json`, `.renovaterc`, `.github/renovate.json`, etc. | (same) | (same) |
| dependabot | `.github/dependabot.yml` | (same) | (same) |
| docker | `Dockerfile`, `Dockerfile.*`, `docker-compose.yml`, `docker-compose.yaml`, `compose.yml`, `compose.yaml` | (same) | (same) |

User-level config files are read from their standard locations (including XDG-based paths where the tool supports them). Repo-level configs are discovered by searching downward from the current directory, skipping known large directories (`node_modules`, `.git`, `target`, `Library`, `.cache`, and others) so scans stay fast. Repo-level `.npmrc`, `.yarnrc.yml`, `pnpm-workspace.yaml`, Renovate configs, Dependabot configs, Dockerfiles, and Docker Compose files are all searched. pnpm settings can live in `~/.npmrc` (pnpm <= 10 only — pnpm >= 11 reads only auth/registry settings from `.npmrc`), the pnpm global config file (`rc` on pnpm <= 10, `config.yaml` on pnpm >= 11), or `pnpm-workspace.yaml`; DepsGuard checks all three locations independently. For pip, uv, and poetry, DepsGuard resolves the single effective user-level config and reports just that file, rather than flagging shadowed files separately. pip and poetry merge their config files by precedence (the highest-precedence file that sets the cooldown wins, or the preferred location if none do); uv reads a single user file (`$XDG_CONFIG_HOME/uv/uv.toml` when `XDG_CONFIG_HOME` is set, otherwise `~/.config/uv/uv.toml`) rather than merging both. For bun, if multiple user-level config files exist (for example both an XDG path and a home-directory path), DepsGuard scans each existing file separately. aube reads the same `~/.npmrc` as npm/pnpm (`minimumReleaseAge`, in minutes) and is also checked on discovered repo-level `.npmrc` files; pip and poetry are scanned at their user-level config (`pip.conf` / `pypoetry/config.toml`). Docker findings are report-only: DepsGuard reports floating image references and package-manager installs that run before hardening config, but it does not rewrite Dockerfiles or Compose files.

## Urgent security fix

If the patched version is newer than your cooldown window, add a narrow exception, install the fix, and then remove the exception.

Prefer a package-specific exception over lowering the global cooldown. That keeps the delay in place for every other dependency.

| Manager | How to bypass the cooldown |
|---------|-----------------------------|
| npm | `npm install <pkg>@<ver> --min-release-age=0` |
| pnpm | Add an entry to `minimumReleaseAgeExclude` in `pnpm-workspace.yaml`, run `pnpm add <pkg>@<ver>`, then remove the entry. Excluding by package name works on pnpm 10.16+; pinning a specific version (`<pkg>@<ver>`) additionally requires pnpm 10.19+. pnpm has no documented CLI override for `minimumReleaseAge`. |
| yarn | Add `<pkg>` (or a glob) to `npmPreapprovedPackages` in `.yarnrc.yml`, or run `YARN_NPM_MINIMAL_AGE_GATE=0s yarn up <pkg>@<ver>` for one command. `npmPreapprovedPackages` exempts matches from all Yarn package gates, not only the age gate. |
| bun | Add `<pkg>` to `install.minimumReleaseAgeExcludes` in a repo-level `bunfig.toml` or user-level `~/.bunfig.toml`, or run `bun add <pkg>@<ver> --minimum-release-age 0`. |
| aube | Add `<pkg>` to `minimumReleaseAgeExclude` in `.npmrc`, or set `AUBE_MINIMUM_RELEASE_AGE=0` (or `npm_config_minimum_release_age=0`) for a single install. |
| uv | Add `"<pkg>" = false` to `exclude-newer-package` in `uv.toml` or `pyproject.toml`, run `uv add <pkg>==<ver>`, then remove the entry. `exclude-newer-package` is a separate per-package override of the global `exclude-newer` cutoff. uv's CLI accepts `--exclude-newer-package PACKAGE=DATE` but not `PACKAGE=false`. |
| pip | Run `pip install <pkg>==<ver> --uploaded-prior-to=P0D` for one install. `P0D` disables the cooldown only for that command; pip has no per-package exclusion in config. |
| poetry | Add `<pkg>` to `solver.min-release-age-exclude` (comma-separated) in `poetry.toml`/`config.toml`, run `poetry add <pkg>@<ver>`, then remove the entry. `solver.min-release-age-exclude-source` exempts every package from a named index instead. |
| Renovate | Security updates already bypass `minimumReleaseAge`. For a version update, add a `packageRules` entry with `matchPackageNames: ["<pkg>"]` and `minimumReleaseAge: null`. |
| Dependabot | Security updates already bypass `cooldown`. For a version update, add `<pkg>` to `cooldown.exclude`. |

Before you bypass the cooldown:

1. Check whether the CVE actually affects your usage.
2. Check whether a known-good older version is already available. A rollback may be safer.
3. Remove temporary exceptions after the upgrade.

## Backups and restore

Before modifying a file, DepsGuard writes a backup to `~/.depsguard/backups/`.

Run `depsguard restore` to list backups and restore one.

## How it works

```
src/
  main.rs    CLI args, run loop
  term.rs    Raw mode + input (Unix termios / Windows console FFI)
  manager.rs Detection, scanning, recommendations
  fix.rs     Read/write .npmrc, TOML, YAML; backup/restore
  ui.rs      Banner, tables, selector
```

- **Zero third-party crates** — intentional for a small security-adjacent tool; see `AGENTS.md` if you change that policy.
- **Colors** use ANSI sequences; modern terminals on Windows (e.g. Windows Terminal) are supported.

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| `depsguard: command not found` | Ensure the install directory is on `PATH`, or use the full path to the binary. |
| Permission errors writing config | DepsGuard only edits files in your user profile; run as a normal user, not elevated unless those files are owned by admin. |
| Keys not working on Windows | Use **Windows Terminal** or another VT-capable terminal; legacy `cmd.exe` may not handle all keys. |
| pnpm workspaces missing | Ensure `pnpm-workspace.yaml` lives under your home directory tree; very unusual layouts may not be discovered. |
| `cargo install` fails | Install Rust via [rustup](https://rustup.rs/) and use Rust **≥ 1.74**. |

## Help & feedback

- [Report a bug or request a feature](https://github.com/arnica/depsguard/issues)
- [Report a security vulnerability](https://github.com/arnica/depsguard/security/advisories/new) (see [`SECURITY.md`](SECURITY.md))
- Development workflow for contributors lives in [`AGENTS.md`](AGENTS.md).

## See also

- [**Dependency Cooldowns** (`cooldowns.dev`)](https://cooldowns.dev/) — a reference guide and companion shell helper (`cooldowns.sh`) focused specifically on **minimum-release-age cooldowns**. Complements DepsGuard: it covers a broader set of ecosystems on the cooldown axis (pip, uv, npm, pnpm, Yarn, Bun, Deno, Cargo), while DepsGuard covers npm/pnpm/yarn/bun/aube/uv/pip/poetry plus Renovate/Dependabot, report-only Docker checks, and other hardening settings (`ignore-scripts`, `block-exotic-subdeps`, `trust-policy`, `strict-dep-builds`) with an interactive TUI, diff preview, and backup/restore.

> **Python ecosystem note:** DepsGuard scans the package managers that expose a release-age cooldown as a **persistent config setting** — `uv` (`exclude-newer`), `pip` (`uploaded-prior-to`, pip ≥ 26.1), and `poetry` (`solver.min-release-age`, poetry ≥ 2.4). `pdm` and `conda` currently offer release-age only via one-off CLI flags / unreleased proposals (nothing to scan in a config file), and `pixi`'s `exclude-newer` is project-scoped (no user-level config); these may be added later. `pipenv` and `hatch` have no cooldown setting yet.

## License

MIT

## Star History

<a href="https://www.star-history.com/?repos=arnica%2Fdepsguard&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=arnica/depsguard&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=arnica/depsguard&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=arnica/depsguard&type=date&legend=top-left" />
 </picture>
</a>

---

**Links:** [Repository](https://github.com/arnica/depsguard) · [Documentation site](https://depsguard.com)
