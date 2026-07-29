//! Provider-neutral identifiers, event vocabulary, payloads, and commands.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::RuntimeError;

// ── Identifiers ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeSessionId(pub Uuid);

impl RuntimeSessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RuntimeSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RuntimeSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Fixed domain vocabulary ─────────────────────────────────────────────────

/// Lifecycle status of a run (domain + host projection).
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

/// Fixed runtime event kinds. Order and names are part of the Slice B1 contract.
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
    TurnCompleted,
    TurnFailed,
    RunCancelled,
    Heartbeat,
}

impl RuntimeEventKind {
    /// Terminal kinds are mutually exclusive: at most one may close a run.
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::TurnCompleted | Self::TurnFailed | Self::RunCancelled
        )
    }

    pub const fn all() -> &'static [RuntimeEventKind] {
        &[
            Self::RunStarted,
            Self::AssistantDelta,
            Self::ReasoningSummaryDelta,
            Self::PlanUpdated,
            Self::ToolCallStarted,
            Self::ToolCallOutput,
            Self::ToolCallCompleted,
            Self::SubagentStarted,
            Self::SubagentProgress,
            Self::SubagentCompleted,
            Self::ArtifactProduced,
            Self::TurnCompleted,
            Self::TurnFailed,
            Self::RunCancelled,
            Self::Heartbeat,
        ]
    }
}

// ── Provider-neutral payloads ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RunStartedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistantDeltaPayload {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningSummaryDeltaPayload {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanUpdatedPayload {
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallStartedPayload {
    pub tool_call_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallOutputPayload {
    pub tool_call_id: String,
    pub chunk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallCompletedPayload {
    pub tool_call_id: String,
    pub status: ToolCallStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubagentStartedPayload {
    pub subagent_id: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubagentProgressPayload {
    pub subagent_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubagentCompletedPayload {
    pub subagent_id: String,
    pub status: SubagentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactProducedPayload {
    pub artifact_id: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TurnCompletedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnFailedPayload {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RunCancelledPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatPayload {
    pub tick: u64,
}

// ── Runtime event envelope ──────────────────────────────────────────────────

/// Ordered, provider-neutral event emitted by an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub run_id: RunId,
    pub sequence: u64,
    pub kind: RuntimeEventKind,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl RuntimeEvent {
    /// Build an event after kind-specific payload validation.
    pub fn new(
        run_id: RunId,
        sequence: u64,
        kind: RuntimeEventKind,
        payload: impl Serialize,
        created_at: DateTime<Utc>,
    ) -> Result<Self, RuntimeError> {
        let raw = serde_json::to_value(payload).map_err(|e| RuntimeError::Internal {
            message: format!("payload serialize failed: {e}"),
        })?;
        let payload = crate::validate::validate_payload(kind, &raw)?;
        Ok(Self {
            run_id,
            sequence,
            kind,
            payload,
            created_at,
        })
    }

    /// Validate payload shape for this event's kind (ingress boundary).
    pub fn validate(&self) -> Result<Self, RuntimeError> {
        let payload = crate::validate::validate_payload(self.kind, &self.payload)?;
        Ok(Self {
            run_id: self.run_id,
            sequence: self.sequence,
            kind: self.kind,
            payload,
            created_at: self.created_at,
        })
    }

    pub fn is_terminal(&self) -> bool {
        self.kind.is_terminal()
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartSession {
    /// Opaque project-scoped conversation key known to the host.
    pub conversation_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSession {
    pub session_id: RuntimeSessionId,
    pub conversation_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendTurn {
    pub session_id: RuntimeSessionId,
    pub run_id: RunId,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub healthy: bool,
    pub adapter_name: String,
    pub version: String,
    /// Human-readable detail; never contains credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
