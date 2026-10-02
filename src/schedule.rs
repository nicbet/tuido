//! Natural-language due date parsing ("Friday at 3pm", "in 2 hours", "Mar 14").
//!
//! Date phrases are only recognized as a trailing run at the end of the text,
//! peeled off one at a time, so ordinary words earlier in a title ("Watch The
//! Tonight Show", "Plan next sprint") are never mistaken for dates.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc, Weekday};
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Builds a case-insensitive regex for `phrase` anchored at the end of the
/// text and preceded by whitespace or the start of the text.
fn trailing(phrase: &str) -> Regex {
    Regex::new(&format!(r"(?i)(?:^|\s){}\s*$", phrase)).unwrap()
}

// 12-hour time: 9pm, at 11:30am
static RE_TIME_12: LazyLock<Regex> = LazyLock::new(|| trailing(r"(?:(?:at|by)\s+)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)"));
// 24-hour time: 21:35, at 9:30
static RE_TIME_24: LazyLock<Regex> = LazyLock::new(|| trailing(r"(?:(?:at|by)\s+)?(\d{1,2}):(\d{2})"));
// in 2 hours, in an hour, in 3 days, in a week
static RE_RELATIVE: LazyLock<Regex> =
    LazyLock::new(|| trailing(r"in\s+(\d{1,4}|an?)\s+(minute|min|hour|hr|day|week)s?"));
// today, tonight, by tomorrow
static RE_DAY_WORD: LazyLock<Regex> = LazyLock::new(|| trailing(r"(?:(?:on|by)\s+)?(today|tonight|tomorrow)"));
// Friday, on Friday, next Friday (full names only: "sat"/"sun" are words)
static RE_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    trailing(r"(?:(?:on|by)\s+)?(?:next\s+)?(monday|tuesday|wednesday|thursday|friday|saturday|sunday)")
});
// 2026-11-01
static RE_ISO_DATE: LazyLock<Regex> = LazyLock::new(|| trailing(r"(?:(?:on|by)\s+)?(\d{4})-(\d{2})-(\d{2})"));
// Mar 14, March 14th, 14 Mar, Mar 14 2027 (a month needs a day number next to it)
const MONTH: &str = r"(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";
static RE_MONTH_DAY: LazyLock<Regex> = LazyLock::new(|| {
    trailing(&format!(
        r"(?:(?:on|by)\s+)?(?:{m}\s+(\d{{1,2}})(?:st|nd|rd|th)?|(\d{{1,2}})(?:st|nd|rd|th)?\s+{m2})(?:,?\s+(\d{{4}}))?",
        m = MONTH,
        m2 = MONTH.replacen('(', "(?P<month2>", 1),
    ))
});

/// A due date found at the end of some text.
#[derive(Debug, PartialEq)]
pub struct ParsedDate {
    pub due: DateTime<Utc>,
    /// The text before the date phrases, i.e. the todo title.
    pub rest: String,
}

/// A calendar day plus the time to use if no time was given.
struct Day {
    date: NaiveDate,
    default_time: NaiveTime,
}

/// One date phrase peeled off the end of the text.
enum Part {
    Time(NaiveTime),
    Day(Day),
    /// An exact moment ("in 2 hours"); can't be combined with a time or day.
    Instant(DateTime<Local>),
}

fn hm(hour: u32, min: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, min, 0).unwrap()
}

/// Converts a local wall-clock time to UTC, resolving DST ambiguities to the
/// earlier instant and skipping forward over DST gaps.
fn local_to_utc(datetime: NaiveDateTime) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&datetime)
        .earliest()
        .or_else(|| {
            Local
                .from_local_datetime(&(datetime + chrono::Duration::hours(1)))
                .earliest()
        })
        .map(|dt| dt.with_timezone(&Utc))
}

