//! C4 typed Tauri commands for conversation-owned process views.

use tauri::{State, WebviewWindow};

use crate::app_state::{AppState, CommandError};
use crate::conversation_service::{ConversationCommandError, ExecOutputDto, ExecProposalDto};
use crate::runtime_service_state::{RuntimeServiceState, require_project_scope};

fn map_error(error: ConversationCommandError) -> CommandError {
    CommandError {
        code: error.code,
        message: error.message,
    }
}

#[tauri::command]
pub fn list_exec_proposals<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
) -> Result<Vec<ExecProposalDto>, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            service.list_exec_proposals(conversation_id)
        })
        .map_err(map_error)
}

#[tauri::command]
pub fn decide_exec_proposal<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
    decision: String,
) -> Result<ExecProposalDto, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            service.decide_exec_proposal(conversation_id, proposal_id, decision)
        })
        .map_err(map_error)
}

#[tauri::command]
pub fn open_exec_output<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
) -> Result<ExecOutputDto, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            service.open_exec_output(conversation_id, proposal_id)
        })
        .map_err(map_error)
}

fn command_to_conversation(error: CommandError) -> ConversationCommandError {
    ConversationCommandError::new(error.code, error.message)
}

/// Fixed public-error vocabulary mirrored from `apps/desktop/src/lib/execGateway.ts`
/// (`EXEC_PUBLIC_ERRORS`). Private until product Tauri commands land (D3B scope).
#[allow(dead_code)] // exercised by unit tests; product commands are out of D3AR2C scope
fn is_exec_public_error(code: &str) -> bool {
    matches!(
        code,
        "process_ownership_mismatch"
            | "process_invalid_decision"
            | "process_not_live"
            | "process_expired"
            | "process_unavailable"
            | "process_output_unavailable"
            | "process_cleanup_unproven"
            | "process_path_rejected"
            | "process_conflict"
    )
}

#[cfg(test)]
mod c4_gateway_tests {
    #[test]
    fn c4_gateway_rejects_invalid_public_error_enum() {
        assert!(super::is_exec_public_error("process_unavailable"));
        assert!(!super::is_exec_public_error("native_error"));
    }
}
