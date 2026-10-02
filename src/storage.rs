use crate::model::Todo;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "todos.json";

/// Where the todo file path came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSource {
    /// `--file` command-line flag.
    Flag,
    /// `TUIDO_FILE` environment variable.
    Env,
    /// `$XDG_CONFIG_HOME/tuido/todos.json`.
    Xdg,
    /// `~/.config/tuido/todos.json`, used despite `XDG_CONFIG_HOME` because only it exists.
    Legacy,
    /// `~/.config/tuido/todos.json`.
    Default,
}

impl PathSource {
    /// Paths the user named explicitly; their directories are not created for them.
    pub fn is_explicit(self) -> bool {
        matches!(self, PathSource::Flag | PathSource::Env)
    }
}

/// Inputs to path resolution, injected so it can be tested without touching
/// the real environment or filesystem.
pub struct PathEnv<'a> {
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    pub home: Option<PathBuf>,
    pub exists: &'a dyn Fn(&Path) -> bool,
}

/// Resolves the todo file. First match wins:
/// 1. `--file`
/// 2. `$TUIDO_FILE`
/// 3. `$XDG_CONFIG_HOME/tuido/todos.json` — unless only the legacy `~/.config`
///    file exists, which is then used so existing data never looks lost
/// 4. `~/.config/tuido/todos.json`
pub fn resolve_todos_path(cli_file: Option<PathBuf>, env: &PathEnv) -> io::Result<(PathBuf, PathSource)> {
    if let Some(file) = cli_file {
        return Ok((file, PathSource::Flag));
    }
    let var = |name| (env.var)(name).filter(|value| !value.is_empty());
    if let Some(file) = var("TUIDO_FILE") {
        return Ok((PathBuf::from(file), PathSource::Env));
    }

    let legacy = env
        .home
        .as_ref()
        .map(|home| home.join(".config").join("tuido").join(FILE_NAME));
    // Per the XDG Base Directory spec, relative values are invalid and ignored.
    if let Some(xdg_home) = var("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()) {
        let xdg = xdg_home.join("tuido").join(FILE_NAME);
        if let Some(legacy) = legacy
            && legacy != xdg
            && !(env.exists)(&xdg)
            && (env.exists)(&legacy)
        {
            return Ok((legacy, PathSource::Legacy));
        }
        return Ok((xdg, PathSource::Xdg));
    }

    legacy
        .map(|path| (path, PathSource::Default))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not determine home directory"))
}

/// Makes an explicitly given path absolute and checks its directory exists, so
/// a typo fails loudly instead of quietly starting a new, empty list.
pub fn validate_explicit_path(path: &Path) -> Result<PathBuf, String> {
    let path = std::path::absolute(path).map_err(|e| format!("invalid path {}: {}", path.display(), e))?;
    match path.parent() {
        Some(dir) if !dir.is_dir() => Err(format!("directory {} does not exist", dir.display())),
        _ => Ok(path),
    }
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
///
/// Symlinks are followed so a linked file (e.g. into a synced folder) keeps
/// being a link: the rename replaces the real file, not the link.
pub fn save_todos(file_path: &Path, todos: &[Todo]) -> io::Result<()> {
    let target = save_target(file_path)?;
    let tmp_path = tmp_path_for(&target);
    let data = serde_json::to_string_pretty(todos)?;
    fs::write(&tmp_path, data)?;
    fs::rename(tmp_path, target)
}

/// The file a save should replace. An existing path is canonicalized (following
/// symlinks). A dangling symlink resolves one hop to its target, whose directory
/// must already exist: an unmounted sync folder must not be shadowed by a new
/// local directory. Any other path gets its directory created.
fn save_target(path: &Path) -> io::Result<PathBuf> {
    if let Ok(real) = fs::canonicalize(path) {
        return Ok(real);
    }
    let is_symlink = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    if !is_symlink {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        return Ok(path.to_path_buf());
    }
    let link = fs::read_link(path)?;
    let target = match path.parent() {
        Some(dir) if link.is_relative() => dir.join(link),
        _ => link,
    };
    match target.parent() {
        Some(dir) if !dir.is_dir() => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("symlink target directory {} does not exist", dir.display()),
        )),
        _ => Ok(target),
    }
}

