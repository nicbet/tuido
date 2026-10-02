# Prep repo for public release

## What
Docs and metadata changes only, plus GitHub repo settings. No code changes.

## Why
The repo is about to go public. The first impression (README, crate metadata, changelog) should be accurate and complete.

## How
- `Cargo.toml` `[package]`: description (matches the GitHub description), `license = "MIT"`, `repository`/`homepage` = https://github.com/nicbet/tuido, `readme = "README.md"`, keywords `["todo", "tui", "terminal", "ratatui", "cli"]` (crates.io allows at most 5), categories `["command-line-utilities"]`, `authors`.
- README:
  - Remove the top-level Prerequisites section and fold "Rust 1.88+" into From Source / Using Cargo. `rust-version` is 1.88.
  - Notifications: Linux needs a D-Bus notification daemon; the app doesn't call notify-send.
  - Contributing: one line about the Exponential issue tracker, linking https://go-exponential.dev.
  - License: "MIT, see [LICENSE](LICENSE)".
- `CHANGELOG.md` in Keep a Changelog format, built from git history: an empty Unreleased section, 0.3.0 (2026-10-02, which includes the untagged 0.2.0 and 0.2.1), and 0.1.0 (2025-09-19). `v0.3.0` already points at HEAD, so nothing is unreleased. README's Releasing steps gain "move Unreleased entries under the new version".
- `Cargo.toml` `exclude` keeps demo/, .xpo/, .github/ and the agent config files out of the published crate.
- LICENSE: `Copyright (c) 2025-2026`.
- Post-merge `gh` settings: topics; enable secret scanning plus push protection and Dependabot alerts; turn off Projects.

## Acceptance criteria
- `cargo package --list --allow-dirty` shows no metadata warnings; fmt, clippy and tests still pass.
- README renders correctly; every link resolves.
