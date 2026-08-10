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

/// Provider-facing raw shell command after ACP adapter normalization.
///
/// This is deliberately not a [`ProviderProcessProposal`]: callers cannot put
/// a shell command into the structured `process.run@1` tool contract.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderShellProposal {
    pub call_id: String,
    /// The complete command string shown for approval and later executed.
    pub command: String,
    /// Project-relative portable cwd (`.` when ACP omitted `cwd`).
    pub portable_cwd: Option<String>,
}

impl std::fmt::Debug for ProviderShellProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderShellProposal")
            .field("call_id", &self.call_id)
            .field("command", &"<redacted>")
            .field("portable_cwd", &self.portable_cwd)
            .finish()
    }
}

#[cfg(test)]
mod shell_contract_tests {
    use super::*;
    use keeper_policy::ExecProposalState;

    #[test]
    fn governed_shell_receipt_retains_the_exact_approved_command_but_redacts_debug() {
        let command = "printf 'left right' && printf quoted";
        let receipt = GovernedShellReceipt {
            process: GovernedProcessReceipt {
                delivery: GovernedProcessDelivery::Executed,
                proposal_id: "proposal-shell".into(),
                state: ExecProposalState::Rejected,
                requested: None,
                decided: None,
                started: None,
                terminal: None,
                provider_ok: None,
                cancelled: false,
                timed_out: false,
            },
            approved_command: command.into(),
            output: None,
        };

        assert_eq!(receipt.approved_command, command);
        assert!(!format!("{receipt:?}").contains(command));
    }
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

/// Memory-only bounded output retained for an ACP terminal response.
#[derive(Clone, PartialEq, Eq)]
pub struct GovernedShellOutput {
    pub output: String,
    pub truncated: bool,
}

impl std::fmt::Debug for GovernedShellOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GovernedShellOutput")
            .field("output", &"<redacted>")
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// Receipt for the distinct governed shell path.
///
/// `approved_command` is the exact proposal subject. On a fresh delivery it
/// must also be present as the sole argv entry in `process.requested`.
#[derive(Clone)]
pub struct GovernedShellReceipt {
    pub process: GovernedProcessReceipt,
    pub approved_command: String,
    pub output: Option<GovernedShellOutput>,
}

impl std::fmt::Debug for GovernedShellReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GovernedShellReceipt")
            .field("process", &self.process)
            .field("approved_command", &"<redacted>")
            .field("output", &self.output)
            .finish()
    }
}
