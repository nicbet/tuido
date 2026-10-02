## What

Make the todo file location configurable. Resolution order, first match wins:

1. `-f/--file <path>` command-line flag
2. `TUIDO_FILE` environment variable
3. `$XDG_CONFIG_HOME/tuido/todos.json`, falling back to the legacy file when only that exists (see Decisions)
4. `~/.config/tuido/todos.json` (today's hard-coded path)

Also add `-h/--help` and `-V/--version`, show the active file in the help popup, and fix bug tuido-2170e7 (atomic save breaks symlinks) because this issue's synced-folder use case depends on it.

## Why

- Users who set `XDG_CONFIG_HOME` expect apps to respect it.
- Separate lists (work and personal) or a file in a synced folder (Dropbox, iCloud, Syncthing) aren't possible today.
- `tuido --version` is the standard way to check which release you're on, now that prebuilt binaries are shipped (tuido-97259f).

## Acceptance Criteria

- [ ] With no flag, no `TUIDO_FILE` and no `XDG_CONFIG_HOME`, the path is `~/.config/tuido/todos.json`, so existing users see no change.
- [ ] Each override works and beats every source below it. A resolution function with injected inputs has a test case for each step of the order.
- [ ] Empty `TUIDO_FILE` and `XDG_CONFIG_HOME` count as unset. A relative `XDG_CONFIG_HOME` is ignored, as the XDG Base Directory spec requires.
- [ ] **Legacy fallback:** if the XDG path was chosen but `$XDG_CONFIG_HOME/tuido/todos.json` doesn't exist while `~/.config/tuido/todos.json` does, tuido reads and writes the legacy file. It never moves or copies the file.
- [ ] `-f PATH`, `--file PATH` and `--file=PATH` all work. If the flag is repeated, the last one wins.
- [ ] Non-UTF-8 paths are accepted through `-f PATH`, `--file PATH` and `TUIDO_FILE`. `--file=PATH` handles UTF-8 only (see Decisions).
- [ ] A value after `-f`/`--file` that starts with `-` is rejected as a likely slip, for example `-f --help`. A file named `-x` can still be given as `./-x`.
- [ ] A relative `--file` or `TUIDO_FILE` path is made absolute at startup.
- [ ] An explicit path (flag or env var) whose parent directory doesn't exist makes tuido refuse to start: an error on stderr and exit code 1, before the TUI starts. The default and XDG paths still create their directory on first save.
- [ ] `-h/--help` prints usage, the resolution order (including the legacy fallback) and the default path to stdout and exits 0. `-V/--version` prints `tuido <version>` to stdout and exits 0. Neither touches the terminal UI.
- [ ] An unknown argument, or `--file` with no value, prints a one-line error plus a hint to use `--help` to stderr and exits 2.
- [ ] The help popup shows `File: <path>`, with the home directory abbreviated to `~`. Long paths wrap rather than being cut off.
- [ ] **Bug tuido-2170e7:** saving through a symlinked todo file updates the file it points to and keeps the symlink. This also holds for a **dangling** symlink, the first-time setup where the target doesn't exist yet: the target file is created if its directory exists. If the target's directory doesn't exist (for example an unmounted sync folder), the save fails with an error and the link is left alone. The temp file is `<filename>.tmp`, for example `todo.txt.tmp`.
- [ ] The README's Configuration section documents all of this, including how to switch to the XDG location and that two tuido instances on the same file overwrite each other.

## Flow

1. **New `src/cli.rs`:**
   ```rust
   pub enum Command { Run { file: Option<PathBuf> }, Help, Version }
   pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String>
   pub const HELP: &str
   ```
   It is hand-rolled. `--file=PATH` is split only when the argument is valid UTF-8. Unit tests cover each form, repeats, and errors.
2. **`src/storage.rs`:**
   ```rust
   pub enum PathSource { Flag, Env, Xdg, Legacy, Default }
   pub struct PathEnv<'a> { var: &'a dyn Fn(&str) -> Option<OsString>, home: Option<PathBuf>, exists: &'a dyn Fn(&Path) -> bool }
   pub fn resolve_todos_path(cli_file: Option<PathBuf>, env: &PathEnv) -> io::Result<(PathBuf, PathSource)>
   pub fn validate_explicit_path(path: &Path) -> Result<PathBuf, String>
   ```
   - The resolver is pure: every input comes in through `PathEnv`. Tests use fake closures and never touch the real environment.
   - `validate_explicit_path` makes the path absolute with `std::path::absolute` and checks the parent directory exists.
   - Fix bug tuido-2170e7 in `save_todos` using a `save_target(path)` helper:
     - If the path canonicalizes, use the real file.
     - Otherwise, if the path is a dangling symlink, resolve one hop with `read_link` (a relative link resolves against the link's directory). The target's directory must already exist; it is never created.
     - Otherwise, use the path as-is, creating its directory as before.

     Write `<target>.tmp` next to the target, then rename it onto the target.
3. **`src/format.rs`:** `display_path(path, home) -> String`, which abbreviates the home directory to `~`.
4. **`src/main.rs`:** `cli::parse(env::args_os().skip(1))`.
   - `Help` and `Version` print and exit 0. A parse error prints to stderr and exits 2.
   - `Run`: resolve the path, apply `validate_explicit_path` for `Flag` or `Env` (exit 1 on failure), then load and run as today.
5. **`src/ui.rs`:** the help popup text is built with `format!`, puts the `File:` line first, and wraps.
6. **README:** extend the Configuration section, add a Usage line, and add a note about concurrent access.

## Decisions

- **The resolver has its inputs injected,** including the existence check. It is testable without `std::env::set_var`, which is `unsafe` in edition 2024 and racy across parallel tests.
- **Fall back to the legacy file instead of moving it.** It is the only option where data can't look lost and the files can't diverge. Moving the file could break sync clients or dotfile links pointing at the old path, and a move across filesystems isn't atomic. A warning would be invisible once the TUI takes over the screen. To switch, users move the file themselves, which the README documents.
- **Hand-rolled argument parsing, not `clap`.** There are three flags and no roadmap item adds more, so a dependency and the compile time and binary size it costs aren't justified.
- **`--file=PATH` is UTF-8 only (revised after code review).** Splitting a non-UTF-8 `OsStr` needs `unsafe { OsStr::from_encoded_bytes_unchecked }`. That case is vanishingly rare and fully covered by `-f PATH`, `--file PATH` and `TUIDO_FILE`, so carrying `unsafe` code in a small app isn't worth it.
- **Flag-looking values after `-f` are rejected (from code review).** `-f --help` is far more likely a slip than a file named `--help`.
- **Explicit paths with a missing parent directory are refused.** A typo would otherwise quietly start a new empty list. The default and XDG locations are app-owned, so creating them is expected.
- **Symlinks are resolved at save time, not at startup.** That stays correct even if the link is created while tuido is running, and every save goes through the same place. A dangling link's target directory is never created, so an unmounted sync folder can't be shadowed by a local directory.
- **Windows and macOS keep `~/.config/tuido` as the fallback,** not `%APPDATA%` or `~/Library/Application Support`, because that is where existing users' data already lives. `XDG_CONFIG_HOME` is honoured on every OS if set.
- **The active file goes in the help popup, not the Info panel.** It answers "which list is this?" without permanent clutter.

## Edge Cases

- **HIGH (migration):** a user with `XDG_CONFIG_HOME` set has existing data at the legacy path. Handled by the legacy fallback.
- **MEDIUM:** a typo in an explicit path. Handled by refusing to start when the parent directory is missing.
- **MEDIUM:** a symlinked todo file, including a dangling link. Fixed (bug tuido-2170e7).
- **MEDIUM:** two instances, or sync conflicts, on one file: the last writer wins. Out of scope; noted in the README.
- **LOW:** a chain of dangling symlinks. Only one hop is resolved, so the save may replace the inner link. Acceptable.
- **LOW:** `--file` points at a directory. Loading fails with a read error, and the existing refuse-to-start path reports it.
- **LOW:** no home directory and no overrides. Same error as today.

## Resolved Questions (decided by the decision sub-agent, adopted)

1. **XDG migration:** fall back to the legacy file only when the XDG file is missing and the legacy one exists. Never move it.
2. **Argument parsing:** hand-rolled.
3. **Flag forms:** `-f PATH`, `--file PATH` and `--file=PATH`, plus `-h` and `-V`. Last one wins.
4. **Missing parent directory:** refuse to start for explicit paths. Auto-create for the default and XDG locations.
5. **Active file in the UI:** shown in the help popup with `~` abbreviation.
