pub mod app_state;
pub mod desktop_lifecycle;
pub mod directory_ops;
pub mod path_scope;
mod project_commands;

use app_state::AppState;
use desktop_lifecycle::{
    DesktopLifecycleState, app_data_override, complete_native_acceptance,
    get_native_acceptance_scenario, mark_project_launcher_ready,
};
use project_commands::{
    create_project, create_project_directory, create_project_with_folder, list_projects,
    open_or_register_project, pick_directory, select_project,
};
use tauri::Manager;

fn configure_desktop_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        create_project,
        list_projects,
        select_project,
        pick_directory,
        open_or_register_project,
        create_project_directory,
        create_project_with_folder,
        mark_project_launcher_ready,
        get_native_acceptance_scenario,
        complete_native_acceptance
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_desktop_commands(tauri::Builder::default())
        .setup(|app| {
            let app_data_dir = app_data_override()
                .map(Ok)
                .unwrap_or_else(|| app.path().app_data_dir())?;
            std::fs::create_dir_all(&app_data_dir)?;
            let state = AppState::open(app_data_dir.join("keeper.db"))?;
            app.manage(state);
            app.manage(DesktopLifecycleState::from_app_data_dir(app_data_dir));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Keeper IDE");
}
