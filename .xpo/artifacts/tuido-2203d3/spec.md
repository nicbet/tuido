## What

Press `u` to undo and `Ctrl+R` to redo the last change to the todo list. Undo covers:
- add, edit and reschedule
- toggle complete and toggle suspend
- delete and reorder (consecutive moves of the same todo count as one step)
- clear-all

After a destructive action, the Info panel shows a hint such as `Deleted "Call mom" (u to undo)`. After an undo or redo, it says what was undone or redone. Also fixes bug tuido-e97675 (Ctrl+letter triggers the plain-letter action), because `Ctrl+R` needs to mean redo.

## Why

Destructive actions are one keystroke with no way back. `d` deletes immediately. `Space` toggles completion, and when completed todos are hidden the item disappears. A slip costs data. Redo makes undo itself safe: one `u` too many doesn't throw away the change it undid.

## Acceptance Criteria

- [ ] `u` (no modifiers) in normal mode undoes the most recent recorded change and saves the restored list to disk.
- [ ] `Ctrl+R` in normal mode redoes the most recently undone change and saves.
- [ ] Any new change clears the redo stack.
- [ ] Deleting and then pressing `u` restores the todo at its original position, with the same id, and selects it.
- [ ] Every user mutation listed in What records exactly **one** undo entry. Actions that change nothing record none:
  - `s` on a completed todo
  - moving at the list boundary
  - unparseable or empty input, and Esc
  - editing with unchanged text and no new date
  - rescheduling to the same date, or clearing an already-empty date
- [ ] Consecutive moves of the same todo (`J`, `K`, Shift+arrows) merge into one undo entry. Any other change breaks the run.
- [ ] Notification stamps (`take_due_notifications`) and view state (`h`, selection, help) are **not** undoable.
- [ ] Undo and redo keep each todo's **current** `notified_at`, matched by id. A todo absent from the current list (for example one that was deleted) keeps the snapshot's value. This is the constraint from tuido-839e8f.
- [ ] History holds at most 100 entries and drops the oldest. It lives for the session only.
- [ ] With nothing to undo or redo, the keys show "Nothing to undo" or "Nothing to redo".
- [ ] Hints and "Undid/Redid: …" messages are shown in a neutral colour, not the red used for errors. A save error after undo or redo replaces the message, rather than being hidden by it.
- [ ] After undo or redo, the selection is restored from the snapshot, then moved to the nearest visible todo if that one is hidden.
- [ ] The `X` confirmation prompt stays, because undo history doesn't survive `q`.
- [ ] **Bug tuido-e97675:** in normal mode, Ctrl+letter only triggers explicitly bound shortcuts (just `Ctrl+R`); `Ctrl+D`, `Ctrl+S` and the like do nothing.
- [ ] `u` and `Ctrl+R` appear in the help popup and the README keybindings.

## Flow

1. **`src/model.rs`:** `#[derive(PartialEq, Debug)]` on `Todo`. This is needed for no-op detection and for "restores exactly" tests.
2. **`src/app.rs`:**
   - `struct UndoEntry { todos: Vec<Todo>, selected: usize, label: String, coalesce: Option<u32> }`, plus `undo_stack` and `redo_stack: Vec<UndoEntry>`. `const UNDO_LIMIT: usize = 100`.
   - `fn checkpoint(&mut self, label: String, coalesce: Option<u32>)`:
     - If `coalesce` is `Some(id)` and the top entry has the same `coalesce`, do nothing. That entry already covers the run.
     - Otherwise push a snapshot of `todos` and `selected`, trim to `UNDO_LIMIT`, and clear `redo_stack`.

     Each mutating method calls it **after its guards and after detecting a no-op, immediately before mutating**. Moves pass `Some(todo.id)`; everything else passes `None`.
   - `fn undo(&mut self)` and `fn redo(&mut self)` share `fn restore(&mut self, from, to, verb)`:
     - Pop from one stack and push the current state onto the other with the same label.
     - Carry current `notified_at` values by id into the restored list.
     - Replace `todos`, restore `selected`, call `fix_selection`, set the `Info` message "Undid/Redid: <label>", then `save()`.
   - `submit_input` compares old and new values and only checkpoints and saves when something changed.
   - `handle_normal_key`:
     - `Char('r')` with CONTROL means redo, and `Char('u')` without modifiers means undo.
     - Any other `Char` with CONTROL is ignored (bug tuido-e97675). These arms come before the plain-letter arms.
