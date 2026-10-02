## What

Remember on disk that an overdue todo has already triggered a desktop notification, so restarting tuido doesn't notify again for every stale overdue item. A todo notifies **once per due date**: moving the due date later than the last notification arms a new one. When several todos become due at once, they are batched into one notification.

## Why

`App::notified` is an in-memory `HashSet<u32>` that starts empty on every launch. Anyone with a few long-overdue todos gets a burst of identical notifications each time they open tuido, which trains them to ignore notifications altogether.

Separately, `check_notifications` calls `notify_rust` directly from inside `App`. That mixes OS side effects into the state logic and makes notification behaviour untestable. Persisting state is a good moment to split them.

## Acceptance Criteria

- [ ] `Todo` gains `notified_at: Option<DateTime<Utc>>`. Existing `todos.json` files without the field load unchanged, and the field is omitted from the JSON while it is `None`.
- [ ] An overdue todo produces exactly one notification. Restarting tuido doesn't produce another.
- [ ] Rescheduling a notified todo to a due date **later than its last notification** produces exactly one new notification when that date passes. Rescheduling to an earlier, already-past date doesn't notify again, because the user just typed it.
- [ ] Un-completing or un-suspending a notified todo doesn't notify again. Only a due-date change re-arms.
- [ ] Notification state persists immediately: marking a todo notified saves the file.
- [ ] If exactly one todo becomes due in a check, the notification is as before: summary "Todo Reminder", body "Reminder: <text>".
- [ ] If two or more become due in the same check, one notification is sent: summary "N todos overdue", body listing up to 3 texts plus "…and K more".
- [ ] Deciding what to notify (`App`) and formatting it (`format.rs`) are pure and unit-tested. Sending happens in the event loop (`main.rs`).
- [ ] Due todos are checked on **every event-loop iteration** (each key event or 1-second timeout), so continuous typing can't delay a notification. (Revised after code review; previously only the idle tick checked.)
- [ ] The in-memory `notified` set is removed.
- [ ] A failed notification (for example no notification daemon on Linux) still counts as notified, so there is no retry every tick.

## Flow

1. **`src/model.rs`:** add `#[serde(default, skip_serializing_if = "Option::is_none")] pub notified_at: Option<DateTime<Utc>>`, set to `None` in `Todo::new`, plus
   ```rust
   /// Overdue and not yet notified for the current due date.
   pub fn needs_notification(&self, now: DateTime<Utc>) -> bool
   ```
   It is true when `is_overdue(now)` and `notified_at` is `None` or earlier than `due_date`.
2. **`src/app.rs`:**
   - Remove `notified: HashSet<u32>` and the two `notified.remove(...)` calls.
   - Replace `check_notifications(&mut self)` with
     ```rust
     pub fn take_due_notifications(&mut self, now: DateTime<Utc>) -> Vec<String>
     ```
     It stamps `notified_at = now` at decision time on each todo that needs a notification, saves once if any were stamped, and returns their texts.
   - Move `notify()` out of `app.rs`.
3. **`src/format.rs`:**
   ```rust
   pub fn notification_message(texts: &[String]) -> Option<(String, String)>
   ```
   It returns the (summary, body) for 0, 1 or many texts.
4. **`src/main.rs`:** on every loop iteration (before drawing), `send_due_notifications(app)`: `take_due_notifications(Utc::now())`, then `notification_message`, then one `notify_rust` notification. Errors are ignored.
5. **Tests:**
   - Model: a `needs_notification` truth table, including the earlier-reschedule case.
   - App:
     - First call returns an overdue todo, a second call returns nothing.
     - The result survives a save and reload round trip, and the test cleans up its temp directory.
     - A later reschedule re-arms; an earlier reschedule doesn't.
     - Completing then un-completing doesn't re-arm.
     - Suspended and completed todos never notify.
   - Format: messages for 1, 3 and 5 texts.
   - Storage: an old-format file without the field loads through `load_todos`, and a `None` field isn't written.

## Decisions

- **Compare `notified_at` with `due_date` instead of clearing the stamp on every due-date change.** Any move of the due date past the last notification, including future paths such as undo (tuido-2203d3) and new date phrases (tuido-9cc989), re-arms automatically without call sites having to remember. Stamping with `now` works because `now >= due` whenever we notify.
- **One notification per due date. Toggling completed or suspended doesn't re-arm.** Otherwise "complete, then undo", or a stray `s s`, would re-notify about a todo the user is looking at right now. One rule, no special cases in toggle code.
- **No repeat or "nag" reminders,** and no follow-up issue. The point is notifications that are rare enough to trust.
- **Batch simultaneous notifications.** The first launch after upgrading always produces a burst. That burst is exactly the spam this issue is meant to remove.
- **Stamp at decision time, inside `App`,** and mark as notified even when sending fails. `notify-rust` errors are invisible to the user, and retrying would call D-Bus every second, for every overdue todo, forever, on systems without a daemon. This also keeps the save in one place.
- **Check on every loop iteration, not only on idle timeouts (from code review).** The idle tick only fires after a full second with no input, so fast typing, a held key or a paste could postpone a notification indefinitely. The check is an O(n) scan with no I/O unless something is newly due, so running it per key event is free.
- **Sending lives in `main.rs`; `App` stays free of OS side effects.** It is testable, and consistent with how terminal I/O already lives in `main`.
- **`skip_serializing_if`:** files without notifications stay identical to today's format. Older tuido versions ignore the unknown field when reading. They drop it when saving, which only means one extra notification.

## Edge Cases

- **MEDIUM:** the first launch after upgrading notifies once for already-overdue todos, because they have no stamps. This is now a single batched notification, and never repeats.
- **MEDIUM:** two tuido instances on the same file can each notify once; the last save wins. Acceptable, as already documented in the README.
- **MEDIUM (cross-issue):** undo (tuido-2203d3) must treat `notified_at` as system state and **not** restore it from a snapshot, or undoing an unrelated edit would re-arm a duplicate. A note has been added to that issue.
- **MEDIUM (cross-issue):** richer date parsing (tuido-9cc989) can produce due dates that have already passed. When it does, the next check would notify about a date the user just typed. That issue should stamp `notified_at = now` when an entered due date is `<= now`. A note has been added to it.
- **LOW:** a failed save leaves the stamps in memory, so nothing repeats in this session. A save is only attempted again when another todo becomes due, so there is no save loop on a broken disk.
- **LOW:** clearing a due date (empty reschedule) leaves a stale `notified_at`. That's harmless, because a later date re-arms.
- **LOW:** a hand-edited `notified_at` in the future means no notification until the due date moves past it. Acceptable.

## Resolved Questions (decided by the decision sub-agent, adopted)

1. **Re-arm on un-complete or un-suspend:** no. Only a due-date change re-arms.
2. **Repeat reminders:** not at all, and no follow-up.
3. **Batching:** yes. Two or more due in one check become a single "N todos overdue" notification listing up to 3 texts.
4. **Where to stamp:** inside `App`, at decision time.
