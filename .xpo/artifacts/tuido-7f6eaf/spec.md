# README demo GIF recorded with VHS

## What
A reproducible, scripted demo of tuido rendered to `demo/demo.gif` via VHS, embedded at the top of the README.

## Why
A README for a TUI is much more convincing with a short animated demo. Scripting it (vs. screen recording) makes it re-recordable in one command when the UI changes.

## How
- `demo/demo.tape` — VHS script.
  - Hidden setup: `export HOME=$(mktemp -d)` so storage resolves to a throwaway `~/.config/tuido/todos.json` (`dirs::home_dir` honours `$HOME` on unix), then launch `./target/release/tuido` and pre-populate a few tasks while hidden so the list is not empty when recording begins.
  - Visible part (~20s): add a task with a natural-language due date, navigate, toggle completion, suspend a task, reorder, hide completed, show help, quit.
- `make demo` target depends on `build` (release) and then runs `vhs demo/demo.tape`.
- README: embed `demo/demo.gif` under the intro; short note in Local Development on re-recording.

## Decisions
- Seed data is entered via keystrokes in a hidden section rather than a JSON fixture, so due dates stay relative to recording time and nothing goes overdue (avoids desktop notifications firing mid-record).
- The release build runs as a Makefile prerequisite, not inside the tape: a cold build inside the hidden section raced the fixed `Sleep` and the recording.
- GIF (not MP4) so it autoplays inline on GitHub. Target < 5 MB.

## Acceptance criteria
- `make demo` produces `demo/demo.gif` without touching the real todo file.
- README shows the GIF.
- GIF is legible, under ~5 MB, and demonstrates add-with-schedule, toggle, suspend, reorder, hide completed, help.
