//! Pure directory helpers for open/new project flows.
//! Adapted from Grok App folder-picker patterns (pick_directory + rfd).
//! Upstream: src-tauri/src/commands.rs pick_directory at
//! cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.

use std::fs;
use std::path::{Path, PathBuf};

use crate::app_state::CommandError;

/// Derive a display name from a folder path (leaf segment).
pub fn project_name_from_path(path: &Path) -> Result<String, CommandError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            command_error(
                "invalid_project_root",
                "The selected folder does not have a usable name.",
            )
        })?;
    Ok(name.to_owned())
}

/// Create `parent/name` as a new project directory. Fails if it already exists.
pub fn create_child_directory(parent: &Path, name: &str) -> Result<PathBuf, CommandError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(command_error(
            "invalid_project_name",
            "Project name cannot be empty.",
        ));
    }
    if trimmed.contains(['/', '\\']) || trimmed == "." || trimmed == ".." {
        return Err(command_error(
            "invalid_project_name",
            "Project name cannot contain path separators.",
        ));
    }
    if !parent.is_dir() {
        return Err(command_error(
            "invalid_project_root",
            "The parent folder does not exist or is not a directory.",
        ));
    }

    let child = parent.join(trimmed);
    if child.exists() {
        return Err(command_error(
            "project_directory_exists",
            "A folder with this project name already exists in the selected location.",
        ));
    }

    fs::create_dir(&child).map_err(|source| {
        eprintln!("Pillow failed to create project directory: {source}");
        command_error(
            "project_directory_create_failed",
            "Could not create the project folder.",
        )
    })?;

    Ok(child)
}

fn command_error(code: &'static str, message: &'static str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Owns the tree for one test. Callers hold the guard for the whole test
    /// body: it removes the directory on drop, including while a test panics,
    /// which the trailing `remove_dir_all` calls it replaced never did.
    fn temp_root(label: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("pillow-dir-ops-{label}-"))
            .tempdir()
            .expect("temp root")
    }

    #[test]
    fn derives_project_name_from_folder_leaf() {
        let path = PathBuf::from(r"F:\work\my-app");
        assert_eq!(project_name_from_path(&path).unwrap(), "my-app");
    }

    #[test]
    fn rejects_empty_or_root_paths_for_names() {
        assert_eq!(
            project_name_from_path(Path::new("")).unwrap_err().code,
            "invalid_project_root"
        );
    }

    #[test]
    fn creates_a_child_directory_under_parent() {
        let parent = temp_root("create");
        let child = create_child_directory(parent.path(), "  demo-project  ").unwrap();
        assert!(child.is_dir());
        assert_eq!(child.file_name().unwrap(), "demo-project");
    }

    #[test]
    fn rejects_existing_child_directory() {
        let parent = temp_root("exists");
        fs::create_dir(parent.path().join("taken")).unwrap();
        let error = create_child_directory(parent.path(), "taken").unwrap_err();
        assert_eq!(error.code, "project_directory_exists");
    }

    #[test]
    fn rejects_path_separators_in_project_name() {
        let parent = temp_root("sep");
        let error = create_child_directory(parent.path(), "a/b").unwrap_err();
        assert_eq!(error.code, "invalid_project_name");
    }
}
