//! Typed Tauri project commands.
//! `pick_directory` adapted from Grok App under MIT.
//! Upstream: src-tauri/src/commands.rs pick_directory at
//! cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.

use tauri::{State, WebviewWindow};

use crate::app_state::{AppState, CommandError, ProjectDto};

#[tauri::command]
pub fn create_project<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    name: String,
    root_path: String,
) -> Result<ProjectDto, CommandError> {
    state.create_project_from_picker_for_window(window.label(), name, root_path)
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectDto>, CommandError> {
    state.list_projects()
}

#[tauri::command]
pub fn select_project<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    project_id: String,
) -> Result<ProjectDto, CommandError> {
    state.select_project_for_window(window.label(), project_id)
}

/// Native folder picker. Returns null when the user cancels.
/// Adapted from Grok App `pick_directory` (rfd::FileDialog).
#[tauri::command]
pub async fn pick_directory<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
) -> Result<Option<String>, String> {
    let picker_operation = state.begin_picker_operation(window.label());
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择文件夹").pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;

    state
        .complete_picker_operation(picker_operation, folder.as_deref())
        .map_err(|error| error.message)
}

/// Select an existing registered project for the path, or register a new one.
#[tauri::command]
pub fn open_or_register_project<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    root_path: String,
) -> Result<ProjectDto, CommandError> {
    state.open_or_register_project_for_window(window.label(), root_path)
}

/// Disabled until Windows-safe handle-relative directory creation exists.
#[tauri::command]
pub fn create_project_directory<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    parent_path: String,
    name: String,
) -> Result<String, CommandError> {
    state.create_project_directory_from_picker_for_window(window.label(), parent_path, name)
}

/// Disabled until Windows-safe handle-relative directory creation exists.
#[tauri::command]
pub fn create_project_with_folder<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    name: String,
    parent_path: String,
) -> Result<ProjectDto, CommandError> {
    state.create_project_with_folder_for_window(window.label(), name, parent_path)
}
