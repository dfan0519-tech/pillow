//! Typed Tauri project commands.
//! `pick_directory` adapted from Grok App under MIT.
//! Upstream: src-tauri/src/commands.rs pick_directory at
//! cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.

use tauri::State;

use crate::app_state::{AppState, CommandError, ProjectDto};
use crate::directory_ops::create_child_directory;
use crate::path_scope;

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    name: String,
    root_path: String,
) -> Result<ProjectDto, CommandError> {
    state.create_project(name, root_path)
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectDto>, CommandError> {
    state.list_projects()
}

#[tauri::command]
pub fn select_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<ProjectDto, CommandError> {
    state.select_project(project_id)
}

/// Native folder picker. Returns null when the user cancels.
/// Adapted from Grok App `pick_directory` (rfd::FileDialog).
#[tauri::command]
pub async fn pick_directory() -> Result<Option<String>, String> {
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择文件夹").pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;

    Ok(folder.map(|path| {
        path_scope::grant_path(&path);
        path.display().to_string()
    }))
}

/// Select an existing registered project for the path, or register a new one.
#[tauri::command]
pub fn open_or_register_project(
    state: State<'_, AppState>,
    root_path: String,
) -> Result<ProjectDto, CommandError> {
    state.open_or_register_project(root_path)
}

/// Create a child directory under `parent_path` named `name` (no registration).
/// Parent must already be on the path allowlist (native picker grants it).
#[tauri::command]
pub fn create_project_directory(parent_path: String, name: String) -> Result<String, CommandError> {
    let parent = std::path::Path::new(parent_path.trim());
    path_scope::require_allowed(parent).map_err(|error| CommandError {
        code: error.code(),
        message: error.message().to_owned(),
    })?;
    let child = create_child_directory(parent, &name)?;
    path_scope::grant_path(&child);
    Ok(child.to_string_lossy().into_owned())
}

/// Create child folder, register, and return the project.
#[tauri::command]
pub fn create_project_with_folder(
    state: State<'_, AppState>,
    name: String,
    parent_path: String,
) -> Result<ProjectDto, CommandError> {
    state.create_project_with_folder(name, parent_path)
}
