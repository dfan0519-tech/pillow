//! Governed tool/process/write contract types shared by adapters and the host.
//!
//! These are pure result/proposal DTOs. Host execution, bridge errors, and
//! coordinator logic live in `keeper-runtime-host`.

use serde_json::Value;

use crate::tool::{FsWriteOutput, ProcessRunOutput, ValidatedToolResult, WritePublicErrorCode};
use crate::types::{
    ProcessProposalDecidedPayload, ProcessProposalRequestedPayload, ProcessStartedPayload,
    ProcessTerminalOutcomePayload, WriteProposalRequestedPayload, WriteProposalResolvedPayload,
};
use keeper_policy::ExecProposalState;

// ── Provider proposals (adapter → host, no project authority) ───────────────

/// Provider-facing read/tool proposal after adapter normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolProposal {
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub arguments: Value,
}

/// Provider-facing write proposal after adapter normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWriteProposal {
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub arguments: Value,
}

/// Provider-facing process proposal after adapter normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProcessProposal {
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub arguments: Value,
}

// ── Delivery markers ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedReadDelivery {
    Executed,
    Replayed,
}

/// Fresh proposal executed through the live approval path, or terminal durable
/// evidence returned without a second apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernedWriteDelivery {
    /// Fresh proposal executed through the live approval path.
    Executed,
    /// Terminal durable evidence returned without a second apply.
    Replayed,
}

/// Fresh proposal executed through the live approval path, or terminal durable
/// evidence returned without a second spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedProcessDelivery {
    /// Fresh proposal executed through the live approval path.
    Executed,
    /// Terminal durable evidence returned without a second spawn.
    Replayed,
}

// ── Provider receipts ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GovernedReadReceipt {
    pub result: ValidatedToolResult,
    pub delivery: GovernedReadDelivery,
}

/// Provider receipt after a write proposal reaches a terminal (or interrupted) outcome.
#[derive(Clone)]
pub struct GovernedWriteReceipt {
    pub delivery: GovernedWriteDelivery,
    pub requested: Option<WriteProposalRequestedPayload>,
    pub resolved: WriteProposalResolvedPayload,
    pub provider_ok: Option<FsWriteOutput>,
    pub provider_error: Option<WritePublicErrorCode>,
}

impl std::fmt::Debug for GovernedWriteReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GovernedWriteReceipt")
            .field("delivery", &self.delivery)
            .field("requested", &self.requested)
            .field("resolved", &self.resolved)
            .field("provider_ok", &self.provider_ok)
            .field("provider_error", &self.provider_error)
            .finish()
    }
}

/// Provider receipt after a process proposal reaches a terminal outcome.
#[derive(Clone, Debug)]
pub struct GovernedProcessReceipt {
    pub delivery: GovernedProcessDelivery,
    pub proposal_id: String,
    pub state: ExecProposalState,
    /// Typed protocol payloads for the caller-emits-event pattern (durable order).
    pub requested: Option<ProcessProposalRequestedPayload>,
    pub decided: Option<ProcessProposalDecidedPayload>,
    pub started: Option<ProcessStartedPayload>,
    pub terminal: Option<ProcessTerminalOutcomePayload>,
    pub provider_ok: Option<ProcessRunOutput>,
    pub cancelled: bool,
    pub timed_out: bool,
}
