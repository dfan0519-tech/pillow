//! Typed Tauri commands for the conversation vertical slice.

use std::sync::Arc;

use keeper_project::ProjectId;
use keeper_session::ConversationId;
use serde::Serialize;
use tauri::{State, WebviewWindow};
use uuid::Uuid;

use crate::app_state::{AppState, CommandError};
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
pub fn list_conversations<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    project_id: String,
) -> Result<Vec<ConversationDto>, CommandError> {
    let conversations = service
        .list_conversations(project_id.clone())
        .map_err(map_error)?;
    let project_id = parse_project_id(&project_id)?;
    let conversation_ids = conversations
        .iter()
        .map(|conversation| parse_conversation_id(&conversation.id))
        .collect::<Result<Vec<_>, _>>()?;
    state.permit_existing_conversations_for_window(
        window.label(),
        project_id,
        &conversation_ids,
    )?;
    Ok(conversations)
}

#[tauri::command]
pub fn create_conversation<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    project_id: String,
    title: Option<String>,
) -> Result<ConversationDto, CommandError> {
    let bootstrap = state.begin_conversation_for_window(window.label(), project_id.clone())?;
    let (conversation, conversation_id) =
        match service.create_conversation_with_id(project_id, title) {
            Ok(conversation) => conversation,
            Err(error) => {
                state.restore_conversation_bootstrap(bootstrap);
                return Err(map_error(error));
            }
        };
    state.bind_new_conversation(bootstrap, conversation_id);
    Ok(conversation)
}

#[tauri::command]
pub fn load_timeline<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    conversation_id: String,
) -> Result<TimelineDto, CommandError> {
    let (id, project_id, root) = service
        .conversation_project_context(&conversation_id)
        .map_err(map_error)?;
    state.bind_existing_conversation_for_window(window.label(), project_id, id, &root)?;
    service.load_timeline(conversation_id).map_err(map_error)
}

#[tauri::command]
pub fn send_turn<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, Arc<ConversationService>>,
    window: WebviewWindow<R>,
    conversation_id: String,
    content: String,
) -> Result<SendTurnResult, CommandError> {
    let (id, project_id, root) = service
        .conversation_project_context(&conversation_id)
        .map_err(map_error)?;
    state.require_conversation_root_for_window(window.label(), project_id, id, &root)?;
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
pub fn get_conversation_acceptance_scenario<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
) -> Result<Option<ConversationAcceptanceScenario>, CommandError> {
    let Ok(phase) = std::env::var("PILLOW_CONVERSATION_ACCEPTANCE_PHASE") else {
        return Ok(None);
    };
    if !matches!(phase.as_str(), "create_stream" | "restore") {
        return Ok(None);
    }
    let Ok(project_name) = std::env::var("PILLOW_ACCEPTANCE_PROJECT_NAME") else {
        return Ok(None);
    };
    let Ok(mut project_root) = std::env::var("PILLOW_ACCEPTANCE_PROJECT_ROOT") else {
        return Ok(None);
    };
    let prompt = std::env::var("PILLOW_CONVERSATION_ACCEPTANCE_PROMPT")
        .unwrap_or_else(|_| "summarize the workspace".to_owned());
    if project_name.trim().is_empty() || project_root.trim().is_empty() {
        return Ok(None);
    }
    if phase == "create_stream" {
        project_root =
            state.register_picker_selection(window.label(), std::path::Path::new(&project_root))?;
    }
    Ok(Some(ConversationAcceptanceScenario {
        phase,
        project_name,
        project_root,
        prompt,
    }))
}

fn parse_project_id(value: &str) -> Result<ProjectId, CommandError> {
    Uuid::parse_str(value)
        .map(ProjectId)
        .map_err(|_| invalid_conversation_authority())
}

fn parse_conversation_id(value: &str) -> Result<ConversationId, CommandError> {
    Uuid::parse_str(value)
        .map(ConversationId)
        .map_err(|_| invalid_conversation_authority())
}

fn invalid_conversation_authority() -> CommandError {
    CommandError {
        code: "conversation_authority_invalid",
        message: "Conversation authority could not be established.".into(),
    }
}

#[tauri::command]
pub fn complete_conversation_acceptance(
    result: ConversationAcceptanceResult,
) -> Result<(), CommandError> {
    let result_path =
        std::env::var_os("PILLOW_CONVERSATION_ACCEPTANCE_RESULT").ok_or_else(|| {
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
