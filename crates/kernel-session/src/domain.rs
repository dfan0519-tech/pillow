//! Identifiers, fixed vocabulary, records and inputs for the session domain.

use chrono::{DateTime, Utc};
use keeper_project::ProjectId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub Uuid);

impl ConversationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Fixed domain vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    pub const ALL: [RunStatus; 7] = [
        RunStatus::Pending,
        RunStatus::Running,
        RunStatus::Cancelling,
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Interrupted,
    ];

    /// Terminal statuses never leave their state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
        )
    }

    /// Non-terminal statuses become `Interrupted` on host restart.
    pub fn is_inflight(self) -> bool {
        !self.is_terminal()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Pending => "pending",
            RunStatus::Running => "running",
            RunStatus::Cancelling => "cancelling",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(RunStatus::Pending),
            "running" => Some(RunStatus::Running),
            "cancelling" => Some(RunStatus::Cancelling),
            "completed" => Some(RunStatus::Completed),
            "failed" => Some(RunStatus::Failed),
            "cancelled" => Some(RunStatus::Cancelled),
            "interrupted" => Some(RunStatus::Interrupted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    RunStarted,
    AssistantDelta,
    ReasoningSummaryDelta,
    PlanUpdated,
    ToolCallStarted,
    ToolCallOutput,
    ToolCallCompleted,
    SubagentStarted,
    SubagentProgress,
    SubagentCompleted,
    ArtifactProduced,
    WriteProposalRequested,
    WriteProposalResolved,
    /// Summary-only process proposal waiting for a one-shot human decision.
    ProcessProposalRequested,
    /// Summary-only process proposal decision (approve-once / reject).
    ProcessProposalDecided,
    /// Process has been claimed and is starting (never for reject/expire).
    ProcessStarted,
    /// Summary-only process terminal outcome (no transcript payload).
    /// Non-terminal at the run level: process end does not close the run.
    ProcessTerminalOutcome,
    TurnCompleted,
    TurnFailed,
    RunCancelled,
    Heartbeat,
}

impl RuntimeEventKind {
    /// Exhaustive fixed vocabulary (B1 / C3 write / C4 process kinds).
    pub const ALL: [RuntimeEventKind; 21] = [
        RuntimeEventKind::RunStarted,
        RuntimeEventKind::AssistantDelta,
        RuntimeEventKind::ReasoningSummaryDelta,
        RuntimeEventKind::PlanUpdated,
        RuntimeEventKind::ToolCallStarted,
        RuntimeEventKind::ToolCallOutput,
        RuntimeEventKind::ToolCallCompleted,
        RuntimeEventKind::SubagentStarted,
        RuntimeEventKind::SubagentProgress,
        RuntimeEventKind::SubagentCompleted,
        RuntimeEventKind::ArtifactProduced,
        RuntimeEventKind::WriteProposalRequested,
        RuntimeEventKind::WriteProposalResolved,
        RuntimeEventKind::ProcessProposalRequested,
        RuntimeEventKind::ProcessProposalDecided,
        RuntimeEventKind::ProcessStarted,
        RuntimeEventKind::ProcessTerminalOutcome,
        RuntimeEventKind::TurnCompleted,
        RuntimeEventKind::TurnFailed,
        RuntimeEventKind::RunCancelled,
        RuntimeEventKind::Heartbeat,
    ];

    /// Terminal kinds are mutually exclusive: at most one may close a run.
    /// Process kinds are never run-terminal.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RuntimeEventKind::TurnCompleted
                | RuntimeEventKind::TurnFailed
                | RuntimeEventKind::RunCancelled
        )
    }

    /// Optional run-status transition driven by this event kind.
    /// Process kinds do not change run status.
    pub fn status_for_kind(self) -> Option<RunStatus> {
        match self {
            RuntimeEventKind::RunStarted => Some(RunStatus::Running),
            RuntimeEventKind::TurnCompleted => Some(RunStatus::Completed),
            RuntimeEventKind::TurnFailed => Some(RunStatus::Failed),
            RuntimeEventKind::RunCancelled => Some(RunStatus::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeEventKind::RunStarted => "run_started",
            RuntimeEventKind::AssistantDelta => "assistant_delta",
            RuntimeEventKind::ReasoningSummaryDelta => "reasoning_summary_delta",
            RuntimeEventKind::PlanUpdated => "plan_updated",
            RuntimeEventKind::ToolCallStarted => "tool_call_started",
            RuntimeEventKind::ToolCallOutput => "tool_call_output",
            RuntimeEventKind::ToolCallCompleted => "tool_call_completed",
            RuntimeEventKind::SubagentStarted => "subagent_started",
            RuntimeEventKind::SubagentProgress => "subagent_progress",
            RuntimeEventKind::SubagentCompleted => "subagent_completed",
            RuntimeEventKind::ArtifactProduced => "artifact_produced",
            RuntimeEventKind::WriteProposalRequested => "write_proposal_requested",
            RuntimeEventKind::WriteProposalResolved => "write_proposal_resolved",
            RuntimeEventKind::ProcessProposalRequested => "process_proposal_requested",
            RuntimeEventKind::ProcessProposalDecided => "process_proposal_decided",
            RuntimeEventKind::ProcessStarted => "process_started",
            RuntimeEventKind::ProcessTerminalOutcome => "process_terminal_outcome",
            RuntimeEventKind::TurnCompleted => "turn_completed",
            RuntimeEventKind::TurnFailed => "turn_failed",
            RuntimeEventKind::RunCancelled => "run_cancelled",
            RuntimeEventKind::Heartbeat => "heartbeat",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "run_started" => Some(RuntimeEventKind::RunStarted),
            "assistant_delta" => Some(RuntimeEventKind::AssistantDelta),
            "reasoning_summary_delta" => Some(RuntimeEventKind::ReasoningSummaryDelta),
            "plan_updated" => Some(RuntimeEventKind::PlanUpdated),
            "tool_call_started" => Some(RuntimeEventKind::ToolCallStarted),
            "tool_call_output" => Some(RuntimeEventKind::ToolCallOutput),
            "tool_call_completed" => Some(RuntimeEventKind::ToolCallCompleted),
            "subagent_started" => Some(RuntimeEventKind::SubagentStarted),
            "subagent_progress" => Some(RuntimeEventKind::SubagentProgress),
            "subagent_completed" => Some(RuntimeEventKind::SubagentCompleted),
            "artifact_produced" => Some(RuntimeEventKind::ArtifactProduced),
            "write_proposal_requested" => Some(RuntimeEventKind::WriteProposalRequested),
            "write_proposal_resolved" => Some(RuntimeEventKind::WriteProposalResolved),
            "process_proposal_requested" => Some(RuntimeEventKind::ProcessProposalRequested),
            "process_proposal_decided" => Some(RuntimeEventKind::ProcessProposalDecided),
            "process_started" => Some(RuntimeEventKind::ProcessStarted),
            "process_terminal_outcome" => Some(RuntimeEventKind::ProcessTerminalOutcome),
            "turn_completed" => Some(RuntimeEventKind::TurnCompleted),
            "turn_failed" => Some(RuntimeEventKind::TurnFailed),
            "run_cancelled" => Some(RuntimeEventKind::RunCancelled),
            "heartbeat" => Some(RuntimeEventKind::Heartbeat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "system" => Some(MessageRole::System),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Domain records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub project_id: ProjectId,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub conversation_id: ConversationId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub event_id: EventId,
    pub run_id: RunId,
    pub sequence: u64,
    pub kind: RuntimeEventKind,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    pub runs: Vec<Run>,
    pub events: Vec<RunEvent>,
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    pub project_id: ProjectId,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRun {
    pub conversation_id: ConversationId,
    /// User turn content persisted as a `Message` with role `user`.
    pub user_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunEvent {
    pub run_id: RunId,
    pub sequence: u64,
    pub kind: RuntimeEventKind,
    pub payload_json: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionError {
    #[error("project not found: {0:?}")]
    ProjectNotFound(ProjectId),
    #[error("conversation not found: {0:?}")]
    ConversationNotFound(ConversationId),
    #[error("run not found: {0:?}")]
    RunNotFound(RunId),
    #[error("illegal run transition from {from:?} to {to:?}")]
    IllegalTransition { from: RunStatus, to: RunStatus },
    /// New events must be strictly contiguous from 0; same-kind `(run_id, sequence)`
    /// replay is handled separately as idempotent success (first durable payload wins).
    #[error("invalid event sequence for run {run_id:?}: expected {expected}, got {got}")]
    InvalidSequence {
        run_id: RunId,
        expected: u64,
        got: u64,
    },
    /// Replay of an existing sequence with a different event kind is not
    /// idempotent: the persisted row and run status must stay unchanged.
    #[error(
        "conflicting event kind for run {run_id:?} sequence {sequence}: \
         persisted {persisted:?}, got {got:?}"
    )]
    ConflictingEventKind {
        run_id: RunId,
        sequence: u64,
        persisted: RuntimeEventKind,
        got: RuntimeEventKind,
    },
    /// SQLite INTEGER is signed; sequences above `i64::MAX` cannot be stored.
    #[error("event sequence {0} exceeds SQLite INTEGER range (i64::MAX)")]
    SequenceOutOfRange(u64),
    #[error("session store error: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_event_kind_round_trips_through_snake_case_strings() {
        for kind in RuntimeEventKind::ALL {
            assert_eq!(RuntimeEventKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(RuntimeEventKind::parse("unknown"), None);
        assert_eq!(
            RuntimeEventKind::parse("process_proposal_requested"),
            Some(RuntimeEventKind::ProcessProposalRequested)
        );
        assert_eq!(
            RuntimeEventKind::parse("process_proposal_decided"),
            Some(RuntimeEventKind::ProcessProposalDecided)
        );
        assert_eq!(
            RuntimeEventKind::parse("process_started"),
            Some(RuntimeEventKind::ProcessStarted)
        );
        assert_eq!(
            RuntimeEventKind::parse("process_terminal_outcome"),
            Some(RuntimeEventKind::ProcessTerminalOutcome)
        );
    }

    #[test]
    fn process_kinds_are_non_terminal_and_do_not_drive_run_status() {
        let process = [
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessStarted,
            RuntimeEventKind::ProcessTerminalOutcome,
        ];
        for kind in process {
            assert!(
                !kind.is_terminal(),
                "{:?} must be non-terminal at run level",
                kind
            );
            assert!(
                kind.status_for_kind().is_none(),
                "{:?} must not derive run status",
                kind
            );
        }
        assert!(RuntimeEventKind::TurnCompleted.is_terminal());
        assert_eq!(
            RuntimeEventKind::TurnCompleted.status_for_kind(),
            Some(RunStatus::Completed)
        );
        assert_eq!(RuntimeEventKind::ALL.len(), 21);
    }
}
