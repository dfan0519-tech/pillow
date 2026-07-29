pub mod app_state;
mod project_commands;

use app_state::AppState;
use project_commands::{create_project, list_projects, select_project};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let state = AppState::open(app_data_dir.join("keeper.db"))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_project,
            list_projects,
            select_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keeper IDE");
}
