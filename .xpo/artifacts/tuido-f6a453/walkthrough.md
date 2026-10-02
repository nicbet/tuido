# Walkthrough: Tests leave tuido-test-* directories behind

## Problem

Storage tests built paths like `$TMPDIR/tuido-test-<pid>-<name>/todos.json` by hand and never deleted the directory. Every `cargo test` run leaked a few more; there were 278 on the development machine.

## Fix

`tempfile` is now a **dev-dependency**, so the release binary is untouched. One shared helper replaces the hand-built paths:

```rust
// src/storage.rs
#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) fn temp_todo_file() -> (TempDir, PathBuf)
}
```

It returns a fresh, existing temporary directory, prefixed `tuido-test-` so any leak would be easy to spot, plus the path of a `todos.json` in it that doesn't exist yet. `TempDir` deletes the directory when it's dropped, including when a test panics.

**The one trap:** keep the guard alive for the whole test.

```rust
let (_dir, path) = temp_todo_file();   // ✅ lives until the end of the test
let (_, path) = temp_todo_file();      // ❌ dropped at once: the directory is deleted
```

The second form is especially sneaky here, because `save_todos` creates missing parent directories. It would quietly recreate the deleted directory, and the test would leak again without failing. The helper's doc comment warns about this.

`pub(crate)` lets tests in other modules use the helper. The notification restart test in `app.rs` already does, and the view-filter tests in tuido-6e287f can too.

## Details

- `explicit_path_needs_an_existing_directory` needs a parent directory that **doesn't** exist. A `TempDir` always exists, so the test now uses `dir/missing/todos.json`. The assertion is as strong as before, and the test now also cleans up the `missing/` directory it creates.
- Tests that only build fake paths and never touch the disk (the resolver's `tuido-resolve`, `format::display_path`) still use `std::env::temp_dir()`, which is fine.
- Verification: the number of `tuido-test-*` directories in `$TMPDIR` was the same before and after three test runs.
- The 278 directories already leaked on the development machine are outside this change's scope. Deleting them is a one-off manual cleanup.
