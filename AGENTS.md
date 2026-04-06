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

Tag pushes run `.github/workflows/release.yml`. Optional secrets (omit to skip that publisher):

| Secret | Purpose |
|--------|---------|
| `CARGO_REGISTRY_TOKEN` | `cargo publish` to crates.io |
| `HOMEBREW_TAP_TOKEN` | Push updated `Formula/depsguard.rb` to the Homebrew tap repo specified by `HOMEBREW_TAP_REPO` |
| `SCOOP_BUCKET_TOKEN` | Push updated `depsguard.json` to `<owner>/scoop-depsguard` |
| `WINGET_PKGS_TOKEN` | Open WinGet PRs via WinGet Releaser (requires existing package id + winget-pkgs fork) |

Templates live under `packaging/`; render scripts are `scripts/release/publish-homebrew-tap.sh` and `publish-scoop-bucket.sh`.

| Variable (repo-level) | Purpose |
|------------------------|---------|
| `HOMEBREW_TAP_REPO` | **Required** for Homebrew publishing. GitHub repo in `owner/repo` format (e.g. `arnica/homebrew-depsguard`). |

### End-user install channels (optional)

Document these in your org’s internal runbooks or public docs once the repos exist; **do not** duplicate in `README.md` unless you have stable public install channels.

**Homebrew (custom tap)**

1. Create a Homebrew tap repo following naming convention `<owner>/homebrew-depsguard` (e.g. `arnica/homebrew-depsguard`).
2. Set the **repo variable** `HOMEBREW_TAP_REPO` to `<owner>/homebrew-depsguard` (required, no default).
3. Set **secret** `HOMEBREW_TAP_TOKEN` (PAT with **repo** scope and push access to the tap repo).
4. Release workflow renders `packaging/homebrew/depsguard.rb.in` and pushes `Formula/depsguard.rb` to the tap repo on each tag.
5. Users install with `brew tap <owner>/depsguard` then `brew install depsguard` (Homebrew auto-maps `<owner>/depsguard` → `<owner>/homebrew-depsguard`).

**Scoop (custom bucket)**

1. Create `<owner>/scoop-depsguard` with `depsguard.json` (see `packaging/scoop/depsguard.json.in`).
2. Users: `scoop bucket add <label> https://github.com/<owner>/scoop-depsguard` then `scoop install depsguard`.
3. Set `SCOOP_BUCKET_TOKEN` with push access to the bucket repo.

**WinGet**

- Optional job uses [WinGet Releaser](https://github.com/vedantmgoyal9/winget-releaser) when `WINGET_PKGS_TOKEN` is set.
- At least one version of `Arnica.DepsGuard` must exist in [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) (first manifest is usually manual); the token owner needs a fork of `winget-pkgs`.

**Other templates in-repo**

| Path | Purpose |
|------|---------|
| `packaging/aur/PKGBUILD` | AUR binary package example (`updpkgsums` after release) |

**Releasing a version**

```bash
cargo install cargo-release
cargo release patch          # dry-run
cargo release patch --execute  # bump, commit, tag, push — triggers release workflow on tag
```

Use release tags in `v<semver>` format (for example `v0.1.1`).

**Changelog**: Release notes are auto-generated by GitHub (`generate_release_notes: true` in the release workflow). There is no separate changelog tool (git-cliff was previously used but has been removed).
