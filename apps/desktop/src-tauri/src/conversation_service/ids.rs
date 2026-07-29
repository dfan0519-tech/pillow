//! Identifier parsing and session error mapping for conversation commands.

use keeper_project::ProjectId;
use keeper_session::{ConversationId, RunId, SessionError};
use uuid::Uuid;

use super::types::ConversationCommandError;

pub(super) fn parse_project_id(value: &str) -> Result<ProjectId, ConversationCommandError> {
    Uuid::parse_str(value)
        .map(ProjectId)
        .map_err(|_| ConversationCommandError::new("project_not_found", "Project not found."))
}

pub(super) fn parse_conversation_id(
    value: &str,
) -> Result<ConversationId, ConversationCommandError> {
    Uuid::parse_str(value).map(ConversationId).map_err(|_| {
        ConversationCommandError::new("conversation_not_found", "Conversation not found.")
    })
}

pub(super) fn parse_run_id(value: &str) -> Result<RunId, ConversationCommandError> {
    Uuid::parse_str(value)
        .map(RunId)
        .map_err(|_| ConversationCommandError::new("run_not_found", "Run not found."))
}

pub(super) fn map_session_error(error: SessionError) -> ConversationCommandError {
    eprintln!("Keeper IDE conversation store error: {error}");
    match error {
        SessionError::ProjectNotFound(_) => {
            ConversationCommandError::new("project_not_found", "Project not found.")
        }
        SessionError::ConversationNotFound(_) => {
            ConversationCommandError::new("conversation_not_found", "Conversation not found.")
        }
        SessionError::RunNotFound(_) => {
            ConversationCommandError::new("run_not_found", "Run not found.")
        }
        SessionError::IllegalTransition { .. } => ConversationCommandError::new(
            "illegal_run_transition",
            "Illegal run status transition.",
        ),
        SessionError::InvalidSequence { .. } => ConversationCommandError::new(
            "invalid_event_sequence",
            "Invalid runtime event sequence.",
        ),
        SessionError::ConflictingEventKind { .. } => ConversationCommandError::new(
            "conflicting_event_kind",
            "Runtime event kind conflicts with the durable sequence.",
        ),
        SessionError::SequenceOutOfRange(_) => ConversationCommandError::new(
            "invalid_event_sequence",
            "Runtime event sequence out of range.",
        ),
        SessionError::Store(_) => ConversationCommandError::new(
            "conversation_store_unavailable",
            "Conversation storage is temporarily unavailable.",
        ),
    }
}
