//! The part of a project's `xsht-config.ini` that decides how a program
//! loads: where the config is and which module roots it names.
//!
//! The runner, the interactive shell, and the tools all find project module
//! roots here, so a `use` that checks is a `use` that loads. Everything else
//! in the file configures tools and is read by them alone.

use crate::runtime::value::{RecordMap, Value};
use crate::source::{SourceId, Span};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const PROJECT_CONFIG_FILE_NAME: &str = "xsht-config.ini";

const MODULE_PATH_KEY: &str = "module_path";

/// The directory of the nearest project config governing `file`: the file's
/// own directory, or else the closest directory above it, all the way to the
/// filesystem root. A relative path is joined onto the current directory
/// first, without resolving symbolic links, so the answer does not depend on
/// where the command was started from.
///
/// A directory at or below the current directory comes back relative to it,
/// so paths built from it read like the paths the user wrote.
pub fn nearest_project_config_dir(file: &Path) -> Option<PathBuf> {
    let cwd = if file.is_absolute() {
        None
    } else {
        // Without a current directory a relative path names nothing.
        Some(std::env::current_dir().ok()?)
    };
    let absolute = match &cwd {
        Some(cwd) => lexically_normalized(&cwd.join(file)),
        None => lexically_normalized(file),
    };
    let dir = absolute
        .parent()?
        .ancestors()
        .find(|dir| dir.join(PROJECT_CONFIG_FILE_NAME).is_file())?;
    let Some(cwd) = cwd else {
        return Some(dir.to_path_buf());
    };
    Some(match dir.strip_prefix(lexically_normalized(&cwd)) {
        Ok(below) if below.as_os_str().is_empty() => PathBuf::from("."),
        Ok(below) => below.to_path_buf(),
        Err(_) => dir.to_path_buf(),
    })
}

/// `path` with `.` components dropped and each `..` cancelling the component
/// before it. Symbolic links are not consulted: a script reached through a
/// link belongs to the project the link is in.
fn lexically_normalized(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component);
                }
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// Reads and decodes the project config at `path`. `None` means the file does
/// not exist; a file that cannot be read or decoded is an error.
pub fn read_project_config(path: &Path) -> Result<Option<RecordMap>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read {PROJECT_CONFIG_FILE_NAME} '{}': {error}",
                path.display()
            ));
        }
    };
    let span = Span::new(SourceId::new(0), 0, 0);
    match crate::modules::ini::decode(&text, span) {
        Ok(Value::Record(fields)) => Ok(Some(fields)),
        Ok(_) => Err(format!(
            "invalid {PROJECT_CONFIG_FILE_NAME} '{}': the file must decode to a record",
            path.display()
        )),
        Err(error) => Err(format!(
            "invalid {PROJECT_CONFIG_FILE_NAME} '{}': {}",
            path.display(),
            error.message
        )),
    }
}

/// The `module_path` entries of a decoded project config, as written. A
/// config without the key names its own directory.
pub fn configured_module_path(fields: &RecordMap) -> Result<Vec<String>, String> {
    match fields.get(MODULE_PATH_KEY) {
        None => Ok(default_module_path()),
        Some(Value::Str(entries)) => Ok(entries.split('\n').map(str::to_string).collect()),
        Some(_) => Err(format!(
            "invalid {PROJECT_CONFIG_FILE_NAME} {MODULE_PATH_KEY}: expected a list of directories, not a section"
        )),
    }
}

/// The module path of a project config that does not set one: the config's
/// own directory.
pub fn default_module_path() -> Vec<String> {
    vec![".".to_string()]
}

/// A path written in a project config, resolved from the config's directory.
pub fn resolve_project_path(config_dir: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir.join(path)
    }
}

/// Module roots for `module_path` entries written in the config in
/// `config_dir`, in order.
pub fn module_roots(config_dir: &Path, module_path: &[String]) -> Vec<PathBuf> {
    module_path
        .iter()
        .map(|entry| resolve_project_path(config_dir, entry))
        .collect()
}

