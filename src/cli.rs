//! Command-line argument parsing.

use std::ffi::OsString;
use std::path::PathBuf;

pub const HELP: &str = "\
tuido - a keyboard-driven TUI todo list

Usage: tuido [OPTIONS]

Options:
  -f, --file <PATH>  Use this todo file instead of the default
  -h, --help         Print help
  -V, --version      Print version

Todo file location (first match wins):
  1. --file <PATH>
  2. $TUIDO_FILE
  3. $XDG_CONFIG_HOME/tuido/todos.json
     (if that file doesn't exist but the one in 4. does, 4. is used)
  4. ~/.config/tuido/todos.json

Press ? inside the app for keyboard shortcuts.
";

#[derive(Debug, PartialEq)]
pub enum Command {
    Run { file: Option<PathBuf> },
    Help,
    Version,
}

/// Parses arguments (without the program name). `--help` and `--version` win
/// as soon as they are seen; a repeated `--file` uses the last value.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut file = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some(flag @ ("-f" | "--file")) => {
                let value = args.next().filter(|v| !v.is_empty());
                let value = value.ok_or_else(|| format!("{} requires a path", flag))?;
                // `-f --help` is far more likely a slip than a file named `--help`.
                if value.as_encoded_bytes().starts_with(b"-") {
                    let value = value.to_string_lossy();
                    return Err(format!(
                        "{} requires a path, got '{}' (use ./{} for a file starting with '-')",
                        flag, value, value
                    ));
                }
                file = Some(PathBuf::from(value));
            }
            // `--file=PATH` is only split for UTF-8 arguments; non-UTF-8 paths
            // work with the separate-argument forms above.
            Some(s) if s.starts_with("--file=") => {
                let value = &s["--file=".len()..];
                if value.is_empty() {
                    return Err("--file requires a path".to_string());
                }
                file = Some(PathBuf::from(value));
            }
            _ => return Err(format!("unexpected argument '{}'", arg.to_string_lossy())),
        }
    }
    Ok(Command::Run { file })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    fn run(file: Option<&str>) -> Result<Command, String> {
        Ok(Command::Run {
            file: file.map(PathBuf::from),
        })
    }

    #[test]
    fn no_args_runs_with_default_file() {
        assert_eq!(parse_args(&[]), run(None));
    }

    #[test]
    fn file_flag_forms() {
        assert_eq!(parse_args(&["-f", "a.json"]), run(Some("a.json")));
        assert_eq!(parse_args(&["--file", "a.json"]), run(Some("a.json")));
        assert_eq!(parse_args(&["--file=a.json"]), run(Some("a.json")));
        assert_eq!(parse_args(&["--file=a=b.json"]), run(Some("a=b.json")));
    }

    #[test]
    fn last_file_flag_wins() {
        assert_eq!(parse_args(&["-f", "a.json", "--file=b.json"]), run(Some("b.json")));
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_args(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_args(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_args(&["-V"]), Ok(Command::Version));
        assert_eq!(parse_args(&["--version"]), Ok(Command::Version));
        assert_eq!(parse_args(&["-f", "a.json", "--help"]), Ok(Command::Help));
    }

    #[test]
    fn errors() {
        assert!(parse_args(&["-f"]).unwrap_err().contains("requires a path"));
        assert!(parse_args(&["--file"]).unwrap_err().contains("requires a path"));
        assert!(parse_args(&["--file="]).unwrap_err().contains("requires a path"));
        assert!(parse_args(&["-f", ""]).unwrap_err().contains("requires a path"));
        assert!(
            parse_args(&["--bogus"])
                .unwrap_err()
                .contains("unexpected argument '--bogus'")
        );
        assert!(parse_args(&["todos.json"]).unwrap_err().contains("unexpected argument"));
        assert!(parse_args(&["-f", "--help"]).unwrap_err().contains("got '--help'"));
        assert_eq!(parse_args(&["-f", "./-x.json"]), run(Some("./-x.json")));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'a', 0xff, b'.', b'j']);
        assert_eq!(
            parse([OsString::from("-f"), path.clone()]),
            Ok(Command::Run {
                file: Some(PathBuf::from(path))
            })
        );
    }
}
