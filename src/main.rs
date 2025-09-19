use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use regex::Regex;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
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
    let local_offset = Local::now().offset().clone();
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
    if let Some(cap) = re_24.captures(text) {
        if let (Ok(hour), Ok(min)) = (cap[1].parse::<u32>(), cap[2].parse::<u32>()) {
            if hour < 24 && min < 60 {
                return chrono::NaiveTime::from_hms_opt(hour, min, 0);
            }
        }
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
        let local_offset = Local::now().offset().clone();
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
        if let Some(due) = todo.due_date {
            if due <= now && !todo.completed && !todo.suspended {
                Notification::new()
                    .summary("Todo Reminder")
                    .body(&format!("Reminder: {}", todo.text))
                    .show()
                    .unwrap();
            }
        }
    }
}

fn format_due_time(due: DateTime<Utc>) -> String {
    let local_offset = Local::now().offset().clone();
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

enum InputMode {
    Normal,
    Adding,
    Editing,
    Rescheduling,
}

struct App {
    todos: Vec<Todo>,
    selected: usize,
    input_mode: InputMode,
    input: String,
    editing_index: Option<usize>,
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
        }
    }

    fn save(&self) {
        save_todos(&self.todos);
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
                        app.editing_index = None;
                    }
                    KeyCode::Char('e') => {
                        if let Some(todo) = app.todos.get(app.selected) {
                            app.input_mode = InputMode::Editing;
                            app.input = todo.text.clone();
                            app.editing_index = Some(app.selected);
                        }
                    }
                    KeyCode::Char('r') => {
                        app.input_mode = InputMode::Rescheduling;
                        app.input.clear();
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
                    _ => {}
                },
                InputMode::Adding | InputMode::Editing | InputMode::Rescheduling => match key.code {
                    KeyCode::Enter => {
                        if matches!(app.input_mode, InputMode::Rescheduling) {
                            // Handle rescheduling - allow empty to clear
                            if let Some(index) = app.editing_index {
                                if let Some(todo) = app.todos.get_mut(index) {
                                    todo.due_date = if app.input.is_empty() {
                                        None
                                    } else {
                                        parse_schedule(&app.input)
                                    };
                                }
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
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.editing_index = None;
                    }
                    _ => {}
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
                Style::default().add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default()
            };
            let is_overdue = if let Some(due) = todo.due_date {
                due < Utc::now() && !todo.completed && !todo.suspended
            } else {
                false
            };
            if is_overdue {
                style = style.fg(Color::Red);
            }
            let main_content = format!("{} {}", prefix, todo.text);

            let mut spans = vec![Span::styled(main_content, style)];

            if let Some(_due) = todo.due_date {
                let icon = "⏰"; // Clock icon
                spans.push(Span::raw(" "));
                spans.push(Span::styled(icon, Style::default().fg(if is_overdue { Color::Red } else { Color::Cyan })));
            }

            let line = Line::from(spans);
            ListItem::new(line).style(if i == app.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            })
        })
        .collect();

    let pending_count = app.todos.iter().filter(|t| !t.completed).count();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("TODOs ({})", pending_count)).padding(Padding::new(1, 1, 1, 1)))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(list, chunks[0]);

    let status_text = "↑↓/jk: navigate  Space: toggle  a: add  e: edit  r: reschedule  d: delete  s: suspend  q: quit";
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(status, chunks[2]);

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
        f.set_cursor(chunks[1].x + 2 + app.input.len() as u16, chunks[1].y + 1);
    } else {
        // Show metadata for selected todo
        let (metadata, color) = if let Some(todo) = app.todos.get(app.selected) {
            let age = if let Some(created) = todo.created_at {
                format!(" ({})", format_age(created))
            } else {
                "".to_string()
            };
            if todo.completed {
                (format!("Status: Completed{}", age), Color::Green)
            } else if todo.suspended {
                (format!("Status: Suspended{}", age), Color::Yellow)
            } else if let Some(due) = todo.due_date {
                let is_overdue = due < Utc::now();
                (format!("Scheduled for: {}{}", format_due_time(due), age), if is_overdue { Color::Red } else { Color::Green })
            } else {
                (format!("No schedule set{}", age), Color::White)
            }
        } else {
            ("No todos".to_string(), Color::White)
        };
        let metadata_paragraph = Paragraph::new(metadata)
            .style(Style::default().fg(color))
            .block(Block::default().borders(Borders::ALL).title("Info").padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(metadata_paragraph, chunks[1]);
    }
}
