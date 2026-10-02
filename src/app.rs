use crate::model::Todo;
use crate::schedule::{clean_text, parse_due_date};
use crate::storage::save_todos;
use crate::text_input::TextInput;
use chrono::{Local, Utc};
use notify_rust::Notification;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Adding,
    Editing,
    Rescheduling,
    ConfirmClear,
    Help,
}

impl InputMode {
    pub fn is_text_entry(&self) -> bool {
        matches!(self, InputMode::Adding | InputMode::Editing | InputMode::Rescheduling)
    }
}

pub struct App {
    pub todos: Vec<Todo>,
    /// Where todos are persisted; `None` disables saving (used in tests).
    storage_path: Option<PathBuf>,
    /// Index into `todos` (not into the visible list).
    pub selected: usize,
    pub input_mode: InputMode,
    pub input: TextInput,
    editing_index: Option<usize>,
    pub hide_completed: bool,
    /// Ids of todos we've already sent an overdue notification for.
    notified: HashSet<u32>,
    /// Transient message (errors, hints) shown in the Info panel.
    pub message: Option<String>,
}

impl App {
    pub fn new(todos: Vec<Todo>, storage_path: Option<PathBuf>) -> Self {
        Self {
            todos,
            storage_path,
            selected: 0,
            input_mode: InputMode::Normal,
            input: TextInput::default(),
            editing_index: None,
            hide_completed: false,
            notified: HashSet::new(),
            message: None,
        }
    }

    fn save(&mut self) {
        if let Some(path) = &self.storage_path
            && let Err(e) = save_todos(path, &self.todos)
        {
            self.message = Some(format!("Failed to save todos: {}", e));
        }
    }

    fn next_id(&self) -> u32 {
        self.todos.iter().map(|t| t.id + 1).max().unwrap_or(0)
    }

    fn is_visible(&self, index: usize) -> bool {
        self.todos
            .get(index)
            .is_some_and(|t| !self.hide_completed || !t.completed)
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        (0..self.todos.len()).filter(|&i| self.is_visible(i)).collect()
    }

    /// The selected todo's index, if it exists and is currently visible.
    pub fn selected_index(&self) -> Option<usize> {
        self.is_visible(self.selected).then_some(self.selected)
    }

    pub fn selected_todo(&self) -> Option<&Todo> {
        self.selected_index().map(|i| &self.todos[i])
    }

    /// Moves the selection to the nearest visible todo (preferring later ones)
    /// if the current one is hidden or out of bounds.
    fn fix_selection(&mut self) {
        if self.is_visible(self.selected) {
            return;
        }
        let visible = self.visible_indices();
        self.selected = visible
            .iter()
            .copied()
            .find(|&i| i > self.selected)
            .or_else(|| visible.last().copied())
            .unwrap_or(0);
    }

    fn next_visible_after(&self, index: usize) -> Option<usize> {
        (index + 1..self.todos.len()).find(|&i| self.is_visible(i))
    }

    fn prev_visible_before(&self, index: usize) -> Option<usize> {
        (0..index).rev().find(|&i| self.is_visible(i))
    }

    fn select_next(&mut self) {
        if let Some(next) = self.next_visible_after(self.selected) {
            self.selected = next;
        }
    }

    fn select_previous(&mut self) {
        if let Some(prev) = self.prev_visible_before(self.selected) {
            self.selected = prev;
        }
    }

    /// Swaps the selected todo with the previous visible one.
    fn move_selected_up(&mut self) {
        let Some(current) = self.selected_index() else { return };
        if let Some(prev) = self.prev_visible_before(current) {
            self.todos.swap(current, prev);
            self.selected = prev;
            self.save();
        }
    }

    /// Swaps the selected todo with the next visible one.
    fn move_selected_down(&mut self) {
        let Some(current) = self.selected_index() else { return };
        if let Some(next) = self.next_visible_after(current) {
            self.todos.swap(current, next);
            self.selected = next;
            self.save();
        }
    }

