# Walkthrough: Undo (and redo)

Also fixes bug **tuido-e97675**: Ctrl+letter in normal mode triggered the plain-letter action, so `Ctrl+D` deleted.

## What was built

- `u` undoes and `Ctrl+R` redoes the last change to the list. That covers add, edit, reschedule, complete, suspend, delete, reorder and clear-all.
- History holds 100 steps for the session.
- After a destructive action (delete, clear-all, or completing a todo that then disappears because completed todos are hidden), the Info panel shows a hint such as `Deleted "Call mom" (u to undo)`.
- After an undo or redo it says `Undid: …` or `Redid: …`.

## How it works

### Snapshots, not inverse operations

```rust
struct UndoEntry { todos: Vec<Todo>, selected: usize, label: String, coalesce: Option<u32> }
undo_stack / redo_stack: VecDeque<UndoEntry>   // UNDO_LIMIT = 100, oldest dropped
```

Each step stores the **whole list** as it was before the change. The alternative is to write an inverse for every action: re-insert at this index, toggle back, and so on. That multiplies code and lets an inverse go subtly wrong. A snapshot is one mechanism that is automatically correct for every action, including ones not written yet. The cost is cloning a personal todo list once per user action: microseconds and a few KB. Coalesced moves skip even that.

### The one rule for new actions: `checkpoint` right before mutating

```rust
fn checkpoint(&mut self, label: String, coalesce: Option<u32>)
```

Each mutating method calls it **after its guards and no-op checks, immediately before it changes anything**. That placement is what guarantees the two properties the tests enforce:
- Every user action records **exactly one** step.
- A no-op records **none**: `s` on a completed todo, moving at the list boundary, empty or unparseable input, Esc, an unchanged edit, rescheduling to the same date, or a declined clear-all.

For edits, the no-op check applies the edit to a **clone** and compares whole `Todo`s with the new `PartialEq` derive. Any field an edit touches in future (richer date parsing in tuido-9cc989, for example) is covered automatically.

`checkpoint` also clears the redo stack: a new change starts a new branch of history.

**When you add an action:** call `checkpoint` in the right place, then add a row to both `every_action_is_one_undoable_step` and `no_op_actions_record_nothing`. Those two tests are the safety net. Each was mutation-checked: removing a guard makes a named case fail.

### Restore: `undo()` and `redo()` share `restore(HistoryStep)`

`restore` pops from one stack, swaps the snapshot in, and pushes the current state onto the other stack with the same label, so the step can be reversed again. Then:
- **`notified_at` is carried over from the current state, by id.** Notification stamps are system state (see tuido-839e8f). If the snapshot's value were restored, undoing an edit made before a notification fired would bring back `None` and send a duplicate notification. A todo absent from the current list, such as a deleted one being restored, keeps its snapshot value. Ids can't be mixed up, because history is linear: by the time an older snapshot holding an id is restored, any later todo that reused that id has already been undone away.
- **Selection** is restored from the snapshot, then `fix_selection` moves it to a visible todo if needed.
- **The message is set before `save()`,** so a save error, which is an Error message, replaces "Undid: …" instead of being hidden by it.

### Merging moves

Reordering is repeated `J`/`K`, so moving one todo ten places would otherwise take ten undos. Moves pass `coalesce: Some(todo.id)`. If the top undo step has the same id, `checkpoint` doesn't push a new step, because the existing one already holds the state from before the run started. Any other change pushes a step with `coalesce: None`, which ends the run.

A subtle case: **undo and redo also end a run.** Without that, "move A, move B, undo, move A" would fold the second A move into the first A step, and the state between them would become unreachable. `restore` clears `coalesce` on the new top of the undo stack. The test `undo_ends_a_run_of_moves` covers this; it was added after a mutation check showed the original tests didn't.

### Messages

`message` changed from `Option<String>` to `Option<Message { text, kind: Info | Error }>`. Errors (parse failures, save failures) stay red. Hints and undo results are cyan. The hint after completing uses `!is_visible(index)` rather than hard-coding `hide_completed`, so the planned hide-suspended filter (tuido-6e287f) gets the hint for free.

### Keys (and bug tuido-e97675)

`handle_normal_key` now checks modifiers first:
- `Ctrl+R` means redo.
- Any other Ctrl+letter is ignored. Before, `Ctrl+D` deleted and `Ctrl+S` suspended.
- `u` with no modifiers means undo.

The clear-all confirmation also requires a plain `y`, so `Ctrl+Y` can't wipe the list. In input modes nothing changed: `u` is typed, and `Ctrl+U` still deletes to the start of the line.

## Decisions

| Decision | Why |
|---|---|
| Snapshots, not inverse operations | One mechanism, correct for every action; negligible cost |
| Redo on `Ctrl+R` only | Makes `u` safe to press; `U` would clash with vim's line-undo habit |
| 100 steps, session only | Persisting would need a file format and raise multi-instance staleness, for no need |
| Keep the `X` confirmation | Undo doesn't survive `q`, and clear-all has the biggest blast radius |
| Hints only for destructive actions | Visible changes need no hint; constant hints get ignored |
| Merge moves of the same todo | One gesture should be one undo |

Open questions were decided by the decision sub-agent. The code review found that the no-op test asserted nothing, because it cleared the stack before its only check. It was fixed to assert per case and mutation-verified.
