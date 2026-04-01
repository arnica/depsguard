# depsguard

Harden your package manager configs against supply chain attacks. Zero dependencies.

Made with love by [Arnica](https://arnica.io) in Atlanta.

```
     ╔═══════════════════════════════════════════════════╗
     ║   ____                   ____                     ║
     ║  |  _ \  ___ _ __  ___  / ___|_   _  __ _ _ __   ║
     ║  | | | |/ _ \ '_ \/ __|| |  _| | | |/ _` | '__|  ║
     ║  | |_| |  __/ |_) \__ \| |_| | |_| | (_| | |     ║
     ║  |____/ \___| .__/|___/ \____|\__,_|\__,_|_|     ║
     ║             |_|    supply chain defense            ║
     ║                                                   ║
     ║        Made with love by Arnica in Atlanta         ║
     ╚═══════════════════════════════════════════════════╝
```

## What it does

DepsGuard scans your system for installed package managers, checks their configs
for supply chain security best practices, and offers to fix them interactively.

**Supported package managers:** npm, pnpm, bun, uv

**Checks performed:**

| Manager | Setting | Value | Purpose |
|---------|---------|-------|---------|
| npm | `min-release-age` | `7` (days) | Delay new package versions by 7 days |
| npm | `ignore-scripts` | `true` | Block malicious post-install scripts |
| pnpm | `minimum-release-age` | `10080` (minutes) | Delay new package versions by 7 days |
| pnpm | `ignore-scripts` | `true` | Block malicious post-install scripts |
| bun | `install.minimumReleaseAge` | `604800` (seconds) | Delay new package versions by 7 days |
| uv | `exclude-newer` | RFC 3339 date (7 days ago) | Exclude recently published packages |

## Config file locations

| Manager | Linux | macOS | Windows |
|---------|-------|-------|---------|
| npm | `~/.npmrc` | `~/.npmrc` | `%USERPROFILE%\.npmrc` |
| pnpm | `~/.npmrc` | `~/.npmrc` | `%USERPROFILE%\.npmrc` |
| bun | `~/.bunfig.toml` | `~/.bunfig.toml` | `%USERPROFILE%\.bunfig.toml` |
| uv | `~/.config/uv/uv.toml` | `~/Library/Application Support/uv/uv.toml` | `%APPDATA%\uv\uv.toml` |

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

# Help
depsguard --help
```

### Interactive mode

1. Scans all detected package managers and shows their status
2. Presents fixable items with a TUI selector
3. Use arrow keys to navigate, space to toggle, enter to apply, q/esc to quit
4. Applies selected fixes and re-scans to confirm

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

**Test coverage:** 73 unit tests, 21 integration tests, 12 cross-platform tests (including Wine).

## Design

- **Zero dependencies** - only uses Rust std library
- **Cross-platform** - Linux, macOS, Windows (tested via Wine)
- **Terminal raw mode** - direct FFI to termios (Unix) / Console API (Windows)
- **ANSI colors** - works on modern terminals including Windows Terminal

### Architecture

```
src/
  main.rs       Entry point, CLI args, interactive loop
  term.rs       Raw terminal mode, ANSI codes, key input (zero-dep FFI)
  manager.rs    Package manager detection, config scanning, recommendations
  fix.rs        Config file modification (flat .npmrc + TOML)
  ui.rs         TUI rendering: banner, status table, interactive selector
```

## License

MIT
