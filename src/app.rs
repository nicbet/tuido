use crate::model::Todo;
use crate::schedule::parse_due_date;
use crate::storage::save_todos;
use crate::text_input::TextInput;
use chrono::{DateTime, Local, Utc};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// How many undo steps are kept (oldest are dropped). History is per session.
const UNDO_LIMIT: usize = 100;
/// Todo texts longer than this are shortened in undo labels and hints.
const LABEL_TEXT_LIMIT: usize = 30;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageKind {
    Info,
    Error,
}

/// Transient feedback shown in the Info panel (or the input box title).
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub kind: MessageKind,
}

#[derive(Clone, Copy)]
enum HistoryStep {
    Undo,
    Redo,
}

/// Which kinds of todos are hidden from the list. Every visibility decision
/// goes through `shows`, so a new filter is one field plus one rule there.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ViewFilter {
    pub hide_completed: bool,
    pub hide_suspended: bool,
}

impl ViewFilter {
    /// Whether `todo` is listed. A todo that is both completed and suspended
    /// counts as completed, matching how it is displayed.
    pub fn shows(&self, todo: &Todo) -> bool {
        if todo.completed {
            !self.hide_completed
        } else if todo.suspended {
            !self.hide_suspended
        } else {
            true
        }
    }

    pub fn hidden_count(&self, todos: &[Todo]) -> usize {
        todos.iter().filter(|todo| !self.shows(todo)).count()
    }
}

/// The todo list as it was before a change, so it can be restored.
struct UndoEntry {
    todos: Vec<Todo>,
    selected: usize,
    /// Describes the change, e.g. `Deleted "Call mom"`.
    label: String,
    /// Id of the todo being moved, so consecutive moves of it share one entry.
    coalesce: Option<u32>,
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
    pub view: ViewFilter,
    pub message: Option<Message>,
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
}

/// Marks a newly entered due date that has already passed as notified, so no
/// notification fires for a date the user just typed.
fn stamp_if_past(todo: &mut Todo, now: DateTime<Utc>) {
    if todo.due_date.is_some_and(|due| due <= now) {
        todo.notified_at = Some(now);
    }
}

