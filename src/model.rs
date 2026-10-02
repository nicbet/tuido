use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub completed: bool,
    pub suspended: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    /// When an overdue notification was last sent for this todo. Compared with
    /// `due_date`, so moving the due date past it re-arms the notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notified_at: Option<DateTime<Utc>>,
}

impl Todo {
    pub fn new(id: u32, text: String) -> Self {
        Self {
            id,
            text,
            completed: false,
            suspended: false,
            due_date: None,
            created_at: Some(Utc::now()),
            notified_at: None,
        }
    }

    /// Due (at or before `now`) and still actionable.
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.due_date.is_some_and(|due| due <= now) && !self.completed && !self.suspended
    }

    /// Overdue and not yet notified for the current due date.
    pub fn needs_notification(&self, now: DateTime<Utc>) -> bool {
        self.is_overdue(now) && self.notified_at.is_none_or(|notified| Some(notified) < self.due_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_notification_once_per_due_date() {
        let now = Utc::now();
        let hours = chrono::Duration::hours;
        let mut todo = Todo::new(0, "x".to_string());
        assert!(!todo.needs_notification(now), "no due date");

        todo.due_date = Some(now + hours(1));
        assert!(!todo.needs_notification(now), "not due yet");

        todo.due_date = Some(now - hours(2));
        assert!(todo.needs_notification(now), "overdue, never notified");

        todo.notified_at = Some(now - hours(1));
        assert!(!todo.needs_notification(now), "notified after it became due");

        todo.due_date = Some(now - chrono::Duration::minutes(30));
        assert!(todo.needs_notification(now), "rescheduled past the last notification");

        todo.due_date = Some(now - hours(3));
        assert!(
            !todo.needs_notification(now),
            "rescheduled to before the last notification"
        );

        todo.due_date = Some(now - chrono::Duration::minutes(30));
        todo.suspended = true;
        assert!(!todo.needs_notification(now), "suspended");
    }

    #[test]
    fn notified_at_is_optional_in_json() {
        let json = r#"{"id":0,"text":"x","completed":false,"suspended":false,"due_date":null,"created_at":null}"#;
        let todo: Todo = serde_json::from_str(json).unwrap();
        assert_eq!(todo.notified_at, None);
        assert!(!serde_json::to_string(&todo).unwrap().contains("notified_at"));
    }

    #[test]
    fn overdue_requires_active_todo() {
        let now = Utc::now();
        let mut todo = Todo::new(0, "x".to_string());
        assert!(!todo.is_overdue(now));
        todo.due_date = Some(now - chrono::Duration::minutes(1));
        assert!(todo.is_overdue(now));
        todo.suspended = true;
        assert!(!todo.is_overdue(now));
        todo.suspended = false;
        todo.completed = true;
        assert!(!todo.is_overdue(now));
    }
}
