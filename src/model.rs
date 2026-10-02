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
        }
    }

    /// Due (at or before `now`) and still actionable.
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.due_date.is_some_and(|due| due <= now) && !self.completed && !self.suspended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
