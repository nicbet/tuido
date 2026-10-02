//! Natural-language due date parsing ("tomorrow at 5pm", "21:30").

use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

static RE_TOMORROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\btomorrow\b").unwrap());
// 24-hour format: 21:35
static RE_TIME_24: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2}):(\d{2})\b").unwrap());
// 12-hour format: 9pm, 11:30am
static RE_TIME_12: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b").unwrap());
// Scheduling phrases to strip from the todo text once a due date was parsed
static RE_CLEAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\s+(?:at|by))?\s*(?:\b\d{1,2}(?::\d{2})?\s*(?:am|pm)\b|\b\d{1,2}:\d{2}\b|\btomorrow\b)",
    )
    .unwrap()
});

/// Converts a local wall-clock time to UTC, resolving DST ambiguities to the
/// earlier instant and skipping forward over DST gaps.
fn local_to_utc(datetime: NaiveDateTime) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&datetime)
        .earliest()
        .or_else(|| Local.from_local_datetime(&(datetime + chrono::Duration::hours(1))).earliest())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Parses a due date out of free text, relative to `now`. A time that has
/// already passed today rolls over to tomorrow; "tomorrow" alone means 9am.
pub fn parse_due_date(text: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let time = extract_time(text);

    if RE_TOMORROW.is_match(text) {
        let date = now.date_naive() + chrono::Duration::days(1);
        let time = time.unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        return local_to_utc(date.and_time(time));
    }

    let time = time?;
    let datetime = now.date_naive().and_time(time);
    let datetime = if datetime < now.naive_local() {
        datetime + chrono::Duration::days(1)
    } else {
        datetime
    };
    local_to_utc(datetime)
}

/// Finds the first time of day in `text`, in 24-hour or 12-hour format.
fn extract_time(text: &str) -> Option<NaiveTime> {
    if let Some(cap) = RE_TIME_24.captures(text)
        && let Ok(hour) = cap[1].parse::<u32>()
        && let Ok(min) = cap[2].parse::<u32>()
        && hour < 24
        && min < 60
    {
        return NaiveTime::from_hms_opt(hour, min, 0);
    }

    if let Some(cap) = RE_TIME_12.captures(text) {
        let hour: u32 = cap[1].parse().ok()?;
        let min: u32 = cap.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
        if !(1..=12).contains(&hour) || min >= 60 {
            return None;
        }
        let pm = cap[3].eq_ignore_ascii_case("pm");
        let hour_24 = match (hour, pm) {
            (12, false) => 0,
            (12, true) => 12,
            (h, true) => h + 12,
            (h, false) => h,
        };
        return NaiveTime::from_hms_opt(hour_24, min, 0);
    }

    None
}

/// Removes the scheduling phrases that `parse_due_date` understood, so that
/// re-editing a todo later doesn't silently reschedule it.
pub fn clean_text(text: &str) -> String {
    let text = RE_CLEAN.replace_all(text, "");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn extracts_times() {
        let t = |h, m| Some(NaiveTime::from_hms_opt(h, m, 0).unwrap());
        assert_eq!(extract_time("at 21:35"), t(21, 35));
        assert_eq!(extract_time("9pm"), t(21, 0));
        assert_eq!(extract_time("11:30am"), t(11, 30));
        assert_eq!(extract_time("12am"), t(0, 0));
        assert_eq!(extract_time("12pm"), t(12, 0));
        assert_eq!(extract_time("15pm"), None);
        assert_eq!(extract_time("0am"), None);
        assert_eq!(extract_time("no time here"), None);
    }

    #[test]
    fn parses_tomorrow_only_as_a_word() {
        let now = local(2026, 3, 10, 12, 0);
        let due = parse_due_date("call mom tomorrow at 5pm", now).unwrap();
        assert_eq!(due, local(2026, 3, 11, 17, 0).with_timezone(&Utc));
        let due = parse_due_date("call mom tomorrow", now).unwrap();
        assert_eq!(due, local(2026, 3, 11, 9, 0).with_timezone(&Utc));
        assert_eq!(parse_due_date("Tomorrowland tickets", now), None);
    }

    #[test]
    fn past_time_rolls_to_next_day() {
        let now = local(2026, 3, 10, 18, 0);
        assert_eq!(parse_due_date("3pm", now).unwrap(), local(2026, 3, 11, 15, 0).with_timezone(&Utc));
        assert_eq!(parse_due_date("8pm", now).unwrap(), local(2026, 3, 10, 20, 0).with_timezone(&Utc));
    }

    #[test]
    fn cleans_scheduling_phrases() {
        assert_eq!(clean_text("Buy groceries tomorrow at 5pm"), "Buy groceries");
        assert_eq!(clean_text("Call dentist 3pm"), "Call dentist");
        assert_eq!(clean_text("Standup at 9:30"), "Standup");
        assert_eq!(clean_text("Pay rent by 11:30am today"), "Pay rent today");
    }
}
