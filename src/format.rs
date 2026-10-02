//! Human-readable rendering of times and durations.

use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use std::path::Path;

fn format_clock(hour: u32, min: u32) -> String {
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    let am_pm = if hour >= 12 { "pm" } else { "am" };
    format!("{}:{:02}{}", hour12, min, am_pm)
}

/// The full due date and time: "Today 3:00pm", "Tomorrow 9:00am", "Fri 9:00am"
/// within the coming week, otherwise "Fri Oct 9, 9:00am" (with the year if it
/// differs). Past dates are shown as dates too, never as "Overdue".
pub fn format_due_long(due: DateTime<Utc>, now: DateTime<Local>) -> String {
    let due = due.with_timezone(&Local);
    let (today, date) = (now.date_naive(), due.date_naive());
    let clock = format_clock(due.hour(), due.minute());
    match (date - today).num_days() {
        0 => format!("Today {}", clock),
        1 => format!("Tomorrow {}", clock),
        2..=6 => format!("{} {}", date.format("%a"), clock),
        _ if date.year() == today.year() => format!("{}, {}", date.format("%a %b %-d"), clock),
        _ => format!("{}, {}", date.format("%a %b %-d %Y"), clock),
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {}", unit)
    } else {
        format!("{} {}s", n, unit)
    }
}

/// Renders a duration coarsely: "3 weeks", "2 days", "5 hours", "12 minutes".
/// Returns `None` for durations under a minute.
fn humanize_duration(duration: chrono::Duration) -> Option<String> {
    let days = duration.num_days();
    if days >= 7 {
        Some(plural(days / 7, "week"))
    } else if days >= 1 {
        Some(plural(days, "day"))
    } else if duration.num_hours() >= 1 {
        Some(plural(duration.num_hours(), "hour"))
    } else if duration.num_minutes() >= 1 {
        Some(plural(duration.num_minutes(), "minute"))
    } else {
        None
    }
}

pub fn format_overdue_duration(due: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match humanize_duration(now - due) {
        Some(d) => format!("{} overdue", d),
        None => "less than a minute overdue".to_string(),
    }
}

pub fn format_age(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match humanize_duration(now - created) {
        Some(d) => format!("added {} ago", d),
        None => "added just now".to_string(),
    }
}

/// How many todo texts a batched notification lists before "…and K more".
const NOTIFICATION_LIST_LIMIT: usize = 3;

/// Summary and body of the desktop notification for todos that just became
/// overdue: the todo itself if there is one, otherwise a batched summary.
pub fn notification_message(texts: &[String]) -> Option<(String, String)> {
    match texts {
        [] => None,
        [text] => Some(("Todo Reminder".to_string(), format!("Reminder: {}", text))),
        _ => {
            let mut lines: Vec<String> = texts
                .iter()
                .take(NOTIFICATION_LIST_LIMIT)
                .map(|text| format!("• {}", text))
                .collect();
            if texts.len() > NOTIFICATION_LIST_LIMIT {
                lines.push(format!("…and {} more", texts.len() - NOTIFICATION_LIST_LIMIT));
            }
            Some((format!("{} todos overdue", texts.len()), lines.join("\n")))
        }
    }
}

/// Shows a path with the home directory abbreviated to `~`.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn humanizes_durations() {
        assert_eq!(humanize_duration(chrono::Duration::seconds(30)), None);
        assert_eq!(humanize_duration(chrono::Duration::minutes(1)).unwrap(), "1 minute");
        assert_eq!(humanize_duration(chrono::Duration::hours(25)).unwrap(), "1 day");
        assert_eq!(humanize_duration(chrono::Duration::days(13)).unwrap(), "1 week");
        assert_eq!(humanize_duration(chrono::Duration::days(14)).unwrap(), "2 weeks");
    }

    #[test]
    fn abbreviates_home_in_paths() {
        let home = std::env::temp_dir().join("home");
        let inside = home.join(".config").join("tuido").join("todos.json");
        let expected = Path::new("~").join(".config").join("tuido").join("todos.json");
        assert_eq!(display_path(&inside, Some(&home)), expected.display().to_string());
        let outside = std::env::temp_dir().join("other.json");
        assert_eq!(display_path(&outside, Some(&home)), outside.display().to_string());
        assert_eq!(display_path(&outside, None), outside.display().to_string());
    }

    #[test]
    fn formats_notification_messages() {
        let texts = |n: usize| (1..=n).map(|i| format!("t{}", i)).collect::<Vec<_>>();
        assert_eq!(notification_message(&[]), None);
        assert_eq!(
            notification_message(&texts(1)).unwrap(),
            ("Todo Reminder".to_string(), "Reminder: t1".to_string())
        );
        assert_eq!(
            notification_message(&texts(3)).unwrap(),
            ("3 todos overdue".to_string(), "• t1\n• t2\n• t3".to_string())
        );
        assert_eq!(
            notification_message(&texts(5)).unwrap(),
            (
                "5 todos overdue".to_string(),
                "• t1\n• t2\n• t3\n…and 2 more".to_string()
            )
        );
    }

    #[test]
    fn formats_clock() {
        assert_eq!(format_clock(0, 5), "12:05am");
        assert_eq!(format_clock(12, 0), "12:00pm");
        assert_eq!(format_clock(21, 30), "9:30pm");
    }

    #[test]
    fn formats_due_times() {
        let now = Local.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
        let at = |m, d, h, min| {
            Local
                .with_ymd_and_hms(2026, m, d, h, min, 0)
                .unwrap()
                .with_timezone(&Utc)
        };
        // 2026-03-10 is a Tuesday.
        assert_eq!(format_due_long(at(3, 10, 15, 0), now), "Today 3:00pm");
        assert_eq!(format_due_long(at(3, 10, 9, 0), now), "Today 9:00am");
        assert_eq!(format_due_long(at(3, 11, 0, 0), now), "Tomorrow 12:00am");
        assert_eq!(format_due_long(at(3, 13, 9, 0), now), "Fri 9:00am");
        assert_eq!(format_due_long(at(3, 17, 9, 0), now), "Tue Mar 17, 9:00am");
        assert_eq!(format_due_long(at(3, 1, 9, 0), now), "Sun Mar 1, 9:00am");
        let next_year = Local
            .with_ymd_and_hms(2027, 1, 4, 18, 30, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(format_due_long(next_year, now), "Mon Jan 4 2027, 6:30pm");
    }
}
