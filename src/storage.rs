use crate::model::Todo;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn config_dir() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".config").join("tuido"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not determine home directory"))
}

/// Default location of the todo file: `~/.config/tuido/todos.json`.
pub fn todos_path() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("todos.json"))
}

/// Loads todos from disk. A missing file yields an empty list, but an unreadable
/// or unparseable file is an error so we never silently overwrite user data.
pub fn load_todos(file_path: &Path) -> Result<Vec<Todo>, String> {
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(file_path).map_err(|e| format!("failed to read {}: {}", file_path.display(), e))?;
    let mut todos: Vec<Todo> =
        serde_json::from_str(&data).map_err(|e| format!("failed to parse {}: {}", file_path.display(), e))?;
    dedupe_ids(&mut todos);
    Ok(todos)
}

/// Older versions could assign duplicate ids; make them unique again.
fn dedupe_ids(todos: &mut [Todo]) {
    let mut seen = HashSet::new();
    if !todos.iter().all(|t| seen.insert(t.id)) {
        for (i, todo) in todos.iter_mut().enumerate() {
            todo.id = i as u32;
        }
    }
}

/// Writes todos atomically (temp file + rename) so a crash mid-write cannot
/// leave a truncated file behind.
pub fn save_todos(file_path: &Path, todos: &[Todo]) -> io::Result<()> {
    if let Some(dir) = file_path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp_path = file_path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(todos)?;
    fs::write(&tmp_path, data)?;
    fs::rename(tmp_path, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tuido-test-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        dir.join("todos.json")
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(load_todos(&temp_file("missing")).unwrap().is_empty());
    }

    #[test]
    fn round_trips_todos() {
        let path = temp_file("roundtrip");
        let todos = vec![Todo::new(0, "a".to_string()), Todo::new(1, "b".to_string())];
        save_todos(&path, &todos).unwrap();
        let loaded = load_todos(&path).unwrap();
        assert_eq!(loaded.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_an_error_and_left_untouched() {
        let path = temp_file("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[{\"broken").unwrap();
        assert!(load_todos(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "[{\"broken");
    }

    #[test]
    fn duplicate_ids_are_renumbered() {
        let mut todos = vec![Todo::new(0, "a".to_string()), Todo::new(0, "b".to_string())];
        dedupe_ids(&mut todos);
        assert_eq!(todos.iter().map(|t| t.id).collect::<Vec<_>>(), [0, 1]);
    }
}
