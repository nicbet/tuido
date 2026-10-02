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

/// "Today 3:00pm", "Tomorrow", "Mar 14", "Jan 02 2027", or "Overdue" for past days.
pub fn format_due_time(due: DateTime<Utc>, now: DateTime<Local>) -> String {
    let today = now.date_naive();
    let due_local = due.with_timezone(&Local);
    let due_date = due_local.date_naive();
    let (hour, min) = (due_local.hour(), due_local.minute());

    let day = if due_date == today {
        "Today".to_string()
    } else if due_date == today + chrono::Duration::days(1) {
        "Tomorrow".to_string()
    } else if due_local < now {
        return "Overdue".to_string();
    } else if due_date.year() == today.year() {
        due_date.format("%b %d").to_string()
    } else {
        due_date.format("%b %d %Y").to_string()
    };

    if hour == 0 && min == 0 {
        day
    } else {
        format!("{} {}", day, format_clock(hour, min))
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
        assert_eq!(format_due_time(at(3, 10, 15, 0), now), "Today 3:00pm");
        assert_eq!(format_due_time(at(3, 11, 0, 0), now), "Tomorrow");
        assert_eq!(format_due_time(at(3, 1, 9, 0), now), "Overdue");
        assert_eq!(format_due_time(at(4, 2, 0, 0), now), "Apr 02");
    }
}
