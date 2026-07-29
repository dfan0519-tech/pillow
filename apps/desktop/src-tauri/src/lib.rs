pub mod app_state;
mod conversation_commands;
pub mod conversation_events;
pub mod conversation_service;
pub mod desktop_lifecycle;
pub mod directory_ops;
pub mod path_scope;
mod project_commands;

use std::sync::Arc;

use app_state::AppState;
use conversation_commands::{
    cancel_run, complete_conversation_acceptance, create_conversation,
    get_conversation_acceptance_scenario, list_conversations, load_timeline, send_turn,
};
use conversation_events::TauriEmitter;
use conversation_service::ConversationService;
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
        complete_native_acceptance,
        list_conversations,
        create_conversation,
        load_timeline,
        send_turn,
        cancel_run,
        get_conversation_acceptance_scenario,
        complete_conversation_acceptance
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
            let db_path = app_data_dir.join("keeper.db");
            let state = AppState::open(&db_path)?;
            let emitter = Arc::new(TauriEmitter::new(app.handle().clone()));
            let conversations = Arc::new(ConversationService::open(
                &db_path, emitter, /* honor_fixture_delays */ true,
            )?);
            let recovered = conversations.recover_inflight()?;
            if recovered > 0 {
                eprintln!("Keeper IDE recovered {recovered} interrupted run(s) as Interrupted");
            }
            app.manage(state);
            app.manage(conversations);
            app.manage(DesktopLifecycleState::from_app_data_dir(app_data_dir));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Keeper IDE");
}