3. **Messages:** replace `message: Option<String>` with `Option<Message { text, kind: Info | Error }>`. Existing error sites become `Error`; hints and undo results are `Info`. `ui::render_info` and `render_input` pick the colour by kind.
4. **Labels:** `Deleted "<text>"`, `Cleared N todos`, `Completed`/`Reopened "<text>"`, `Suspended`/`Resumed "<text>"`, `Added "<text>"`, `Edited "<text>"`, `Rescheduled "<text>"`, `Moved "<text>"`. Texts are truncated to about 30 characters on a character boundary.
5. **Hints,** for destructive actions only: after delete, clear-all, and completing a todo while completed todos are hidden, set an `Info` message `<label> (u to undo)`.
6. **Help text and README:** `u : Undo last change` and `Ctrl+R : Redo`.
7. **Tests:**
   - Each action records exactly one entry, and undo restores the prior list exactly.
   - Redo reapplies; a new change clears redo.
   - No-op paths record nothing.
   - Move coalescing, and a run broken by another change.
   - Delete then undo restores position, id and selection.
   - `notified_at` is carried over, including for a deleted todo.
   - Empty undo and redo stacks.
   - The 100-entry limit.
   - Undo with completed todos hidden.
   - Ctrl+letter is ignored.
   - The message kind decides the colour.

## Decisions

- **Snapshots of the whole list, not inverse operations.** One mechanism covers every current and future action and can't get an inverse wrong. The cost is one `Vec<Todo>` clone per user action: microseconds and a few KB for a personal list.
- **Explicit `checkpoint(label, coalesce)` calls rather than diffing before and after every key.** Labels are needed for the hints, and they're only known inside the action. Placing the call after the guards and no-op checks gives no-op safety, and tests enforce "exactly one entry per action, none per no-op".
- **Redo is included and bound to `Ctrl+R` only.** It's about 20 lines with snapshots, and it makes `u` safe to press. `U` was rejected because it collides with vim's line-undo habit and with future shifted bindings.
- **100 entries, session only.** Persisting history would need a new file format and would raise multi-instance staleness questions, for no roadmap need. Editors reset undo on restart too.
- **Keep the `X` confirmation.** Undo doesn't survive `q`, and clear-all has the largest blast radius.
- **Merge reorder moves.** Repeated `J`/`K` is the only way to reorder, so ten undo steps for one gesture would be hostile. It costs one field and one branch.
- **Hints only for destructive actions.** Visible changes don't need one, and constant hints train users to ignore the Info panel.
- **`notified_at` comes from the current state.** It's system state; restoring it from the snapshot would cause duplicate notifications (tuido-839e8f).
- **Undo and redo work in normal mode only.** In input modes, `u` is a typed character and `Ctrl+U` keeps its existing meaning (delete to the start of the line).

## Edge Cases

- **MEDIUM:** undo after the file was changed by another tuido instance overwrites that instance's changes. That's already the "last writer wins" behaviour documented in the README.
- **MEDIUM:** undoing an add removes the new todo, and its id becomes free again. Ids stay unique, because history is linear and `next_id` uses max + 1.
- **LOW:** undo while completed todos are hidden can restore a hidden todo. The selection falls back to the nearest visible one, and the message still says what was undone.

## Resolved Questions (decided by the decision sub-agent, adopted)

1. **Redo:** yes, in this issue, on `Ctrl+R` only.
2. **Depth and persistence:** 100 entries, session only.
3. **`X` confirmation:** keep it.
4. **Reorder coalescing:** yes, keyed by todo id.
5. **Hint scope:** destructive actions only.
