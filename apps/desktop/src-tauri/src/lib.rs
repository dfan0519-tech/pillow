pub mod app_state;
mod conversation_commands;
pub mod conversation_events;
pub mod conversation_service;
pub mod desktop_lifecycle;
pub mod directory_ops;
mod exec_commands;
mod git_commands;
mod librarian_audit_commands;
pub mod path_scope;
mod permission_rule_commands;
mod project_commands;
mod resource_commands;
mod role_dispatch_commands;
pub mod runtime_composition;
mod runtime_service_state;
mod runtime_slot_commands;
mod settings_commands;
mod write_commands;

#[cfg(test)]
mod exec_commands_tests;
#[cfg(test)]
mod git_commands_tests;
#[cfg(test)]
mod librarian_audit_commands_tests;
#[cfg(test)]
mod permission_rule_commands_tests;
#[cfg(test)]
mod resource_commands_tests;
#[cfg(test)]
mod role_dispatch_commands_tests;
#[cfg(test)]
mod runtime_slot_commands_tests;
#[cfg(test)]
mod settings_commands_tests;
#[cfg(test)]
mod write_commands_tests;

use std::sync::Arc;

use app_state::AppState;
use conversation_commands::{
    cancel_run, complete_conversation_acceptance, create_conversation,
    get_conversation_acceptance_scenario, list_conversations, load_timeline, send_turn,
};
use conversation_events::{RuntimeEventEmitter, TauriEmitter};
use desktop_lifecycle::{
    ConversationExitDecision, DesktopLifecycleState, app_data_override, complete_native_acceptance,
    conversation_exit_decision, get_native_acceptance_scenario, mark_project_launcher_ready,
    write_conversation_shutdown_success_marker,
};
use exec_commands::{decide_exec_proposal, list_exec_proposals, open_exec_output};
use git_commands::{
    GitCommandState, commit_git_changes, discover_git_repositories, get_git_status, open_git_diff,
    push_git_branch, stage_git_paths, unstage_git_paths,
};
use keeper_storage_sqlite::SqlitePermissionRuleRepository;
use keeper_storage_sqlite::SqliteSettingsRepository;
use librarian_audit_commands::run_librarian_audit;
use permission_rule_commands::{
    add_permission_rule, delete_permission_rule, list_permission_rules,
};
use pillow_runtime_slots::RuntimeSlotService;
use project_commands::{
    create_project, create_project_directory, create_project_with_folder, list_projects,
    open_or_register_project, pick_directory, select_project,
};
use resource_commands::{
    list_project_resources, list_recent_project_resources, preview_project_resource,
    read_project_resource, search_project_resources,
};
use role_dispatch_commands::{list_role_dispatches, propose_role_dispatch};
use runtime_composition::{RuntimeOpenOutcome, open_conversation_service_or_degrade};
use runtime_service_state::RuntimeServiceState;
use runtime_slot_commands::{
    create_external_cli_slot, create_grok_api_key_slot, create_grok_login_slot, create_slot,
    delete_runtime_slot, detect_local_clis, get_external_cli_status,
    get_runtime_slot_model_catalog, list_runtime_slots, probe_model_catalog,
    set_runtime_slot_model, start_runtime_slot, update_grok_api_key_slot, update_slot,
};
use settings_commands::{
    RuntimeStartupState, clear_runtime_setting, get_onboarding_settings, get_permission_mode,
    get_runtime_settings, get_runtime_startup, get_theme_settings, set_grok_executable,
    set_grok_profile, set_onboarding_dismissed, set_permission_mode, set_runtime_adapter,
    set_ui_theme,
};
use tauri::Manager;
use write_commands::{decide_write_proposal, list_write_proposals, open_write_proposal_diff};

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
        complete_conversation_acceptance,
        list_project_resources,
        read_project_resource,
        search_project_resources,
        list_recent_project_resources,
        preview_project_resource,
        list_write_proposals,
        decide_write_proposal,
        open_write_proposal_diff,
        list_exec_proposals,
        decide_exec_proposal,
        open_exec_output,
        propose_role_dispatch,
        list_role_dispatches,
        list_permission_rules,
        add_permission_rule,
        delete_permission_rule,
        discover_git_repositories,
        get_git_status,
        open_git_diff,
        stage_git_paths,
        unstage_git_paths,
        commit_git_changes,
        push_git_branch,
        get_runtime_settings,
        get_runtime_startup,
        set_runtime_adapter,
        set_grok_executable,
        set_grok_profile,
        clear_runtime_setting,
        get_theme_settings,
        set_ui_theme,
        get_permission_mode,
        set_permission_mode,
        get_onboarding_settings,
        set_onboarding_dismissed,
        list_runtime_slots,
        create_slot,
        update_slot,
        create_grok_login_slot,
        create_grok_api_key_slot,
        update_grok_api_key_slot,
        delete_runtime_slot,
        create_external_cli_slot,
        get_external_cli_status,
        get_runtime_slot_model_catalog,
        set_runtime_slot_model,
        start_runtime_slot,
        detect_local_clis,
        probe_model_catalog,
        run_librarian_audit
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
            let settings = SqliteSettingsRepository::open(&db_path)
                .map_err(|error| format!("settings store open failed: {error}"))?;
            let permission_rules = SqlitePermissionRuleRepository::open(&db_path)
                .map_err(|error| format!("permission rule store open failed: {error}"))?;
            let emitter: Arc<dyn RuntimeEventEmitter> =
                Arc::new(TauriEmitter::new(app.handle().clone()));
            // Production default: GrokAcpAdapter. Fake only when explicitly
            // selected via env or stored settings (env wins). Never silent-
            // fallback from a missing Grok install to Fake.
            //
            // Runtime-config failures (bad exe/profile paths, release stored
            // Fake) degrade: shell + Settings stay up; conversation stays off.
            // SQLite / non-config failures remain fatal (not user-fixable).
            let outcome = open_conversation_service_or_degrade(&db_path, Arc::clone(&emitter))
                .map_err(|error| format!("runtime open failed: {error}"))?;
            let startup = RuntimeStartupState::from_outcome(&outcome);
            let service = match outcome {
                RuntimeOpenOutcome::Ready { service, selection } => {
                    eprintln!("Pillow runtime selection: {}", selection.as_str());
                    let recovered = service.recover_inflight()?;
                    if recovered > 0 {
                        eprintln!("Pillow recovered {recovered} interrupted run(s) as Interrupted");
                    }
                    Some(service)
                }
                RuntimeOpenOutcome::Degraded { code, message, .. } => {
                    eprintln!(
                        "Pillow runtime degraded ({code}): {message}. Settings remain available."
                    );
                    // Do not open Fake. The replaceable runtime state starts
                    // empty and remains fail-closed until a governed slot starts.
                    None
                }
            };
            let runtime = RuntimeServiceState::new(&db_path, emitter, service);
            let runtime_slots = RuntimeSlotService::open(&db_path, &app_data_dir)
                .map_err(|error| format!("runtime slot store open failed: {error}"))?;
            app.manage(state);
            app.manage(settings);
            app.manage(permission_rules);
            app.manage(runtime);
            app.manage(runtime_slots);
            app.manage(Arc::new(startup));
            app.manage(GitCommandState::new());
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
                if let Some(service) = app_handle.try_state::<RuntimeServiceState>() {
                    let shutdown = service.shutdown_if_active();
                    if matches!(&shutdown, Ok(false)) {
                        return;
                    }
                    match conversation_exit_decision(shutdown.map(|_| ())) {
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
                if let Some(service) = app_handle.try_state::<RuntimeServiceState>() {
                    match service.shutdown_if_active() {
                        Ok(true) => write_conversation_shutdown_success_marker(),
                        Ok(false) => {}
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
