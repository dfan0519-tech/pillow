pub mod app_state;
mod conversation_commands;
pub mod conversation_events;
pub mod conversation_service;
pub mod desktop_lifecycle;
pub mod directory_ops;
pub mod path_scope;
mod project_commands;
pub mod runtime_composition;

use std::sync::Arc;

use app_state::AppState;
use conversation_commands::{
    cancel_run, complete_conversation_acceptance, create_conversation,
    get_conversation_acceptance_scenario, list_conversations, load_timeline, send_turn,
};
use conversation_events::TauriEmitter;
use conversation_service::ConversationService;
use desktop_lifecycle::{
    ConversationExitDecision, DesktopLifecycleState, app_data_override, complete_native_acceptance,
    conversation_exit_decision, get_native_acceptance_scenario, mark_project_launcher_ready,
    write_conversation_shutdown_success_marker,
};
use project_commands::{
    create_project, create_project_directory, create_project_with_folder, list_projects,
    open_or_register_project, pick_directory, select_project,
};
use runtime_composition::open_conversation_service;
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
    let app = configure_desktop_commands(tauri::Builder::default())
        .setup(|app| {
            let app_data_dir = app_data_override()
                .map(Ok)
                .unwrap_or_else(|| app.path().app_data_dir())?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("keeper.db");
            let state = AppState::open(&db_path)?;
            let emitter = Arc::new(TauriEmitter::new(app.handle().clone()));
            // Production default: GrokAcpAdapter. Fake only when
            // PILLOW_RUNTIME=fake (explicit development / test selection).
            // Never silent-fallback from a missing Grok install to Fake.
            let (conversations, selection) = open_conversation_service(&db_path, emitter)?;
            eprintln!("Pillow runtime selection: {}", selection.as_str());
            let conversations = Arc::new(conversations);
            let recovered = conversations.recover_inflight()?;
            if recovered > 0 {
                eprintln!("Pillow recovered {recovered} interrupted run(s) as Interrupted");
            }
            app.manage(state);
            app.manage(conversations);
            app.manage(DesktopLifecycleState::from_app_data_dir(app_data_dir));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Pillow");

    app.run(|app_handle, event| {
        // Graceful ConversationService drain on app exit (window close / acceptance exit).
        // Fail closed on ExitRequested: prevent_exit when shutdown fails. A later
        // Exit event cannot recover a failed drain — do not claim that it can.
        match event {
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } => {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.revoke_window(&label);
                }
            }
            tauri::RunEvent::ExitRequested { api, .. } => {
                if let Some(service) = app_handle.try_state::<Arc<ConversationService>>() {
                    match conversation_exit_decision(service.shutdown()) {
                        ConversationExitDecision::AllowExit => {
                            // Marker only after successful ConversationService::shutdown.
                            write_conversation_shutdown_success_marker();
                        }
                        ConversationExitDecision::PreventExit { reason } => {
                            eprintln!(
                                "Pillow conversation shutdown failed; preventing exit: {reason}"
                            );
                            api.prevent_exit();
                        }
                    }
                }
            }
            tauri::RunEvent::Exit => {
                // Best-effort idempotent cleanup if Exit fires without a prior
                // successful ExitRequested drain (e.g. forced termination paths).
                // Do not treat this as recovery for a failed prevent_exit drain.
                if let Some(service) = app_handle.try_state::<Arc<ConversationService>>() {
                    match service.shutdown() {
                        Ok(()) => write_conversation_shutdown_success_marker(),
                        Err(error) => {
                            eprintln!("Pillow conversation shutdown on Exit: {error}");
                        }
                    }
                }
            }
            _ => {}
        }
    });
}
