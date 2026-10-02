mod app;
mod cli;
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
use std::path::PathBuf;
use std::time::Duration;

/// How often the UI refreshes (overdue colors, notifications) without input.
const TICK_RATE: Duration = Duration::from_secs(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_file = match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Command::Run { file }) => file,
        Ok(cli::Command::Help) => {
            print!("{}", cli::HELP);
            return Ok(());
        }
        Ok(cli::Command::Version) => {
            println!("tuido {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Err(e) => {
            eprintln!("tuido: {}\nTry 'tuido --help' for more information.", e);
            std::process::exit(2);
        }
    };

    // Resolve and load before touching the terminal so errors print normally.
    let path = todo_file(cli_file).unwrap_or_else(|e| exit_with_error(&e));
    let todos = storage::load_todos(&path).unwrap_or_else(|e| {
        exit_with_error(&format!(
            "{}\nRefusing to start so the file is not overwritten; fix or move it and retry.",
            e
        ))
    });
    let mut app = App::new(todos, Some(path));

    // `ratatui::init` also installs a panic hook that restores the terminal.
    let mut terminal = ratatui::init();
    let res = run_app(&mut terminal, &mut app);
    ratatui::restore();
    res?;
    Ok(())
}

fn exit_with_error(message: &str) -> ! {
    eprintln!("tuido: {}", message);
    std::process::exit(1);
}

/// Resolves the todo file from the CLI flag, environment, and defaults.
fn todo_file(cli_file: Option<PathBuf>) -> Result<PathBuf, String> {
    let env = storage::PathEnv {
        var: &|name| std::env::var_os(name),
        home: dirs::home_dir(),
        exists: &|path| path.exists(),
    };
    let (path, source) = storage::resolve_todos_path(cli_file, &env).map_err(|e| e.to_string())?;
    if source.is_explicit() {
        storage::validate_explicit_path(&path)
    } else {
        Ok(path)
    }
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
