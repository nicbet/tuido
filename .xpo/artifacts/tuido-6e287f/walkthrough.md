# Walkthrough: Toggle to hide suspended todos (unify view filters)

## What was built

`H` (Shift+h) hides and shows suspended todos, independently of `h`, which hides completed ones. Behind it, the old `App::hide_completed: bool` became a single `ViewFilter` that every visibility decision goes through.

## The core: `ViewFilter`

```rust
// src/app.rs
pub struct ViewFilter { pub hide_completed: bool, pub hide_suspended: bool }

impl ViewFilter {
    pub fn shows(&self, todo: &Todo) -> bool {
        if todo.completed { !self.hide_completed }
        else if todo.suspended { !self.hide_suspended }
        else { true }
    }
    pub fn hidden_count(&self, todos: &[Todo]) -> usize
}
```

`App::is_visible` calls `self.view.shows(todo)`. Everything that cares about visibility already went through `is_visible`:
- `visible_indices`
- `selected_index`
- `fix_selection`
- navigation and reorder (`next_visible_after`, `prev_visible_before`)
- undo's selection restore
- the "(u to undo)" hints

So the new filter reached all of them without touching them. Of the filter flags, only the list title reads an individual one.

**Precedence:** the `if` chain checks completed first, so a todo that is both completed and suspended counts as **completed**: `h` hides it, and `H` alone doesn't. That matches how the todo is displayed and counted (tuido-ae3d6f). A 4×4 truth-table test pins this down.

**Adding a filter later:** add a field, a rule in `shows` (think about precedence if the new category overlaps existing ones), and a one-line key arm:

```rust
KeyCode::Char('H') => self.toggle_view(|view| &mut view.hide_suspended),
```

`toggle_view` flips the flag and calls `fix_selection`, so the cursor never sits on an invisible todo. Filters are view state: they aren't saved, aren't undoable, and reset on launch.

## Details that are easy to miss

- **Suspending can now hide a todo.** If suspended todos are hidden, pressing `s` makes the todo vanish. `toggle_suspended` therefore gained the same handling `toggle_completed` already had: an Info hint `Suspended "x" (u to undo)`, triggered by `!is_visible(index)`, and a `fix_selection` call. The spec didn't list `fix_selection`; a mutation check confirmed a test fails without it.
- **List title.** With no filter, or only `h`, it's unchanged: `TODOs (3) · 2 suspended / 4 hidden`. When `H` is on, the `· N suspended` segment is dropped and `/ N hidden` becomes the total hidden count, for example `TODOs (3) / 6 hidden`. The segment disappearing is the signal that suspended todos are hidden, and nothing is counted twice.
- **Everything hidden.** Instead of the misleading "No todos", the Info panel says `All N todos hidden · h/H to show`, or `The only todo is hidden · h/H to show`. The condition `hidden_count == len` on a non-empty list doesn't allocate, which matters because it runs every frame.
- **`H` only acts in normal mode.** In add, edit and reschedule it's a typed character (tested).

## Decisions (made by the decision sub-agent)

| Decision | Why |
|---|---|
| Independent `h`/`H` keys, not one `h` that cycles | A cycle is a hidden mode, and it couldn't hide suspended without completed |
| One "hidden" total in the title | Shortest option, no double counting |
| No persistence | A new config format for one keypress isn't worth it, and filters that silently return after a restart would hide todos without explanation |
| An all-hidden hint in the Info panel | "No todos" is wrong when todos exist |

Code review (APPROVE WITH NITS) led to: the singular wording being recorded in the spec, a title test for a completed and suspended todo under `H` alone, the `toggle_view` helper, and the allocation-free all-hidden check.
