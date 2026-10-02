# Walkthrough: Prep repo for public release

## What and why
The repo was about to go from private to public. This change covers how it looks to a first-time visitor and makes the crate ready to publish. It changes docs and metadata only. No source files changed.

## History rewrite (before this branch)
Four early commits (`show version number…` through `render completed todo items…`) were authored and committed with a work email. They were rewritten to `nicbet@gmail.com` with `git filter-repo --mailmap`. The tree at HEAD is identical, which was checked by comparing tree hashes. `main` and the tags were force-pushed. `v0.3.0` (an annotated tag) was recreated on the new HEAD by filter-repo. `v0.1.0` comes before the rewritten commits, so its hash didn't change. GPG signatures on the rewritten commits were lost, which can't be avoided. Commit SHAs recorded in old `.xpo/issues.db` MERGE events (`base_sha`) refer to the old history.

## Changes
- **Cargo.toml**: crates.io metadata (description, license, repository, homepage, readme, at most 5 keywords, categories, authors). `exclude` keeps `demo/` (GIF, ~340 KB), `.xpo/`, `.github/` and agent config (`.claude/`, `.codex/`, `.mcp.json`, `AGENTS.md`, `CLAUDE.md`) out of the packaged crate. The README's relative `demo/demo.gif` link still works on crates.io, which resolves relative links against `repository`.
- **README**:
  - The top-level Prerequisites section is gone. "Requires Rust 1.88 or newer" (matching `rust-version`) sits under From Source and Using Cargo, so release-binary users don't think they need Rust.
  - The Linux notification note is corrected: notify-rust talks to a D-Bus notification daemon; it doesn't shell out to `notify-send`.
  - Contributing links the Exponential tracker (https://go-exponential.dev) and explains `.xpo/`.
  - License links `LICENSE`.
  - Releasing step 1 now includes moving Unreleased changelog entries under the new version.
- **CHANGELOG.md**: Keep a Changelog format, rebuilt from git history. Versions 0.2.0 and 0.2.1 were bumped in Cargo.toml but never tagged, so their changes are listed under 0.3.0 with a note saying so. Unreleased is empty because `v0.3.0` points at HEAD. v0.1.0 had no reordering (added later), which was checked against the v0.1.0 source.
- **LICENSE**: `2025-2026`. The first edit dropped the space before the name; this was caught in review and fixed.

## Follow-up outside the repo
After the merge, these GitHub settings are applied via `gh`: topics, secret scanning plus push protection, Dependabot alerts, and Projects turned off. Changing the visibility to public is left to the owner.
