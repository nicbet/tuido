## What

Teach due-date parsing more phrases, alongside today's times ("9pm", "21:30") and "tomorrow":

- **Days:** `today`, `tonight`, full weekday names (`Friday`, `on Friday`, `next Friday`)
- **Relative:** `in N minutes|hours|days|weeks`, and `in an hour`, `in a day`, `in a week`
- **Absolute dates:** ISO `2026-11-01`, and month plus day (`Mar 14`, `March 14`, `14 Mar`), with an optional year

Each day phrase can combine with a time ("Friday at 3pm", "Mar 14 9:30").

**Date phrases are recognised only as a trailing run at the end of the input,** so ordinary words earlier in a title are never taken. The same parser serves add, edit and reschedule. While typing, the input box title previews exactly what will be saved.

## Why

Common phrases are silently ignored today. "Call mom Friday" becomes a todo with no due date, and the word stays in the text. The README also once claimed "by Friday" worked.

## Acceptance Criteria

- [ ] **Trailing-run parsing:** phrases are peeled from the **end** of the input, one at a time, until nothing more matches. Each run has at most one time part and one day part, and a relative minutes/hours phrase is a complete instant that can't combine with a time. Whatever remains is the todo text.
  - "Watch The Tonight Show" gets no date.
  - "Call dentist 3pm about X" gets no date. This is a behaviour change, documented in the README.
- [ ] **Reschedule mode:** the whole input must be consumed. Leftover text such as "fri 3pm" gives the existing "Could not understand" error.
- [ ] **Weekday:** always the next occurrence, 1 to 7 days ahead, never today. "next Friday" means the same as "Friday". Full names only; no abbreviations such as `mon` or `sat`.
- [ ] **Month names:** full names or abbreviations (including `sept`), recognised only with a day number next to them (`Mar 14`, `14 March`, `March 14 2027`).
- [ ] **No-year date already past this year:** rolls forward to next year, but today's date stays this year. An explicit year or an ISO date never rolls.
- [ ] **Defaults without a time:** a day defaults to 9:00am and `tonight` to 8:00pm. So `today` typed in the afternoon, or `tonight` after 8pm, is immediately overdue, which the preview shows (see the next criterion).
- [ ] **Past dates don't notify (from tuido-839e8f):** when a **newly parsed** due date is `<= now`, `notified_at` is set to `now`.
  - In the edit path this happens only when a new date was parsed, and only after the no-op comparison, so a no-op edit records no undo step.
  - It happens after `checkpoint` in all paths.
- [ ] **Existing behaviour is unchanged:** a bare time that has passed rolls over to tomorrow; "tomorrow" alone means 9am; 12-hour and 24-hour times. The demo seeds are regression tests:
  - "Review open pull requests 4pm"
  - "Plan next sprint tomorrow at 10am", where "next sprint" stays in the text
  - "Reply to Sam's email", which gets no due date
- [ ] **One source of truth:** the parser returns the due date and the text left after the trailing run is removed. `clean_text` and `RE_CLEAN` are deleted. Connector words are removed only as part of a phrase: `at` and `by` before a time; `on`, `by` and `next` before a day; `in` as part of a relative phrase.
- [ ] **Preview:** in add, edit and reschedule modes, the input box title shows `→ <long date>` when the current input parses.
  - A pending error message is shown instead until the next keystroke, which clears it.
  - The long format is `format_due_long`: "Today 3:00pm", "Tomorrow 9:00am", "Fri 9:00am" within 7 days, "Fri Oct 9, 9:00am" beyond that, and the year only when it differs. A past result shows the real date, never "Overdue".
  - The Info panel's "Scheduled for:" also uses `format_due_long`. The list keeps the compact `format_due_time`.
- [ ] Parsing is DST-safe, reusing `local_to_utc`.
- [ ] Table-driven tests cover every phrase family, combinations, non-matches and false-positive guards: "Watch The Tonight Show", "Rome wasn't built in a day", "march 10 miles", "Buy sun cream", "May the force", and the earlier-phrase case above.
- [ ] The README's "Adding Tasks with Due Dates" section has a table of supported phrases and the trailing-run rule.
- [ ] The README demo GIF is re-recorded with `make demo`, because the visible add step now shows the live preview.

