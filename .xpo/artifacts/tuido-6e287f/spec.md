## What

`H` (Shift+h) hides suspended todos, independently of `h`, which hides completed ones. Replace the `App::hide_completed: bool` with one `ViewFilter` type that every visibility decision goes through, so a future filter is one field and one rule, not another special case spread across the code.

## Why

Suspended means "parked, not now". With many of them, the actionable list gets noisy. This was deferred from tuido-ae3d6f, whose design note asked for a unified view filter rather than a second bool.

## Acceptance Criteria

- [ ] A `ViewFilter` (in `app.rs`) holds which categories are hidden and answers `shows(&Todo) -> bool` and `hidden_count(&[Todo]) -> usize`. `App::is_visible`, and through it `visible_indices`, `selected_index`, `fix_selection`, navigation, reorder and undo selection, use it. Only `ui::list_title` reads an individual flag (`hide_suspended`).
- [ ] **Filtering follows display precedence:** a todo that is both completed and suspended counts as **completed**. It is hidden by `h`, and **shown** when only `H` is active.
- [ ] `H` in normal mode toggles hiding suspended todos. `h` is unchanged. In add, edit and reschedule modes, `H` types a literal `H`.
- [ ] Toggling a filter keeps the selection on a visible todo (`fix_selection`). Toggles don't record undo steps.
- [ ] Suspending a todo while suspended todos are hidden makes it vanish, shows the `<label> (u to undo)` hint (the same as completing while completed are hidden; both use `!is_visible(index)`), and moves the selection to a visible todo.
- [ ] **List title:**
  - No filter, or only `h`: exactly as today, for example `TODOs (3) · 2 suspended / 4 hidden`.
  - With `H`: the `· N suspended` segment is dropped, and `/ N hidden` is the total from `hidden_count`. Only `H` gives `TODOs (3) / 2 hidden`; both give `TODOs (3) / 6 hidden`.
  - The hidden segment appears only when its count is greater than 0.
  - A todo that is both completed and suspended counts once, as completed, under every filter.
- [ ] **Everything hidden:** when todos exist but none is visible, the Info panel shows `All N todos hidden · h/H to show`, or `The only todo is hidden · h/H to show` when there is exactly one (revised after code review, for grammar), instead of "No todos".
- [ ] Filters are not persisted. They reset on launch, as `hide_completed` does today.
- [ ] The help text (`h / H : Toggle hide completed / suspended`) and the README keybindings are updated.
- [ ] Tests:
  - A `ViewFilter::shows` truth table, including completed and suspended together under each filter.
  - Navigation and reorder skip hidden suspended todos.
  - Suspending while hidden shows the hint, and undo brings the todo back and selects it.
  - Title variants, including a completed and suspended todo with only `H` active.
  - Toggles record no undo step.
  - The all-hidden Info text, singular and plural.
  - `H` in add mode inserts `H`.
  - Existing `hide_completed` tests are ported.

## Flow

1. **`src/app.rs`:**
   ```rust
   #[derive(Default, Clone, Copy, PartialEq, Debug)]
   pub struct ViewFilter { pub hide_completed: bool, pub hide_suspended: bool }
   impl ViewFilter { pub fn shows(&self, todo: &Todo) -> bool; pub fn hidden_count(&self, todos: &[Todo]) -> usize }
   ```
   - `App.view: ViewFilter` replaces `hide_completed`.
   - `is_visible` calls `self.view.shows(t)`.
   - The `h` and `H` arms are one line each: `toggle_view(|v| &mut v.hide_…)`, which flips the flag and calls `fix_selection`.
   - `toggle_suspended` adds the vanish hint and calls `fix_selection`.
2. **`src/ui.rs`:**
   - `list_title` uses `app.view`.
   - `render_info` shows the all-hidden text. The condition is `hidden_count == len` on a non-empty list, which needs no allocation.
   - Update `HELP_TEXT`.
3. **README:** keybindings.

## Decisions

- **Independent flags in a struct, and independent keys (`h`/`H`), not a cycling mode.** A cycle is a hidden mode you have to keep track of, and it couldn't hide suspended without hiding completed too. `H` pairs with `h` the way `J`/`K` pair with `j`/`k`.
- **Completed beats suspended when filtering,** consistent with how the todo is displayed and counted.
- **One "hidden" total in the title.** It's the shortest option and doesn't count suspended todos twice. The `· N suspended` segment disappearing is itself the signal that suspended todos are hidden.
- **Filters aren't persisted.** Persisting would need a new config file or format, and a filter that returns silently after a restart hides todos without explanation.
- **Adding a filter:** a field, a rule in `shows` (mind the precedence chain if the new category overlaps existing ones), and a one-line key arm. Add title wording if needed.

## Edge Cases

- **MEDIUM:** every todo hidden. Handled by the all-hidden Info text.
- **LOW:** undo restoring a todo that is hidden. Selection falls back to the nearest visible one, which is already handled by `fix_selection`.

## Resolved Questions (decided by the decision sub-agent, adopted)

1. **Key:** `H` toggles hide-suspended, independently of `h`.
2. **Title:** one `/ N hidden` total. Drop `· N suspended` while suspended todos are hidden.
3. **Persistence:** none.
4. **Everything hidden:** `All N todos hidden · h/H to show`, with a singular form for one todo.
