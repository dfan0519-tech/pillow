//! Opaque checked outcomes for governed execute results.
//!
//! Callers outside this crate cannot construct a "successful" outcome from a
//! raw public receipt. Production hosts and [`crate::testkit`] doubles must use
//! [`CheckedGovernedOutcome::try_from_receipt`].
//!
//! ```compile_fail
//! use keeper_agent_protocol::{GovernedReadDelivery, GovernedReadReceipt};
//! use keeper_governance_port::GovernedReadOutcome;
//!
//! // Tuple field is private — external crates cannot forge outcomes.
//! let _ = GovernedReadOutcome(todo!(GovernedReadReceipt));
//! ```

use keeper_agent_protocol::{
    GovernedProcessDelivery, GovernedProcessReceipt, GovernedReadReceipt, GovernedWriteDelivery,
    GovernedWriteReceipt, ToolExecutionState, WriteProposalState,
};
use keeper_policy::{ExecProposalState, is_exec_terminal};

use crate::GovernedOutcomeValidationError;

/// Checked conversion between a public protocol receipt and an opaque port outcome.
pub trait CheckedGovernedOutcome<R>: Sized {
    /// Validate `receipt` and wrap it as an opaque outcome.
    fn try_from_receipt(receipt: R) -> Result<Self, GovernedOutcomeValidationError>;

    /// Consume the outcome and return the underlying protocol receipt.
    fn into_receipt(self) -> R;

    /// Borrow the underlying protocol receipt.
    fn receipt(&self) -> &R;
}

/// Opaque, validated governed read outcome.
///
/// Inner receipt is private. Construct only via [`CheckedGovernedOutcome`].
#[derive(Debug, Clone)]
pub struct GovernedReadOutcome(GovernedReadReceipt);

/// Opaque, validated governed write outcome.
///
/// Inner receipt is private. Construct only via [`CheckedGovernedOutcome`].
#[derive(Debug, Clone)]
pub struct GovernedWriteOutcome(GovernedWriteReceipt);

/// Opaque, validated governed process outcome.
///
/// Inner receipt is private. Construct only via [`CheckedGovernedOutcome`].
#[derive(Debug, Clone)]
pub struct GovernedProcessOutcome(GovernedProcessReceipt);

impl CheckedGovernedOutcome<GovernedReadReceipt> for GovernedReadOutcome {
    fn try_from_receipt(
        receipt: GovernedReadReceipt,
    ) -> Result<Self, GovernedOutcomeValidationError> {
        validate_read_receipt(&receipt)?;
        Ok(Self(receipt))
    }

    fn into_receipt(self) -> GovernedReadReceipt {
        self.0
    }

    fn receipt(&self) -> &GovernedReadReceipt {
        &self.0
    }
}

impl CheckedGovernedOutcome<GovernedWriteReceipt> for GovernedWriteOutcome {
    fn try_from_receipt(
        receipt: GovernedWriteReceipt,
    ) -> Result<Self, GovernedOutcomeValidationError> {
        validate_write_receipt(&receipt)?;
        Ok(Self(receipt))
    }

    fn into_receipt(self) -> GovernedWriteReceipt {
        self.0
    }

    fn receipt(&self) -> &GovernedWriteReceipt {
        &self.0
    }
}

impl CheckedGovernedOutcome<GovernedProcessReceipt> for GovernedProcessOutcome {
    fn try_from_receipt(
        receipt: GovernedProcessReceipt,
    ) -> Result<Self, GovernedOutcomeValidationError> {
        validate_process_receipt(&receipt)?;
        Ok(Self(receipt))
    }

    fn into_receipt(self) -> GovernedProcessReceipt {
        self.0
    }

    fn receipt(&self) -> &GovernedProcessReceipt {
        &self.0
    }
}

