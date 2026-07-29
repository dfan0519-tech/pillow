use tauri::State;

use crate::app_state::{AppState, CommandError, ProjectDto};

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
