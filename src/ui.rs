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

fn todo_item(todo: &Todo, is_selected: bool, now: DateTime<Utc>) -> ListItem<'_> {
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

    let mut spans = vec![Span::styled(format!("{} {}", prefix, todo.text), style)];

    if todo.due_date.is_some() && !todo.completed {
        let icon = if is_overdue { "❗" } else { "⏰" }; // Exclamation for overdue, clock for scheduled
        spans.push(Span::raw(" "));
        spans.push(Span::styled(icon, Style::default()));
    }

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
    ListItem::new(Line::from(spans)).style(item_style)
}

fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let now = Utc::now();
    let visible = app.visible_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| todo_item(&app.todos[i], i == app.selected, now))
        .collect();

    let pending_count = app.todos.iter().filter(|t| !t.completed).count();
    let completed_count = app.todos.len() - pending_count;

    let title = if app.hide_completed && completed_count > 0 {
        format!("TODOs ({}) / {} hidden", pending_count, completed_count)
    } else {
        format!("TODOs ({})", pending_count)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
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

    #[test]
    fn renders_empty_state() {
        let screen = render(&App::new(Vec::new(), None));
        assert!(screen.contains("No todos"));
    }
}
