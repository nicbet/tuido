# Walkthrough: Respect XDG_CONFIG_HOME and allow a custom todo file path

Also fixes bug **tuido-2170e7**: the atomic save broke symlinked todo files.

## What was built

Before this change, tuido always used `~/.config/tuido/todos.json`. Now the file is resolved in this order, first match wins:

1. `-f/--file PATH`
2. `$TUIDO_FILE`
3. `$XDG_CONFIG_HOME/tuido/todos.json`
4. `~/.config/tuido/todos.json`

There are also `--help` and `--version`, the help popup (`?`) shows which file is in use, and saving through a symlink now works. That last part matters because the main reason to pick a custom path is to put the file in a synced folder, often via a symlink.

## How the pieces fit

```
main.rs   cli::parse(args) ──► Command::{Run{file}, Help, Version}
             │ Run
             ▼
          storage::resolve_todos_path(file, &PathEnv) ──► (path, PathSource)
             │ if source.is_explicit() (Flag | Env)
             ▼
          storage::validate_explicit_path(path)   // absolute + parent dir must exist
             ▼
          storage::load_todos(path) ──► App::new(todos, Some(path))
                                          └─ every save: storage::save_todos → save_target()
```

Everything up to `App::new` happens **before** `ratatui::init()`, so every error prints as a normal terminal message with a meaningful exit code:
- **0:** help or version printed.
- **2:** bad command-line usage.
- **1:** the file can't be resolved or loaded.

### `cli.rs`: argument parsing

A hand-rolled loop over `OsString` arguments. Notes:
- **`--help` and `--version` return immediately,** so `tuido -f x --help` prints help.
- **Repeated `--file` uses the last value,** so an alias like `alias tw='tuido -f work.json'` can still be overridden.
- **A value after `-f` that starts with `-` is rejected.** `-f --help` is almost certainly a slip; a file actually named `-x` can be given as `./-x`. The check uses `as_encoded_bytes()`, which is safe to read, so non-UTF-8 values still pass.
- **`--file=PATH` is split only for UTF-8 arguments.** Splitting a non-UTF-8 `OsStr` safely requires `unsafe { OsStr::from_encoded_bytes_unchecked }`. Because `-f PATH`, `--file PATH` and `TUIDO_FILE` all accept non-UTF-8 paths, we chose not to carry `unsafe` code for a vanishingly rare form. This was decided in code review and recorded in the spec.

`clap` wasn't used. There are three flags and none on the roadmap, so the dependency, compile time and binary size weren't worth it. If flags multiply, revisit that.

### `storage.rs`: resolution

`resolve_todos_path(cli_file, &PathEnv)` is **pure**. The environment lookup, the home directory and the file-exists check are all injected through `PathEnv` (`&dyn Fn` closures). Tests pass fake closures and never call `std::env::set_var`, which is `unsafe` in edition 2024 and races between parallel tests.

Rules worth knowing:
- **Empty variables count as unset.** A relative `XDG_CONFIG_HOME` is ignored, as the XDG Base Directory spec requires.
- **Legacy fallback:** if `XDG_CONFIG_HOME` is set but `$XDG_CONFIG_HOME/tuido/todos.json` doesn't exist while `~/.config/tuido/todos.json` does, the legacy file is used, reported as `PathSource::Legacy`. This is the important migration guard. Users who already had `XDG_CONFIG_HOME` set would otherwise open tuido after upgrading and see an empty list. Their data wouldn't be gone, but it would look gone, and the first save would create a second file. We **never move** the file, because sync clients or dotfile links may point at the old path. The README explains how to switch by hand.

`PathSource::is_explicit()` separates paths the user typed (flag or env var) from app-owned ones. For explicit paths, `validate_explicit_path` makes them absolute, so messages and the help popup show an unambiguous path, and **refuses to start if the parent directory is missing**. A typo like `-f ~/Dropbx/todos.json` should fail loudly, not quietly start a new, empty list. App-owned directories (default and XDG) are still created on first save.

### `storage.rs`: saving, and the symlink bug

The save is atomic: write a temp file, then `rename` it over the target. `rename` replaces whatever directory entry it is pointed at. So when `todos.json` was a symlink into `~/Dropbox`, the old code replaced the **link** with a regular file, and the synced copy silently stopped updating.

`save_target(path)` decides what to replace:
1. **The path exists:** `canonicalize` it and replace the real file. The link is untouched.
2. **The path is a dangling symlink:** this is the normal first-time setup, `ln -s ~/Dropbox/todos.json …` before the target exists. Resolve one hop with `read_link`; a relative link is resolved against the link's own directory. The target's directory **must already exist**, and it is never created. If the sync folder isn't mounted, creating it would put a local directory in its place and the data would silently diverge. Instead the save fails with an error, which is shown in the Info panel.
3. **Anything else:** use the path as-is and create its directory, which is the old behaviour.

The temp file is written next to the **real** target, so the rename stays on one filesystem and remains atomic, even when the link points into another mount. It is named by appending `.tmp` to the whole file name (`todo.txt.tmp`); previously `with_extension` produced `todo.json.tmp` for a `.txt` file.

Known limitation: a chain of dangling links (`a -> b -> missing`) resolves only one hop, so the inner link `b` would be replaced. This is rare and documented in the spec.

### UI

`format::display_path` abbreviates the home directory to `~`. The help popup puts `File: …` on the **first** line, because the popup was already clipped on short terminals, and wraps long paths such as iCloud's.

## Decisions (with where they came from)

| Decision | Source |
|---|---|
| Fall back to the legacy file; never move it | Decision sub-agent (Q1) |
| Hand-rolled parser, not clap | Decision sub-agent (Q2) |
| `-f`, `--file`, `--file=`; last one wins | Decision sub-agent (Q3) |
| Refuse explicit paths whose directory is missing | Decision sub-agent (Q4) |
| Active file shown in the help popup | Decision sub-agent (Q5) |
| Fix symlink saves here (bug tuido-2170e7) | Raised by the decision sub-agent; filed and fixed |
| Dangling links: write the target, never create its directory | Code review (should-fix) |
| `--file=` UTF-8 only, no `unsafe` | Code review; spec amended |
| Reject `-f -something` | Code review (nit) |

## Testing

- Unit tests for every parser form and error.
- Resolver tests for each precedence step, empty and relative variables, the legacy fallback in both directions, and a missing home directory.
- Validation tests against real temp directories.
- Unix-only tests for a symlink, a relative dangling link, and a dangling link into a missing directory.
- End to end with a scratch `HOME`: the exit codes, the legacy fallback (the XDG file is not created), saving through a regular and a dangling symlink, and a relative `TUIDO_FILE`.
