# Walkthrough: Richer natural-language due dates

## What was built

Due dates can now be written as:
- a time: `3pm`, `at 21:30`
- a day word: `today`, `tonight`, `tomorrow`
- a weekday: `Friday`, `on Friday`, `next Friday`
- a relative phrase: `in 30 minutes`, `in an hour`, `in 3 days`, `in a week`
- a date: `2026-04-15`, `Mar 14`, `14 March`, `Mar 14 2027`

A day can be combined with a time: `Friday at 3pm`. While typing, the input box title previews exactly what Enter will save, for example `Add Todo → Fri 9:00am`.

## The core idea: only a trailing run counts

```rust
pub fn parse_due_date(text: &str, now: DateTime<Local>) -> Option<ParsedDate { due, rest }>
```

The parser looks only at the **end** of the text. It repeatedly asks "does a date phrase end the remaining text?", cuts that phrase off, and asks again, until nothing matches. Whatever is left is `rest`, the todo's title.

This one rule gets rid of a whole class of bugs that richer vocabularies bring. Titles naturally contain date words: "Watch The Tonight Show", "Rome wasn't built in a day later", "Plan next sprint", "Buy sun cream". Anchored to the end, none of them parse. Without that anchoring, a title like "Tonight Show" could never be fixed: every edit would re-parse it and eat the word again.

It also means there's one source of truth. Before this change, a separate cleaning regex (`RE_CLEAN`) had to be kept in sync with the parser by hand. Now the parser itself returns the leftover text, so the two can't drift apart.

Trade-off, documented in the README: a date in the middle of the text is not read. "Call dentist 3pm about the crown" gets no due date. The preview makes this visible as you type.

### Peeling and conflicts

Each phrase family is a regex built by `trailing(phrase)`, which is `(?i)(?:^|\s)PHRASE\s*$`. A connector word that belongs to the phrase (`at`, `by`, `on`, `next`, `in`) is inside the match, so it's removed with the phrase and never on its own. That's why "Plan **next** sprint" keeps "next": it isn't followed by a weekday.

Each family has a `PartParser` that turns its captures into a `Part`:
- **`Time`** comes from 12-hour and 24-hour times.
- **`Day { date, default_time }`** comes from day words, weekdays, `in N days/weeks`, and dates.
- **`Instant`** comes from `in N minutes/hours`: an exact moment.

A run may have **one** Time and **one** Day, or a single Instant on its own. The first phrase that would break this (a second time, say, or a time after "in 2 hours") ends the run, and it stays in the title. So "Meet at 5pm in 2 hours" is due in 2 hours with the title "Meet at 5pm".

Resolution:
- An Instant is used as-is.
- A Day uses its time, or its default: 9am; `tonight` 8pm; `in N days` the current time.
- A bare Time keeps the old rule: today, or tomorrow if that time has already passed. The demo relies on this, because its seeds never go overdue.

Days are turned into UTC by the existing `local_to_utc`, which is DST-safe.

### Choices worth knowing (made by the decision sub-agent)

| Rule | Why |
|---|---|
| A weekday is always 1–7 days ahead, never today; "next Friday" equals "Friday" | The result doesn't depend on the clock, and "next" is famously ambiguous. Users who mean today type `today` |
| Full weekday names only | `sat` and `sun` are ordinary words; partial abbreviation support would be a trap |
| Month names (including abbreviations) only next to a day number | Makes `may`, `march` and `mar` safe |
| A no-year date already past this year rolls to next year; ISO dates and explicit years never roll | Matches the bare-time rollover; nobody schedules for last March |
| Feb 29 without a year means the next leap year; an impossible date like Feb 30 doesn't parse | Correct calendar behaviour |
| No `next week`, `this weekend` or `end of month` | They collide with titles ("Plan next week's offsite"), and nobody asked for them |
| `H:MM` is always a time, so "Read John 3:16" gets 3:16am | One predictable rule. Documented in the README and pinned by a test |
| Trailing punctuation (`Friday.`) isn't stripped | Keeps the grammar small. Documented; the preview shows it |

Overflow safety: relative amounts are limited to 4 digits, so `Duration` arithmetic can't panic, and unknown units return `None` rather than falling through to "weeks".

## App integration (`app.rs`)

- **Add and edit** use `rest` as the title. If there's no title (the input was only a date), the existing "Todo text cannot be empty" error applies.
- **Reschedule** requires the **whole** input to be a date. Otherwise "fri 3pm" would silently drop "fri", because abbreviations aren't supported.
- **Past dates don't notify.** This was a constraint passed on from tuido-839e8f. A newly entered due date `<= now` gets `notified_at = now`. The stamp happens after `checkpoint`, so undo stays correct. For edits it happens only when a date was actually parsed, and only after the "did anything change?" comparison. Otherwise the stamp itself would make a no-op edit look like a change and record a spurious undo step.
- **`input_preview()`** mirrors `submit_input` exactly, so the preview never promises a date that Enter rejects. Code review caught a gap here for date-only input in add mode. A pending error message clears on the next keystroke so the preview can show.

## Display (`format.rs`, `ui.rs`)

`format_due_long` shows:
- "Today 3:00pm"
- "Tomorrow 9:00am"
- "Fri 9:00am" within the coming week
- "Fri Oct 9, 9:00am", with the year only when it differs

Past dates show their real date. The overdue duration is already shown separately in the Info panel. It replaced the old compact `format_due_time`, whose only user was the Info panel. The spec assumed the list used it too, but it doesn't; the list only shows icons.

## Testing

- Table-driven tests against a fixed `now` (Thursday 2026-03-12 14:00) for every phrase family: combinations, defaults, rollover, leap day, invalid dates, false-positive guards, and the demo seed phrases as regression tests.
- App tests: past-date stamping, the no-op edit, reschedule with leftover text, and the preview mirroring submit.
- A UI test for the preview title.
- Mutation-checked: removing the end anchoring, the reschedule full-consumption check, the add stamp, the 1–7 day weekday rule, or the year rollover each fails specific tests.
- The code reviewer probed about 45 extra inputs (unicode before the date, tabs, NBSP, overflow, another timezone) and measured about 1.2µs per parse, so the per-frame preview is free.
- `demo/demo.gif` was re-recorded with `make demo`. It now shows the preview, and the recording ended before any seeded time came due, so no notifications fired.

## Extending the grammar

Add a `trailing(...)` regex and a `PartParser`, register them in `PARSERS`, and add rows to the test tables, including a `no_date` guard if the new words can appear in ordinary titles. `peel` takes the first family that matches the end of the text. Families can't currently match the same tail; if a new one can, put the more specific family first.
