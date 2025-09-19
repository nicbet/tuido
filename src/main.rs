use chrono::{DateTime, Utc};
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
    let lower = text.to_lowercase();
    let now = Utc::now();
    if lower.contains("tomorrow") {
        let tomorrow = now + chrono::Duration::days(1);
        if let Some(time_str) = extract_time(&lower) {
            if let Ok(time) = chrono::NaiveTime::parse_from_str(&time_str, "%I:%M%p") {
                let datetime = tomorrow.date_naive().and_time(time).and_utc();
                return Some(datetime);
            }
        }
        Some(tomorrow.date_naive().and_hms_opt(9, 0, 0)?.and_utc())
    } else if lower.contains("at ") {
        if let Some(time_str) = extract_time(&lower) {
            if let Ok(time) = chrono::NaiveTime::parse_from_str(&time_str, "%I:%M%p") {
                let datetime = now.date_naive().and_time(time);
                let datetime = if datetime < now.naive_utc() {
                    datetime + chrono::Duration::days(1)
                } else {
                    datetime
                };
                return Some(datetime.and_utc());
            }
        }
        None
    } else {
        None
    }
}

fn extract_time(text: &str) -> Option<String> {
    if let Some(start) = text.find("at ") {
        let after_at = &text[start + 3..];
        let words: Vec<&str> = after_at.split_whitespace().collect();
        if !words.is_empty() {
            let time = words[0];
            if time.contains(':') || time.ends_with("am") || time.ends_with("pm") {
                return Some(time.to_string());
            }
        }
    }
    None
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

enum InputMode {
    Normal,
    Adding,
    Editing,
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
                InputMode::Adding | InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        if !app.input.is_empty() {
                            let due_date = parse_due_date(&app.input);
                            let text = if due_date.is_some() {
                                // Remove time part from text
                                let lower = app.input.to_lowercase();
                                if let Some(at_pos) = lower.find("at ") {
                                    app.input[..at_pos].trim().to_string()
                                } else if lower.contains("tomorrow") {
                                    app.input.replace("tomorrow", "").trim().to_string()
                                } else {
                                    app.input.clone()
                                }
                            } else {
                                app.input.clone()
                            };
                            if let Some(index) = app.editing_index {
                                // Editing existing
                                if let Some(todo) = app.todos.get_mut(index) {
                                    todo.text = text;
                                    todo.due_date = due_date;
                                }
                            } else {
                                // Adding new
                                let mut todo = Todo::new(app.todos.len() as u32, text);
                                todo.due_date = due_date;
                                app.todos.push(todo);
                            }
                            app.save();
                        }
                        app.input_mode = InputMode::Normal;
                        app.editing_index = None;
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
        .constraints(
            match app.input_mode {
                InputMode::Normal => [Constraint::Percentage(100)].as_ref(),
                InputMode::Adding | InputMode::Editing => [Constraint::Min(1), Constraint::Length(3)].as_ref(),
            }
        )
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
            let content = format!("{} {}", prefix, todo.text);
            let line = Line::from(vec![Span::styled(content, style)]);
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

    if let InputMode::Adding | InputMode::Editing = app.input_mode {
        let title = if app.editing_index.is_some() { "Edit Todo" } else { "Add Todo" };
        let input = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(title).padding(Padding::new(1, 1, 0, 0)));
        f.render_widget(input, chunks[1]);
        f.set_cursor(chunks[1].x + 2 + app.input.len() as u16, chunks[1].y + 1);
    }
}
