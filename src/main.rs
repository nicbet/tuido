use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use notify_rust::Notification;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

/// How often the UI refreshes (overdue colors, notifications) without input.
const TICK_RATE: Duration = Duration::from_secs(1);

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

    fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.due_date.is_some_and(|due| due <= now) && !self.completed && !self.suspended
    }
}

fn get_config_dir() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".config").join("tuido"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not determine home directory"))
}

fn todos_path() -> io::Result<PathBuf> {
    Ok(get_config_dir()?.join("todos.json"))
}

/// Loads todos from disk. A missing file yields an empty list, but an unreadable
/// or unparseable file is an error so we never silently overwrite user data.
fn load_todos() -> Result<Vec<Todo>, String> {
    let file_path = todos_path().map_err(|e| e.to_string())?;
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read {}: {}", file_path.display(), e))?;
    let mut todos: Vec<Todo> = serde_json::from_str(&data)
        .map_err(|e| format!("failed to parse {}: {}", file_path.display(), e))?;

    // Older versions could assign duplicate ids; make them unique again.
    let mut seen = HashSet::new();
    if !todos.iter().all(|t| seen.insert(t.id)) {
        for (i, todo) in todos.iter_mut().enumerate() {
            todo.id = i as u32;
        }
    }
    Ok(todos)
}

/// Writes todos atomically (temp file + rename) so a crash mid-write cannot
/// leave a truncated file behind.
fn save_todos(file_path: &Path, todos: &[Todo]) -> io::Result<()> {
    if let Some(dir) = file_path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp_path = file_path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(todos)?;
    fs::write(&tmp_path, data)?;
    fs::rename(tmp_path, file_path)
}

