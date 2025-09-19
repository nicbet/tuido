use chrono::{DateTime, Local, Timelike, Utc};
use chrono_english::Dialect;
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
}

impl Todo {
    pub fn new(id: u32, text: String) -> Self {
        Self {
            id,
            text,
            completed: false,
            suspended: false,
            due_date: None,
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
    // Try chrono-english for fuzzy parsing
    if let Ok(dt_local) = chrono_english::parse_date_string(text, Local::now(), Dialect::Us) {
        return Some(dt_local.with_timezone(&Utc));
    }

    // Fallback to regex for specific cases
    let lower = text.to_lowercase();
    let now = Utc::now();

    // Extract time anywhere in text
    if let Some(time) = extract_time_anywhere(&lower) {
        let datetime = now.date_naive().and_time(time);
        let datetime = if datetime < now.naive_utc() {
            datetime + chrono::Duration::days(1)
        } else {
            datetime
        };
        return Some(datetime.and_utc());
    }

    None
}

fn extract_time_anywhere(text: &str) -> Option<chrono::NaiveTime> {
    // 24-hour format: 21:35
    let re_24 = Regex::new(r"\b(\d{1,2}):(\d{2})\b").unwrap();
    if let Some(cap) = re_24.captures(text) {
        if let (Ok(hour), Ok(min)) = (cap[1].parse::<u32>(), cap[2].parse::<u32>()) {
            if hour < 24 && min < 60 {
                return chrono::NaiveTime::from_hms_opt(hour, min, 0);
            }
        }
    }

    // 12-hour format: 9pm, 11:30am
    let re_12 = Regex::new(r"\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b").unwrap();
    if let Some(cap) = re_12.captures(text) {
        let hour: u32 = cap[1].parse().unwrap();
        let min: u32 = cap.get(2).map_or(0, |m| m.as_str().parse().unwrap());
        let am_pm = &cap[3];
        let hour_24 = if am_pm == "pm" && hour != 12 { hour + 12 } else if am_pm == "am" && hour == 12 { 0 } else { hour };
        if hour_24 < 24 && min < 60 {
            return chrono::NaiveTime::from_hms_opt(hour_24, min, 0);
        }
    }

    None
}

fn parse_schedule(input: &str) -> Option<DateTime<Utc>> {
    // Try chrono-english
    if let Ok(dt_local) = chrono_english::parse_date_string(input, Local::now(), Dialect::Us) {
        return Some(dt_local.with_timezone(&Utc));
    }

    // Fallback to regex
    if let Some(time) = extract_time_anywhere(input) {
        let now = Utc::now();
        let datetime = now.date_naive().and_time(time);
        let datetime = if datetime < now.naive_utc() {
            datetime + chrono::Duration::days(1)
        } else {
            datetime
        };
        Some(datetime.and_utc())
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
    let now_local = Local::now();
    let today = now_local.date_naive();
    let due_local = due.with_timezone(&Local);
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
                                let due_date = parse_due_date(&app.input);
                                let text = app.input.clone();
                                if let Some(index) = app.editing_index {
                                    if let Some(todo) = app.todos.get_mut(index) {
                                        todo.text = text;
                                        todo.due_date = due_date;
                                    }
                                } else {
                                    let mut todo = Todo::new(app.todos.len() as u32, text);
                                    todo.due_date = due_date;
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
            let style = if todo.completed {
                Style::default().add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default()
            };
            let main_content = format!("{} {}", prefix, todo.text);

            let mut spans = vec![Span::styled(main_content, style)];

            if let Some(_due) = todo.due_date {
                let icon = "⏰"; // Clock icon
                spans.push(Span::raw(" "));
                spans.push(Span::styled(icon, Style::default().fg(Color::Cyan)));
            }

            let line = Line::from(spans);
            ListItem::new(line).style(if i == app.selected {
                Style::default().bg(Color::Blue)
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
        let metadata = if let Some(todo) = app.todos.get(app.selected) {
            if todo.completed {
                "Status: Completed".to_string()
            } else if todo.suspended {
                "Status: Suspended".to_string()
            } else if let Some(due) = todo.due_date {
                format!("Scheduled for: {}", format_due_time(due))
            } else {
                "No schedule set".to_string()
            }
        } else {
            "No todos".to_string()
        };
        let metadata_paragraph = Paragraph::new(metadata)
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::ALL).title("Info").padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(metadata_paragraph, chunks[1]);
    }
}
