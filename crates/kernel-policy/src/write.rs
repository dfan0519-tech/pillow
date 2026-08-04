//! Write proposal approval state machine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Proposal time-to-live after durable creation (seconds).
pub const WRITE_PROPOSAL_TTL_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteProposalState {
    PendingApproval,
    Approved,
    Applying,
    Succeeded,
    Rejected,
    Expired,
    Cancelled,
    Interrupted,
    Failed,
}

impl WriteProposalState {
    pub const ALL: &'static [Self] = &[
        Self::PendingApproval,
        Self::Approved,
        Self::Applying,
        Self::Succeeded,
        Self::Rejected,
        Self::Expired,
        Self::Cancelled,
        Self::Interrupted,
        Self::Failed,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteDecision {
    ApproveOnce,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyError {
    #[error("illegal write proposal transition from {from:?} to {to:?}")]
    IllegalTransition {
        from: WriteProposalState,
        to: WriteProposalState,
    },
    #[error("write proposal is already terminal")]
    AlreadyTerminal { state: WriteProposalState },
    #[error("write proposal has expired")]
    Expired,
    #[error("write decision is not allowed in state {state:?}")]
    DecisionNotAllowed { state: WriteProposalState },
}

/// Terminal states never accept further transitions.
pub fn is_terminal(state: WriteProposalState) -> bool {
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

/// Expiry is true at `now >= expires_at`.
pub fn is_expired(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now >= expires_at
}

/// Validate a pure state transition. Terminal states never reopen.
pub fn validate_write_transition(
    from: WriteProposalState,
    to: WriteProposalState,
) -> Result<(), PolicyError> {
    if from == to {
        return Err(PolicyError::IllegalTransition { from, to });
    }
    if is_terminal(from) {
        return Err(PolicyError::IllegalTransition { from, to });
    }

    let allowed = match from {
        WriteProposalState::PendingApproval => matches!(
            to,
            WriteProposalState::Approved
                | WriteProposalState::Rejected
                | WriteProposalState::Expired
                | WriteProposalState::Cancelled
                | WriteProposalState::Interrupted
        ),
        WriteProposalState::Approved => matches!(
            to,
            WriteProposalState::Applying
                | WriteProposalState::Expired
                | WriteProposalState::Cancelled
                | WriteProposalState::Interrupted
        ),
        WriteProposalState::Applying => matches!(
            to,
            WriteProposalState::Succeeded
                | WriteProposalState::Failed
                | WriteProposalState::Interrupted
        ),
        WriteProposalState::Succeeded
        | WriteProposalState::Rejected
        | WriteProposalState::Expired
        | WriteProposalState::Cancelled
        | WriteProposalState::Interrupted
        | WriteProposalState::Failed => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(PolicyError::IllegalTransition { from, to })
    }
}

/// Apply a single human decision to a non-terminal proposal.
///
/// Decisions are only accepted from [`WriteProposalState::PendingApproval`].
/// If the proposal is already past `expires_at`, returns
/// [`PolicyError::Expired`] without mutating authority.
pub fn apply_write_decision(
    state: WriteProposalState,
    decision: WriteDecision,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<WriteProposalState, PolicyError> {
    if is_terminal(state) {
        return Err(PolicyError::AlreadyTerminal { state });
    }
    if is_expired(expires_at, now) {
        return Err(PolicyError::Expired);
    }
    if state != WriteProposalState::PendingApproval {
        return Err(PolicyError::DecisionNotAllowed { state });
    }

    let next = match decision {
        WriteDecision::ApproveOnce => WriteProposalState::Approved,
        WriteDecision::Reject => WriteProposalState::Rejected,
    };
    validate_write_transition(state, next)?;
    Ok(next)
}