/// `"Call mom"`, shortened with an ellipsis if long.
fn quoted(text: &str) -> String {
    if text.chars().count() > LABEL_TEXT_LIMIT {
        let short: String = text.chars().take(LABEL_TEXT_LIMIT - 1).collect();
        format!("\"{}…\"", short)
    } else {
        format!("\"{}\"", text)
    }
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
            view: ViewFilter::default(),
            message: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    pub fn storage_path(&self) -> Option<&Path> {
        self.storage_path.as_deref()
    }

    fn info(&mut self, text: String) {
        self.message = Some(Message {
            text,
            kind: MessageKind::Info,
        });
    }

    fn error(&mut self, text: String) {
        self.message = Some(Message {
            text,
            kind: MessageKind::Error,
        });
    }

    /// Persists the list. Call after setting any hint so a save error replaces it.
    fn save(&mut self) {
        if let Some(path) = &self.storage_path
            && let Err(e) = save_todos(path, &self.todos)
        {
            self.error(format!("Failed to save todos: {}", e));
        }
    }

    fn next_id(&self) -> u32 {
        self.todos.iter().map(|t| t.id + 1).max().unwrap_or(0)
    }

    fn is_visible(&self, index: usize) -> bool {
        self.todos.get(index).is_some_and(|todo| self.view.shows(todo))
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

    /// Records the current list as an undo step, right before a change.
    /// Call it only once the change is certain to happen (after guards and
    /// no-op checks), so every recorded step undoes something real.
    /// Consecutive calls with the same `coalesce` id share one step.
    fn checkpoint(&mut self, label: String, coalesce: Option<u32>) {
        self.redo_stack.clear();
        if coalesce.is_some() && self.undo_stack.back().is_some_and(|top| top.coalesce == coalesce) {
            return;
        }
        self.undo_stack.push_back(UndoEntry {
            todos: self.todos.clone(),
            selected: self.selected,
            label,
            coalesce,
        });
        if self.undo_stack.len() > UNDO_LIMIT {
            self.undo_stack.pop_front();
        }
    }

    fn undo(&mut self) {
        self.restore(HistoryStep::Undo);
    }

    fn redo(&mut self) {
        self.restore(HistoryStep::Redo);
    }

    /// Pops a step from the undo (or redo) stack, restores it, and pushes the
    /// current state onto the other stack so the step can be reversed again.
    fn restore(&mut self, step: HistoryStep) {
        let (from, to, verb, done) = match step {
            HistoryStep::Undo => (&mut self.undo_stack, &mut self.redo_stack, "undo", "Undid"),
            HistoryStep::Redo => (&mut self.redo_stack, &mut self.undo_stack, "redo", "Redid"),
        };
        let Some(entry) = from.pop_back() else {
            self.info(format!("Nothing to {}", verb));
            return;
        };
        let message = format!("{}: {}", done, entry.label);
        let current = std::mem::replace(&mut self.todos, entry.todos);
        // `notified_at` is system state: keep the current value so restoring
        // a snapshot never re-arms a notification that was already sent.
        let notified: HashMap<u32, _> = current.iter().map(|t| (t.id, t.notified_at)).collect();
        for todo in &mut self.todos {
            if let Some(&notified_at) = notified.get(&todo.id) {
                todo.notified_at = notified_at;
            }
        }
        to.push_back(UndoEntry {
            todos: current,
            selected: self.selected,
            label: entry.label,
            coalesce: None,
        });
        // Undo/redo ends any run of moves: a later move starts a new step.
        if let Some(top) = self.undo_stack.back_mut() {
            top.coalesce = None;
        }
        self.selected = entry.selected;
        self.fix_selection();
        self.info(message);
        self.save();
    }

    /// Swaps the selected todo with the previous visible one.
    fn move_selected_up(&mut self) {
        let Some(current) = self.selected_index() else { return };
        let Some(prev) = self.prev_visible_before(current) else {
            return;
        };
        self.move_todo(current, prev);
    }

    /// Swaps the selected todo with the next visible one.
    fn move_selected_down(&mut self) {
        let Some(current) = self.selected_index() else { return };
        let Some(next) = self.next_visible_after(current) else {
            return;
        };
        self.move_todo(current, next);
    }

    fn move_todo(&mut self, from: usize, to: usize) {
        let todo = &self.todos[from];
        self.checkpoint(format!("Moved {}", quoted(&todo.text)), Some(todo.id));
        self.todos.swap(from, to);
        self.selected = to;
        self.save();
    }

    fn toggle_completed(&mut self) {
        let Some(index) = self.selected_index() else { return };
        let todo = &self.todos[index];
        let verb = if todo.completed { "Reopened" } else { "Completed" };
        let label = format!("{} {}", verb, quoted(&todo.text));
        self.checkpoint(label.clone(), None);
        let todo = &mut self.todos[index];
        todo.completed = !todo.completed;
        // The todo vanishes if completed todos are hidden: point at undo.
        if !self.is_visible(index) {
            self.info(format!("{} (u to undo)", label));
        }
        self.save();
        self.fix_selection();
    }

    fn toggle_suspended(&mut self) {
        let Some(index) = self.selected_index() else { return };
        let todo = &self.todos[index];
        // Completed wins; `s` is a no-op so un-completing restores the prior suspended state.
        if todo.completed {
            return;
        }
        let verb = if todo.suspended { "Resumed" } else { "Suspended" };
        let label = format!("{} {}", verb, quoted(&todo.text));
        self.checkpoint(label.clone(), None);
        let todo = &mut self.todos[index];
        todo.suspended = !todo.suspended;
        // The todo vanishes if suspended todos are hidden: point at undo.
        if !self.is_visible(index) {
            self.info(format!("{} (u to undo)", label));
        }
        self.save();
        self.fix_selection();
    }

    fn delete_selected(&mut self) {
        let Some(index) = self.selected_index() else { return };
        let label = format!("Deleted {}", quoted(&self.todos[index].text));
        self.checkpoint(label.clone(), None);
        self.todos.remove(index);
        self.info(format!("{} (u to undo)", label));
        self.save();
        self.fix_selection();
    }

    fn clear_all(&mut self) {
        if self.todos.is_empty() {
            return;
        }
        let label = format!("Cleared {} todos", self.todos.len());
        self.checkpoint(label.clone(), None);
        self.todos.clear();
        self.selected = 0;
        self.info(format!("{} (u to undo)", label));
        self.save();
    }

    /// Flips one view filter, keeping the selection on a visible todo.
    /// View state only: not saved and not undoable.
    fn toggle_view(&mut self, flag: fn(&mut ViewFilter) -> &mut bool) {
        let flag = flag(&mut self.view);
        *flag = !*flag;
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
        let now_utc = now.with_timezone(&Utc);
        let mut changed = false;

        if self.input_mode == InputMode::Rescheduling {
            if let Some(index) = self.editing_index.filter(|&i| i < self.todos.len()) {
                let new_due = if input.is_empty() {
                    None
                } else {
                    // The whole input must be a date: "fri 3pm" must not silently drop "fri".
                    match parse_due_date(&input, now) {
                        Some(parsed) if parsed.rest.is_empty() => Some(parsed.due),
                        _ => {
                            self.error(format!("Could not understand \"{}\" — schedule unchanged", input));
                            return;
                        }
                    }
                };
                if self.todos[index].due_date != new_due {
                    self.checkpoint(format!("Rescheduled {}", quoted(&self.todos[index].text)), None);
                    let todo = &mut self.todos[index];
                    todo.due_date = new_due;
                    stamp_if_past(todo, now_utc);
                    changed = true;
                }
            }
        } else if !input.is_empty() {
            let (text, new_due_date) = match parse_due_date(&input, now) {
                Some(parsed) => (parsed.rest, Some(parsed.due)),
                None => (input, None),
            };
            if text.is_empty() {
                self.error("Todo text cannot be empty".to_string());
                return;
            }
            if let Some(index) = self.editing_index.filter(|&i| i < self.todos.len()) {
                // Apply the edit to a copy and compare whole todos, so any
                // field an edit may touch is covered by the no-op check.
                let mut edited = self.todos[index].clone();
                edited.text = text;
                edited.due_date = new_due_date.or(edited.due_date);
                if edited != self.todos[index] {
                    self.checkpoint(format!("Edited {}", quoted(&edited.text)), None);
                    // Stamped after the no-op comparison, so it can't make a no-op edit count.
                    if new_due_date.is_some() {
                        stamp_if_past(&mut edited, now_utc);
                    }
                    self.todos[index] = edited;
                    changed = true;
                }
            } else if self.editing_index.is_none() {
                self.checkpoint(format!("Added {}", quoted(&text)), None);
                let mut todo = Todo::new(self.next_id(), text);
                todo.due_date = new_due_date;
                stamp_if_past(&mut todo, now_utc);
                self.todos.push(todo);
                self.selected = self.todos.len() - 1;
                changed = true;
            }
        }

        self.input_mode = InputMode::Normal;
        self.editing_index = None;
        if changed {
            self.save();
        }
    }

    /// The due date the current input would set, for the live preview.
    /// Mirrors `submit_input`, so it never promises a date that Enter rejects:
    /// rescheduling needs the whole input to be a date, adding and editing
    /// need some title text left over.
    pub fn input_preview(&self, now: DateTime<Local>) -> Option<DateTime<Utc>> {
        let parsed = parse_due_date(self.input.value().trim(), now)?;
        let valid = match self.input_mode {
            InputMode::Rescheduling => parsed.rest.is_empty(),
            InputMode::Adding | InputMode::Editing => !parsed.rest.is_empty(),
            _ => false,
        };
        valid.then_some(parsed.due)
    }

    fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.editing_index = None;
        self.message = None;
    }

    /// Marks every todo that needs an overdue notification as notified at `now`,
    /// saves, and returns their texts for the caller to send. Stamping happens
    /// here, before sending, so a failed send is never retried every tick.
    /// Not undoable: notification state is system state, not a user change.
    pub fn take_due_notifications(&mut self, now: DateTime<Utc>) -> Vec<String> {
        let mut texts = Vec::new();
        for todo in &mut self.todos {
            if todo.needs_notification(now) {
                todo.notified_at = Some(now);
                texts.push(todo.text.clone());
            }
        }
        if !texts.is_empty() {
            self.save();
        }
        texts
    }

    /// Handles a key press. Returns `false` when the app should quit.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.input_mode {
            InputMode::Normal => return self.handle_normal_key(code, modifiers),
            InputMode::Adding | InputMode::Editing | InputMode::Rescheduling => self.handle_text_key(code, modifiers),
            InputMode::ConfirmClear => {
                self.input_mode = InputMode::Normal;
                if code == KeyCode::Char('y') && modifiers.is_empty() {
                    self.clear_all();
                }
            }
            InputMode::Help => self.input_mode = InputMode::Normal,
        }
        true
    }

    fn handle_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.message = None;
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Char('r') if ctrl => self.redo(),
            // Only explicitly bound Ctrl shortcuts act; Ctrl+D must not delete.
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char('q') => return false,
            KeyCode::Char('u') if modifiers.is_empty() => self.undo(),
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
            KeyCode::Char('h') => self.toggle_view(|view| &mut view.hide_completed),
            KeyCode::Char('H') => self.toggle_view(|view| &mut view.hide_suspended),
            _ => {}
        }
        true
    }

    fn handle_text_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        // A pending error stays until the user changes the input; the live
        // preview takes its place in the title.
        if !matches!(code, KeyCode::Enter | KeyCode::Esc) {
            self.message = None;
        }
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
        app.view.hide_completed = true;
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_previous();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn actions_ignore_hidden_selection() {
        let mut app = app_with(&["a"]);
        app.todos[0].completed = true;
        press(&mut app, KeyCode::Char('h'));
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
        app.view.hide_completed = true;
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
    fn suspend_is_noop_on_completed_todo() {
        let mut app = app_with(&["a"]);
        press(&mut app, KeyCode::Char('s'));
        assert!(app.todos[0].suspended);
        press(&mut app, KeyCode::Char('s'));
        app.todos[0].completed = true;
        press(&mut app, KeyCode::Char('s'));
        assert!(!app.todos[0].suspended);

        // A completed todo that was already suspended keeps both flags.
        app.todos[0].suspended = true;
        press(&mut app, KeyCode::Char('s'));
        assert!(app.todos[0].suspended && app.todos[0].completed);
    }

    fn overdue(app: &mut App, index: usize, hours_ago: i64) {
        app.todos[index].due_date = Some(Utc::now() - chrono::Duration::hours(hours_ago));
    }

    #[test]
    fn notifies_once_per_due_date() {
        let mut app = app_with(&["a", "b", "parked", "done", "later"]);
        overdue(&mut app, 0, 2);
        overdue(&mut app, 1, 1);
        overdue(&mut app, 2, 1);
        app.todos[2].suspended = true;
        overdue(&mut app, 3, 1);
        app.todos[3].completed = true;
        app.todos[4].due_date = Some(Utc::now() + chrono::Duration::hours(1));

        assert_eq!(app.take_due_notifications(Utc::now()), ["a", "b"]);
        assert!(app.take_due_notifications(Utc::now()).is_empty());

        // Toggling completion does not re-arm.
        app.todos[0].completed = true;
        app.todos[0].completed = false;
        assert!(app.take_due_notifications(Utc::now()).is_empty());

        // An earlier, already-past due date does not re-arm.
        overdue(&mut app, 1, 5);
        assert!(app.take_due_notifications(Utc::now()).is_empty());

        // A later due date re-arms once it passes.
        let later = Utc::now() + chrono::Duration::hours(3);
        app.todos[0].due_date = Some(Utc::now() + chrono::Duration::hours(2));
        assert!(app.take_due_notifications(Utc::now()).is_empty());
        assert_eq!(app.take_due_notifications(later), ["a", "later"]);
    }

    #[test]
    fn notification_state_survives_restart() {
        let (_dir, path) = crate::storage::test_support::temp_todo_file();

        let mut app = App::new(vec![Todo::new(0, "a".to_string())], Some(path.clone()));
        overdue(&mut app, 0, 1);
        assert_eq!(app.take_due_notifications(Utc::now()), ["a"]);

        let todos = crate::storage::load_todos(&path).unwrap();
        let mut restarted = App::new(todos, Some(path));
        assert!(restarted.take_due_notifications(Utc::now()).is_empty());
    }

    fn ctrl(app: &mut App, c: char) -> bool {
        app.handle_key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn message(app: &App) -> (&str, MessageKind) {
        let msg = app.message.as_ref().expect("a message");
        (msg.text.as_str(), msg.kind)
    }

    /// Runs `action` on a fresh three-todo list, checks it recorded exactly one
    /// undo step, and that undo restores the original list.
    fn assert_undoable(name: &str, action: impl Fn(&mut App)) {
        let mut app = app_with(&["a", "b", "c"]);
        app.selected = 1;
        let before = app.todos.clone();
        action(&mut app);
        assert_ne!(app.todos, before, "{}: action changed nothing", name);
        assert_eq!(app.undo_stack.len(), 1, "{}: expected one undo step", name);
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.todos, before, "{}: undo did not restore", name);
        assert_eq!(app.selected, 1, "{}: selection not restored", name);
    }

    #[test]
    fn every_action_is_one_undoable_step() {
        assert_undoable("add", |app| {
            press(app, KeyCode::Char('a'));
            type_text(app, "new");
            press(app, KeyCode::Enter);
        });
        assert_undoable("edit", |app| {
            press(app, KeyCode::Char('e'));
            type_text(app, "!");
            press(app, KeyCode::Enter);
        });
        assert_undoable("reschedule", |app| {
            press(app, KeyCode::Char('r'));
            type_text(app, "tomorrow");
            press(app, KeyCode::Enter);
        });
        assert_undoable("complete", |app| press(app, KeyCode::Char(' ')));
        assert_undoable("suspend", |app| press(app, KeyCode::Char('s')));
        assert_undoable("delete", |app| press(app, KeyCode::Char('d')));
        assert_undoable("move", |app| press(app, KeyCode::Char('J')));
        assert_undoable("clear all", |app| {
            press(app, KeyCode::Char('X'));
            press(app, KeyCode::Char('y'));
        });
    }

    #[test]
    fn no_op_actions_record_nothing() {
        let mut app = app_with(&["a", "b"]);
        app.todos[1].completed = true;
        // Gives "a" a due date, so rescheduling to the same date can be checked.
        let tomorrow = |app: &mut App| {
            press(app, KeyCode::Char('r'));
            type_text(app, "tomorrow");
            press(app, KeyCode::Enter);
        };
        tomorrow(&mut app);

        type Case<'a> = (&'a str, &'a dyn Fn(&mut App));
        let cases: &[Case] = &[
            ("move at the top", &|app| press(app, KeyCode::Char('K'))),
            ("reschedule to the same date", &|app| tomorrow(app)),
            ("suspend a completed todo", &|app| {
                press(app, KeyCode::Char('j'));
                press(app, KeyCode::Char('s'));
            }),
            ("move at the bottom", &|app| press(app, KeyCode::Char('J'))),
            ("view state only", &|app| {
                press(app, KeyCode::Char('k'));
                press(app, KeyCode::Char('h'));
                press(app, KeyCode::Char('h'));
                press(app, KeyCode::Char('H'));
                press(app, KeyCode::Char('H'));
            }),
            ("empty add", &|app| {
                press(app, KeyCode::Char('a'));
                press(app, KeyCode::Enter);
            }),
            ("cancelled add", &|app| {
                press(app, KeyCode::Char('a'));
                type_text(app, "x");
                press(app, KeyCode::Esc);
            }),
            ("unchanged edit", &|app| {
                press(app, KeyCode::Char('e'));
                press(app, KeyCode::Enter);
            }),
            ("unparseable reschedule", &|app| {
                press(app, KeyCode::Char('r'));
                type_text(app, "gibberish");
                press(app, KeyCode::Enter);
                press(app, KeyCode::Esc);
            }),
            ("declined clear all", &|app| {
                press(app, KeyCode::Char('X'));
                press(app, KeyCode::Char('n'));
            }),
            ("Ctrl+Y does not confirm clear all", &|app| {
                press(app, KeyCode::Char('X'));
                ctrl(app, 'y');
            }),
            ("notification stamp", &|app| {
                overdue(app, 0, 1);
                app.take_due_notifications(Utc::now());
            }),
        ];
        for (name, action) in cases {
            let before = app.undo_stack.len();
            action(&mut app);
            assert_eq!(app.undo_stack.len(), before, "{} recorded an undo step", name);
        }

        // Clearing an already-empty schedule is a no-op too.
        app.todos[0].due_date = None;
        let before = app.undo_stack.len();
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.undo_stack.len(),
            before,
            "clearing an empty schedule recorded a step"
        );
    }

    #[test]
    fn redo_reapplies_and_new_changes_clear_it() {
        let mut app = app_with(&["a", "b"]);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.todos.len(), 2);
        assert!(ctrl(&mut app, 'r'));
        assert_eq!(app.todos.len(), 1);
        assert_eq!(message(&app), ("Redid: Deleted \"a\"", MessageKind::Info));

        press(&mut app, KeyCode::Char('u'));
        press(&mut app, KeyCode::Char('s'));
        ctrl(&mut app, 'r');
        assert_eq!(message(&app), ("Nothing to redo", MessageKind::Info));
        assert_eq!(app.todos.len(), 2);
    }

    #[test]
    fn consecutive_moves_of_one_todo_are_one_step() {
        let mut app = app_with(&["a", "b", "c", "d"]);
        let original = app.todos.clone();
        for _ in 0..3 {
            press(&mut app, KeyCode::Char('J'));
        }
        assert_eq!(app.undo_stack.len(), 1);
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.todos, original);

        // Another change ends the run.
        press(&mut app, KeyCode::Char('J'));
        press(&mut app, KeyCode::Char('s'));
        press(&mut app, KeyCode::Char('J'));
        assert_eq!(app.undo_stack.len(), 3);
    }

    #[test]
    fn undo_ends_a_run_of_moves() {
        // Move a, move b, undo; moving a again must be a new step, or the
        // state after the first move would become unreachable.
        let mut app = app_with(&["a", "b", "c", "d"]);
        press(&mut app, KeyCode::Char('J')); // [b, a, c, d]
        press(&mut app, KeyCode::Char('k'));
        press(&mut app, KeyCode::Char('J')); // [a, b, c, d]
        press(&mut app, KeyCode::Char('u')); // [b, a, c, d]
        let after_first_move = app.todos.clone();
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('J')); // [b, c, a, d]
        assert_eq!(app.undo_stack.len(), 2);
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.todos, after_first_move);
    }

    #[test]
    fn undo_delete_restores_position_id_and_selection() {
        let mut app = app_with(&["a", "b", "c"]);
        app.selected = 1;
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(message(&app), ("Deleted \"b\" (u to undo)", MessageKind::Info));
        app.selected = 0;
        press(&mut app, KeyCode::Char('u'));
        assert_eq!((app.todos[1].id, app.todos[1].text.as_str()), (1, "b"));
        assert_eq!(app.selected, 1);
        assert_eq!(message(&app), ("Undid: Deleted \"b\"", MessageKind::Info));
    }

    #[test]
    fn undo_keeps_current_notification_state() {
        let mut app = app_with(&["a", "b"]);
        overdue(&mut app, 0, 1);
        app.todos[0].due_date = Some(Utc::now() + chrono::Duration::minutes(1));
        // Edit, then the todo becomes due and is notified, then undo the edit.
        press(&mut app, KeyCode::Char('e'));
        type_text(&mut app, "!");
        press(&mut app, KeyCode::Enter);
        let later = Utc::now() + chrono::Duration::minutes(2);
        assert_eq!(app.take_due_notifications(later), ["a!"]);
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.todos[0].text, "a");
        assert!(
            app.take_due_notifications(later).is_empty(),
            "undo re-armed a notification"
        );

        // A deleted todo keeps the state it had when it was deleted.
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('u'));
        assert!(app.todos[0].notified_at.is_some());
        assert!(app.take_due_notifications(later).is_empty());
    }

    #[test]
    fn empty_history_says_so() {
        let mut app = app_with(&["a"]);
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(message(&app), ("Nothing to undo", MessageKind::Info));
        ctrl(&mut app, 'r');
        assert_eq!(message(&app), ("Nothing to redo", MessageKind::Info));
    }

    #[test]
    fn history_is_bounded() {
        let mut app = app_with(&["a"]);
        for _ in 0..UNDO_LIMIT + 5 {
            press(&mut app, KeyCode::Char('s'));
        }
        assert_eq!(app.undo_stack.len(), UNDO_LIMIT);
    }

    #[test]
    fn completing_while_hidden_hints_and_undo_selects_it_again() {
        let mut app = app_with(&["a", "b", "c"]);
        app.view.hide_completed = true;
        app.selected = 1;
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(message(&app), ("Completed \"b\" (u to undo)", MessageKind::Info));
        assert_eq!(app.selected, 2);
        press(&mut app, KeyCode::Char('u'));
        assert!(!app.todos[1].completed);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn long_texts_are_shortened_in_labels() {
        let long = "x".repeat(40);
        let label = quoted(&long);
        assert_eq!(label.chars().count(), LABEL_TEXT_LIMIT + 2);
        assert!(label.ends_with("…\""));
        assert_eq!(quoted("héllo"), "\"héllo\"");
    }

    #[test]
    fn unbound_ctrl_letters_do_nothing() {
        let mut app = app_with(&["a"]);
        let before = app.todos.clone();
        for c in ['d', 's', 'x', 'j', 'u'] {
            assert!(ctrl(&mut app, c));
        }
        assert!(ctrl(&mut app, 'q'), "Ctrl+Q must not quit");
        assert_eq!(app.todos, before);
        assert!(app.input_mode == InputMode::Normal);
    }

    #[test]
    fn past_dates_typed_by_the_user_do_not_notify() {
        let mut app = app_with(&["a"]);
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "Old thing 2025-01-01");
        press(&mut app, KeyCode::Enter);
        assert!(app.todos[1].notified_at.is_some());

        app.selected = 0;
        press(&mut app, KeyCode::Char('r'));
        type_text(&mut app, "2025-01-01");
        press(&mut app, KeyCode::Enter);
        assert!(app.todos[0].notified_at.is_some());

        assert!(app.take_due_notifications(Utc::now()).is_empty());
    }

    #[test]
    fn edit_with_the_same_parsed_date_is_a_no_op() {
        let mut app = app_with(&[]);
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "Call mom tomorrow");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.todos[0].text, "Call mom");
        let steps = app.undo_stack.len();

        press(&mut app, KeyCode::Char('e'));
        type_text(&mut app, " tomorrow");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.undo_stack.len(), steps);
        assert_eq!(app.todos[0].text, "Call mom");
    }

    #[test]
    fn reschedule_needs_the_whole_input_to_be_a_date() {
        let mut app = app_with(&["a"]);
        press(&mut app, KeyCode::Char('r'));
        type_text(&mut app, "fri 3pm");
        assert_eq!(app.input_preview(Local::now()), None);
        press(&mut app, KeyCode::Enter);
        assert!(app.input_mode == InputMode::Rescheduling);
        assert_eq!(app.message.as_ref().map(|m| m.kind), Some(MessageKind::Error));
        assert_eq!(app.todos[0].due_date, None);

        // Editing the input clears the error so the preview can show.
        press(&mut app, KeyCode::Home);
        type_text(&mut app, "next ");
        assert!(app.message.is_none());
        press(&mut app, KeyCode::End);
        for _ in 0.."fri 3pm".len() {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(&mut app, "friday 3pm");
        assert!(app.input_preview(Local::now()).is_some());
        press(&mut app, KeyCode::Enter);
        assert!(app.input_mode == InputMode::Normal);
        assert!(app.todos[0].due_date.is_some());
    }

    #[test]
    fn preview_only_while_typing() {
        let mut app = app_with(&["a"]);
        assert_eq!(app.input_preview(Local::now()), None);
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "Plan next sprint");
        assert_eq!(app.input_preview(Local::now()), None);
        type_text(&mut app, " tomorrow");
        assert!(app.input_preview(Local::now()).is_some());
    }

    #[test]
    fn preview_matches_what_enter_accepts() {
        // A date with no title would be rejected on Enter, so it isn't previewed.
        let mut app = app_with(&[]);
        press(&mut app, KeyCode::Char('a'));
        type_text(&mut app, "tomorrow");
        assert_eq!(app.input_preview(Local::now()), None);
        press(&mut app, KeyCode::Enter);
        assert!(app.input_mode == InputMode::Adding, "Enter should have been rejected");
    }

    #[test]
    fn view_filter_follows_display_precedence() {
        let todo = |completed, suspended| {
            let mut todo = Todo::new(0, "x".to_string());
            todo.completed = completed;
            todo.suspended = suspended;
            todo
        };
        let filters = [(false, false), (true, false), (false, true), (true, true)].map(|(c, s)| ViewFilter {
            hide_completed: c,
            hide_suspended: s,
        });
        // (completed, suspended) -> shown with [no filter, h, H, h+H]
        let cases = [
            ((false, false), [true, true, true, true]),
            ((true, false), [true, false, true, false]),
            ((false, true), [true, true, false, false]),
            ((true, true), [true, false, true, false]), // counts as completed
        ];
        for ((completed, suspended), expected) in cases {
            let shown = filters.map(|filter| filter.shows(&todo(completed, suspended)));
            assert_eq!(shown, expected, "completed={} suspended={}", completed, suspended);
        }
    }

    #[test]
    fn hidden_suspended_todos_are_skipped() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos[1].suspended = true;
        press(&mut app, KeyCode::Char('H'));
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected, 2);
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.selected, 0);
        press(&mut app, KeyCode::Char('J'));
        let texts: Vec<_> = app.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["c", "b", "a"]);
    }

    #[test]
    fn suspending_while_hidden_hints_and_undo_brings_it_back() {
        let mut app = app_with(&["a", "b", "c"]);
        press(&mut app, KeyCode::Char('H'));
        app.selected = 1;
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(message(&app), ("Suspended \"b\" (u to undo)", MessageKind::Info));
        assert_eq!(app.selected, 2, "selection moved to a visible todo");
        press(&mut app, KeyCode::Char('u'));
        assert!(!app.todos[1].suspended);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn filter_keys_are_view_state_only() {
        let mut app = app_with(&["a"]);
        app.todos[0].suspended = true;
        press(&mut app, KeyCode::Char('H'));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(
            app.view,
            ViewFilter {
                hide_completed: true,
                hide_suspended: true
            }
        );
        assert!(app.undo_stack.is_empty());
        press(&mut app, KeyCode::Char('H'));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.view, ViewFilter::default());
    }

    #[test]
    fn shift_h_is_typed_in_input_modes() {
        let mut app = app_with(&[]);
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('H'));
        assert_eq!(app.input.value(), "H");
        assert_eq!(app.view, ViewFilter::default());
    }

    #[test]
    fn next_id_is_unique_after_deletion() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos.remove(0);
        assert_eq!(app.next_id(), 3);
    }
}
