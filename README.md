# depsguard

```
╶┬┐┌─╴┌─┐┌─┐┌─╴╷ ╷┌─┐┌─┐╶┬┐
 ││├╴ ├─┘└─┐│╶┐│ │├─┤├┬┘ ││
╶┴┘└─╴╵  └─┘└─┘└─┘╵ ╵╵└╴╶┴┘
```

Harden your package manager configs against supply chain attacks. Zero dependencies.

By **[arnica](https://arnica.io)**

## What it does

DepsGuard scans your system for installed package managers and `pnpm-workspace.yaml` files, checks their configs for supply chain security best practices, and offers to fix them interactively. Backups are created before any changes.

**Supported package managers:** npm, pnpm, bun, uv

**Checks performed:**

| Manager | Config | Setting | Value | Purpose |
|---------|--------|---------|-------|---------|
| npm | `~/.npmrc` | `min-release-age` | `7` (days) | Delay new versions by 7 days |
| npm/pnpm | `~/.npmrc` | `ignore-scripts` | `true` | Block malicious install scripts |
| pnpm | `pnpm-workspace.yaml` | `minimumReleaseAge` | `10080` (min) | Delay new versions by 7 days (default) |
| pnpm | `pnpm-workspace.yaml` | `blockExoticSubdeps` | `true` | Block untrusted transitive deps |
| pnpm | `pnpm-workspace.yaml` | `trustPolicy` | `no-downgrade` | Block provenance downgrades |
| pnpm | `pnpm-workspace.yaml` | `strictDepBuilds` | `true` | Fail on unreviewed build scripts |
| bun | `~/.bunfig.toml` | `install.minimumReleaseAge` | `604800` (sec) | Delay new versions by 7 days |
| uv | `uv.toml` | `exclude-newer` | `7 days` | Delay new versions by 7 days |

## Config file locations

| Manager | Linux | macOS | Windows |
|---------|-------|-------|---------|
| npm/pnpm | `~/.npmrc` | `~/.npmrc` | `%USERPROFILE%\.npmrc` |
| pnpm | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` | `pnpm-workspace.yaml` |
| bun | `~/.bunfig.toml` | `~/.bunfig.toml` | `%USERPROFILE%\.bunfig.toml` |
| uv | `~/.config/uv/uv.toml` | `~/Library/Application Support/uv/uv.toml` | `%APPDATA%\uv\uv.toml` |

`pnpm-workspace.yaml` files are discovered by searching from the home directory downward, skipping known large directories (`node_modules`, `Library`, `.cache`, `target`, etc.).

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Interactive mode - scan, select fixes, apply
depsguard

# Scan only, no changes
depsguard --scan

# Restore config files from backup
depsguard --restore

# Help
depsguard --help
```

### Interactive mode

1. Scans all detected package managers and pnpm workspaces
2. Shows a summary of issues (not set / misconfigured / ok)
3. Presents fixable items grouped by config file with a TUI selector
4. Use arrow keys to navigate, space to toggle, enter to apply
5. Press Esc to go back, q to quit
6. Backs up config files before applying any changes

### Backup & restore

Before modifying any config file, depsguard creates a timestamped `.bak` backup next to the original (e.g. `~/.npmrc.2026-04-01T12-00-00Z.bak`). Run `depsguard --restore` to select and restore from these backups.

## Build

```bash
# Native
cargo build --release

# Cross-compile for Windows
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

## Test

```bash
# All tests (unit + integration)
cargo test

# Unit tests only
cargo test --bin depsguard

# Integration tests (requires npm, pnpm, bun, uv installed)
cargo test --test integration

# Cross-platform tests (Wine tests require: apt install wine64)
cargo test --test cross_platform
```

## Design

- **Zero dependencies** - only uses Rust std library
- **Cross-platform** - Linux, macOS, Windows (tested via Wine)
- **Terminal raw mode** - direct FFI to termios (Unix) / Console API (Windows)
- **ANSI colors** - works on modern terminals including Windows Terminal
- **Smart search** - finds `pnpm-workspace.yaml` files across projects, skipping large dirs

### Architecture

```
src/
  main.rs       Entry point, CLI args, interactive loop
  term.rs       Raw terminal mode, ANSI codes, key input (zero-dep FFI)
  manager.rs    Package manager detection, config scanning, recommendations
  fix.rs        Config file modification (flat .npmrc, TOML, YAML) + backup/restore
  ui.rs         TUI rendering: banner, status table, interactive selector
```

## License

MIT