fn validate_read_receipt(
    receipt: &GovernedReadReceipt,
) -> Result<(), GovernedOutcomeValidationError> {
    // Delivery is a closed two-variant enum; both shapes are legal.
    let _ = receipt.delivery;
    match receipt.result.state() {
        ToolExecutionState::Succeeded
        | ToolExecutionState::Failed
        | ToolExecutionState::Cancelled => Ok(()),
        ToolExecutionState::Proposed
        | ToolExecutionState::Validated
        | ToolExecutionState::Executing => {
            Err(GovernedOutcomeValidationError::InvalidTerminalShape)
        }
    }
}

fn validate_write_receipt(
    receipt: &GovernedWriteReceipt,
) -> Result<(), GovernedOutcomeValidationError> {
    if !is_write_terminal(receipt.resolved.state) {
        return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
    }

    match receipt.delivery {
        GovernedWriteDelivery::Replayed => {
            if receipt.requested.is_some() {
                return Err(GovernedOutcomeValidationError::InvalidDeliveryShape);
            }
        }
        GovernedWriteDelivery::Executed => {
            // Fresh path always carries the Requested payload that was committed.
            if receipt.requested.is_none() {
                return Err(GovernedOutcomeValidationError::InvalidDeliveryShape);
            }
        }
    }

    let has_ok = receipt.provider_ok.is_some();
    let has_err = receipt.provider_error.is_some();
    // Exactly one provider surface: success XOR public error.
    if has_ok == has_err {
        return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
    }

    match receipt.resolved.state {
        WriteProposalState::Succeeded => {
            if !has_ok || has_err || receipt.resolved.error_code.is_some() {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
        }
        WriteProposalState::Rejected
        | WriteProposalState::Expired
        | WriteProposalState::Cancelled
        | WriteProposalState::Interrupted
        | WriteProposalState::Failed => {
            if has_ok || !has_err {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
            if receipt.resolved.error_code != receipt.provider_error {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
        }
        WriteProposalState::PendingApproval
        | WriteProposalState::Approved
        | WriteProposalState::Applying => {
            return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
        }
    }

    Ok(())
}

fn validate_process_receipt(
    receipt: &GovernedProcessReceipt,
) -> Result<(), GovernedOutcomeValidationError> {
    let terminal = receipt
        .terminal
        .as_ref()
        .ok_or(GovernedOutcomeValidationError::InvalidTerminalShape)?;

    if !is_exec_terminal(receipt.state) {
        return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
    }
    if terminal.state != receipt.state {
        return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
    }
    if terminal.proposal_id != receipt.proposal_id {
        return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
    }

    match receipt.delivery {
        GovernedProcessDelivery::Replayed => {
            if receipt.requested.is_some() || receipt.decided.is_some() || receipt.started.is_some()
            {
                return Err(GovernedOutcomeValidationError::InvalidDeliveryShape);
            }
        }
        GovernedProcessDelivery::Executed => {
            // Fresh path always carries the Requested payload committed at reserve.
            if receipt.requested.is_none() {
                return Err(GovernedOutcomeValidationError::InvalidDeliveryShape);
            }
        }
    }

    // Started requires a prior Decided (approve path). Reject/expire omit both.
    if receipt.started.is_some() && receipt.decided.is_none() {
        return Err(GovernedOutcomeValidationError::InvalidLifecycleOrder);
    }

    // Flag / state consistency.
    match receipt.state {
        ExecProposalState::Cancelled => {
            if !receipt.cancelled {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
            if receipt.timed_out {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
        }
        ExecProposalState::Succeeded => {
            if receipt.cancelled || receipt.timed_out {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
            if receipt.provider_ok.is_none() {
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
        }
        ExecProposalState::Failed
        | ExecProposalState::Rejected
        | ExecProposalState::Expired
        | ExecProposalState::Interrupted => {
            // cancelled flag is only for Cancelled terminal; timed_out may pair with Failed.
            if receipt.cancelled && receipt.state != ExecProposalState::Cancelled {
                // Already not Cancelled; a true cancelled flag on non-cancel terminal is conflict.
                return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
            }
        }
        ExecProposalState::PendingApproval
        | ExecProposalState::Approved
        | ExecProposalState::Running => {
            return Err(GovernedOutcomeValidationError::InvalidTerminalShape);
        }
    }

    if let Some(ok) = receipt.provider_ok.as_ref()
        && (ok.cancelled != receipt.cancelled || ok.timed_out != receipt.timed_out)
    {
        return Err(GovernedOutcomeValidationError::ConflictingProviderOutcome);
    }

    if let Some(decided) = receipt.decided.as_ref()
        && decided.proposal_id != receipt.proposal_id
    {
        return Err(GovernedOutcomeValidationError::InvalidLifecycleOrder);
    }
    if let Some(started) = receipt.started.as_ref()
        && started.proposal_id != receipt.proposal_id
    {
        return Err(GovernedOutcomeValidationError::InvalidLifecycleOrder);
    }
    if let Some(requested) = receipt.requested.as_ref()
        && requested.proposal_id != receipt.proposal_id
    {
        return Err(GovernedOutcomeValidationError::InvalidLifecycleOrder);
    }

    Ok(())
}

fn is_write_terminal(state: WriteProposalState) -> bool {
    matches!(
        state,
        WriteProposalState::Succeeded
            | WriteProposalState::Rejected
            | WriteProposalState::Expired
            | WriteProposalState::Cancelled
            | WriteProposalState::Interrupted
            | WriteProposalState::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use keeper_agent_protocol::{
        FsWriteOutput, GovernedReadDelivery, ProcessProposalDecidedPayload,
        ProcessProposalRequestedPayload, ProcessRiskClass, ProcessRunOutput, ProcessStartedPayload,
        ProcessTerminalOutcomePayload, ToolActiveContext, ToolExecutionState, ToolProposal,
        ToolPublicError, ToolPublicErrorCode, ToolResult, ValidatedToolResult, WriteMode,
        WriteProposalRequestedPayload, WriteProposalResolvedPayload, WritePublicErrorCode,
        canonical_arguments_digest,
    };
    use keeper_policy::ExecDecision;
    use serde_json::json;
    use uuid::Uuid;

    fn fixed_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
    }

    fn sample_read_result(state: ToolExecutionState) -> ValidatedToolResult {
        let run_id = keeper_agent_protocol::RunId(Uuid::nil());
        let context = ToolActiveContext::new("project", "conversation", run_id).unwrap();
        let arguments = json!({"path":"a.txt","offset_bytes":0,"max_bytes":64});
        let proposal = ToolProposal {
            project_id: context.project_id().into(),
            conversation_id: context.conversation_id().into(),
            run_id,
            call_id: "call-1".into(),
            tool_id: "fs.read".into(),
            tool_version: 1,
            arguments_digest: canonical_arguments_digest(&arguments).unwrap(),
            arguments,
        }
        .validate_for_context(&context)
        .unwrap();

        let (output, error) = match state {
            ToolExecutionState::Succeeded => (
                Some(json!({
                    "path": "a.txt",
                    "content_kind": "utf8",
                    "content": "hi",
                    "bytes_returned": 2,
                    "truncated": false
                })),
                None,
            ),
            ToolExecutionState::Failed => (
                None,
                Some(ToolPublicError::canonical(
                    ToolPublicErrorCode::PathRejected,
                )),
            ),
            ToolExecutionState::Cancelled => (None, None),
            other => panic!("unexpected state for sample: {other:?}"),
        };

        ToolResult {
            project_id: proposal.project_id().into(),
            conversation_id: proposal.conversation_id().into(),
            run_id,
            call_id: proposal.call_id().into(),
            tool_id: proposal.tool_id().into(),
            tool_version: proposal.tool_version(),
            proposal_fingerprint: proposal.fingerprint().clone(),
            arguments_digest: proposal.normalized_arguments_digest().clone(),
            state,
            output,
            error,
        }
        .validate_for_proposal(&context, &proposal)
        .unwrap()
    }

    fn write_requested() -> WriteProposalRequestedPayload {
        WriteProposalRequestedPayload {
            proposal_id: "wp-1".into(),
            path: "src/a.ts".into(),
            mode: WriteMode::Create,
            summary: "Create src/a.ts".into(),
            expires_at: fixed_time(),
        }
    }

    fn write_resolved(
        state: WriteProposalState,
        error_code: Option<WritePublicErrorCode>,
    ) -> WriteProposalResolvedPayload {
        WriteProposalResolvedPayload {
            proposal_id: "wp-1".into(),
            path: "src/a.ts".into(),
            mode: WriteMode::Create,
            state,
            error_code,
            auto_approved_by_rule: None,
        }
    }

    fn write_ok() -> FsWriteOutput {
        FsWriteOutput {
            path: "src/a.ts".into(),
            mode: WriteMode::Create,
            bytes_written: 3,
            content_digest:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        }
    }

    fn process_requested() -> ProcessProposalRequestedPayload {
        ProcessProposalRequestedPayload {
            proposal_id: "ep-1".into(),
            program: "node".into(),
            argv: vec!["--version".into()],
            portable_cwd: Some(".".into()),
            summary: "\"node\" \"--version\"".into(),
            risk_class: ProcessRiskClass::Standard,
            expires_at: fixed_time(),
        }
    }

    fn process_decided(state: ExecProposalState) -> ProcessProposalDecidedPayload {
        ProcessProposalDecidedPayload {
            proposal_id: "ep-1".into(),
            program: "node".into(),
            decision: ExecDecision::ApproveOnce,
            state,
            auto_approved_by_rule: None,
        }
    }

    fn process_started() -> ProcessStartedPayload {
        ProcessStartedPayload {
            proposal_id: "ep-1".into(),
            program: "node".into(),
            argv: vec!["--version".into()],
            portable_cwd: Some(".".into()),
        }
    }

    fn process_terminal(
        state: ExecProposalState,
        cancelled: Option<bool>,
        timed_out: Option<bool>,
    ) -> ProcessTerminalOutcomePayload {
        ProcessTerminalOutcomePayload {
            proposal_id: "ep-1".into(),
            program: "node".into(),
            state,
            exit_code: if state == ExecProposalState::Succeeded {
                Some(0)
            } else {
                None
            },
            duration_ms: Some(12),
            truncated: Some(false),
            encoding_lossy: Some(false),
            captured_bytes: Some(8),
            cancelled,
            timed_out,
        }
    }

    fn process_ok(cancelled: bool, timed_out: bool) -> ProcessRunOutput {
        ProcessRunOutput {
            exit_code: Some(0),
            duration_ms: 12,
            stdout_sha256: "sha256:out".into(),
            stderr_sha256: "sha256:err".into(),
            captured_bytes: 8,
            truncated: false,
            encoding_lossy: false,
            cancelled,
            timed_out,
        }
    }

    #[test]
    fn legal_read_outcomes_construct() {
        for (state, delivery) in [
            (
                ToolExecutionState::Succeeded,
                GovernedReadDelivery::Executed,
            ),
            (ToolExecutionState::Failed, GovernedReadDelivery::Executed),
            (
                ToolExecutionState::Cancelled,
                GovernedReadDelivery::Executed,
            ),
            (
                ToolExecutionState::Succeeded,
                GovernedReadDelivery::Replayed,
            ),
        ] {
            let receipt = GovernedReadReceipt {
                result: sample_read_result(state),
                delivery,
            };
            let outcome = GovernedReadOutcome::try_from_receipt(receipt).expect("legal read");
            assert_eq!(outcome.receipt().result.state(), state);
            assert_eq!(outcome.receipt().delivery, delivery);
            let back = outcome.into_receipt();
            assert_eq!(back.result.state(), state);
        }
    }

    #[test]
    fn legal_write_outcomes_construct() {
        // Executed success
        let ok = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(write_requested()),
            resolved: write_resolved(WriteProposalState::Succeeded, None),
            provider_ok: Some(write_ok()),
            provider_error: None,
        };
        GovernedWriteOutcome::try_from_receipt(ok).expect("executed success");

        // Executed rejected
        let rejected = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(write_requested()),
            resolved: write_resolved(
                WriteProposalState::Rejected,
                Some(WritePublicErrorCode::ApprovalRejected),
            ),
            provider_ok: None,
            provider_error: Some(WritePublicErrorCode::ApprovalRejected),
        };
        GovernedWriteOutcome::try_from_receipt(rejected).expect("executed rejected");

        // Replayed cancelled (no lifecycle)
        let replayed = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Replayed,
            requested: None,
            resolved: write_resolved(
                WriteProposalState::Cancelled,
                Some(WritePublicErrorCode::Cancelled),
            ),
            provider_ok: None,
            provider_error: Some(WritePublicErrorCode::Cancelled),
        };
        GovernedWriteOutcome::try_from_receipt(replayed).expect("replayed cancelled");
    }

    #[test]
    fn legal_process_outcomes_construct() {
        // Executed success with full lifecycle
        let executed = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: Some(process_requested()),
            decided: Some(process_decided(ExecProposalState::Approved)),
            started: Some(process_started()),
            terminal: Some(process_terminal(
                ExecProposalState::Succeeded,
                Some(false),
                Some(false),
            )),
            provider_ok: Some(process_ok(false, false)),
            cancelled: false,
            timed_out: false,
        };
        GovernedProcessOutcome::try_from_receipt(executed).expect("executed success");

        // Executed rejected (decided, no started)
        let rejected = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Rejected,
            requested: Some(process_requested()),
            decided: Some(ProcessProposalDecidedPayload {
                proposal_id: "ep-1".into(),
                program: "node".into(),
                decision: ExecDecision::Reject,
                state: ExecProposalState::Rejected,
                auto_approved_by_rule: None,
            }),
            started: None,
            terminal: Some(process_terminal(
                ExecProposalState::Rejected,
                Some(false),
                Some(false),
            )),
            provider_ok: None,
            cancelled: false,
            timed_out: false,
        };
        GovernedProcessOutcome::try_from_receipt(rejected).expect("executed rejected");

        // Replayed success (zero lifecycle)
        let replayed = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Replayed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: None,
            decided: None,
            started: None,
            terminal: Some(process_terminal(
                ExecProposalState::Succeeded,
                Some(false),
                Some(false),
            )),
            provider_ok: Some(process_ok(false, false)),
            cancelled: false,
            timed_out: false,
        };
        GovernedProcessOutcome::try_from_receipt(replayed).expect("replayed success");

        // Cancelled terminal
        let cancelled = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Cancelled,
            requested: Some(process_requested()),
            decided: None,
            started: None,
            terminal: Some(process_terminal(
                ExecProposalState::Cancelled,
                Some(true),
                Some(false),
            )),
            provider_ok: None,
            cancelled: true,
            timed_out: false,
        };
        GovernedProcessOutcome::try_from_receipt(cancelled).expect("executed cancelled");
    }

    #[test]
    fn illegal_write_shapes_are_rejected() {
        // Replayed must not carry Requested
        let bad_replay = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Replayed,
            requested: Some(write_requested()),
            resolved: write_resolved(WriteProposalState::Succeeded, None),
            provider_ok: Some(write_ok()),
            provider_error: None,
        };
        assert_eq!(
            GovernedWriteOutcome::try_from_receipt(bad_replay).unwrap_err(),
            GovernedOutcomeValidationError::InvalidDeliveryShape
        );

        // Executed without Requested
        let no_requested = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: None,
            resolved: write_resolved(WriteProposalState::Succeeded, None),
            provider_ok: Some(write_ok()),
            provider_error: None,
        };
        assert_eq!(
            GovernedWriteOutcome::try_from_receipt(no_requested).unwrap_err(),
            GovernedOutcomeValidationError::InvalidDeliveryShape
        );

        // Both provider_ok and provider_error
        let both = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(write_requested()),
            resolved: write_resolved(WriteProposalState::Succeeded, None),
            provider_ok: Some(write_ok()),
            provider_error: Some(WritePublicErrorCode::WriteFailed),
        };
        assert_eq!(
            GovernedWriteOutcome::try_from_receipt(both).unwrap_err(),
            GovernedOutcomeValidationError::ConflictingProviderOutcome
        );

        // Non-terminal resolved state
        let pending = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(write_requested()),
            resolved: write_resolved(WriteProposalState::PendingApproval, None),
            provider_ok: Some(write_ok()),
            provider_error: None,
        };
        assert_eq!(
            GovernedWriteOutcome::try_from_receipt(pending).unwrap_err(),
            GovernedOutcomeValidationError::InvalidTerminalShape
        );

        // Success with error_code set
        let success_with_err = GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(write_requested()),
            resolved: write_resolved(
                WriteProposalState::Succeeded,
                Some(WritePublicErrorCode::WriteFailed),
            ),
            provider_ok: Some(write_ok()),
            provider_error: None,
        };
        assert_eq!(
            GovernedWriteOutcome::try_from_receipt(success_with_err).unwrap_err(),
            GovernedOutcomeValidationError::ConflictingProviderOutcome
        );
    }

    #[test]
    fn illegal_process_shapes_are_rejected() {
        // Missing terminal
        let no_terminal = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Replayed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: None,
            decided: None,
            started: None,
            terminal: None,
            provider_ok: Some(process_ok(false, false)),
            cancelled: false,
            timed_out: false,
        };
        assert_eq!(
            GovernedProcessOutcome::try_from_receipt(no_terminal).unwrap_err(),
            GovernedOutcomeValidationError::InvalidTerminalShape
        );

        // Replayed with lifecycle
        let replay_lifecycle = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Replayed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: Some(process_requested()),
            decided: None,
            started: None,
            terminal: Some(process_terminal(
                ExecProposalState::Succeeded,
                Some(false),
                Some(false),
            )),
            provider_ok: Some(process_ok(false, false)),
            cancelled: false,
            timed_out: false,
        };
        assert_eq!(
            GovernedProcessOutcome::try_from_receipt(replay_lifecycle).unwrap_err(),
            GovernedOutcomeValidationError::InvalidDeliveryShape
        );

        // Started without Decided
        let started_only = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: Some(process_requested()),
            decided: None,
            started: Some(process_started()),
            terminal: Some(process_terminal(
                ExecProposalState::Succeeded,
                Some(false),
                Some(false),
            )),
            provider_ok: Some(process_ok(false, false)),
            cancelled: false,
            timed_out: false,
        };
        assert_eq!(
            GovernedProcessOutcome::try_from_receipt(started_only).unwrap_err(),
            GovernedOutcomeValidationError::InvalidLifecycleOrder
        );

        // Cancelled state without cancelled flag
        let cancel_flag = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Cancelled,
            requested: Some(process_requested()),
            decided: None,
            started: None,
            terminal: Some(process_terminal(
                ExecProposalState::Cancelled,
                Some(false),
                Some(false),
            )),
            provider_ok: None,
            cancelled: false,
            timed_out: false,
        };
        assert_eq!(
            GovernedProcessOutcome::try_from_receipt(cancel_flag).unwrap_err(),
            GovernedOutcomeValidationError::ConflictingProviderOutcome
        );

        // Succeeded without provider_ok
        let success_no_ok = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "ep-1".into(),
            state: ExecProposalState::Succeeded,
            requested: Some(process_requested()),
            decided: Some(process_decided(ExecProposalState::Approved)),
            started: Some(process_started()),
            terminal: Some(process_terminal(
                ExecProposalState::Succeeded,
                Some(false),
                Some(false),
            )),
            provider_ok: None,
            cancelled: false,
            timed_out: false,
        };
        assert_eq!(
            GovernedProcessOutcome::try_from_receipt(success_no_ok).unwrap_err(),
            GovernedOutcomeValidationError::ConflictingProviderOutcome
        );
    }
}