    fn toggle_completed(&mut self) {
        let Some(index) = self.selected_index() else { return };
        let todo = &mut self.todos[index];
        todo.completed = !todo.completed;
        self.save();
        self.fix_selection();
    }

    fn toggle_suspended(&mut self) {
        let Some(index) = self.selected_index() else { return };
        let todo = &mut self.todos[index];
        todo.suspended = !todo.suspended;
        self.save();
    }

    fn delete_selected(&mut self) {
        let Some(index) = self.selected_index() else { return };
        self.todos.remove(index);
        self.save();
        self.fix_selection();
    }

    fn clear_all(&mut self) {
        self.todos.clear();
        self.selected = 0;
        self.save();
    }

    fn toggle_hide_completed(&mut self) {
        self.hide_completed = !self.hide_completed;
        self.fix_selection();
    }

    fn start_input(&mut self, mode: InputMode, initial: String, editing_index: Option<usize>) {
        self.input_mode = mode;
        self.input.set(initial);
        self.editing_index = editing_index;
        self.message = None;
    }

    fn submit_input(&mut self) {
        let input = self.input.value().trim().to_string();
        let now = Local::now();

        if self.input_mode == InputMode::Rescheduling {
            if let Some(index) = self.editing_index
                && let Some(todo) = self.todos.get_mut(index)
            {
                if input.is_empty() {
                    todo.due_date = None;
                } else if let Some(due) = parse_due_date(&input, now) {
                    todo.due_date = Some(due);
                    self.notified.remove(&todo.id);
                } else {
                    self.message = Some(format!("Could not understand \"{}\" — schedule unchanged", input));
                    return;
                }
            }
        } else if !input.is_empty() {
            let new_due_date = parse_due_date(&input, now);
            let text = if new_due_date.is_some() {
                clean_text(&input)
            } else {
                input
            };
            if text.is_empty() {
                self.message = Some("Todo text cannot be empty".to_string());
                return;
            }
            if let Some(index) = self.editing_index {
                if let Some(todo) = self.todos.get_mut(index) {
                    todo.text = text;
                    if new_due_date.is_some() {
                        todo.due_date = new_due_date;
                        self.notified.remove(&todo.id);
                    }
                }
            } else {
                let mut todo = Todo::new(self.next_id(), text);
                todo.due_date = new_due_date;
                self.todos.push(todo);
                self.selected = self.todos.len() - 1;
            }
        }

        self.input_mode = InputMode::Normal;
        self.editing_index = None;
        self.save();
    }

    fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.editing_index = None;
        self.message = None;
    }

    /// Sends a notification for every overdue todo that hasn't had one yet.
    pub fn check_notifications(&mut self) {
        let now = Utc::now();
        for todo in &self.todos {
            if todo.is_overdue(now) && self.notified.insert(todo.id) {
                notify(todo);
            }
        }
    }

    /// Handles a key press. Returns `false` when the app should quit.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.input_mode {
            InputMode::Normal => return self.handle_normal_key(code, modifiers),
            InputMode::Adding | InputMode::Editing | InputMode::Rescheduling => self.handle_text_key(code, modifiers),
            InputMode::ConfirmClear => {
                if code == KeyCode::Char('y') {
                    self.clear_all();
                }
                self.input_mode = InputMode::Normal;
            }
            InputMode::Help => self.input_mode = InputMode::Normal,
        }
        true
    }

    fn handle_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.message = None;
        match code {
            KeyCode::Char('q') => return false,
            KeyCode::Down if modifiers.contains(KeyModifiers::SHIFT) => self.move_selected_down(),
            KeyCode::Up if modifiers.contains(KeyModifiers::SHIFT) => self.move_selected_up(),
            KeyCode::Char('J') => self.move_selected_down(),
            KeyCode::Char('K') => self.move_selected_up(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Char(' ') => self.toggle_completed(),
            KeyCode::Char('a') => self.start_input(InputMode::Adding, String::new(), None),
            KeyCode::Char('e') => {
                if let Some(index) = self.selected_index() {
                    let text = self.todos[index].text.clone();
                    self.start_input(InputMode::Editing, text, Some(index));
                }
            }
            KeyCode::Char('r') => {
                if let Some(index) = self.selected_index() {
                    self.start_input(InputMode::Rescheduling, String::new(), Some(index));
                }
            }
            KeyCode::Char('d') => self.delete_selected(),
            KeyCode::Char('s') => self.toggle_suspended(),
            KeyCode::Char('X') => {
                if !self.todos.is_empty() {
                    self.input_mode = InputMode::ConfirmClear;
                }
            }
            KeyCode::Char('?') => self.input_mode = InputMode::Help,
            KeyCode::Char('h') => self.toggle_hide_completed(),
            _ => {}
        }
        true
    }

    fn handle_text_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Enter => self.submit_input(),
            KeyCode::Esc => self.cancel_input(),
            KeyCode::Char('w') if ctrl => self.input.delete_word_before_cursor(),
            KeyCode::Char('u') if ctrl => self.input.delete_to_start(),
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char(c) => self.input.insert(c),
            KeyCode::Backspace => self.input.delete_before_cursor(),
            KeyCode::Delete => self.input.delete_at_cursor(),
            KeyCode::Left => self.input.move_left(),
            KeyCode::Right => self.input.move_right(),
            KeyCode::Home => self.input.move_home(),
            KeyCode::End => self.input.move_end(),
            _ => {}
        }
    }
}

