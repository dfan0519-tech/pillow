//! Process proposal approval state machine.
//!
//! Separate vocabulary from write proposals. C4.0 uses `Running` as the sole
//! execution-claim state (`Approved -> Running`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Process proposal time-to-live after durable creation (seconds).
pub const EXEC_PROPOSAL_TTL_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecProposalState {
    PendingApproval,
    Approved,
    Running,
    Succeeded,
    Failed,
    Rejected,
    Expired,
    Cancelled,
    Interrupted,
}

impl ExecProposalState {
    pub const ALL: &'static [Self] = &[
        Self::PendingApproval,
        Self::Approved,
        Self::Running,
        Self::Succeeded,
        Self::Failed,
        Self::Rejected,
        Self::Expired,
        Self::Cancelled,
        Self::Interrupted,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecDecision {
    ApproveOnce,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecPolicyError {
    #[error("illegal exec proposal transition from {from:?} to {to:?}")]
    IllegalTransition {
        from: ExecProposalState,
        to: ExecProposalState,
    },
    #[error("exec proposal is already terminal")]
    AlreadyTerminal { state: ExecProposalState },
    #[error("exec proposal has expired")]
    Expired,
    #[error("exec decision is not allowed in state {state:?}")]
    DecisionNotAllowed { state: ExecProposalState },
}

/// Terminal states never accept further transitions.
pub fn is_exec_terminal(state: ExecProposalState) -> bool {
    matches!(
        state,
        ExecProposalState::Succeeded
            | ExecProposalState::Failed
            | ExecProposalState::Rejected
            | ExecProposalState::Expired
            | ExecProposalState::Cancelled
            | ExecProposalState::Interrupted
    )
}

/// Expiry is true at `now >= expires_at`.
pub fn is_exec_expired(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now >= expires_at
}

/// Validate a pure process-proposal state transition. Terminal states never reopen.
///
/// Allowed graph:
/// - `PendingApproval` → `Approved` | `Rejected` | `Expired` | `Cancelled` | `Interrupted`
/// - `Approved` → `Running` | `Expired` | `Cancelled` | `Interrupted`
/// - `Running` → `Succeeded` | `Failed` | `Cancelled` | `Interrupted`
///
/// `Approved -> Running` is the only execution claim.
pub fn validate_exec_transition(
    from: ExecProposalState,
    to: ExecProposalState,
) -> Result<(), ExecPolicyError> {
    if from == to {
        return Err(ExecPolicyError::IllegalTransition { from, to });
    }
    if is_exec_terminal(from) {
        return Err(ExecPolicyError::IllegalTransition { from, to });
    }

    let allowed = match from {
        ExecProposalState::PendingApproval => matches!(
            to,
            ExecProposalState::Approved
                | ExecProposalState::Rejected
                | ExecProposalState::Expired
                | ExecProposalState::Cancelled
                | ExecProposalState::Interrupted
        ),
        ExecProposalState::Approved => matches!(
            to,
            ExecProposalState::Running
                | ExecProposalState::Expired
                | ExecProposalState::Cancelled
                | ExecProposalState::Interrupted
        ),
        ExecProposalState::Running => matches!(
            to,
            ExecProposalState::Succeeded
                | ExecProposalState::Failed
                | ExecProposalState::Cancelled
                | ExecProposalState::Interrupted
        ),
        ExecProposalState::Succeeded
        | ExecProposalState::Failed
        | ExecProposalState::Rejected
        | ExecProposalState::Expired
        | ExecProposalState::Cancelled
        | ExecProposalState::Interrupted => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(ExecPolicyError::IllegalTransition { from, to })
    }
}

/// Apply a single human decision to a non-terminal process proposal.
///
/// Decisions are only accepted from [`ExecProposalState::PendingApproval`].
/// If the proposal is already past `expires_at`, returns
/// [`ExecPolicyError::Expired`] without mutating authority.
pub fn apply_exec_decision(
    state: ExecProposalState,
    decision: ExecDecision,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<ExecProposalState, ExecPolicyError> {
    if is_exec_terminal(state) {
        return Err(ExecPolicyError::AlreadyTerminal { state });
    }
    if is_exec_expired(expires_at, now) {
        return Err(ExecPolicyError::Expired);
    }
    if state != ExecProposalState::PendingApproval {
        return Err(ExecPolicyError::DecisionNotAllowed { state });
    }

    let next = match decision {
        ExecDecision::ApproveOnce => ExecProposalState::Approved,
        ExecDecision::Reject => ExecProposalState::Rejected,
    };
    validate_exec_transition(state, next)?;
    Ok(next)
}
