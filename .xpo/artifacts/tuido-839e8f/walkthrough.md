# Walkthrough: Persist notification state so overdue reminders don't repeat on every launch

## The problem

tuido sends a desktop notification when a todo becomes overdue. It used to remember which todos it had notified about in an in-memory `HashSet<u32>` that started empty on every launch. So each time you opened tuido, every stale overdue todo notified again. That is a good way to teach people to ignore notifications.

## The rule now: one notification per due date

```rust
// model.rs
pub notified_at: Option<DateTime<Utc>>,   // #[serde(default, skip_serializing_if = "Option::is_none")]

pub fn needs_notification(&self, now) -> bool {
    self.is_overdue(now) && self.notified_at.is_none_or(|n| Some(n) < self.due_date)
}
```

When we notify, we stamp `notified_at = now`. Because we only ever notify once `now >= due_date`, the stamp is always at or after the due date it covers. So:

- **Restart:** the stamp is on disk, so `notified_at >= due_date` and nothing is sent.
- **Reschedule to a later date:** once the new date passes, `notified_at < due_date`, so exactly one new notification is sent. Nothing at the call site needed to clear anything.
- **Reschedule to an earlier, already-past date:** `due_date < notified_at`, so nothing is sent. The user just typed that date and doesn't need telling.
- **Un-complete or un-suspend:** the due date hasn't changed, so nothing is sent. This is deliberate. With undo coming (tuido-2203d3), "complete, then `u`" must not re-notify about a todo the user is looking at.

Comparing with `due_date` rather than clearing the stamp on every due-date change is the key design choice. Any code path that moves a due date (today's reschedule and edit; undo and richer parsing later) re-arms correctly for free. Nothing has to remember to reset a flag.

## Who does what

```
main.rs  loop {
           send_due_notifications(app)  ──►  app.take_due_notifications(now) -> Vec<String>   (stamps + saves)
                                        ──►  format::notification_message(&texts) -> (summary, body)
                                        ──►  notify_rust::Notification::show()   (errors ignored)
           draw; poll(1s) / handle key
         }
```

- **`App::take_due_notifications`** is pure state logic: no OS calls. It stamps every todo that needs a notification, saves **once** if anything changed, and returns the texts. Previously the notification was sent from inside `App`, which made the behaviour untestable. OS side effects now live in `main.rs`, next to the terminal I/O.
- **Stamping happens when the decision is made, before sending.** If sending fails (Linux with no notification daemon is common), the todo still counts as notified. The alternative, stamping only after a successful send, would retry over D-Bus every second, for every overdue todo, forever.
- **`format::notification_message`:**
  - One todo gives the familiar "Todo Reminder" / "Reminder: <text>".
  - Two or more give a single "N todos overdue" notification listing up to 3 texts plus "…and K more".

  Batching matters most on the **first launch after upgrading**: existing files have no stamps, so every already-overdue todo is due at once. That is now one notification, once.
- **The check runs on every loop iteration,** meaning every key event and every 1-second idle timeout. Originally it ran only on the idle timeout, and code review pointed out that fast typing, a held key or a paste could postpone it indefinitely. The check is an O(n) scan with no allocation or I/O unless something is newly due, so running it per key event costs nothing.

## File format compatibility

`#[serde(default)]` means old files load, and `skip_serializing_if = "Option::is_none"` means a file with no notifications is byte-for-byte the old format. Older tuido binaries reading a new file ignore the unknown field, because `Todo` doesn't use `deny_unknown_fields`. If they save, they drop it, which costs at most one repeat notification.

## Constraints passed on to other issues

- **Undo (tuido-2203d3):** `notified_at` is system state. Undo must keep the current value rather than restoring it from a snapshot. Otherwise undoing an unrelated edit would bring back `None` and cause a duplicate notification. Restoring an older `due_date` is fine, because the comparison handles it.
- **Richer dates (tuido-9cc989):** once a user can type an absolute date that has already passed, the add, edit and reschedule paths should stamp `notified_at = now` for it, so they don't notify about a date that was just entered.

## Testing

- Model truth table: no due date, not yet due, overdue, notified, re-armed, earlier reschedule, suspended.
- App: notifies once; toggling completion doesn't re-arm; later and earlier reschedules; suspended and completed todos never notify.
- Restart round trip through a real file in a temp directory, which the test cleans up. Older tests still leak temp directories; that's tracked in tuido-f6a453.
- Message formatting for 0, 1, 3 and 5 todos.
- Loading and re-saving an old-format file through `load_todos` and `save_todos`.

There was intentionally no end-to-end run with overdue todos, because that would fire real desktop notifications on the developer's machine.
