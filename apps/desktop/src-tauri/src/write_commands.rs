//! Typed Tauri commands for governed write proposals.
//!
//! Decide accepts conversation/proposal/decision only. Raw approval grants
//! stay host-private (same contract as process proposals).

use tauri::{State, WebviewWindow};

use crate::app_state::{AppState, CommandError};
use crate::conversation_service::{ConversationCommandError, WriteDiffDto, WriteProposalDto};
use crate::runtime_service_state::{RuntimeServiceState, require_project_scope};

fn map_error(error: ConversationCommandError) -> CommandError {
    CommandError {
        code: error.code,
        message: error.message,
    }
}

#[tauri::command]
pub fn list_write_proposals<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
) -> Result<Vec<WriteProposalDto>, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            // Listing never creates or consumes conversation authority.
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            service.list_write_proposals(conversation_id)
        })
        .map_err(map_error)
}

#[tauri::command]
pub fn decide_write_proposal<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
    decision: String,
) -> Result<WriteProposalDto, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            // Host resolves the live grant; no renderer grant parameter.
            service.decide_write_proposal(conversation_id, proposal_id, decision)
        })
        .map_err(map_error)
}

#[tauri::command]
pub fn open_write_proposal_diff<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
) -> Result<WriteDiffDto, CommandError> {
    service
        .with_service(|service, active_project_id| {
            let (id, project_id, root) = service.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            service.open_write_proposal_diff(conversation_id, proposal_id)
        })
        .map_err(map_error)
}

fn command_to_conversation(error: CommandError) -> ConversationCommandError {
    ConversationCommandError::new(error.code, error.message)
}