/// Sends a desktop notification. Failures (e.g. no notification daemon) are
/// deliberately ignored: a missing notification must never crash the TUI.
fn notify(todo: &Todo) {
    let _ = Notification::new()
        .summary("Todo Reminder")
        .body(&format!("Reminder: {}", todo.text))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(texts: &[&str]) -> App {
        let todos = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Todo::new(i as u32, t.to_string()))
            .collect();
        App::new(todos, None)
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(code, KeyModifiers::NONE);
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    #[test]
    fn ctrl_shortcuts_do_not_insert_text() {
        let mut app = app_with(&[]);
        app.start_input(InputMode::Adding, "foo bar".to_string(), None);
        app.handle_key(KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(app.input.value(), "foo ");
        app.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.input.value(), "");
    }

    #[test]
    fn adds_todo_with_schedule_and_selects_it() {
        let mut app = app_with(&["existing"]);
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "Call mom tomorrow at 5pm");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.todos.len(), 2);
        assert_eq!(app.todos[1].text, "Call mom");
        assert!(app.todos[1].due_date.is_some());
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn unparseable_reschedule_keeps_due_date() {
        let mut app = app_with(&["a"]);
        let due = Utc::now();
        app.todos[0].due_date = Some(due);
        press(&mut app, KeyCode::Char('r'));
        type_text(&mut app, "gibberish");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.todos[0].due_date, Some(due));
        assert!(app.message.is_some());
        assert!(app.input_mode == InputMode::Rescheduling);
    }

    #[test]
    fn hidden_completed_items_are_skipped() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos[1].completed = true;
        app.hide_completed = true;
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_previous();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn actions_ignore_hidden_selection() {
        let mut app = app_with(&["a"]);
        app.todos[0].completed = true;
        app.toggle_hide_completed();
        assert_eq!(app.selected_index(), None);
        press(&mut app, KeyCode::Char('e'));
        assert!(app.input_mode == InputMode::Normal);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.todos.len(), 1);
    }

    #[test]
    fn reorder_skips_hidden_items() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos[1].completed = true;
        app.hide_completed = true;
        app.move_selected_down();
        assert_eq!(app.selected, 2);
        assert_eq!(app.todos[2].text, "a");
        assert_eq!(app.todos[1].text, "b");
    }

    #[test]
    fn clear_all_requires_confirmation() {
        let mut app = app_with(&["a", "b"]);
        press(&mut app, KeyCode::Char('X'));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.todos.len(), 2);
        press(&mut app, KeyCode::Char('X'));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.todos.is_empty());
    }

    #[test]
    fn next_id_is_unique_after_deletion() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos.remove(0);
        assert_eq!(app.next_id(), 3);
    }
}
