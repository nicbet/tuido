# Walkthrough: README demo GIF recorded with VHS

## What was built
A scripted terminal recording of tuido that renders to `demo/demo.gif` and sits at the top of the README. Run `make demo` to regenerate it.

## The pieces
- **`demo/demo.tape`**: a [VHS](https://github.com/charmbracelet/vhs) script. VHS runs a headless terminal (ttyd and ffmpeg under the hood), replays the keystrokes in the script, and writes a GIF. The script has two parts:
  1. **Hidden setup** (`Hide` … `Show`): `export HOME=$(mktemp -d)` and then launch `./target/release/tuido`. tuido finds its data file through `dirs::home_dir()` (see `src/storage.rs`), which reads `$HOME` on unix. Every recording therefore starts with an empty, throwaway todo file, and the developer's real list is never read or written. Five tasks are then typed in at `Type@1ms` so the list isn't empty when recording starts.
  2. **Visible demo**: adds a task with a natural-language schedule ("tomorrow at 5pm"), completes two tasks, suspends one, reorders with `J`, toggles hide-completed with `h`, opens help with `?`, and quits. Each action is followed by a `Sleep` so viewers can follow it.
- **`Makefile`**: `demo: build` and then `vhs demo/demo.tape`. It's also listed in `make help`.
- **`README.md`**: `![tuido demo](demo/demo.gif)` under the intro, plus a "Re-record the README demo GIF" step in Local Development.

## Key decisions
- **Seed by keystroke rather than a JSON fixture.** Due dates are stored as absolute UTC timestamps. A checked-in fixture would go stale, its tasks would show as overdue, and tuido would fire real desktop notifications while recording. Typing "4pm" or "tomorrow at 10am" off-camera keeps the dates relative to whenever the recording runs. A time that has already passed rolls over to tomorrow, so nothing is ever overdue.
- **Build outside the tape.** The first version ran `cargo build` inside the hidden section and waited a fixed `Sleep`. In a clean worktree the build took longer than that, so the recording could start before the binary existed. Making `build` a Makefile prerequisite removes the race.
- **GIF instead of MP4.** GitHub autoplays GIFs inline in a README, while MP4s only work as uploaded attachments and show a player. The current GIF is about 330 KB at 24 fps and 1100×600.

## Notes for future changes
- If keybindings change, update the tape and re-run `make demo`. The visible actions assume the initial selection lands on the fourth task after the hidden `k`.
- If storage ever stops using `$HOME` (for example, it switches to `dirs::config_dir()` or an env var), update the hidden setup to match. Otherwise recordings will write to the real todo file.
