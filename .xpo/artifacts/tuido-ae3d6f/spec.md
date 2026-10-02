## What

Give suspended todos their own look in the list. Today they differ from active items only by a `[~]` prefix, and they still show the ⏰ "scheduled" icon even though a suspended todo never becomes overdue or notifies. Along the way, consolidate how a todo's display is chosen, which is currently a hand-rolled `if` cascade in `ui::todo_item` that will keep growing as states are added.

## Why

- **Scanning:** suspended means "parked, not now". It should visually recede next to actionable items, in the same way completed items do.
- **Correctness of signals:** the ⏰ icon promises a reminder that will never fire.
- **Maintainability:** prefix, text style, selected style, icon and Info-panel colour are decided in three separate `if` chains across `todo_item` and `render_info`. Adding a state means editing all of them in step.

## Acceptance Criteria

- [ ] A suspended (not completed) todo renders as yellow italic text with the `[~]` prefix, distinct from active, overdue and completed items.
- [ ] A selected suspended todo renders black text on a yellow background, mirroring the overdue (red) and completed (grey) selections.
- [ ] Suspended todos show no icon, even when they have a due date.
- [ ] The Info panel for a suspended todo is yellow and still shows its due date if it has one: `Status: Suspended · due <time> (<age>)`.
- [ ] The list title counts only actionable todos (not completed, not suspended) and appends ` · M suspended` when M > 0, for example `TODOs (3) · 2 suspended / 4 hidden`. With no suspended todos the title is unchanged.
- [ ] `s` on a completed todo does nothing.
- [ ] Rendering is driven by one `DisplayState` classification, so precedence is defined in one place and covered by a unit test.
- [ ] Existing styles for active, overdue and completed todos are unchanged. Tests assert the styles, not just the characters.

## Flow

1. `src/ui.rs`: add
   ```rust
   enum DisplayState { Completed, Suspended, Overdue, Scheduled, Plain }
   fn display_state(todo: &Todo, now: DateTime<Utc>) -> DisplayState
   ```
   Precedence is Completed > Suspended > Overdue > Scheduled > Plain. Overdue comes from `todo.is_overdue(now)` rather than repeating its conditions, so the classification can never drift from what notifications use.
2. Methods on `DisplayState`:
   - `prefix()`
   - `text_style(selected)`: span style. It must set black text when selected for Suspended and Overdue, because the span's colour wins over the row style.
   - `row_style(selected)`
   - `icon() -> Option<&str>`: ⏰ for Scheduled, ❗ for Overdue, `None` otherwise.
   - `info_color()`

   Rewrite `todo_item` and the status branch of `render_info` to use them.
3. `render_list`: compute `actionable` and `suspended` counts and build the title as in the AC.
4. `src/app.rs` `toggle_suspended`: return early if the todo is completed.
5. Tests:
   - `ui.rs`: precedence table for `display_state`.
   - Style assertions on rendered cells (foreground, background, modifiers) for each state, selected and unselected.
   - Title variants.
   - No ⏰ on a suspended todo.
   - `app.rs`: `s` on a completed todo is a no-op.

## Decisions

- **Classify once, then map state to presentation**, rather than adding more branches. Cheap: one classification per visible todo per frame, the same work as today.
- **Keep the `[~]` prefix.** It still matters on monochrome terminals and for colour-blind users, because colour must not be the only signal.
- **The classification lives in `ui.rs`, not on `Todo`.** "Scheduled versus plain" and the precedence order are presentation concerns. The model keeps `is_overdue` for notifications.
- **No icon for suspended,** rather than a ⏸ glyph. `[~]` and the style already say "suspended", and every extra emoji risks misaligned rows in terminals that disagree about glyph width.
- **`s` is a no-op on completed todos, and completing doesn't clear `suspended`.** Clearing would lose data, because un-completing wouldn't restore the suspended state. It would also mean undo (tuido-2203d3) has to capture two fields for a single toggle. Note for tuido-2203d3: the no-op must not record an undo entry.
- **No hide-suspended toggle in this issue.** Filed as a follow-up. If it's built, `hide_completed` should become one view-filter enum rather than a second bool.

## Edge Cases

- **MEDIUM:** Existing files may contain todos that are both `completed` and `suspended`. Completed wins visually and in counts. `s` can't create new ones.
- **LOW:** Terminals without italic support show plain yellow. That is acceptable.
- **LOW:** Under the light themes some terminals use, yellow-on-default can have low contrast. The selected state is black on yellow, which is fine.

## Assumptions

- Yellow means "suspended" in the list. `render_input` also uses yellow for the input text, but the input box replaces the Info panel and never appears in the list, so there's no clash.

## Resolved Questions (decided by the review sub-agent, adopted)

1. **Icon:** drop it; no ⏸.
2. **Hide-suspended toggle:** not in scope; filed as a follow-up.
3. **Title count:** count actionable todos only, and append ` · M suspended` when M > 0.
4. **`s` on completed:** no-op. Completing doesn't clear `suspended`.
