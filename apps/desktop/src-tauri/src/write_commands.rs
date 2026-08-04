//! Typed Tauri commands for governed write proposals.

use std::sync::Arc;

use tauri::{State, WebviewWindow};

use crate::app_state::{AppState, CommandError};
use crate::conversation_service::{
    ConversationCommandError, ConversationService, WriteDiffDto, WriteProposalDto,
};

fn map_error(error: ConversationCommandError) -> CommandError {
    CommandError {
        code: error.code,
        message: error.message,
    }
}

#[tauri::command]
pub fn list_write_proposals<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    conversation_id: String,
) -> Result<Vec<WriteProposalDto>, CommandError> {
    let (id, project_id, root) = service
        .conversation_project_context(&conversation_id)
        .map_err(map_error)?;
    // Require an already-bound window conversation capability — never create or
    // consume conversation authority as part of a write list request.
    let _ = state.require_conversation_root_for_window(window.label(), project_id, id, &root)?;
    service
        .list_write_proposals(conversation_id)
        .map_err(map_error)
}

#[tauri::command]
pub fn decide_write_proposal<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
    approval_grant: String,
    decision: String,
) -> Result<WriteProposalDto, CommandError> {
    let (id, project_id, root) = service
        .conversation_project_context(&conversation_id)
        .map_err(map_error)?;
    let _ = state.require_conversation_root_for_window(window.label(), project_id, id, &root)?;
    // Reject decision payloads that try to smuggle digests/content via unused params:
    // the command signature only accepts conversationId, proposalId, approvalGrant, decision.
    service
        .decide_write_proposal(conversation_id, proposal_id, approval_grant, decision)
        .map_err(map_error)
}

#[tauri::command]
pub fn open_write_proposal_diff<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    conversation_id: String,
    proposal_id: String,
) -> Result<WriteDiffDto, CommandError> {
    let (id, project_id, root) = service
        .conversation_project_context(&conversation_id)
        .map_err(map_error)?;
    let _ = state.require_conversation_root_for_window(window.label(), project_id, id, &root)?;
    service
        .open_write_proposal_diff(conversation_id, proposal_id)
        .map_err(map_error)
}
