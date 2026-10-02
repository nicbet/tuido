## What

Tests that touch the filesystem create `$TMPDIR/tuido-test-<pid>-<name>/` and never remove it. This machine currently has 278 such directories. Switch those tests to `tempfile::TempDir`, which deletes itself when dropped.

## Why

Every `cargo test` run leaks more directories. That is harmless but untidy, and it gets worse as more storage tests are added; the view-filter work in tuido-6e287f is next.

## Acceptance Criteria

- [ ] `tempfile` is added as a **dev-dependency** only, with no change to the release binary's dependency tree. Check with `cargo tree -e normal`.
- [ ] One shared test helper, `storage::test_support::temp_todo_file() -> (TempDir, PathBuf)`, used by the storage tests and the notification restart test in `app.rs`.
- [ ] After `cargo test`, no new `tuido-test-*` directories exist in `$TMPDIR`.
- [ ] The tests that use fake paths without touching the disk (the resolver tests' `tuido-resolve` paths, and `display_path` in `format.rs`) stay as they are.
- [ ] All existing tests pass unchanged in what they assert.

## Flow

1. `Cargo.toml`: add `[dev-dependencies] tempfile = "3"`.
2. `src/storage.rs`: add a `#[cfg(test)] pub(crate) mod test_support` containing `temp_todo_file()`. It returns the `TempDir` guard, which callers keep alive with `let (_dir, path) = …`, and `dir/todos.json` (not yet created). Replace `temp_file(name)` everywhere. The symlink tests use `path.parent()` as before.
3. `src/app.rs`: `notification_state_survives_restart` uses the helper; delete the manual `remove_dir_all`.
4. Verify: count `tuido-test-*` directories before and after `cargo test`.

## Decisions

- **`tempfile`, not a hand-rolled drop guard.** It's a dev-only dependency, so there's no runtime or binary cost. It handles unique naming and cleanup on drop, including on panic, across platforms. A hand-rolled guard would be more code to maintain.
- **Keep the guard alive by binding it** (`let (_dir, path)`), not `let (_, path)`. Binding to `_` would drop the guard immediately and delete the directory before the test uses it. The helper's doc comment says so.
- **The existing leftover directories on this machine are not deleted by this change.** That's a one-off manual cleanup, offered to the user separately.

## Edge Cases

- **LOW:** a test that panics still cleans up, because `TempDir` drops during unwinding.
- **LOW:** the helper returns a path to a file that doesn't exist yet in an existing directory, which matches what every current caller expects.
