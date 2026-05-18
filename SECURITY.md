# Security policy

## Supported versions

We release fixes for the **latest tagged version** and the **default branch** (`main`). Older tags may not receive backports unless flagged as critical.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for undisclosed security problems.

1. Use [GitHub Security Advisories](https://github.com/arnica/depsguard/security/advisories/new) to report privately (preferred), or
2. Contact Arnica engineering at the address published on [arnica.io](https://arnica.io) if you cannot use GitHub.

Include steps to reproduce, affected version or commit, and impact. We aim to acknowledge within a few business days.

## Supply chain notes

- The published **crate** on crates.io intentionally declares **no third-party Rust dependencies**; review `Cargo.toml` and CI ([`audit.yml`](.github/workflows/audit.yml)) for automation around the dependency graph.
- **Prebuilt binaries** are built in GitHub Actions from this repository; verify release artifacts using the published `.sha256` files when downloading from [Releases](https://github.com/arnica/depsguard/releases).

## Scope

Reports about **misconfiguration of npm/pnpm/bun/uv** (the tools DepsGuard configures) are usually general product-security topics for those ecosystems. We still welcome reports if you believe DepsGuard **misapplies** a setting or **corrupts** config files.
