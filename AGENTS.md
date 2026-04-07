# Agents

Guidelines for AI agents working on this codebase.

## Project overview

DepsGuard is a zero-dependency Rust CLI that scans package manager configs (npm, pnpm, bun, uv) for supply chain security best practices and offers interactive fixes. It targets Linux, macOS, and Windows.

## Commit messages

Use **Conventional Commits** (<https://www.conventionalcommits.org/>).

Format: `<type>(<optional scope>): <description>`

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`, `perf`, `style`, `build`.

Examples:

- `feat(manager): add yarn berry support`
- `fix(fix): preserve comments when writing .npmrc`
- `test: add integration tests for bun config`
- `docs: update supported managers table`
- `refactor(term): simplify Windows console FFI`

Rules:

- Use lowercase for the description; no trailing period.
- Keep the subject line under 72 characters.
- Use the body (separated by a blank line) to explain *why*, not *what*, when the change is non-trivial.
- Reference issue numbers in the footer when applicable (`Closes #42`).

## Zero-dependency constraint

This project intentionally has **no external crates**. All functionality (terminal raw mode, TOML editing, ANSI colors, key input) is implemented using only the Rust standard library and platform FFI. Do not add dependencies to `Cargo.toml`.

## Rust conventions

### Code style

- Run `cargo fmt` before committing. All code must pass `cargo fmt -- --check`.
- Run `cargo clippy -- -D warnings` and fix all warnings. Treat clippy lints as errors.
- Prefer `rustfmt` defaults; do not add a `rustfmt.toml` unless there is a strong reason.

### Error handling

- Use `Result<T, E>` for fallible operations; avoid `unwrap()` and `expect()` in non-test code.
- Prefer descriptive error messages that help the user understand what went wrong and how to fix it.
- In `main`, surface errors with user-friendly messages rather than raw debug output.

### Safety and FFI

- Minimize `unsafe` blocks and document each one with a `// SAFETY:` comment explaining the invariant.
- Keep FFI (termios on Unix, Console API on Windows) isolated in `src/term.rs`.

### Naming and structure

- Follow Rust naming conventions: `snake_case` for functions/variables, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- Keep modules focused — see the **How it works** section in `README.md` for module responsibilities.
- Prefer small, composable functions over long procedural blocks.

### Testing

- Run the full test suite with `cargo test` before marking work as done.
- Unit tests go in the same file as the code they test, inside a `#[cfg(test)] mod tests` block.
- Integration tests live in `tests/`. Cross-platform tests that require Wine go in `tests/cross_platform.rs`.
- Test names should read as sentences: `fn detects_missing_npmrc_setting()` not `fn test1()`.

### Cross-platform

- All file path logic must handle Linux, macOS, and Windows. Use `std::path::PathBuf` and `std::env::consts::OS` / `cfg!(target_os = ...)` for platform branching.
- Terminal code must work on both Unix (termios) and Windows (Console API).

### Documentation

- Public functions and types should have a doc comment (`///`).
- Keep comments focused on *why*, not *what*. The code should be self-explanatory for the *what*.

- End-user documentation belongs in **`README.md`** (install, usage, troubleshooting). Maintainer-only topics (tests, releases, package automation secrets) stay here.

## Build & verify

```bash
cargo fmt -- --check   # formatting
cargo clippy -- -D warnings  # lints
cargo test             # all tests
cargo build --release  # release binary
```

## Release & distribution (CI secrets)

Pushes to `main` on release-relevant paths run `.github/workflows/release.yml`.
The workflow creates a release tag, publishes artifacts, and runs optional publishers.

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | `cargo publish` to crates.io |
| `RELEASE_BOT_TOKEN` | PAT used by release workflow to push Homebrew formula updates directly to `main` |
| `WINGET_PKGS_TOKEN` | Open WinGet PRs via WinGet Releaser (requires existing package id + winget-pkgs fork) |

Homebrew formula is now maintained in this repository at `Formula/depsguard.rb`.
On each release, CI renders it from `packaging/homebrew/depsguard.rb.in` and pushes
the updated formula commit directly to `main`.

Both Homebrew and Scoop manifests are maintained in this repository (`Formula/depsguard.rb` and `bucket/depsguard.json`).
Templates live under `packaging/`; render script for Scoop is `scripts/release/publish-scoop-bucket.sh`.

### End-user install channels (optional)

Document these in your org’s internal runbooks or public docs once the repos exist; **do not** duplicate in `README.md` unless you have stable public install channels.

**Homebrew (formula in this repo)**

1. Keep `Formula/depsguard.rb` as the canonical formula path.
2. Release workflow updates formula `url`/`sha256` directly on `main` for each release tag.
3. Users tap with explicit repo URL: `brew tap <owner>/depsguard https://github.com/<owner>/depsguard`.

**Scoop (bucket in this repo)**

1. Scoop manifest lives at `bucket/depsguard.json` (rendered from `packaging/scoop/depsguard.json.in`).
2. Release workflow updates the manifest directly on `main` for each release tag.
3. Users: `scoop bucket add depsguard https://github.com/<owner>/depsguard` then `scoop install depsguard`.

**WinGet**

- Optional job uses [WinGet Releaser](https://github.com/vedantmgoyal9/winget-releaser) when `WINGET_PKGS_TOKEN` is set.
- At least one version of `Arnica.DepsGuard` must exist in [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) (first manifest is usually manual); the token owner needs a fork of `winget-pkgs`.

**Other templates in-repo**

| Path | Purpose |
|------|---------|
| `packaging/aur/PKGBUILD` | AUR binary package example (`updpkgsums` after release) |

**Releasing a version**

Use release tags in `v<semver>` format (for example `v0.1.1`). The current workflow
creates/pushes release tags automatically from `main`.

**Changelog**: Release notes are auto-generated by GitHub (`generate_release_notes: true` in the release workflow). There is no separate changelog tool (git-cliff was previously used but has been removed).
