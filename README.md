# Tuido

A TUI todo app built with Rust, featuring a text-based user interface for managing tasks, scheduling, and notifications.

## Features

- **Task Management**: Add, edit, delete, and toggle completion of todos
- **Scheduling**: Set due dates using natural language (e.g., "tomorrow at 9pm", "3pm")
- **Notifications**: Desktop notifications for overdue tasks
- **Suspend Tasks**: Temporarily suspend tasks without deleting them
- **Persistent Storage**: Todos are saved to `~/.config/tuido/todos.json`
- **Keyboard-Driven**: Efficient navigation and editing with keyboard shortcuts

## Prerequisites

- Rust (latest stable version recommended)
- Linux/macOS/Windows with terminal support

## Installation

### From Source

1. Clone the repository:

   ```bash
   git clone https://github.com/nicbet/tuido.git
   cd tuido
   ```

2. Build and install:

   ```bash
   make install
   ```

   Or manually:

   ```bash
   cargo build --release
   sudo cp target/release/tuido /usr/local/bin/
   ```

### Using Cargo

```bash
cargo install --git https://github.com/nicbet/tuido.git
```

## Building

### Release Build

```bash
make build
# or
cargo build --release
```

### Debug Build

```bash
make debug
# or
cargo build
```

## Local Development

1. Run the application:

   ```bash
   make run
   # or
   cargo run
   ```

2. Run tests:

   ```bash
   make test
   # or
   cargo test
   ```

3. Lint the code:

   ```bash
   make lint
   # or
   cargo clippy -- -D warnings
   ```

4. Check code without building:

   ```bash
   make check
   # or
   cargo check
   ```

5. Clean build artifacts:

   ```bash
   make clean
   # or
   cargo clean
   ```

## Usage

Run `tuido` in your terminal to start the application.

### Keybindings

- **Navigation**:
  - `↑/↓` or `j/k`: Move up/down in the list
  - `Space`: Toggle completion of selected task
  - `s`: Suspend/unsuspend selected task

- **Task Management**:
  - `a`: Add new task
  - `e`: Edit selected task
  - `d`: Delete selected task
  - `r`: Reschedule selected task

- **General**:
  - `q`: Quit the application

### Adding Tasks with Due Dates

When adding or editing a task, you can include scheduling information using natural language:

- "Buy groceries tomorrow at 5pm"
- "Call dentist 3pm"
- "Finish report by Friday"

The app will parse the time and set appropriate due dates. Times can be in 12-hour (9pm) or 24-hour (21:30) format.

### Notifications

The app checks for overdue tasks on startup and shows desktop notifications. Ensure your system supports notifications (Linux: notify-send, macOS: built-in, Windows: toast notifications).

## Configuration

Todos are stored in `~/.config/tuido/todos.json`. The configuration directory is created automatically on first run.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and linting
5. Submit a pull request

## License

MIT