/// Parses date phrases from the end of `text`, relative to `now`.
///
/// - A bare time that has already passed today rolls over to tomorrow.
/// - A day without a time means 9:00am (`tonight`: 8:00pm; `in N days`: now's time).
/// - A weekday is always 1–7 days ahead; `next Friday` is the same as `Friday`.
/// - A month and day without a year that is already past rolls to next year.
pub fn parse_due_date(text: &str, now: DateTime<Local>) -> Option<ParsedDate> {
    let mut rest = text.trim_end();
    let (mut time, mut day, mut instant) = (None, None, None);

    while let Some((start, part)) = peel(rest, now) {
        match part {
            Part::Time(t) if time.is_none() && instant.is_none() => time = Some(t),
            Part::Day(d) if day.is_none() && instant.is_none() => day = Some(d),
            Part::Instant(i) if time.is_none() && day.is_none() && instant.is_none() => instant = Some(i),
            // A second time or day, or one that conflicts, belongs to the title.
            _ => break,
        }
        rest = rest[..start].trim_end();
    }

    let due = match (instant, day, time) {
        (Some(instant), _, _) => instant.with_timezone(&Utc),
        (None, Some(day), time) => local_to_utc(day.date.and_time(time.unwrap_or(day.default_time)))?,
        (None, None, Some(time)) => {
            let today = now.date_naive().and_time(time);
            let datetime = if today < now.naive_local() {
                today + chrono::Duration::days(1)
            } else {
                today
            };
            local_to_utc(datetime)?
        }
        (None, None, None) => return None,
    };
    Some(ParsedDate {
        due,
        rest: rest.to_string(),
    })
}

/// Turns a phrase's regex captures into a part, or `None` if it's invalid (e.g. "Feb 30").
type PartParser = fn(&Captures, DateTime<Local>) -> Option<Part>;

/// Every phrase family. To add one: a `trailing(...)` regex, a `PartParser`,
/// and an entry here (plus rows in the test tables).
///
/// `peel` takes the first family whose regex matches the end of the text. The
/// families can't match the same tail, so order doesn't matter today; if a new
/// family could overlap an existing one, put the more specific one first.
static PARSERS: &[(&LazyLock<Regex>, PartParser)] = &[
    (&RE_TIME_12, time_12),
    (&RE_TIME_24, time_24),
    (&RE_RELATIVE, relative),
    (&RE_DAY_WORD, day_word),
    (&RE_WEEKDAY, weekday),
    (&RE_ISO_DATE, iso_date),
    (&RE_MONTH_DAY, month_day),
];

/// Matches one date phrase at the end of `text`, returning where it starts.
fn peel(text: &str, now: DateTime<Local>) -> Option<(usize, Part)> {
    PARSERS.iter().find_map(|(re, parse)| {
        let caps = re.captures(text)?;
        Some((caps.get(0)?.start(), parse(&caps, now)?))
    })
}

fn time_12(caps: &Captures, _: DateTime<Local>) -> Option<Part> {
    let hour: u32 = caps[1].parse().ok()?;
    let min: u32 = caps.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
    if !(1..=12).contains(&hour) || min >= 60 {
        return None;
    }
    let pm = caps[3].eq_ignore_ascii_case("pm");
    let hour = match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    };
    Some(Part::Time(hm(hour, min)))
}

fn time_24(caps: &Captures, _: DateTime<Local>) -> Option<Part> {
    let hour: u32 = caps[1].parse().ok()?;
    let min: u32 = caps[2].parse().ok()?;
    NaiveTime::from_hms_opt(hour, min, 0).map(Part::Time)
}

fn relative(caps: &Captures, now: DateTime<Local>) -> Option<Part> {
    let n: i64 = match caps[1].to_ascii_lowercase().as_str() {
        "a" | "an" => 1,
        digits => digits.parse().ok()?,
    };
    let unit = caps[2].to_ascii_lowercase();
    let days = match unit.as_str() {
        "minute" | "min" => return Some(Part::Instant(now + chrono::Duration::minutes(n))),
        "hour" | "hr" => return Some(Part::Instant(now + chrono::Duration::hours(n))),
        "day" => n,
        "week" => n * 7,
        _ => return None,
    };
    // Calendar days in local time, so "in 1 day" is the same wall-clock time tomorrow.
    let date = now.date_naive().checked_add_signed(chrono::Duration::days(days))?;
    Some(Part::Day(Day {
        date,
        default_time: hm(now.hour(), now.minute()),
    }))
}

fn day_word(caps: &Captures, now: DateTime<Local>) -> Option<Part> {
    let today = now.date_naive();
    let (date, default_time) = match caps[1].to_ascii_lowercase().as_str() {
        "today" => (today, hm(9, 0)),
        "tonight" => (today, hm(20, 0)),
        _ => (today.succ_opt()?, hm(9, 0)), // tomorrow
    };
    Some(Part::Day(Day { date, default_time }))
}