## Flow

1. **`src/schedule.rs`, rewritten around one entry point:**
   ```rust
   pub struct ParsedDate { pub due: DateTime<Utc>, pub rest: String }
   pub fn parse_due_date(text: &str, now: DateTime<Local>) -> Option<ParsedDate>
   ```
   - The parts are `enum Part { Time(NaiveTime), Day(NaiveDate), Instant(DateTime<Local>) }`.
   - Each phrase family is one `LazyLock<Regex>` anchored with `(?:^|\s)…\s*$`, with its optional connector inside the match. Peeling loop: trim the end, try each family on the tail, accept a match if it doesn't conflict with the parts found so far, cut it off, and repeat.
   - Resolve the parts:
     - An Instant stands alone.
     - Otherwise use the Day (default today, with a bare time rolling over if past) and the Time (default 9am, or 8pm for tonight).
2. **`src/app.rs` `submit_input`:**
   - Add and edit use `parsed.rest` as the text.
   - Reschedule requires `rest` to be empty.
   - Stamp `notified_at` for past dates as described in the AC.
   - Clear a pending message on any edit keystroke in text modes.
3. **`src/format.rs`:** add `format_due_long(due, now)`, with tests.
4. **`src/ui.rs`:** add the preview to the `render_input` title; use `format_due_long` in `render_info`.
5. **README:** phrase table and the trailing-run rule. Re-record `demo/demo.gif` with `make demo`.

## Decisions

- **Trailing-run anchoring, not an escape syntax.** Titles naturally contain date words ("Tonight Show", "in a day", "next sprint"). Taking dates only from the end removes that whole class of false positive, removes the "several phrases" ambiguity, and keeps the grammar small. Re-editing a todo with such a title is now safe too.
- **Weekdays are 1 to 7 days ahead; "next X" equals "X".** The result never depends on the clock, and "next" meaning "the week after" is ambiguous. Users who mean today type "today".
- **Full weekday names only, but month abbreviations with a day number.** Partial weekday abbreviation support would be a trap (`sat` and `sun` are words). Requiring a day number makes month names safe.
- **Roll past no-year dates to next year,** which matches the bare-time rollover. Nobody adds a todo for last March.
- **No `next week`, `this weekend` or `end of month`,** and no follow-up issue. They collide with ordinary titles and nobody has asked for them.
- **The parser returns the remaining text,** so there is no separate cleaning regex to keep in sync, which was the issue's own stated risk.
- **Hand-written regexes, not a date-parsing crate.** The grammar is small, crates bring false-positive behaviour that's harder to control, and the dependency count stays low.
- **Relative "in N days/weeks" uses local calendar days (same wall-clock time); minutes and hours are exact durations.** "In 1 day" across a DST change still means "same time tomorrow".
- **Precedence on overlaps:** a 4-digit number counts as a year only immediately after a month-day. `HH:MM` is always a time. An ISO `YYYY-MM-DD` is always a date. Because matching is anchored to the end, each match is unambiguous on its own.

## Edge Cases

- **MEDIUM:** "march 10 miles" isn't a false positive, because it isn't trailing. "Run march 10" would parse as March 10. Accepted; the preview shows it.
- **MEDIUM:** `today` without a time typed after 9am, or `tonight` after 8pm, is immediately overdue. It is stamped, so it doesn't notify, and the preview makes it visible.
- **MEDIUM:** undo is unaffected. Stamping happens after `checkpoint`, and undo carries the current `notified_at` over.
- **LOW:** Feb 29 in a non-leap year: the next occurrence rolls to the next leap year, or the phrase is rejected if that isn't possible. An invalid date such as "Feb 30" doesn't parse.

## Resolved Questions (decided by the decision sub-agent, adopted)

1. **Same weekday:** always 1 to 7 days ahead; "next X" equals "X".
2. **Abbreviations:** no weekday abbreviations; month abbreviations only with a day number.
3. **Escape hatch:** none. Trailing-run anchoring replaces it.
4. **Past no-year dates:** roll to next year; explicit years and ISO dates never roll.
5. **Preview format:** new `format_due_long` for the preview and the Info panel; the list stays compact.
6. **Extra phrases:** out of scope, and no follow-up.
