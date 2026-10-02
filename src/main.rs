mod app;
mod format;
mod model;
mod schedule;
mod storage;
mod text_input;
mod ui;

use app::App;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use std::io;
use std::time::Duration;

/// How often the UI refreshes (overdue colors, notifications) without input.
const TICK_RATE: Duration = Duration::from_secs(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load before touching the terminal so errors print normally.
    let path = storage::todos_path()?;
    let todos = match storage::load_todos(&path) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("tuido: {}", e);
            eprintln!("Refusing to start so the file is not overwritten; fix or move it and retry.");
            std::process::exit(1);
        }
    };
    let mut app = App::new(todos, Some(path));

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
        terminal.draw(|f| ui::ui(f, app))?;

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
