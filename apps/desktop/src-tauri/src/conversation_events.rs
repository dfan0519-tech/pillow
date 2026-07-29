//! Typed renderer event envelope for conversation runtime streams.
//!
//! Payloads stay provider-neutral (snake_case fixture shapes). The outer DTO
//! uses camelCase to match the accepted TypeScript `RuntimeEventDto`.

use chrono::{DateTime, Utc};
use keeper_session::{
    Conversation, ConversationId, Message, MessageRole, Run, RunEvent, RunId, RunStatus,
    RuntimeEventKind, Timeline,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tauri event name for live runtime stream updates.
pub const CONVERSATION_RUNTIME_EVENT: &str = "conversation://runtime-event";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDto {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineMessageDto {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDto {
    pub run_id: String,
    pub conversation_id: String,
    pub status: String,
}

/// Renderer-facing runtime event. `kind` + `payload` mirror the protocol fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventDto {
    pub event_id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub sequence: u64,
    pub kind: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDto {
    pub conversation_id: String,
    pub messages: Vec<TimelineMessageDto>,
    pub events: Vec<RuntimeEventDto>,
    pub runs: Vec<RunDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnResult {
    pub run_id: String,
}

impl From<&Conversation> for ConversationDto {
    fn from(conversation: &Conversation) -> Self {
        Self {
            id: conversation.id.0.to_string(),
            project_id: conversation.project_id.0.to_string(),
            title: conversation
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("新对话")
                .to_owned(),
            created_at: conversation.created_at.to_rfc3339(),
            updated_at: conversation.updated_at.to_rfc3339(),
        }
    }
}

impl From<&Message> for TimelineMessageDto {
    fn from(message: &Message) -> Self {
        Self {
            id: message.id.0.to_string(),
            role: message.role.as_str().to_owned(),
            content: message.content.clone(),
            created_at: message.created_at.to_rfc3339(),
        }
    }
}

impl From<&Run> for RunDto {
    fn from(run: &Run) -> Self {
        Self {
            run_id: run.id.0.to_string(),
            conversation_id: run.conversation_id.0.to_string(),
            status: run.status.as_str().to_owned(),
        }
    }
}

/// Map a stored domain event to the renderer DTO.
///
/// Domain sequences are 0-based; the accepted UI/protocol stream is 1-based.
pub fn runtime_event_dto(
    event: &RunEvent,
    conversation_id: ConversationId,
) -> Result<RuntimeEventDto, serde_json::Error> {
    let payload = serde_json::from_str(&event.payload_json)?;
    Ok(RuntimeEventDto {
        event_id: event.event_id.0.to_string(),
        run_id: event.run_id.0.to_string(),
        conversation_id: conversation_id.0.to_string(),
        sequence: event.sequence.saturating_add(1),
        kind: event.kind.as_str().to_owned(),
        payload,
        created_at: event.created_at.to_rfc3339(),
    })
}

pub fn timeline_dto(timeline: &Timeline) -> Result<TimelineDto, serde_json::Error> {
    let conversation_id = timeline.conversation.id;
    let events = timeline
        .events
        .iter()
        .map(|event| runtime_event_dto(event, conversation_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TimelineDto {
        conversation_id: conversation_id.0.to_string(),
        messages: timeline
            .messages
            .iter()
            .filter(|message| matches!(message.role, MessageRole::User | MessageRole::System))
            .map(TimelineMessageDto::from)
            .collect(),
        events,
        runs: timeline.runs.iter().map(RunDto::from).collect(),
    })
}

pub fn status_for_kind(kind: RuntimeEventKind) -> Option<RunStatus> {
    match kind {
        RuntimeEventKind::RunStarted => Some(RunStatus::Running),
        RuntimeEventKind::TurnCompleted => Some(RunStatus::Completed),
        RuntimeEventKind::TurnFailed => Some(RunStatus::Failed),
        RuntimeEventKind::RunCancelled => Some(RunStatus::Cancelled),
        _ => None,
    }
}

/// Sink used by [`crate::conversation_service::ConversationService`] after durable commit.
pub trait RuntimeEventEmitter: Send + Sync {
    fn emit_runtime_event(&self, event: &RuntimeEventDto) -> Result<(), String>;
}

/// Production no-op used until the Tauri app handle is attached.
pub struct NoopEmitter;

impl RuntimeEventEmitter for NoopEmitter {
    fn emit_runtime_event(&self, _event: &RuntimeEventDto) -> Result<(), String> {
        Ok(())
    }
}

type EmitHook = dyn Fn(&RuntimeEventDto) + Send + Sync;

/// Recording emitter for persistence-before-emit proofs.
pub struct RecordingEmitter {
    events: std::sync::Mutex<Vec<RuntimeEventDto>>,
    /// Optional hook invoked at emit time (e.g. assert SQLite already has the row).
    on_emit: Option<Box<EmitHook>>,
}

impl RecordingEmitter {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
            on_emit: None,
        }
    }

    pub fn with_hook<F>(hook: F) -> Self
    where
        F: Fn(&RuntimeEventDto) + Send + Sync + 'static,
    {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
            on_emit: Some(Box::new(hook)),
        }
    }

    pub fn snapshot(&self) -> Vec<RuntimeEventDto> {
        self.events.lock().expect("emitter").clone()
    }

    pub fn clear(&self) {
        self.events.lock().expect("emitter").clear();
    }
}

impl Default for RecordingEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeEventEmitter for RecordingEmitter {
    fn emit_runtime_event(&self, event: &RuntimeEventDto) -> Result<(), String> {
        if let Some(hook) = self.on_emit.as_ref() {
            hook(event);
        }
        self.events
            .lock()
            .map_err(|_| "emitter lock poisoned".to_owned())?
            .push(event.clone());
        Ok(())
    }
}

/// Tauri app-handle emitter. Requires `core:event` listen permission on the window.
pub struct TauriEmitter<R: tauri::Runtime> {
    handle: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriEmitter<R> {
    pub fn new(handle: tauri::AppHandle<R>) -> Self {
        Self { handle }
    }
}

impl<R: tauri::Runtime> RuntimeEventEmitter for TauriEmitter<R> {
    fn emit_runtime_event(&self, event: &RuntimeEventDto) -> Result<(), String> {
        use tauri::Emitter;
        self.handle
            .emit(CONVERSATION_RUNTIME_EVENT, event)
            .map_err(|error| error.to_string())
    }
}

/// Build a live DTO from a freshly persisted domain event (protocol sequence preserved).
pub fn live_runtime_event_dto(
    event_id: String,
    run_id: RunId,
    conversation_id: ConversationId,
    protocol_sequence: u64,
    kind: RuntimeEventKind,
    payload: Value,
    created_at: DateTime<Utc>,
) -> RuntimeEventDto {
    RuntimeEventDto {
        event_id,
        run_id: run_id.0.to_string(),
        conversation_id: conversation_id.0.to_string(),
        sequence: protocol_sequence,
        kind: kind.as_str().to_owned(),
        payload,
        created_at: created_at.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_session::EventId;
    use uuid::Uuid;

    #[test]
    fn domain_sequence_zero_maps_to_protocol_one_on_dto() {
        let event = RunEvent {
            event_id: EventId(Uuid::nil()),
            run_id: RunId(Uuid::nil()),
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"prompt_preview":"x"}"#.into(),
            created_at: Utc::now(),
        };
        let dto = runtime_event_dto(&event, ConversationId(Uuid::nil())).unwrap();
        assert_eq!(dto.sequence, 1);
        assert_eq!(dto.kind, "run_started");
    }
}
