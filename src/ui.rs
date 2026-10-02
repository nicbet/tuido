use crate::app::{App, InputMode};
use crate::format::{format_age, format_due_time, format_overdue_duration};
use crate::model::Todo;
use chrono::{DateTime, Local, Utc};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
};

const HELP_TEXT: &str = "Keyboard Commands:\n\nNavigation:\n  ↑↓ / jk : Navigate items\n  Shift+↑↓ / JK : Reorder items\n\nActions:\n  Space : Toggle completed\n  a : Add new todo\n  e : Edit selected todo\n  r : Reschedule selected todo\n  d : Delete selected todo\n  s : Suspend selected todo\n  h : Toggle hide completed\n  X : Clear all todos\n\nOther:\n  ? : Show this help\n  q : Quit application\n\nPress any key to close";

pub fn ui(f: &mut Frame, app: &App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3), Constraint::Length(2)])
        .split(size);

    render_list(f, app, chunks[0]);
    if app.input_mode.is_text_entry() {
        render_input(f, app, chunks[1]);
    } else {
        render_info(f, app, chunks[1]);
    }
    render_status_bar(f, chunks[2]);

    match app.input_mode {
        InputMode::ConfirmClear => render_confirm_clear(f, app, size),
        InputMode::Help => render_help(f, size),
        _ => {}
    }
}

/// How a todo is presented. Precedence: Completed > Suspended > Overdue > Scheduled > Plain.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayState {
    Completed,
    Suspended,
    Overdue,
    Scheduled,
    Plain,
}

fn display_state(todo: &Todo, now: DateTime<Utc>) -> DisplayState {
    if todo.completed {
        DisplayState::Completed
    } else if todo.suspended {
        DisplayState::Suspended
    } else if todo.is_overdue(now) {
        DisplayState::Overdue
    } else if todo.due_date.is_some() {
        DisplayState::Scheduled
    } else {
        DisplayState::Plain
    }
}

impl DisplayState {
    fn prefix(self) -> &'static str {
        match self {
            DisplayState::Completed => "[x]",
            DisplayState::Suspended => "[~]",
            DisplayState::Overdue | DisplayState::Scheduled | DisplayState::Plain => "[ ]",
        }
    }

    /// Style of the todo text. The span's foreground wins over the row style,
    /// so selected states that change the row background must also set it here.
    fn text_style(self, selected: bool) -> Style {
        let style = match self {
            DisplayState::Completed => Style::default().add_modifier(Modifier::CROSSED_OUT).fg(Color::DarkGray),
            DisplayState::Suspended => Style::default().add_modifier(Modifier::ITALIC).fg(Color::Yellow),
            DisplayState::Overdue => Style::default().fg(Color::Red),
            DisplayState::Scheduled | DisplayState::Plain => Style::default(),
        };
        match self {
            DisplayState::Completed | DisplayState::Suspended | DisplayState::Overdue if selected => {
                style.fg(Color::Black)
            }
            _ => style,
        }
    }

    /// Style of the whole row (background highlight when selected).
    fn row_style(self, selected: bool) -> Style {
        if !selected {
            return Style::default();
        }
        match self {
            DisplayState::Completed => Style::default().bg(Color::DarkGray).fg(Color::White),
            DisplayState::Suspended => Style::default().bg(Color::Yellow).fg(Color::Black),
            DisplayState::Overdue => Style::default().bg(Color::Red).fg(Color::Black),
            DisplayState::Scheduled | DisplayState::Plain => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    fn icon(self) -> Option<&'static str> {
        match self {
            DisplayState::Overdue => Some("❗"),
            DisplayState::Scheduled => Some("⏰"),
            DisplayState::Completed | DisplayState::Suspended | DisplayState::Plain => None,
        }
    }

    fn info_color(self) -> Color {
        match self {
            DisplayState::Completed | DisplayState::Scheduled => Color::Green,
            DisplayState::Suspended => Color::Yellow,
            DisplayState::Overdue => Color::Red,
            DisplayState::Plain => Color::White,
        }
    }
}

fn todo_item(todo: &Todo, is_selected: bool, now: DateTime<Utc>) -> ListItem<'_> {
    let state = display_state(todo, now);
    let mut spans = vec![Span::styled(
        format!("{} {}", state.prefix(), todo.text),
        state.text_style(is_selected),
    )];
    if let Some(icon) = state.icon() {
        spans.push(Span::raw(" "));
        spans.push(Span::raw(icon));
    }
    ListItem::new(Line::from(spans)).style(state.row_style(is_selected))
}

fn list_title(app: &App) -> String {
    let completed = app.todos.iter().filter(|t| t.completed).count();
    let suspended = app.todos.iter().filter(|t| !t.completed && t.suspended).count();
    let actionable = app.todos.len() - completed - suspended;

    let mut title = format!("TODOs ({})", actionable);
    if suspended > 0 {
        title.push_str(&format!(" · {} suspended", suspended));
    }
    if app.hide_completed && completed > 0 {
        title.push_str(&format!(" / {} hidden", completed));
    }
    title
}

fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let now = Utc::now();
    let visible = app.visible_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| todo_item(&app.todos[i], i == app.selected, now))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(list_title(app))
                .padding(Padding::new(1, 1, 1, 1)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    // A ListState keeps the selected item scrolled into view on long lists.
    let mut list_state = ListState::default().with_selected(visible.iter().position(|&i| i == app.selected));
    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
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
    let inner_width = area.width.saturating_sub(4);
    let cursor_col = Line::raw(app.input.before_cursor()).width() as u16;
    let scroll = cursor_col.saturating_sub(inner_width.saturating_sub(1));
    let input = Paragraph::new(app.input.value())
        .style(Style::default().fg(Color::Yellow))
        .scroll((0, scroll))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .padding(Padding::new(1, 1, 0, 0)),
        );
    f.render_widget(input, area);
    f.set_cursor_position((area.x + 2 + cursor_col - scroll, area.y + 1));
}

/// Shows metadata for the selected todo, or a pending message.
fn render_info(f: &mut Frame, app: &App, area: Rect) {
    let now = Utc::now();
    let (metadata, color) = if let Some(msg) = &app.message {
        (msg.clone(), Color::Red)
    } else if let Some(todo) = app.selected_todo() {
        let state = display_state(todo, now);
        let time_info = match (todo.due_date, todo.created_at) {
            (Some(due), _) if state == DisplayState::Overdue => format!(" ({})", format_overdue_duration(due, now)),
            (_, Some(created)) => format!(" ({})", format_age(created, now)),
            _ => String::new(),
        };
        let due = || todo.due_date.map(|due| format_due_time(due, Local::now()));

        let text = match state {
            DisplayState::Completed => format!("Status: Completed{}", time_info),
            DisplayState::Suspended => match due() {
                Some(due) => format!("Status: Suspended · due {}{}", due, time_info),
                None => format!("Status: Suspended{}", time_info),
            },
            DisplayState::Overdue | DisplayState::Scheduled | DisplayState::Plain => match due() {
                Some(due) => format!("Scheduled for: {}{}", due, time_info),
                None => format!("No schedule set{}", time_info),
            },
        };
        (text, state.info_color())
    } else {
        ("No todos".to_string(), Color::White)
    };
    let metadata_paragraph = Paragraph::new(metadata).style(Style::default().fg(color)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Info")
            .padding(Padding::new(1, 1, 0, 0)),
    );
    f.render_widget(metadata_paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect) {
    // Split into left and right for key hints and version
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(10)])
        .split(area);

    let key = |k| Span::styled(k, Style::default().fg(Color::Red));
    let status_spans = vec![
        Span::raw(" "), // Add left padding
        key("↑↓"),
        Span::raw("/"),
        key("jk"),
        Span::raw(": navigate  "),
        key("Space"),
        Span::raw(": toggle  "),
        key("a"),
        Span::raw(": add  "),
        key("e"),
        Span::raw(": edit  "),
        key("d"),
        Span::raw(": delete  "),
        key("?"),
        Span::raw(": help  "),
        key("q"),
        Span::raw(": quit  "),
    ];
    let status = Paragraph::new(Line::from(status_spans)).block(Block::default().borders(Borders::TOP));
    f.render_widget(status, bottom_chunks[0]);

    let version = Paragraph::new(format!("v{}", env!("CARGO_PKG_VERSION")))
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(version, bottom_chunks[1]);
}

fn render_confirm_clear(f: &mut Frame, app: &App, size: Rect) {
    let prompt = Paragraph::new(format!(
        "Delete all {} todos? This cannot be undone.\n\ny: yes, any other key: cancel",
        app.todos.len()
    ))
    .style(Style::default().fg(Color::Red))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Clear All")
            .padding(Padding::new(1, 1, 1, 1)),
    );
    let area = centered_rect(50, 30, size);
    f.render_widget(Clear, area);
    f.render_widget(prompt, area);
}

