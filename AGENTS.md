# Agents

Guidelines for AI agents working on this codebase.

## Project overview

DepsGuard is a zero-dependency Rust CLI that scans package manager configs (npm, pnpm, bun, uv) for supply chain security best practices and offers interactive fixes. It targets Linux, macOS, and Windows.

## Git workflow

- **Never push directly to `main`** without explicit permission from the user. Always use a feature branch and open a pull request.
- Do not force-push or run destructive git operations unless explicitly asked.

## AI disclosure

All agent involvement in this repository must be disclosed:

- **Pull requests**: every PR must state whether an AI agent or assistant produced any part of it (code, tests, docs, commit messages), naming the tool and model (e.g. "Claude Code — Claude Fable 5"). The PR template has a section for this; write "None" when no agent was used.
- **Commits**: agent-authored or agent-assisted commits must carry a `Co-Authored-By` trailer naming the agent and model, e.g. `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- **Comments and reviews**: when an agent posts on a person's behalf, it must identify itself as an agent and name the model, e.g. "Authored by Claude (Fable 5) via Claude Code, on behalf of @user".

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

## Version-gated settings

Most cooldown/security settings only exist — or only accept the value DepsGuard
writes — from a specific tool version onward. Getting this wrong causes the
worst false positive: telling a user to add a setting their installed tool
cannot use (see [#52](https://github.com/arnica/depsguard/issues/52), where
poetry 1.8.5 was told to set `solver.min-release-age`, added in poetry 2.4).

**Invariant:** when the installed version does not support a setting, the
recommendation is `CheckStatus::Unsupported` **regardless of whether the setting
is currently missing, wrong, or correct**. `Unsupported` is informational
(`needs_fix()` is `false`): it renders as `ℹ requires <tool> >= X (have Y)` with
a `WARNING` badge and is never offered in the fix selector. A missing setting on
an unsupported version must never become `Missing`/`FileMissing` (which read as
`ACTION NEEDED`).

**Do it via the shared helpers** in `manager::types` — compute the normal
`Recommendation`, then apply the gate. For a plain major.minor floor, prefer
the all-in-one helper:

- `gate_min_version(rec, tool, min_major, min_minor, have)` — **the default.**
  Returns `rec` unchanged when `have >= min_major.min_minor`, otherwise an
  `Unsupported` verdict. It does the version check *and* applies the verdict, so
  scanners never hand-roll the comparison.

When the gate is more than a major.minor floor — patch-level precision, or a
value-form gate (pip/uv, below) — compute the condition yourself and apply the
verdict directly with the lower-level helpers:

- `mark_unsupported(rec, tool, min_major, min_minor, have)` — unconditional,
  standard message.
- `mark_unsupported_with_message(rec, msg)` — unconditional, custom message.

All three convert *any* status to `Unsupported`. Do **not** re-introduce an
"only-if-configured" guard (that was the #52 bug): the version check decides
*whether* a setting is supported; the helper applies that verdict to every
state.

For settings whose feature exists earlier but whose *value form* needs a newer
version (pip/uv relative durations vs. absolute dates), gate **every** state on
the version floor, not just missing or new-form values: the fix always writes
the new form, so even a configured old-form value (e.g. a working absolute
date) must become `Unsupported` — otherwise the offered fix would replace a
working config with a value the tool cannot parse. Give the old-form case a
message that names the current value, so a working config is not reduced to a
bare "requires >= X" upgrade prompt.

The invariant is enforced by the table-driven
`missing_setting_on_unsupported_version_is_never_actionable` test in
`src/manager/mod.rs`. **Add a row there for every new version-gated setting.**

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
| `WINGATE_RELEASE_TOKEN` | Open WinGet PRs via WinGet Releaser (requires existing package id + winget-pkgs fork) |

Homebrew has two channels:

- **homebrew-core (primary)** — `depsguard` is published in
  [Homebrew/homebrew-core](https://github.com/Homebrew/homebrew-core/blob/HEAD/Formula/d/depsguard.rb),
  so users just run `brew install depsguard` (no tap). Version bumps are handled by
  Homebrew autobump: BrewTestBot opens the bump PR automatically when a new GitHub
  tag is published, so no CI in this repo is required. Do **not** add `no_autobump!`
  to the core formula or autobump stops.
- **`arnica/depsguard` tap (legacy fallback)** — still maintained at
  `Formula/depsguard.rb`, rendered from `packaging/homebrew/depsguard.rb.in` and pushed
  to `main` by the `sync-homebrew-formula` job on each release.

The Scoop manifest is maintained in this repository at `bucket/depsguard.json`
(rendered from `packaging/scoop/depsguard.json.in` via `scripts/release/publish-scoop-bucket.sh`).

### End-user install channels (optional)

Document these in your org’s internal runbooks or public docs once the repos exist; **do not** duplicate in `README.md` unless you have stable public install channels.

**Homebrew (homebrew-core, primary)**

1. Canonical formula lives in homebrew-core at `Formula/d/depsguard.rb`; users run `brew install depsguard`.
2. New versions are bumped automatically by Homebrew autobump on each GitHub release tag — no action needed in this repo.
3. To force an immediate bump instead of waiting for autobump, run `brew bump-formula-pr depsguard` locally.

**Homebrew (`arnica/depsguard` tap, legacy fallback)**

1. `Formula/depsguard.rb` remains the tap formula path.
2. The `sync-homebrew-formula` release job updates its `url`/`sha256` directly on `main` for each release tag.
3. Users tap with explicit repo URL: `brew tap <owner>/depsguard https://github.com/<owner>/depsguard`.

**Scoop (bucket in this repo)**

1. Scoop manifest lives at `bucket/depsguard.json` (rendered from `packaging/scoop/depsguard.json.in`).
2. Release workflow updates the manifest directly on `main` for each release tag.
3. Users: `scoop bucket add depsguard https://github.com/<owner>/depsguard` then `scoop install depsguard`.

**WinGet**

- Optional job uses [WinGet Releaser](https://github.com/vedantmgoyal9/winget-releaser) when `WINGATE_RELEASE_TOKEN` is set.
- At least one version of `Arnica.DepsGuard` must exist in [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) (first manifest is usually manual); the token owner needs a fork of `winget-pkgs`.

**Other templates in-repo**

| Path | Purpose |
|------|---------|
| `packaging/aur/PKGBUILD` | AUR binary package example (`updpkgsums` after release) |

**Releasing a version**

Use release tags in `v<semver>` format (for example `v0.1.1`). The current workflow
creates/pushes release tags automatically from `main`.

**Changelog**: Release notes are auto-generated by GitHub (`generate_release_notes: true` in the release workflow). There is no separate changelog tool (git-cliff was previously used but has been removed).
