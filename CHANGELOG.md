# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-10-02

Versions 0.2.0 and 0.2.1 were never tagged; their changes are included here.

### Added

- Undo (`u`) and redo (`Ctrl+R`) for add, edit, reschedule, complete, suspend, delete, reorder, and clear all.
- Richer natural-language due dates: `today`, `tonight`, weekdays (`Friday`, `next Friday`), relative times (`in 2 hours`, `in 3 days`), and absolute dates (`2026-04-15`, `Mar 14`). The input box previews the parsed due date while typing.
- `-f/--file`, the `TUIDO_FILE` environment variable, and `XDG_CONFIG_HOME` support for choosing the todo file; `--help` and `--version`.
- Hide/show suspended todos (`H`), alongside hide/show completed (`h`).
- Reorder todos with `Shift+↑/↓` or `J/K`.
- Clear all todos (`X`, with confirmation).
- Help screen (`?`), which also shows the todo file in use.
- Cursor movement and word/line deletion while adding or editing.
- Version number shown in the app.
- Prebuilt release binaries for Linux (static musl), macOS, and Windows on x86_64 and arm64.

### Changed

- Suspended todos are styled distinctly and no longer show the scheduled icon.
- Completed todos render in a subdued color without an icon.
- Overdue notifications are remembered across restarts, so each overdue todo notifies once (and again only after it is rescheduled).

### Fixed

- Overdue coloring and Info panel text.
- Tests no longer leave `tuido-test-*` directories in the system temp dir.

## [0.1.0] - 2025-09-19

Initial release: add, edit, delete, complete, and suspend todos; due dates from
times of day and `tomorrow`; desktop notifications for overdue todos; storage in
`~/.config/tuido/todos.json`.

[Unreleased]: https://github.com/nicbet/tuido/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/nicbet/tuido/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/nicbet/tuido/releases/tag/v0.1.0