/// Converts a local wall-clock time to UTC, resolving DST ambiguities to the
/// earlier instant and skipping forward over DST gaps.
fn local_to_utc(datetime: NaiveDateTime) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&datetime)
        .earliest()
        .or_else(|| Local.from_local_datetime(&(datetime + chrono::Duration::hours(1))).earliest())
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_due_date(text: &str, now: DateTime<Local>) -> Option<DateTime<Utc>> {
    let time = extract_time_anywhere(text);

    if RE_TOMORROW.is_match(text) {
        let date = now.date_naive() + chrono::Duration::days(1);
        let time = time.unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());
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

fn extract_time_anywhere(text: &str) -> Option<chrono::NaiveTime> {
    if let Some(cap) = RE_TIME_24.captures(text)
        && let Ok(hour) = cap[1].parse::<u32>()
        && let Ok(min) = cap[2].parse::<u32>()
        && hour < 24
        && min < 60
    {
        return chrono::NaiveTime::from_hms_opt(hour, min, 0);
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
        return chrono::NaiveTime::from_hms_opt(hour_24, min, 0);
    }

    None
}

/// Removes the scheduling phrases that `parse_due_date` understood, so that
/// re-editing a todo later doesn't silently reschedule it.
fn clean_text(text: &str) -> String {
    let text = RE_CLEAN.replace_all(text, "");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sends a desktop notification. Failures (e.g. no notification daemon) are
/// deliberately ignored: a missing notification must never crash the TUI.
fn notify(todo: &Todo) {
    let _ = Notification::new()
        .summary("Todo Reminder")
        .body(&format!("Reminder: {}", todo.text))
        .show();
}

fn format_clock(hour: u32, min: u32) -> String {
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    let am_pm = if hour >= 12 { "pm" } else { "am" };
    format!("{}:{:02}{}", hour12, min, am_pm)
}

fn format_due_time(due: DateTime<Utc>, now: DateTime<Local>) -> String {
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
    } else if due_date.format("%Y").to_string() == today.format("%Y").to_string() {
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

fn format_overdue_duration(due: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match humanize_duration(now - due) {
        Some(d) => format!("{} overdue", d),
        None => "less than a minute overdue".to_string(),
    }
}

fn format_age(created: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match humanize_duration(now - created) {
        Some(d) => format!("added {} ago", d),
        None => "added just now".to_string(),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Adding,
    Editing,
    Rescheduling,
    ConfirmClear,
    Help,
}

struct App {
    todos: Vec<Todo>,
    /// Where todos are persisted; `None` disables saving (used in tests).
    storage_path: Option<PathBuf>,
    /// Index into `todos` (not into the visible list).
    selected: usize,
    input_mode: InputMode,
    input: String,
    editing_index: Option<usize>,
    /// Cursor position in the input, counted in chars (not bytes).
    cursor_position: usize,
    hide_completed: bool,
    /// Ids of todos we've already sent an overdue notification for.
    notified: HashSet<u32>,
    /// Transient message (errors, hints) shown in the Info panel.
    message: Option<String>,
}

impl App {
    fn new(todos: Vec<Todo>, storage_path: Option<PathBuf>) -> Self {
        Self {
            todos,
            storage_path,
            selected: 0,
            input_mode: InputMode::Normal,
            input: String::new(),
            editing_index: None,
            cursor_position: 0,
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

    fn visible_indices(&self) -> Vec<usize> {
        (0..self.todos.len()).filter(|&i| self.is_visible(i)).collect()
    }

    /// The selected todo's index, if it exists and is currently visible.
    fn selected_index(&self) -> Option<usize> {
        self.is_visible(self.selected).then_some(self.selected)
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

    fn select_next(&mut self) {
        if let Some(next) = (self.selected + 1..self.todos.len()).find(|&i| self.is_visible(i)) {
            self.selected = next;
        }
    }

    fn select_previous(&mut self) {
        if let Some(prev) = (0..self.selected).rev().find(|&i| self.is_visible(i)) {
            self.selected = prev;
        }
    }

    /// Swaps the selected todo with the previous visible one.
    fn move_selected_up(&mut self) {
        let Some(current) = self.selected_index() else { return };
        if let Some(prev) = (0..current).rev().find(|&i| self.is_visible(i)) {
            self.todos.swap(current, prev);
            self.selected = prev;
            self.save();
        }
    }

    /// Swaps the selected todo with the next visible one.
    fn move_selected_down(&mut self) {
        let Some(current) = self.selected_index() else { return };
        if let Some(next) = (current + 1..self.todos.len()).find(|&i| self.is_visible(i)) {
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

    fn toggle_hide_completed(&mut self) {
        self.hide_completed = !self.hide_completed;
        self.fix_selection();
    }

    fn start_input(&mut self, mode: InputMode, initial: String, editing_index: Option<usize>) {
        self.input_mode = mode;
        self.cursor_position = initial.chars().count();
        self.input = initial;
        self.editing_index = editing_index;
        self.message = None;
    }

    fn submit_input(&mut self) {
        let input = self.input.trim().to_string();
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
            let text = if new_due_date.is_some() { clean_text(&input) } else { input };
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

    /// Byte offset in `input` of the char at `char_pos`.
    fn byte_index(&self, char_pos: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_pos)
            .map_or(self.input.len(), |(i, _)| i)
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.byte_index(self.cursor_position);
        self.input.insert(idx, c);
        self.cursor_position += 1;
    }

    fn delete_char_before_cursor(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            let idx = self.byte_index(self.cursor_position);
            self.input.remove(idx);
        }
    }

    fn delete_char_at_cursor(&mut self) {
        if self.cursor_position < self.input.chars().count() {
            let idx = self.byte_index(self.cursor_position);
            self.input.remove(idx);
        }
    }

    fn delete_word_before_cursor(&mut self) {
        let chars: Vec<char> = self.input.chars().collect();
        let mut start = self.cursor_position;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let (from, to) = (self.byte_index(start), self.byte_index(self.cursor_position));
        self.input.replace_range(from..to, "");
        self.cursor_position = start;
    }

    fn delete_to_line_start(&mut self) {
        let to = self.byte_index(self.cursor_position);
        self.input.replace_range(..to, "");
        self.cursor_position = 0;
    }

    /// Sends a notification for every overdue todo that hasn't had one yet.
    fn check_notifications(&mut self) {
        let now = Utc::now();
        for todo in &self.todos {
            if todo.is_overdue(now) && self.notified.insert(todo.id) {
                notify(todo);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.input_mode {
            InputMode::Normal => {
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
            }
            InputMode::Adding | InputMode::Editing | InputMode::Rescheduling => match code {
                KeyCode::Enter => self.submit_input(),
                KeyCode::Esc => self.cancel_input(),
                KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.delete_word_before_cursor()
                }
                KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.delete_to_line_start()
                }
                KeyCode::Char(_) if modifiers.contains(KeyModifiers::CONTROL) => {}
                KeyCode::Char(c) => self.insert_char(c),
                KeyCode::Backspace => self.delete_char_before_cursor(),
                KeyCode::Delete => self.delete_char_at_cursor(),
                KeyCode::Left => self.cursor_position = self.cursor_position.saturating_sub(1),
                KeyCode::Right => {
                    self.cursor_position = (self.cursor_position + 1).min(self.input.chars().count())
                }
                KeyCode::Home => self.cursor_position = 0,
                KeyCode::End => self.cursor_position = self.input.chars().count(),
                _ => {}
            },
            InputMode::ConfirmClear => {
                if code == KeyCode::Char('y') {
                    self.todos.clear();
                    self.selected = 0;
                    self.save();
                }
                self.input_mode = InputMode::Normal;
            }
            InputMode::Help => self.input_mode = InputMode::Normal,
        }
        true
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load before touching the terminal so errors print normally.
    let todos = match load_todos() {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("tuido: {}", e);
            eprintln!("Refusing to start so the file is not overwritten; fix or move it and retry.");
            std::process::exit(1);
        }
    };
    let mut app = App::new(todos, Some(todos_path()?));

    // `ratatui::init` also installs a panic hook that restores the terminal.
    let mut terminal = ratatui::init();
    let res = run_app(&mut terminal, &mut app);
    ratatui::restore();
    res?;
    Ok(())
}

fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    app.check_notifications();
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(TICK_RATE)? {
            // Ignore key release/repeat events (reported on Windows).
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && !app.handle_key(key.code, key.modifiers)
            {
                return Ok(());
            }
        } else {
            app.check_notifications();
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3), Constraint::Length(2)])
        .split(size);

    let now = Utc::now();
    let visible = app.visible_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let todo = &app.todos[i];
            let is_selected = i == app.selected;
            let prefix = if todo.completed {
                "[x]"
            } else if todo.suspended {
                "[~]"
            } else {
                "[ ]"
            };
            let mut style = if todo.completed {
                Style::default().add_modifier(Modifier::CROSSED_OUT).fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let is_overdue = todo.is_overdue(now);
            if is_overdue {
                style = style.fg(if is_selected { Color::Black } else { Color::Red });
            } else if todo.completed && is_selected {
                style = style.fg(Color::Black);
            }
            let main_content = format!("{} {}", prefix, todo.text);

            let mut spans = vec![Span::styled(main_content, style)];

            if todo.due_date.is_some() && !todo.completed {
                let icon = if is_overdue { "❗" } else { "⏰" }; // Exclamation for overdue, clock for scheduled
                spans.push(Span::raw(" "));
                spans.push(Span::styled(icon, Style::default()));
            }

            let line = Line::from(spans);
            let item_style = if is_selected {
                if is_overdue {
                    Style::default().bg(Color::Red).fg(Color::Black)
                } else if todo.completed {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else {
                    Style::default().add_modifier(Modifier::REVERSED)
                }
            } else {
                Style::default()
            };
            ListItem::new(line).style(item_style)
        })
        .collect();

    let pending_count = app.todos.iter().filter(|t| !t.completed).count();
    let completed_count = app.todos.len() - pending_count;

    let title = if app.hide_completed && completed_count > 0 {
        format!("TODOs ({}) / {} hidden", pending_count, completed_count)
    } else {
        format!("TODOs ({})", pending_count)
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title).padding(Padding::new(1, 1, 1, 1)))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    // A ListState keeps the selected item scrolled into view on long lists.
    let mut list_state = ListState::default().with_selected(visible.iter().position(|&i| i == app.selected));
    f.render_stateful_widget(list, chunks[0], &mut list_state);

    // Split the bottom chunk into left and right for status and version
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(10)])
        .split(chunks[2]);

    // Create status line with styled keys
    let status_spans = vec![
        Span::raw(" "), // Add left padding
        Span::styled("↑↓", Style::default().fg(Color::Red)),
        Span::raw("/"),
        Span::styled("jk", Style::default().fg(Color::Red)),
        Span::raw(": navigate  "),
        Span::styled("Space", Style::default().fg(Color::Red)),
        Span::raw(": toggle  "),
        Span::styled("a", Style::default().fg(Color::Red)),
        Span::raw(": add  "),
        Span::styled("e", Style::default().fg(Color::Red)),
        Span::raw(": edit  "),
        Span::styled("d", Style::default().fg(Color::Red)),
        Span::raw(": delete  "),
        Span::styled("?", Style::default().fg(Color::Red)),
        Span::raw(": help  "),
        Span::styled("q", Style::default().fg(Color::Red)),
        Span::raw(": quit  "),
    ];
    let status = Paragraph::new(Line::from(status_spans)).block(Block::default().borders(Borders::TOP));
    f.render_widget(status, bottom_chunks[0]);

    // Version in bottom right
    let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version = Paragraph::new(version_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(version, bottom_chunks[1]);

    if let InputMode::Adding | InputMode::Editing | InputMode::Rescheduling = app.input_mode {
        let base_title = match app.input_mode {
            InputMode::Adding => "Add Todo",
            InputMode::Editing => "Edit Todo",
            InputMode::Rescheduling => "Reschedule Todo (empty clears)",
            _ => "Input",
        };
        let title = match &app.message {
            Some(msg) => format!("{} — {}", base_title, msg),
            None => base_title.to_string(),
        };
        // Scroll horizontally so the cursor stays visible in long inputs.
        let inner_width = chunks[1].width.saturating_sub(4);
        let cursor_col = Line::raw(&app.input[..app.byte_index(app.cursor_position)]).width() as u16;
        let scroll = cursor_col.saturating_sub(inner_width.saturating_sub(1));
        let input = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .scroll((0, scroll))
            .block(Block::default().borders(Borders::ALL).title(title).padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(input, chunks[1]);
        f.set_cursor_position((chunks[1].x + 2 + cursor_col - scroll, chunks[1].y + 1));
    } else {
        // Show metadata for selected todo
        let (metadata, color) = if let Some(msg) = &app.message {
            (msg.clone(), Color::Red)
        } else if let Some(todo) = app.selected_index().map(|i| &app.todos[i]) {
            let is_overdue = todo.is_overdue(now);

            let time_info = match (todo.due_date, todo.created_at) {
                (Some(due), _) if is_overdue => format!(" ({})", format_overdue_duration(due, now)),
                (_, Some(created)) => format!(" ({})", format_age(created, now)),
                _ => String::new(),
            };

            if todo.completed {
                (format!("Status: Completed{}", time_info), Color::Green)
            } else if todo.suspended {
                (format!("Status: Suspended{}", time_info), Color::Yellow)
            } else if let Some(due) = todo.due_date {
                (
                    format!("Scheduled for: {}{}", format_due_time(due, Local::now()), time_info),
                    if is_overdue { Color::Red } else { Color::Green },
                )
            } else {
                (format!("No schedule set{}", time_info), Color::White)
            }
        } else {
            ("No todos".to_string(), Color::White)
        };
        let metadata_paragraph = Paragraph::new(metadata)
            .style(Style::default().fg(color))
            .block(Block::default().borders(Borders::ALL).title("Info").padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(metadata_paragraph, chunks[1]);
    }

    if app.input_mode == InputMode::ConfirmClear {
        let prompt = Paragraph::new(format!(
            "Delete all {} todos? This cannot be undone.\n\ny: yes, any other key: cancel",
            app.todos.len()
        ))
        .style(Style::default().fg(Color::Red))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Clear All").padding(Padding::new(1, 1, 1, 1)));
        let area = centered_rect(50, 30, size);
        f.render_widget(Clear, area);
        f.render_widget(prompt, area);
    }

    if app.input_mode == InputMode::Help {
        let help_text = "Keyboard Commands:\n\nNavigation:\n  ↑↓ / jk : Navigate items\n  Shift+↑↓ / JK : Reorder items\n\nActions:\n  Space : Toggle completed\n  a : Add new todo\n  e : Edit selected todo\n  r : Reschedule selected todo\n  d : Delete selected todo\n  s : Suspend selected todo\n  h : Toggle hide completed\n  X : Clear all todos\n\nOther:\n  ? : Show this help\n  q : Quit application\n\nPress any key to close";
        let help_paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Help").padding(Padding::new(1, 1, 1, 1)));

        let area = centered_rect(60, 80, size);
        f.render_widget(Clear, area);
        f.render_widget(help_paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    fn app_with(texts: &[&str]) -> App {
        let todos = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Todo::new(i as u32, t.to_string()))
            .collect();
        App::new(todos, None)
    }

    #[test]
    fn extracts_times() {
        let t = |h, m| Some(NaiveTime::from_hms_opt(h, m, 0).unwrap());
        assert_eq!(extract_time_anywhere("at 21:35"), t(21, 35));
        assert_eq!(extract_time_anywhere("9pm"), t(21, 0));
        assert_eq!(extract_time_anywhere("11:30am"), t(11, 30));
        assert_eq!(extract_time_anywhere("12am"), t(0, 0));
        assert_eq!(extract_time_anywhere("12pm"), t(12, 0));
        assert_eq!(extract_time_anywhere("15pm"), None);
        assert_eq!(extract_time_anywhere("0am"), None);
        assert_eq!(extract_time_anywhere("no time here"), None);
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

    #[test]
    fn humanizes_durations() {
        assert_eq!(humanize_duration(chrono::Duration::seconds(30)), None);
        assert_eq!(humanize_duration(chrono::Duration::minutes(1)).unwrap(), "1 minute");
        assert_eq!(humanize_duration(chrono::Duration::hours(25)).unwrap(), "1 day");
        assert_eq!(humanize_duration(chrono::Duration::days(13)).unwrap(), "1 week");
        assert_eq!(humanize_duration(chrono::Duration::days(14)).unwrap(), "2 weeks");
    }

    #[test]
    fn formats_clock() {
        assert_eq!(format_clock(0, 5), "12:05am");
        assert_eq!(format_clock(12, 0), "12:00pm");
        assert_eq!(format_clock(21, 30), "9:30pm");
    }

    #[test]
    fn input_editing_handles_multibyte_chars() {
        let mut app = app_with(&[]);
        app.start_input(InputMode::Adding, String::new(), None);
        for c in "héllo wörld 🎉".chars() {
            app.insert_char(c);
        }
        assert_eq!(app.input, "héllo wörld 🎉");
        app.delete_char_before_cursor();
        assert_eq!(app.input, "héllo wörld ");
        app.delete_word_before_cursor();
        assert_eq!(app.input, "héllo ");
        app.handle_key(KeyCode::Home, KeyModifiers::NONE);
        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        app.delete_char_at_cursor();
        assert_eq!(app.input, "hllo ");
        app.handle_key(KeyCode::End, KeyModifiers::NONE);
        app.delete_to_line_start();
        assert_eq!(app.input, "");
    }

    #[test]
    fn ctrl_shortcuts_do_not_insert_text() {
        let mut app = app_with(&[]);
        app.start_input(InputMode::Adding, "foo bar".to_string(), None);
        app.handle_key(KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(app.input, "foo ");
        app.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.input, "");
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
        app.message = None;
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        assert!(app.input_mode == InputMode::Normal);
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
    fn next_id_is_unique_after_deletion() {
        let mut app = app_with(&["a", "b", "c"]);
        app.todos.remove(0);
        assert_eq!(app.next_id(), 3);
    }
}
