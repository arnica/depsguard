---
name: depsguard
description: Install and run DepsGuard, a zero-dependency CLI that scans and fixes package manager configs (npm, pnpm, bun, uv) for supply chain security best practices.
version: 0.1.0
license: MIT
homepage: https://depsguard.com/
repository: https://github.com/arnica/depsguard
---

# DepsGuard

DepsGuard is a zero-dependency Rust CLI that scans package manager configs for
supply chain security best practices and offers interactive fixes. It targets
Linux, macOS, and Windows, and supports `npm`, `pnpm`, `yarn`, `bun`, and `uv`.

## When to use this skill

Use this skill when an agent or user needs to:

- Audit a project's `.npmrc`, `.yarnrc.yml`, `bunfig.toml`, `pnpm-workspace.yaml`,
  `uv.toml`, or related package manager configuration for hardening gaps.
- Apply recommended supply chain security settings (e.g. lockfile enforcement,
  provenance checks, registry pinning) to an existing project.
- Install DepsGuard as part of a developer onboarding or CI pipeline.

## Install

### Homebrew (macOS / Linux)

```sh
brew tap arnica/depsguard https://github.com/arnica/depsguard
brew install depsguard
```

### APT (Debian / Ubuntu)

```sh
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://depsguard.com/apt/gpg.key \
  | sudo gpg --dearmor -o /etc/apt/keyrings/depsguard.gpg
echo "deb [signed-by=/etc/apt/keyrings/depsguard.gpg] https://depsguard.com/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/depsguard.list >/dev/null
sudo apt update
sudo apt install depsguard
```

### Scoop (Windows)

```sh
scoop bucket add depsguard https://github.com/arnica/depsguard
scoop install depsguard
```

### Cargo

```sh
cargo install depsguard
```

## Usage

Run in the root of a project:

```sh
depsguard
```

DepsGuard will detect the package managers in use, list hardening findings, and
prompt you interactively to apply fixes. Use `depsguard --help` for flags.

## Notes for agents

- DepsGuard has **zero runtime dependencies**; the binary is self-contained.
- It only reads and writes files under the current working directory (and the
  user's home directory for shell / package manager rc files when explicitly
  permitted).
- It is safe to run in CI in a dry-run / check-only mode for drift detection.
- Source, issues, and release notes:
  <https://github.com/arnica/depsguard>
