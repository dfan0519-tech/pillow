//! Typed Tauri commands for the conversation vertical slice.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app_state::CommandError;
use crate::conversation_events::{ConversationDto, SendTurnResult, TimelineDto};
use crate::conversation_service::{ConversationCommandError, ConversationService};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAcceptanceScenario {
    phase: String,
    project_name: String,
    project_root: String,
    prompt: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAcceptanceResult {
    phase: String,
    status: String,
    project_id: String,
    conversation_id: String,
    run_id: String,
    run_status: String,
    event_count: usize,
    error_message: Option<String>,
}

fn map_error(error: ConversationCommandError) -> CommandError {
    CommandError {
        code: error.code,
        message: error.message,
    }
}

#[tauri::command]
pub fn list_conversations(
    service: State<'_, Arc<ConversationService>>,
    project_id: String,
) -> Result<Vec<ConversationDto>, CommandError> {
    service.list_conversations(project_id).map_err(map_error)
}

#[tauri::command]
pub fn create_conversation(
    service: State<'_, Arc<ConversationService>>,
    project_id: String,
    title: Option<String>,
) -> Result<ConversationDto, CommandError> {
    service
        .create_conversation(project_id, title)
        .map_err(map_error)
}

#[tauri::command]
pub fn load_timeline(
    service: State<'_, Arc<ConversationService>>,
    conversation_id: String,
) -> Result<TimelineDto, CommandError> {
    service.load_timeline(conversation_id).map_err(map_error)
}

#[tauri::command]
pub fn send_turn(
    service: State<'_, Arc<ConversationService>>,
    conversation_id: String,
    content: String,
) -> Result<SendTurnResult, CommandError> {
    service
        .send_turn(conversation_id, content)
        .map_err(map_error)
}

#[tauri::command]
pub fn cancel_run(
    service: State<'_, Arc<ConversationService>>,
    run_id: String,
) -> Result<(), CommandError> {
    service.cancel_run(run_id).map_err(map_error)
}

/// Optional native conversation acceptance scenario (Slice B1 recovery).
#[tauri::command]
pub fn get_conversation_acceptance_scenario() -> Option<ConversationAcceptanceScenario> {
    let phase = std::env::var("KEEPER_IDE_CONVERSATION_ACCEPTANCE_PHASE").ok()?;
    if !matches!(phase.as_str(), "create_stream" | "restore") {
        return None;
    }
    let project_name = std::env::var("KEEPER_IDE_ACCEPTANCE_PROJECT_NAME").ok()?;
    let project_root = std::env::var("KEEPER_IDE_ACCEPTANCE_PROJECT_ROOT").ok()?;
    let prompt = std::env::var("KEEPER_IDE_CONVERSATION_ACCEPTANCE_PROMPT")
        .unwrap_or_else(|_| "summarize the workspace".to_owned());
    if project_name.trim().is_empty() || project_root.trim().is_empty() {
        return None;
    }
    Some(ConversationAcceptanceScenario {
        phase,
        project_name,
        project_root,
        prompt,
    })
}

#[tauri::command]
pub fn complete_conversation_acceptance(
    result: ConversationAcceptanceResult,
) -> Result<(), CommandError> {
    let result_path =
        std::env::var_os("KEEPER_IDE_CONVERSATION_ACCEPTANCE_RESULT").ok_or_else(|| {
            CommandError {
                code: "conversation_acceptance_unavailable",
                message: "Conversation acceptance is not configured.".into(),
            }
        })?;
    let path = std::path::PathBuf::from(result_path);
    if !matches!(result.status.as_str(), "passed" | "failed") {
        return Err(CommandError {
            code: "conversation_acceptance_invalid",
            message: "Invalid conversation acceptance status.".into(),
        });
    }
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "phase": result.phase,
        "status": result.status,
        "projectId": result.project_id,
        "conversationId": result.conversation_id,
        "runId": result.run_id,
        "runStatus": result.run_status,
        "eventCount": result.event_count,
        "errorMessage": result.error_message,
        "ipcBoundary": "real-tauri-webview",
        "pid": std::process::id(),
        "completedAtUnixMs": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|_| CommandError {
        code: "conversation_acceptance_unavailable",
        message: "Failed to serialize conversation acceptance evidence.".into(),
    })?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, &bytes).map_err(|_| CommandError {
        code: "conversation_acceptance_unavailable",
        message: "Failed to write conversation acceptance evidence.".into(),
    })?;
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(CommandError {
            code: "conversation_acceptance_unavailable",
            message: "Failed to replace conversation acceptance evidence.".into(),
        });
    }
    std::fs::rename(temporary, path).map_err(|_| CommandError {
        code: "conversation_acceptance_unavailable",
        message: "Failed to publish conversation acceptance evidence.".into(),
    })?;
    Ok(())
}
