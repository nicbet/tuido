# Walkthrough: GitHub Actions CI and cross-platform release binaries

## What was built

Two workflows in `.github/workflows/`. Together they answer two separate questions: *is this commit healthy?* and *ship this version*.

### `ci.yml`: is this commit healthy?

Runs on every push to `main` and on every pull request. It is one job on `ubuntu-latest` that runs three checks in order of cost:

1. `cargo fmt --check` takes seconds and fails fastest on the most trivial problem.
2. `cargo clippy --all-targets --locked -- -D warnings` covers lints and also compiles the test code.
3. `cargo test --locked` runs the unit tests.

`--locked` makes CI fail if `Cargo.lock` is out of date with `Cargo.toml`. Without it, a forgotten lockfile update would pass CI but break `cargo install --locked` for users.

The `concurrency` block cancels a run that is still in progress when a newer commit lands on the same branch or PR, so rapid pushes don't queue up redundant runs.

### `release.yml`: ship this version

Runs **only** when a tag matching `v*` is pushed. It has three jobs, chained with `needs:`:

```
verify-version ──► build (×6 targets, parallel) ──► publish
```

- **`verify-version`** reads the crate version with `cargo metadata` and compares it to the tag with the leading `v` stripped. This matters because the TUI shows `env!("CARGO_PKG_VERSION")` in its status bar. A `v0.4.0` release containing a binary that reports `v0.3.0` would be confusing and hard to undo once published. This job runs on a cheap Linux runner *before* the matrix, so a typo in the tag costs seconds rather than macOS minutes.
- **`build`** is a 6-entry matrix:
  - **Linux (x86_64, arm64), musl, built with `cross`.** musl produces a fully static binary that runs on any distribution. This only works because the app has no C dependencies. Desktop notifications come from `notify-rust`, which on Linux uses `zbus`, a pure-Rust D-Bus implementation, so there is no `libdbus` to link. If someone later adds a dependency that needs a system C library, this is the target that will break first. `cross` runs the build in a container with the right linker, which is the simplest way to get aarch64 musl working on an x86 runner.
  - **macOS (x86_64, arm64)** on `macos-latest`, which is Apple Silicon. arm64 builds natively, and x86_64 is a plain `--target` cross-compile that Apple's toolchain supports out of the box.
  - **Windows (x86_64, arm64)** on `windows-latest`. The MSVC toolchain on the hosted image includes ARM64 build tools, so arm64 is also just a `--target` flag.
  - `fail-fast: false` lets every target finish even if one fails, so a single broken platform doesn't hide the state of the others.
- **Packaging** (inside `build`) produces `tuido-<tag>-<target>.tar.gz` (or `.zip` on Windows) containing the binary, `README.md` and `LICENSE`, plus a `<archive>.sha256` next to it. The script is plain bash on all three OSes: GitHub's Windows runners ship Git Bash, and `7z` is preinstalled for zipping. For the checksum it uses `sha256sum` where available and falls back to `shasum -a 256` on macOS.
- **`publish`** downloads all the artifacts (`merge-multiple: true` flattens them into one directory) and creates the release with `softprops/action-gh-release@v3`. `generate_release_notes: true` builds the changelog from merged PRs and commits. It is the only job with `contents: write` permission. The build jobs run with the default read-only token, so a compromised build step can't modify the repo.

## Key decisions and why

- **Linux-only CI, binaries only on tags.** The repo is private, so Actions minutes are billed: macOS at 10× and Windows at 2× the Linux rate. The first draft ran tests on all three OSes for every push and offered a manual (`workflow_dispatch`) build. In review, the user cut both to save minutes. The cost is that an OS-specific break (for example a path or rename quirk on Windows) is only found when the release is built. Nothing is published unless all 6 builds succeed, so a bad release can't go out half-built. To recover, fix the problem, bump the version and tag again.
- **Matrix values reach scripts through `env`, not inline `${{ }}`.** `${{ }}` is substituted into the script text *before* bash sees it, which is the classic Actions injection vector, and it also hides the real value from shellcheck. That is exactly the warning actionlint raised on `[[ "${{ matrix.target }}" == *windows* ]]`. Reading `$TARGET` from the environment avoids both problems.
- **Hand-written YAML, not `cargo-dist`.** The project is a single binary with no installers. Roughly 120 lines of readable YAML are easier to own than a generated workflow plus a `dist` config. Revisit if Homebrew, MSI or shell installers are wanted.
- **Actions pinned to major versions** (checkout v7, upload-artifact v7, download-artifact v8, action-gh-release v3, rust-cache v2, install-action v2). These were the current majors when this was written. Pinning to commit SHAs plus Dependabot is the stricter option if supply-chain hardening becomes a priority.
- **`rustfmt.toml` with `max_width = 120`.** The existing code was written at roughly 120 columns. The default of 100 would have reformatted 29 places; 120 reformatted 13. The one-time reformat in this change is purely mechanical.

## How to cut a release

Documented in the README's "Releasing" section:

1. Bump `version` in `Cargo.toml` and commit.
2. `git tag vX.Y.Z && git push origin vX.Y.Z`

## Not yet verified

These are verified by the first real tag push:
- Static linking of the Linux musl binaries (`ldd` should say "not a dynamic executable").
- The Windows and cross-architecture builds.

Verified locally: actionlint (with shellcheck) passes on both workflows, fmt, clippy and the tests pass, and an `aarch64-apple-darwin` build was packaged with the actual workflow script, with `shasum -c` passing on the result.

## Things a future reader should know

- The stale tag `v0.1.0` predates these workflows and won't trigger anything. Only newly pushed tags fire `release.yml`.
- Re-pushing an existing tag updates that release in place and overwrites its assets.
- On a private repo the CI badge only renders for viewers who have access to the repo.