/// The project module roots of the program whose entry script is `entry`:
/// the `module_path` of the nearest project config, searched after
/// file-relative lookup and `XSH_MODULE_PATH`. An entry script with no config
/// above it has no project roots; a config that cannot be read or decoded is
/// an error.
pub fn project_module_roots(entry: &Path) -> Result<Vec<PathBuf>, String> {
    let Some(config_dir) = nearest_project_config_dir(entry) else {
        return Ok(Vec::new());
    };
    let config_file = config_dir.join(PROJECT_CONFIG_FILE_NAME);
    // The file can vanish between the search and the read; that is the same
    // as never having found it.
    let Some(fields) = read_project_config(&config_file)? else {
        return Ok(Vec::new());
    };
    let module_path = configured_module_path(&fields)
        .map_err(|message| format!("{message} in '{}'", config_file.display()))?;
    Ok(module_roots(&config_dir, &module_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xsh-project-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin/tools")).expect("create project tree");
        dir
    }

    #[test]
    fn project_module_roots_come_from_the_nearest_config_above_the_entry() {
        let project = temp_project("nearest");
        fs::write(
            project.join(PROJECT_CONFIG_FILE_NAME),
            "module_path = lib\n  /opt/shared\n",
        )
        .expect("write config");
        let entry = project.join("bin/tools/run.xsh");
        assert_eq!(
            project_module_roots(&entry),
            Ok(vec![project.join("lib"), PathBuf::from("/opt/shared")])
        );

        // A nearer config replaces the outer one; without `module_path` it
        // names its own directory.
        fs::write(project.join("bin").join(PROJECT_CONFIG_FILE_NAME), "").expect("write config");
        assert_eq!(
            project_module_roots(&entry),
            Ok(vec![project.join("bin").join(".")])
        );
        fs::remove_dir_all(&project).expect("remove project tree");
    }

    #[test]
    fn parent_components_cancel_before_the_walk() {
        assert_eq!(
            lexically_normalized(Path::new("/a/b/../c/./d.xsh")),
            PathBuf::from("/a/c/d.xsh")
        );
        let project = temp_project("normalized");
        fs::write(project.join("bin").join(PROJECT_CONFIG_FILE_NAME), "").expect("write config");
        // `bin/../run.xsh` is beside `bin`, not inside it.
        assert_eq!(
            nearest_project_config_dir(&project.join("bin/../run.xsh")),
            None
        );
        assert_eq!(
            nearest_project_config_dir(&project.join("bin/tools/../run.xsh")),
            Some(project.join("bin"))
        );
        fs::remove_dir_all(&project).expect("remove project tree");
    }

    #[test]
    fn an_entry_without_a_config_has_no_project_roots() {
        let project = temp_project("none");
        assert_eq!(
            project_module_roots(&project.join("bin/run.xsh")),
            Ok(Vec::new())
        );
        fs::remove_dir_all(&project).expect("remove project tree");
    }

    #[test]
    fn a_malformed_config_is_an_error_not_an_empty_module_path() {
        let project = temp_project("malformed");
        let config = project.join(PROJECT_CONFIG_FILE_NAME);
        let entry = project.join("bin/run.xsh");

        fs::write(&config, "module_path lib\n").expect("write config");
        let message = project_module_roots(&entry).expect_err("undecodable config");
        assert!(
            message.starts_with("invalid xsht-config.ini '"),
            "{message}"
        );

        fs::write(&config, "[module_path]\nroot = lib\n").expect("write config");
        let message = project_module_roots(&entry).expect_err("module_path section");
        assert!(
            message.contains("module_path: expected a list of directories"),
            "{message}"
        );

        fs::write(&config, [0xff, 0xfe]).expect("write config");
        let message = project_module_roots(&entry).expect_err("non-UTF-8 config");
        assert!(
            message.starts_with("failed to read xsht-config.ini '"),
            "{message}"
        );
        fs::remove_dir_all(&project).expect("remove project tree");
    }
}