fn weekday(caps: &Captures, now: DateTime<Local>) -> Option<Part> {
    let target: Weekday = caps[1].parse().ok()?;
    let today = now.date_naive();
    let ahead = (target.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
    let ahead = if ahead == 0 { 7 } else { ahead };
    Some(Part::Day(Day {
        date: today + chrono::Duration::days(ahead.into()),
        default_time: hm(9, 0),
    }))
}

fn iso_date(caps: &Captures, _: DateTime<Local>) -> Option<Part> {
    let date = NaiveDate::from_ymd_opt(caps[1].parse().ok()?, caps[2].parse().ok()?, caps[3].parse().ok()?)?;
    Some(Part::Day(Day {
        date,
        default_time: hm(9, 0),
    }))
}

fn month_day(caps: &Captures, now: DateTime<Local>) -> Option<Part> {
    // Either "Mar 14" (groups 1, 2) or "14 Mar" (groups 3, month2).
    let (month, day) = match (caps.get(1), caps.get(2)) {
        (Some(month), Some(day)) => (month.as_str(), day.as_str()),
        _ => (caps.name("month2")?.as_str(), caps.get(3)?.as_str()),
    };
    let month = month_number(month)?;
    let day: u32 = day.parse().ok()?;
    let today = now.date_naive();
    let date = match caps.get(5) {
        Some(year) => NaiveDate::from_ymd_opt(year.as_str().parse().ok()?, month, day)?,
        // No year: the next occurrence, today included (Feb 29 finds a leap year).
        None => (today.year()..=today.year() + 8)
            .filter_map(|year| NaiveDate::from_ymd_opt(year, month, day))
            .find(|&date| date >= today)?,
    };
    Some(Part::Day(Day {
        date,
        default_time: hm(9, 0),
    }))
}

fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let prefix = name.get(..3)?.to_ascii_lowercase();
    MONTHS.iter().position(|&m| m == prefix).map(|i| i as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thursday, 2026-03-12 at 14:00 local time.
    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 3, 12, 14, 0, 0).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap().with_timezone(&Utc)
    }

    #[track_caller]
    fn parses(text: &str, rest: &str, due: DateTime<Utc>) {
        assert_eq!(
            parse_due_date(text, now()),
            Some(ParsedDate {
                due,
                rest: rest.to_string()
            }),
            "parsing {:?}",
            text
        );
    }

    #[track_caller]
    fn no_date(text: &str) {
        assert_eq!(parse_due_date(text, now()), None, "{:?} should not parse", text);
    }

    #[test]
    fn times() {
        parses("Call dentist 3pm", "Call dentist", at(2026, 3, 12, 15, 0));
        parses("Standup at 9:30", "Standup", at(2026, 3, 13, 9, 30)); // passed: tomorrow
        parses("Ship by 11:30am", "Ship", at(2026, 3, 13, 11, 30));
        parses("Late call 21:35", "Late call", at(2026, 3, 12, 21, 35));
        parses("Midnight snack 12am", "Midnight snack", at(2026, 3, 13, 0, 0));
        parses("Lunch 12pm", "Lunch", at(2026, 3, 13, 12, 0));
        no_date("Invalid 15pm");
        no_date("Invalid 0am");
        no_date("Invalid 25:00");
    }

    #[test]
    fn day_words() {
        parses("Buy groceries tomorrow at 5pm", "Buy groceries", at(2026, 3, 13, 17, 0));
        parses("Buy groceries 5pm tomorrow", "Buy groceries", at(2026, 3, 13, 17, 0));
        parses("Call mom tomorrow", "Call mom", at(2026, 3, 13, 9, 0));
        parses("Report by tomorrow", "Report", at(2026, 3, 13, 9, 0));
        parses("Pay rent by 11:30am today", "Pay rent", at(2026, 3, 12, 11, 30));
        parses("Water plants today", "Water plants", at(2026, 3, 12, 9, 0)); // overdue, as typed
        parses("Movie tonight", "Movie", at(2026, 3, 12, 20, 0));
        parses("Movie tonight at 9pm", "Movie", at(2026, 3, 12, 21, 0));
    }

    #[test]
    fn weekdays() {
        parses("Call mom Friday", "Call mom", at(2026, 3, 13, 9, 0));
        parses("Call mom on friday at 3pm", "Call mom", at(2026, 3, 13, 15, 0));
        parses("Call mom next Friday", "Call mom", at(2026, 3, 13, 9, 0));
        parses("Retro Thursday", "Retro", at(2026, 3, 19, 9, 0)); // today is Thursday: next week
        parses("Brunch Sunday 11am", "Brunch", at(2026, 3, 15, 11, 0));
    }

    #[test]
    fn relative() {
        parses("Stretch in 30 minutes", "Stretch", at(2026, 3, 12, 14, 30));
        parses("Check oven in an hour", "Check oven", at(2026, 3, 12, 15, 0));
        parses("Follow up in 2 hours", "Follow up", at(2026, 3, 12, 16, 0));
        parses("Water plants in 3 days", "Water plants", at(2026, 3, 15, 14, 0));
        parses("Review in a week", "Review", at(2026, 3, 19, 14, 0));
        parses("Review in 2 weeks at 10am", "Review", at(2026, 3, 26, 10, 0));
        // An exact moment can't take a time as well; the time stays in the title.
        parses("Meet at 5pm in 2 hours", "Meet at 5pm", at(2026, 3, 12, 16, 0));
    }

    #[test]
    fn absolute_dates() {
        parses("Taxes 2026-04-15", "Taxes", at(2026, 4, 15, 9, 0));
        parses("Taxes on 2026-04-15 at 5pm", "Taxes", at(2026, 4, 15, 17, 0));
        parses("Dentist Mar 14", "Dentist", at(2026, 3, 14, 9, 0));
        parses("Dentist March 14th 9:30", "Dentist", at(2026, 3, 14, 9, 30));
        parses("Dentist 14 Mar", "Dentist", at(2026, 3, 14, 9, 0));
        parses("Party Sept 3", "Party", at(2026, 9, 3, 9, 0));
        parses("Conference Mar 14 2027", "Conference", at(2027, 3, 14, 9, 0));
        parses("Conference Mar 14, 2027 at 9am", "Conference", at(2027, 3, 14, 9, 0));
        parses("Today's thing Mar 12", "Today's thing", at(2026, 3, 12, 9, 0)); // today stays
        parses("Anniversary Mar 1", "Anniversary", at(2027, 3, 1, 9, 0)); // past: next year
        parses("Old ISO 2025-01-01", "Old ISO", at(2025, 1, 1, 9, 0)); // explicit: never rolls
        parses("Leap day Feb 29", "Leap day", at(2028, 2, 29, 9, 0));
        no_date("Impossible Feb 30");
        no_date("Impossible 2026-13-01");
    }

    #[test]
    fn only_a_trailing_run_is_parsed() {
        no_date("Watch The Tonight Show");
        no_date("Call dentist 3pm about the crown");
        no_date("Rome wasn't built in a day later");
        no_date("march 10 miles");
        no_date("Buy sun cream");
        no_date("May the force be with you");
        no_date("Reply to Sam's email");
        no_date("Room 101");
        // Punctuation after the date stops it being read as one (documented).
        no_date("Call mom Friday.");
        // `H:MM` is always a time, even in a verse reference (documented choice).
        parses("Read John 3:16", "Read John", at(2026, 3, 13, 3, 16));
        // Only the trailing date is taken; the rest stays in the title.
        parses(
            "Plan next sprint tomorrow at 10am",
            "Plan next sprint",
            at(2026, 3, 13, 10, 0),
        );
        parses(
            "Rome wasn't built in a day",
            "Rome wasn't built",
            at(2026, 3, 13, 14, 0),
        );
        parses(
            "Review open pull requests 4pm",
            "Review open pull requests",
            at(2026, 3, 12, 16, 0),
        );
        parses(
            "Write release notes tomorrow at 5pm",
            "Write release notes",
            at(2026, 3, 13, 17, 0),
        );
    }

    #[test]
    fn only_one_time_and_one_day() {
        parses("Thing Friday tomorrow", "Thing Friday", at(2026, 3, 13, 9, 0));
        parses("Thing 3pm 5pm", "Thing 3pm", at(2026, 3, 12, 17, 0));
    }

    #[test]
    fn date_only_input_leaves_empty_rest() {
        parses("tomorrow at 9am", "", at(2026, 3, 13, 9, 0));
        parses("  Friday  ", "", at(2026, 3, 13, 9, 0));
    }

    #[test]
    fn huge_relative_amounts_do_not_panic() {
        assert!(parse_due_date("x in 9999 weeks", now()).is_some());
        no_date("x in 99999 days"); // more than 4 digits isn't a relative phrase
    }
}
