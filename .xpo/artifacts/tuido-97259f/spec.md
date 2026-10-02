## What

Two GitHub Actions workflows:

- **`ci.yml`** checks every push and pull request to `main` on Linux only: formatting, clippy and tests.
- **`release.yml`** runs only when a `v*` tag is pushed. It builds `tuido` for 6 targets, packages each one with a SHA-256 checksum, and publishes them to a GitHub Release.

## Why

- Nothing checks changes automatically today. The last review found crash-level bugs that a test run would have caught.
- The only way to install is to build from source, which needs a Rust 1.88+ toolchain.
- The repo is private, so Actions minutes are billed, at 10× for macOS runners and 2× for Windows. Regular pushes therefore use only Linux, and the expensive runners only run for releases.

## Acceptance Criteria

- [ ] A push or PR to `main` runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --locked`, on `ubuntu-latest` only. The run fails if any of them fails.
- [ ] A regular push or PR never starts a macOS or Windows runner and never builds release binaries.
- [ ] Pushing tag `vX.Y.Z` creates a GitHub Release `vX.Y.Z` with one archive and one `.sha256` file per target.
- [ ] The release fails early, before any build job starts, if the tag doesn't match `version` in `Cargo.toml`. For example, `v0.3.1` against `0.3.0` fails.
- [ ] Linux binaries are statically linked (`ldd` reports "not a dynamic executable"). The macOS and Windows binaries start and print the TUI.
- [ ] The README gains an "Install from a release" section and a CI badge.

## Flow

1. Add `rustfmt.toml` with `max_width = 120`, then run `cargo fmt` once. At the default width of 100, 29 places in the code change; at 120, only 13 do.
2. `.github/workflows/ci.yml`:
   - Triggers on `push` to `main` and on `pull_request`. A newer run on the same ref cancels the older one.
   - A single job, `check`, on `ubuntu-latest`, using `dtolnay/rust-toolchain@stable` with clippy and rustfmt, plus `Swatinem/rust-cache`. It runs fmt, then clippy, then `test --locked`.
3. `.github/workflows/release.yml`:
   - Triggers only on `push` of tags matching `v*`. There is no `workflow_dispatch`.
   - Job `verify-version`: compares `${GITHUB_REF_NAME#v}` with the version that `cargo metadata` reports. `build` depends on it, so a mismatch stops the run before any macOS or Windows runner starts.
   - Job `build` runs this matrix:

     | target | runner | how |
     |---|---|---|
     | x86_64-unknown-linux-musl | ubuntu-latest | `cross` |
     | aarch64-unknown-linux-musl | ubuntu-latest | `cross` |
     | x86_64-apple-darwin | macos-latest | native cross-target |
     | aarch64-apple-darwin | macos-latest | native |
     | x86_64-pc-windows-msvc | windows-latest | native |
     | aarch64-pc-windows-msvc | windows-latest | native cross-target |

     Each build runs `cargo build --release --locked --target $T`.
   - Packaging: `tuido-vX.Y.Z-$T.tar.gz` on Unix and `.zip` on Windows. Each archive contains the binary, `README.md` and `LICENSE`, and gets a `<archive>.sha256` file next to it.
   - Job `publish`: uses `softprops/action-gh-release@v3` with `generate_release_notes: true` and uploads every archive and checksum. It needs `permissions: contents: write`.
4. README: add the CI badge, an "Install from a release" section, and the supported-platforms list.

## Decisions

- **Release binaries are built on tag pushes only.** There are no per-commit builds and no manual dispatch, so the expensive runners are used only when publishing.
- **CI tests on Linux only.** This reverses the earlier answer to resolved question 3 (see below) to save minutes. The trade-off: a macOS- or Windows-specific break first shows up when the release is built. The release `build` job uses `fail-fast: false`, so one broken target doesn't hide the state of the others, and nothing is published unless every target builds.
- **musl, not glibc, for Linux.** A static binary runs on any distribution, including Alpine and older glibc versions. The app needs no system libraries; notifications use `zbus`, which is pure Rust with no libdbus. `cross` handles the aarch64 linker.
- **Hand-written workflows, not `cargo-dist`.** About 120 lines of YAML that are easy to read, with no extra tool or `dist` config to maintain. If installers or a Homebrew tap are wanted later, `cargo-dist` would be worth another look.
- **Third-party actions pinned to major versions.** Pinning to full commit SHAs is safer but noisier. It could be added later with Dependabot. The versions were current when this was written: checkout v7, upload-artifact v7, download-artifact v8, action-gh-release v3.
- **Matrix values reach shell scripts through `env`.** Interpolating `${{ }}` directly into scripts is avoided, which also keeps shellcheck clean.
- **Separate macOS binaries, not a universal binary.** Two plain archives are simpler. `lipo` can merge them later if wanted.

## Edge Cases

- **HIGH:** A tag that doesn't match `Cargo.toml` would publish a binary whose `v…` label in the TUI disagrees with the release name. `verify-version` blocks this.
- **MEDIUM:** A platform-specific test or build failure is only found at release time. To recover, fix the problem, bump the version and tag again. Optionally, delete the failed tag first.
- **MEDIUM:** Re-pushing a tag that already has a release. `action-gh-release` updates the existing release and overwrites its assets. Accepted.
- **LOW:** Windows terminals send key-release events. This is already filtered by `KeyEventKind::Press`, so nothing to do.
- **LOW:** The existing stale tag `v0.1.0` won't re-trigger anything, because only newly pushed tags fire the workflow.

## Assumptions

- GitHub Releases on `nicbet/tuido` are the distribution channel. Publishing to crates.io is not in scope.
- The code compiles on macOS and Windows as-is, because path handling uses `PathBuf` throughout.

## Resolved Questions (confirmed by user)

1. **Formatting:** enforce `cargo fmt` in CI, using `rustfmt.toml` with `max_width = 120` and a one-time reformat.
2. **Targets:** build all 6.
3. **Cross-OS tests:** ~~run tests on Linux, macOS and Windows for every push and PR~~. Revised by the user: run tests on Linux only, to save minutes.
4. **First release:** the user will push the first tag themselves. The agent does not tag.
5. **Release triggers (revision):** binaries build only when a tag is pushed. `workflow_dispatch` was removed.