/// `todo.txt` -> `todo.txt.tmp`, next to the original.
fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
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
        assert!(!tmp_path_for(&path).exists());
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
    fn loads_files_written_before_notified_at_existed() {
        let path = temp_file("legacy-format");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = r#"[{"id":0,"text":"old","completed":false,"suspended":false,"due_date":null,"created_at":null}]"#;
        fs::write(&path, legacy).unwrap();

        let todos = load_todos(&path).unwrap();
        assert_eq!(todos[0].notified_at, None);
        save_todos(&path, &todos).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("notified_at"));
    }

    #[test]
    fn tmp_file_appends_to_full_name() {
        assert_eq!(tmp_path_for(Path::new("/x/todo.txt")), Path::new("/x/todo.txt.tmp"));
        assert_eq!(tmp_path_for(Path::new("/x/todos.json")), Path::new("/x/todos.json.tmp"));
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_symlink_keeps_the_link() {
        let link = temp_file("symlink");
        let dir = link.parent().unwrap();
        fs::create_dir_all(dir).unwrap();
        let real = dir.join("synced.json");
        save_todos(&real, &[Todo::new(0, "old".to_string())]).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        save_todos(&link, &[Todo::new(0, "new".to_string())]).unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(load_todos(&real).unwrap()[0].text, "new");
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_dangling_symlink_creates_the_target() {
        let link = temp_file("dangling");
        let dir = link.parent().unwrap();
        fs::create_dir_all(dir.join("sync")).unwrap();
        // Relative link, as `ln -s sync/new.json todos.json` would create.
        std::os::unix::fs::symlink("sync/new.json", &link).unwrap();

        save_todos(&link, &[Todo::new(0, "first".to_string())]).unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(load_todos(&dir.join("sync/new.json")).unwrap()[0].text, "first");
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_into_missing_directory_is_an_error() {
        let link = temp_file("unmounted");
        let dir = link.parent().unwrap();
        fs::create_dir_all(dir).unwrap();
        std::os::unix::fs::symlink(dir.join("unmounted/todos.json"), &link).unwrap();

        assert!(save_todos(&link, &[Todo::new(0, "x".to_string())]).is_err());
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(!dir.join("unmounted").exists());
    }

    /// A fake environment rooted in an absolute temp dir (absolute on every OS).
    struct FakeEnv {
        vars: Vec<(&'static str, OsString)>,
        home: Option<PathBuf>,
        existing: Vec<PathBuf>,
    }

    impl FakeEnv {
        fn new() -> Self {
            FakeEnv {
                vars: Vec::new(),
                home: Some(root().join("home")),
                existing: Vec::new(),
            }
        }

        fn var(mut self, name: &'static str, value: impl Into<OsString>) -> Self {
            self.vars.push((name, value.into()));
            self
        }

        fn resolve(&self, cli_file: Option<&str>) -> io::Result<(PathBuf, PathSource)> {
            let var = |name: &str| self.vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
            let exists = |path: &Path| self.existing.iter().any(|p| p == path);
            let env = PathEnv {
                var: &var,
                home: self.home.clone(),
                exists: &exists,
            };
            resolve_todos_path(cli_file.map(PathBuf::from), &env)
        }
    }

    fn root() -> PathBuf {
        std::env::temp_dir().join("tuido-resolve")
    }

    fn legacy() -> PathBuf {
        root().join("home/.config/tuido/todos.json")
    }

    fn xdg() -> PathBuf {
        root().join("xdg/tuido/todos.json")
    }

    #[test]
    fn resolves_default_path() {
        assert_eq!(FakeEnv::new().resolve(None).unwrap(), (legacy(), PathSource::Default));
    }

    #[test]
    fn resolution_precedence() {
        let env = FakeEnv::new()
            .var("TUIDO_FILE", "/env.json")
            .var("XDG_CONFIG_HOME", root().join("xdg"));
        assert_eq!(
            env.resolve(Some("cli.json")).unwrap(),
            ("cli.json".into(), PathSource::Flag)
        );
        assert_eq!(env.resolve(None).unwrap(), ("/env.json".into(), PathSource::Env));

        let env = FakeEnv::new().var("XDG_CONFIG_HOME", root().join("xdg"));
        assert_eq!(env.resolve(None).unwrap(), (xdg(), PathSource::Xdg));
    }

    #[test]
    fn empty_and_relative_vars_are_ignored() {
        let env = FakeEnv::new().var("TUIDO_FILE", "").var("XDG_CONFIG_HOME", "");
        assert_eq!(env.resolve(None).unwrap(), (legacy(), PathSource::Default));
        let env = FakeEnv::new().var("XDG_CONFIG_HOME", "relative/config");
        assert_eq!(env.resolve(None).unwrap(), (legacy(), PathSource::Default));
    }

    #[test]
    fn falls_back_to_legacy_file_only_when_xdg_file_is_missing() {
        let mut env = FakeEnv::new().var("XDG_CONFIG_HOME", root().join("xdg"));
        env.existing.push(legacy());
        assert_eq!(env.resolve(None).unwrap(), (legacy(), PathSource::Legacy));

        env.existing.push(xdg());
        assert_eq!(env.resolve(None).unwrap(), (xdg(), PathSource::Xdg));
    }

    #[test]
    fn missing_home_is_an_error_unless_overridden() {
        let mut env = FakeEnv::new();
        env.home = None;
        assert!(env.resolve(None).is_err());
        assert_eq!(env.resolve(Some("cli.json")).unwrap().1, PathSource::Flag);
    }

    #[test]
    fn explicit_path_needs_an_existing_directory() {
        let path = temp_file("explicit");
        assert!(validate_explicit_path(&path).unwrap_err().contains("does not exist"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert_eq!(validate_explicit_path(&path).unwrap(), path);
        assert!(
            validate_explicit_path(Path::new("relative.json"))
                .unwrap()
                .is_absolute()
        );
    }

    #[test]
    fn duplicate_ids_are_renumbered() {
        let mut todos = vec![Todo::new(0, "a".to_string()), Todo::new(0, "b".to_string())];
        dedupe_ids(&mut todos);
        assert_eq!(todos.iter().map(|t| t.id).collect::<Vec<_>>(), [0, 1]);
    }
}
