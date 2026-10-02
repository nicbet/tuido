# Walkthrough: Make suspended todos visually distinct

## What changed and why

Suspended todos ("parked, not now") used to look exactly like active todos apart from a `[~]` prefix. They also showed the ⏰ "scheduled" icon, which promises a reminder that never fires, because `Todo::is_overdue` excludes suspended todos. They now render in yellow italic, show no icon, and get their own selected highlight (black on yellow). The list title separates them from the actionable count, for example `TODOs (3) · 2 suspended`.

The bigger change is structural. How a todo looks used to be decided by three separate `if` chains:
- the prefix, in `todo_item`
- the text and row styles, in `todo_item`
- the Info-panel text and colour, in `render_info`

Each chain encoded the precedence between states slightly differently. Adding a state meant editing all three in step.

## How it fits together (`src/ui.rs`)

```rust
enum DisplayState { Completed, Suspended, Overdue, Scheduled, Plain }
fn display_state(todo, now) -> DisplayState
```

`display_state` is the **only** place precedence is decided: Completed > Suspended > Overdue > Scheduled > Plain. For Overdue it calls `todo.is_overdue(now)` rather than repeating the conditions, so the UI can never disagree with notifications about what counts as overdue. This will matter again when tuido-839e8f changes the notification logic.

Each presentation choice is then a method that matches on every variant:
- `prefix()`: `[x]`, `[~]` or `[ ]`.
- `text_style(selected)`: the style of the text span. Ratatui merges the span's style on top of the row's, so the **span's foreground wins**. Any state whose selected row has a coloured background (overdue red, completed grey, suspended yellow) must therefore also set a black foreground on the span. Otherwise you get yellow text on a yellow bar. That's why `text_style` takes `selected`.
- `row_style(selected)`: the background highlight. Plain and scheduled rows keep the original `REVERSED`.
- `icon()`: ❗ overdue, ⏰ scheduled, nothing otherwise.
- `info_color()`: the Info-panel colour.

All of these match every variant explicitly; there are no `_ =>` arms. If a new state is added (for example by the view-filter work in tuido-6e287f), the compiler points at every method that needs a decision instead of silently falling back to `[ ]` or no icon.

`DisplayState` lives in `ui.rs`, not on `Todo`. "Scheduled versus plain" and the precedence order are presentation decisions. The model only owns `is_overdue`, which behaviour such as notifications depends on.

`list_title(app)` was extracted as a small pure function. It counts completed todos, suspended-but-not-completed todos, and actionable ones as the remainder. A todo with both flags counts as completed, matching the display precedence.

## Behaviour change in `src/app.rs`

`s` on a completed todo is now a no-op. Before, it toggled a flag that the UI couldn't show, because completed wins. Completing a todo deliberately does **not** clear `suspended`: if it did, un-completing would lose the suspended state. The no-op returns before `save()`, so when undo (tuido-2203d3) arrives it must not record an entry for it.

## How we know nothing else changed

The tests compare foreground, background and modifiers on the rendered terminal cells, not just the text. That matters because the bug class here is "right text, wrong colour". Those same style tests, minus the suspended rows, were also run against `main`'s old `ui.rs` and passed, which proves plain, scheduled, overdue and completed rows look identical before and after the refactor.

Note for future test writers: rendered cells also carry an `underline_color`, so compare `(fg, bg, add_modifier)` rather than whole `Style` values. The `row_style` test helper does this.

## Decisions (open questions were answered by the decision sub-agent)

- **No ⏸ icon.** `[~]` plus the style is enough, and every extra emoji risks misaligned rows on terminals that disagree about glyph width.
- **`[~]` is kept.** Colour must not be the only signal, for monochrome terminals and colour-blind users.
- **No hide-suspended key yet.** It was filed as tuido-6e287f, with guidance to replace `hide_completed` with a single view filter rather than adding a second bool.

## Performance

One classification per visible row plus two O(n) counts for the title each frame. That is the same order of work as before.
