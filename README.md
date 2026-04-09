# depsguard

```text
     _                                          _
  __| | ___ _ __  ___  __ _ _   _  __ _ _ __ __| |
 / _` |/ _ \ '_ \/ __|/ _` | | | |/ _` | '__/ _` |
| (_| |  __/ |_) \__ \ (_| | |_| | (_| | | | (_| |
 \__,_|\___| .__/|___/\__, |\__,_|\__,_|_|  \__,_|
           |_|        |___/
```

Guard your dependencies against supply chain attacks. **Single static binary, zero Rust crate dependencies.**

By **[[arnica](https://arnica.io)]**

## Table of contents

- [Overview](#overview)
- [Install](#install)
- [Usage](#usage)
- [What gets checked](#what-gets-checked)
- [Config file locations](#config-file-locations)
- [Backups and restore](#backups-and-restore)
- [How it works](#how-it-works)
- [Troubleshooting](#troubleshooting)
- [Help & feedback](#help--feedback)
- [License](#license)

## Overview

DepsGuard looks for **npm**, **pnpm**, **yarn**, **bun**, and **uv** on your machine, reads their config files, compares them to recommended supply-chain settings, and can **apply fixes interactively**. It also scans for **Renovate** and **Dependabot** configs in your repos. It never runs package installs; it only edits config files you approve, and it writes **backups** before any change.

### Key features

- Interactive TUI: scan, review, toggle fixes, apply
- `scan` subcommand for read-only reporting
- `restore` subcommand to pick a backup and roll back a file
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
echo "deb [signed-by=/etc/apt/keyrings/depsguard.gpg] https://depsguard.com/apt stable main" | sudo tee /etc/apt/sources.list.d/depsguard.list >/dev/null
sudo apt update
sudo apt install depsguard
```

#### macOS (Intel / Apple Silicon)

```bash
# Homebrew tap
brew tap arnica/depsguard https://github.com/arnica/depsguard
brew install depsguard
```

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
| Homebrew (custom tap) | yes | yes | no | `brew tap arnica/depsguard https://github.com/arnica/depsguard ; brew install depsguard` |
| Scoop (custom bucket) | no | no | yes | `scoop bucket add depsguard https://github.com/arnica/depsguard ; scoop install depsguard` |
| WinGet | no | no | yes | `winget install Arnica.DepsGuard` |

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
depsguard scan         # report only; no writes
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
| npm/pnpm | `~/.npmrc` | `ignore-scripts` | `true` | Reduce install-script risk |
| pnpm | `~/.npmrc` | `minimum-release-age` | `10080` (minutes) | Delay new versions by 7 days (requires pnpm >= 10.16) |
| yarn | `.yarnrc.yml` | `npmMinimalAgeGate` | `7d` | Delay new versions by 7 days (requires yarn >= 4.10) |
| pnpm | `pnpm-workspace.yaml` | `minimumReleaseAge` | `10080` (minutes) | Delay new versions by 7 days (requires pnpm >= 10.16) |
| pnpm | `pnpm-workspace.yaml` | `strictDepBuilds` | `true` | Fail on unreviewed build scripts (requires pnpm >= 10.3) |
| pnpm | `pnpm-workspace.yaml` | `trustPolicy` | `no-downgrade` | Block provenance downgrades (requires pnpm >= 10.21) |
| pnpm | `pnpm-workspace.yaml` | `blockExoticSubdeps` | `true` | Block untrusted transitive deps (requires pnpm >= 10.26) |
| bun | `~/.bunfig.toml` | `install.minimumReleaseAge` | `604800` (seconds) | ~7 day delay |
| uv | `uv.toml` | `exclude-newer` | `7 days` | Delay new publishes |
| renovate | `renovate.json` etc. | `minimumReleaseAge` | `7 days` | Delay dependency update PRs by 7 days |
| dependabot | `.github/dependabot.yml` | `cooldown.default-days` | `7` | Delay dependency update PRs by 7 days |

## Config file locations

| Manager | Linux | macOS | Windows |
|---------|-------|-------|---------|
| npm/pnpm | `~/.npmrc` | `~/.npmrc` | `%USERPROFILE%\.npmrc` |
| pnpm global | `$XDG_CONFIG_HOME/pnpm/rc` or `~/.config/pnpm/rc` | `$XDG_CONFIG_HOME/pnpm/rc` or `~/Library/Preferences/pnpm/rc` | `%LOCALAPPDATA%\pnpm\config\rc` |
| yarn | `~/.yarnrc.yml` | `~/.yarnrc.yml` | `%USERPROFILE%\.yarnrc.yml` |
| pnpm | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` |
| bun | `$XDG_CONFIG_HOME/.bunfig.toml` or `~/.bunfig.toml` | `$XDG_CONFIG_HOME/.bunfig.toml` or `~/.bunfig.toml` | `%USERPROFILE%\.bunfig.toml` |
| uv | `$XDG_CONFIG_HOME/uv/uv.toml` or `~/.config/uv/uv.toml` | `$XDG_CONFIG_HOME/uv/uv.toml` or `~/.config/uv/uv.toml` | `%APPDATA%\uv\uv.toml` |
| renovate | `renovate.json`, `.renovaterc`, `.github/renovate.json`, etc. | (same) | (same) |
| dependabot | `.github/dependabot.yml` | (same) | (same) |

User-level config files are read from their standard locations (including XDG-based paths where the tool supports them). Repo-level configs are discovered by searching downward from the current directory, skipping known large directories (`node_modules`, `.git`, `target`, `Library`, `.cache`, and others) so scans stay fast. Repo-level `.npmrc`, `.yarnrc.yml`, `pnpm-workspace.yaml`, Renovate configs, and Dependabot configs are all searched. pnpm settings can live in `~/.npmrc`, the pnpm global `rc` file, or `pnpm-workspace.yaml`; DepsGuard checks all three locations. When `~/.npmrc` is missing, DepsGuard uses pnpm's global config path so fixes can create the default global `rc` file directly.

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

## License

MIT

---

**Links:** [Repository](https://github.com/arnica/depsguard) · [Documentation site](https://depsguard.com)
