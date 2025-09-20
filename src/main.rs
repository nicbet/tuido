use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use regex::Regex;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding, Paragraph},
    Frame, Terminal,
};
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

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
}

fn get_config_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".config").join("tuido")
}

fn load_todos() -> Vec<Todo> {
    let config_dir = get_config_dir();
    let file_path = config_dir.join("todos.json");
    if file_path.exists() {
        let data = fs::read_to_string(file_path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_todos(todos: &Vec<Todo>) {
    let config_dir = get_config_dir();
    fs::create_dir_all(&config_dir).unwrap();
    let file_path = config_dir.join("todos.json");
    let data = serde_json::to_string_pretty(todos).unwrap();
    fs::write(file_path, data).unwrap();
}

fn parse_due_date(text: &str) -> Option<DateTime<Utc>> {
    let lower = text.to_lowercase();
    let local_offset = *Local::now().offset();
    let now_local = Utc::now().with_timezone(&local_offset);

    // Check for tomorrow
    if lower.contains("tomorrow") {
        let tomorrow = now_local + chrono::Duration::days(1);
        let date = tomorrow.date_naive();
        if let Some(time) = extract_time_anywhere(&lower) {
            let datetime = date.and_time(time);
            let datetime_fixed = local_offset.from_local_datetime(&datetime).unwrap();
            return Some(datetime_fixed.with_timezone(&Utc));
        }
        let datetime = date.and_hms_opt(9, 0, 0).unwrap();
        let datetime_fixed = local_offset.from_local_datetime(&datetime).unwrap();
        return Some(datetime_fixed.with_timezone(&Utc));
    }

    // Extract time anywhere in text
    if let Some(time) = extract_time_anywhere(&lower) {
        let date = now_local.date_naive();
        let datetime = date.and_time(time);
        let datetime = if datetime < now_local.naive_local() {
            datetime + chrono::Duration::days(1)
        } else {
            datetime
        };
        let datetime_fixed = local_offset.from_local_datetime(&datetime).unwrap();
        return Some(datetime_fixed.with_timezone(&Utc));
    }

    None
}

fn extract_time_anywhere(text: &str) -> Option<chrono::NaiveTime> {
    // 24-hour format: 21:35
    let re_24 = Regex::new(r"(?i)\b(\d{1,2}):(\d{2})\b").unwrap();
    if let Some(cap) = re_24.captures(text)
        && let Ok(hour) = cap[1].parse::<u32>()
        && let Ok(min) = cap[2].parse::<u32>()
        && hour < 24 && min < 60 {
        return chrono::NaiveTime::from_hms_opt(hour, min, 0);
    }

    // 12-hour format: 9pm, 11:30am
    let re_12 = Regex::new(r"(?i)\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b").unwrap();
    if let Some(cap) = re_12.captures(text) {
        let hour: u32 = cap[1].parse().unwrap();
        let min: u32 = cap.get(2).map_or(0, |m| m.as_str().parse().unwrap());
        let am_pm = cap[3].to_lowercase();
        let hour_24 = if am_pm == "pm" && hour != 12 { hour + 12 } else if am_pm == "am" && hour == 12 { 0 } else { hour };
        if hour_24 < 24 && min < 60 {
            return chrono::NaiveTime::from_hms_opt(hour_24, min, 0);
        }
    }

    None
}

fn clean_text(text: &str) -> String {
    // Remove " at " + time patterns (case insensitive)
    let re_time = Regex::new(r"(?i)\s+at\s+\d{1,2}(?::\d{2})?\s*(?:am|pm)?\b").unwrap();
    let text = re_time.replace_all(text, "");

    // Remove standalone time patterns
    let re_standalone = Regex::new(r"(?i)\b\d{1,2}(?::\d{2})?\s*(?:am|pm)\b").unwrap();
    let text = re_standalone.replace_all(&text, "");

    text.trim().to_string()
}

fn parse_schedule(input: &str) -> Option<DateTime<Utc>> {
    if let Some(time) = extract_time_anywhere(input) {
        let local_offset = *Local::now().offset();
        let now_local = Utc::now().with_timezone(&local_offset);
        let date = now_local.date_naive();
        let datetime = date.and_time(time);
        let datetime = if datetime < now_local.naive_local() {
            datetime + chrono::Duration::days(1)
        } else {
            datetime
        };
        let datetime_fixed = local_offset.from_local_datetime(&datetime).unwrap();
        Some(datetime_fixed.with_timezone(&Utc))
    } else {
        None
    }
}

fn check_notifications(todos: &Vec<Todo>) {
    let now = Utc::now();
    for todo in todos {
        if let Some(due) = todo.due_date
            && due <= now && !todo.completed && !todo.suspended {
            Notification::new()
                .summary("Todo Reminder")
                .body(&format!("Reminder: {}", todo.text))
                .show()
                .unwrap();
        }
    }
}

fn format_due_time(due: DateTime<Utc>) -> String {
    let local_offset = *Local::now().offset();
    let now_local = Utc::now().with_timezone(&local_offset);
    let today = now_local.date_naive();
    let due_local = due.with_timezone(&local_offset);
    let due_date = due_local.date_naive();
    let due_time = due_local.time();
    let hour = due_time.hour();
    let min = due_time.minute();

    if due_date == today {
        if hour == 0 && min == 0 {
            "Today".to_string()
        } else {
            let hour12 = if hour == 0 { 12 } else if hour > 12 { hour - 12 } else { hour };
            let am_pm = if hour >= 12 { "pm" } else { "am" };
            format!("Today {:02}:{:02}{}", hour12, min, am_pm)
        }
    } else if due_date == today + chrono::Duration::days(1) {
        if hour == 0 && min == 0 {
            "Tomorrow".to_string()
        } else {
            let hour12 = if hour == 0 { 12 } else if hour > 12 { hour - 12 } else { hour };
            let am_pm = if hour >= 12 { "pm" } else { "am" };
            format!("Tomorrow {:02}:{:02}{}", hour12, min, am_pm)
        }
    } else if due_local < now_local {
        "Overdue".to_string()
    } else {
        due_date.format("%b %d").to_string()
    }
}

fn format_overdue_duration(due: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now - due;
    if duration > chrono::Duration::hours(24) {
        let days = duration.num_days() + if duration.num_hours() % 24 > 0 { 1 } else { 0 };
        if days >= 7 {
            let weeks = days / 7;
            format!("{} weeks overdue", weeks)
        } else {
            format!("{} days overdue", days)
        }
    } else {
        let hours = duration.num_hours();
        if hours > 0 {
            format!("{} hours overdue", hours)
        } else {
            let minutes = duration.num_minutes();
            if minutes > 0 {
                format!("{} minutes overdue", minutes)
            } else {
                "less than a minute overdue".to_string()
            }
        }
    }
}

fn format_age(created: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now - created;
    if duration > chrono::Duration::hours(24) {
        let days = duration.num_days() + if duration.num_hours() % 24 > 0 { 1 } else { 0 };
        if days >= 7 {
            let weeks = days / 7;
            format!("added {} weeks ago", weeks)
        } else {
            format!("added {} days ago", days)
        }
    } else {
        let hours = duration.num_hours();
        if hours > 0 {
            format!("added {} hours ago", hours)
        } else {
            let minutes = duration.num_minutes();
            if minutes > 0 {
                format!("added {} minutes ago", minutes)
            } else {
                "added just now".to_string()
            }
        }
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
    Help,
}

struct App {
    todos: Vec<Todo>,
    selected: usize,
    input_mode: InputMode,
    input: String,
    editing_index: Option<usize>,
    cursor_position: usize,
}

impl App {
    fn new() -> Self {
        let todos = load_todos();
        check_notifications(&todos);
        Self {
            todos,
            selected: 0,
            input_mode: InputMode::Normal,
            input: String::new(),
            editing_index: None,
            cursor_position: 0,
        }
    }

    fn save(&self) {
        save_todos(&self.todos);
    }

    fn move_selected_up(&mut self) {
        if self.selected > 0 {
            self.todos.swap(self.selected, self.selected - 1);
            self.selected -= 1;
            self.save();
        }
    }

    fn move_selected_down(&mut self) {
        if self.selected < self.todos.len().saturating_sub(1) {
            self.todos.swap(self.selected, self.selected + 1);
            self.selected += 1;
            self.save();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                        app.move_selected_down();
                    }
                    KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                        app.move_selected_up();
                    }
                    KeyCode::Char('j') if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                        app.move_selected_down();
                    }
                    KeyCode::Char('k') if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) => {
                        app.move_selected_up();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.selected < app.todos.len().saturating_sub(1) {
                            app.selected += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.selected > 0 {
                            app.selected -= 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if let Some(todo) = app.todos.get_mut(app.selected) {
                            todo.completed = !todo.completed;
                            app.save();
                        }
                    }
                    KeyCode::Char('a') => {
                        app.input_mode = InputMode::Adding;
                        app.input.clear();
                        app.cursor_position = 0;
                        app.editing_index = None;
                    }
                    KeyCode::Char('e') => {
                        if let Some(todo) = app.todos.get(app.selected) {
                            app.input_mode = InputMode::Editing;
                            app.input = todo.text.clone();
                            app.cursor_position = app.input.len();
                            app.editing_index = Some(app.selected);
                        }
                    }
                    KeyCode::Char('r') => {
                        app.input_mode = InputMode::Rescheduling;
                        app.input.clear();
                        app.cursor_position = 0;
                        app.editing_index = Some(app.selected);
                    }
                    KeyCode::Char('d') => {
                        if !app.todos.is_empty() {
                            app.todos.remove(app.selected);
                            if app.selected >= app.todos.len() && app.selected > 0 {
                                app.selected -= 1;
                            }
                            app.save();
                        }
                    }
                    KeyCode::Char('s') => {
                        if let Some(todo) = app.todos.get_mut(app.selected) {
                            todo.suspended = !todo.suspended;
                            app.save();
                        }
                    }
                    KeyCode::Char('X') => {
                        app.todos.clear();
                        app.selected = 0;
                        app.save();
                    }
                    KeyCode::Char('?') => {
                        app.input_mode = InputMode::Help;
                    }
                    _ => {}
                },
                InputMode::Adding | InputMode::Editing | InputMode::Rescheduling => match key.code {
                    KeyCode::Enter => {
                        if matches!(app.input_mode, InputMode::Rescheduling) {
                            // Handle rescheduling - allow empty to clear
                            if let Some(index) = app.editing_index
                                && let Some(todo) = app.todos.get_mut(index) {
                                todo.due_date = if app.input.is_empty() {
                                    None
                                } else {
                                    parse_schedule(&app.input)
                                };
                            }
                        } else {
                            // Handle adding/editing - require non-empty
                            if !app.input.is_empty() {
                                let new_due_date = parse_due_date(&app.input);
                                let mut text = app.input.clone();
                                if new_due_date.is_some() {
                                    text = clean_text(&text);
                                }
                                if let Some(index) = app.editing_index {
                                    if let Some(todo) = app.todos.get_mut(index) {
                                        todo.text = text;
                                        todo.due_date = new_due_date.or(todo.due_date);
                                    }
                                } else {
                                    let mut todo = Todo::new(app.todos.len() as u32, text);
                                    todo.due_date = new_due_date;
                                    app.todos.push(todo);
                                }
                            }
                        }
                        app.input_mode = InputMode::Normal;
                        app.editing_index = None;
                        app.save();
                    }
                    KeyCode::Char(c) => {
                        if app.cursor_position <= app.input.len() {
                            app.input.insert(app.cursor_position, c);
                            app.cursor_position += 1;
                        }
                    }
                    KeyCode::Backspace => {
                        if app.cursor_position > 0 {
                            app.cursor_position -= 1;
                            app.input.remove(app.cursor_position);
                        }
                    }
                    KeyCode::Delete => {
                        if app.cursor_position < app.input.len() {
                            app.input.remove(app.cursor_position);
                        }
                    }
                    KeyCode::Left => {
                        if app.cursor_position > 0 {
                            app.cursor_position -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if app.cursor_position < app.input.len() {
                            app.cursor_position += 1;
                        }
                    }
                    KeyCode::Home => {
                        app.cursor_position = 0;
                    }
                    KeyCode::End => {
                        app.cursor_position = app.input.len();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.editing_index = None;
                    }
                    _ => {
                        // Handle Ctrl+W (delete word) and Ctrl+U (delete line)
                        if let KeyCode::Char(c) = key.code {
                            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                match c {
                                    'w' => {
                                        // Delete word backwards
                                        while app.cursor_position > 0 && app.input.chars().nth(app.cursor_position - 1).unwrap_or(' ').is_whitespace() {
                                            app.cursor_position -= 1;
                                            app.input.remove(app.cursor_position);
                                        }
                                        while app.cursor_position > 0 && !app.input.chars().nth(app.cursor_position - 1).unwrap_or(' ').is_whitespace() {
                                            app.cursor_position -= 1;
                                            app.input.remove(app.cursor_position);
                                        }
                                    }
                                    'u' => {
                                        // Delete from cursor to beginning of line
                                        while app.cursor_position > 0 {
                                            app.cursor_position -= 1;
                                            app.input.remove(app.cursor_position);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                },
                InputMode::Help => {
                    app.input_mode = InputMode::Normal;
                },
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3), Constraint::Length(2)].as_ref())
        .split(size);

    let items: Vec<ListItem> = app
        .todos
        .iter()
        .enumerate()
        .map(|(i, todo)| {
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
            let is_overdue = if let Some(due) = todo.due_date {
                due < Utc::now() && !todo.completed && !todo.suspended
            } else {
                false
            };
            if is_overdue {
                if i == app.selected {
                    style = style.fg(Color::Black);
                } else {
                    style = style.fg(Color::Red);
                }
            } else if todo.completed && i == app.selected {
                style = style.fg(Color::Black);
            }
            let main_content = format!("{} {}", prefix, todo.text);

            let mut spans = vec![Span::styled(main_content, style)];

            if let Some(_due) = todo.due_date {
                if !todo.completed {
                    let icon = if is_overdue { "❗" } else { "⏰" }; // Warning triangle for overdue, clock for scheduled
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(icon, Style::default()));
                }
            }

            let line = Line::from(spans);
            let item_style = if i == app.selected {
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
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("TODOs ({})", pending_count)).padding(Padding::new(1, 1, 1, 1)))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(list, chunks[0]);

    // Split the bottom chunk into left and right for status and version
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(10)].as_ref())
        .split(chunks[2]);

    // Create status line with styled keys
    let status_spans = vec![
        Span::raw(" "),  // Add left padding
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
    let status_line = Line::from(status_spans);
    let status = Paragraph::new(status_line)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(status, bottom_chunks[0]);

    // Version in bottom right
    let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version = Paragraph::new(version_text)
        .style(Style::default().fg(Color::White))
        .alignment(ratatui::layout::Alignment::Right)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(version, bottom_chunks[1]);

    if let InputMode::Adding | InputMode::Editing | InputMode::Rescheduling = app.input_mode {
        let title = match app.input_mode {
            InputMode::Adding => "Add Todo",
            InputMode::Editing => "Edit Todo",
            InputMode::Rescheduling => "Reschedule Todo",
            _ => "Input",
        };
        let input = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(title).padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(input, chunks[1]);
        f.set_cursor(chunks[1].x + 2 + app.cursor_position as u16, chunks[1].y + 1);
    } else {
        // Show metadata for selected todo
        let (metadata, color) = if let Some(todo) = app.todos.get(app.selected) {
            let is_overdue = if let Some(due) = todo.due_date {
                due < Utc::now() && !todo.completed && !todo.suspended
            } else {
                false
            };

            let time_info = if let Some(created) = todo.created_at {
                if is_overdue && todo.due_date.is_some() {
                    format!(" ({})", format_overdue_duration(todo.due_date.unwrap()))
                } else {
                    format!(" ({})", format_age(created))
                }
            } else {
                "".to_string()
            };

            if todo.completed {
                (format!("Status: Completed{}", time_info), Color::Green)
            } else if todo.suspended {
                (format!("Status: Suspended{}", time_info), Color::Yellow)
            } else if let Some(due) = todo.due_date {
                (format!("Scheduled for: {}{}", format_due_time(due), time_info), if is_overdue { Color::Red } else { Color::Green })
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

    if app.input_mode == InputMode::Help {
        let help_text = "Keyboard Commands:\n\nNavigation:\n  ↑↓ / jk : Navigate items\n  Shift+↑↓ / Shift+jk : Reorder items\n\nActions:\n  Space : Toggle completed\n  a : Add new todo\n  e : Edit selected todo\n  r : Reschedule selected todo\n  d : Delete selected todo\n  s : Suspend selected todo\n  X : Clear all todos\n\nOther:\n  ? : Show this help\n  q : Quit application\n\nPress any key to close";
        let help_paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Help").padding(Padding::new(1, 1, 1, 1)));

        let area = centered_rect(60, 80, f.size());
        f.render_widget(help_paragraph, area);
    }
}