fn render_help(f: &mut Frame, size: Rect) {
    let help_paragraph = Paragraph::new(HELP_TEXT)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .padding(Padding::new(1, 1, 1, 1)),
        );
    let area = centered_rect(60, 80, size);
    f.render_widget(Clear, area);
    f.render_widget(help_paragraph, area);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|f| ui(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_todos_and_hides_completed() {
        let mut app = App::new(
            vec![Todo::new(0, "visible".to_string()), Todo::new(1, "done".to_string())],
            None,
        );
        app.todos[1].completed = true;
        let screen = render(&app);
        assert!(screen.contains("[ ] visible"));
        assert!(screen.contains("[x] done"));
        assert!(screen.contains("TODOs (1)"));

        app.hide_completed = true;
        let screen = render(&app);
        assert!(!screen.contains("done"));
        assert!(screen.contains("1 hidden"));
    }

    /// (fg, bg, modifiers) of the first cell of the list row that contains `text`.
    fn row_style(app: &App, text: &str) -> (Option<Color>, Option<Color>, Modifier) {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|f| ui(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect();
            if let Some(col) = row.find(&format!("] {}", text)) {
                // `col` is a byte offset of "] "; step back over "[x" to the '['.
                let x = (row[..col].chars().count() as u16)
                    .checked_sub(2)
                    .expect("prefix '[' should be left of the match");
                let style = buffer[(x, y)].style();
                return (style.fg, style.bg, style.add_modifier);
            }
        }
        panic!("row {:?} not rendered", text);
    }

    fn app_with_states() -> App {
        let past = Utc::now() - chrono::Duration::hours(1);
        let future = Utc::now() + chrono::Duration::hours(1);
        let mut todos: Vec<Todo> = ["plain", "scheduled", "overdue", "done", "parked"]
            .iter()
            .enumerate()
            .map(|(i, t)| Todo::new(i as u32, t.to_string()))
            .collect();
        todos[1].due_date = Some(future);
        todos[2].due_date = Some(past);
        todos[3].completed = true;
        todos[4].suspended = true;
        todos[4].due_date = Some(past);
        App::new(todos, None)
    }

    #[test]
    fn display_state_precedence() {
        let now = Utc::now();
        let past = Some(now - chrono::Duration::hours(1));
        let future = Some(now + chrono::Duration::hours(1));
        let state = |completed, suspended, due_date| {
            let mut todo = Todo::new(0, "x".to_string());
            todo.completed = completed;
            todo.suspended = suspended;
            todo.due_date = due_date;
            display_state(&todo, now)
        };
        assert_eq!(state(true, true, past), DisplayState::Completed);
        assert_eq!(state(true, false, past), DisplayState::Completed);
        assert_eq!(state(false, true, past), DisplayState::Suspended);
        assert_eq!(state(false, true, None), DisplayState::Suspended);
        assert_eq!(state(false, false, past), DisplayState::Overdue);
        assert_eq!(state(false, false, future), DisplayState::Scheduled);
        assert_eq!(state(false, false, None), DisplayState::Plain);
    }

    #[test]
    fn unselected_row_styles() {
        let mut app = app_with_states();
        app.selected = usize::MAX; // nothing selected
        let style = |fg, modifier| (Some(fg), Some(Color::Reset), modifier);
        assert_eq!(row_style(&app, "plain"), style(Color::Reset, Modifier::empty()));
        assert_eq!(row_style(&app, "scheduled"), style(Color::Reset, Modifier::empty()));
        assert_eq!(row_style(&app, "overdue"), style(Color::Red, Modifier::empty()));
        assert_eq!(row_style(&app, "done"), style(Color::DarkGray, Modifier::CROSSED_OUT));
        assert_eq!(row_style(&app, "parked"), style(Color::Yellow, Modifier::ITALIC));
    }

    #[test]
    fn selected_row_styles() {
        let mut app = app_with_states();
        let mut selected = |i: usize, text: &str| {
            app.selected = i;
            row_style(&app, text)
        };
        let bold = Modifier::BOLD; // List highlight_style
        let style = |fg, bg, modifier| (Some(fg), Some(bg), bold | modifier);
        assert_eq!(
            selected(0, "plain"),
            style(Color::Reset, Color::Reset, Modifier::REVERSED)
        );
        assert_eq!(
            selected(1, "scheduled"),
            style(Color::Reset, Color::Reset, Modifier::REVERSED)
        );
        assert_eq!(
            selected(2, "overdue"),
            style(Color::Black, Color::Red, Modifier::empty())
        );
        assert_eq!(
            selected(3, "done"),
            style(Color::Black, Color::DarkGray, Modifier::CROSSED_OUT)
        );
        assert_eq!(
            selected(4, "parked"),
            style(Color::Black, Color::Yellow, Modifier::ITALIC)
        );
    }

    #[test]
    fn icons_by_state() {
        let screen = render(&app_with_states());
        let line = |text: &str| screen.lines().find(|l| l.contains(text)).unwrap().to_string();
        assert!(line("scheduled").contains('⏰'));
        assert!(line("overdue").contains('❗'));
        assert!(!line("parked").contains('⏰') && !line("parked").contains('❗'));
        assert!(line("parked").contains("[~] parked"));
        assert!(!line("done").contains('⏰'));
    }

    #[test]
    fn title_counts_actionable_and_suspended() {
        let mut app = app_with_states();
        assert_eq!(list_title(&app), "TODOs (3) · 1 suspended");
        app.hide_completed = true;
        assert_eq!(list_title(&app), "TODOs (3) · 1 suspended / 1 hidden");
        // A completed+suspended todo counts as completed only.
        app.todos[4].completed = true;
        assert_eq!(list_title(&app), "TODOs (3) / 2 hidden");
    }

    #[test]
    fn info_panel_shows_due_date_for_suspended() {
        let mut app = app_with_states();
        app.selected = 4;
        let screen = render(&app);
        assert!(screen.contains("Status: Suspended · due "));
    }

    #[test]
    fn renders_empty_state() {
        let screen = render(&App::new(Vec::new(), None));
        assert!(screen.contains("No todos"));
    }
}
