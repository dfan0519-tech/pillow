//! Explicit rejection reasons — never silent `None` / empty Option.

use thiserror::Error;

use crate::types::{AssignmentState, SlotId, SlotRole};

/// Why constructing an assignment / proposed command failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProposeError {
    /// Controller must not dispatch to its own slot.
    #[error("role dispatch refuses self-assignment for slot {slot_id:?}")]
    SelfDispatch { slot_id: SlotId },

    /// Target slot was not found in the caller's resolution.
    #[error("role dispatch target slot was not found")]
    TargetMissing {
        slot_id: Option<SlotId>,
        role: Option<SlotRole>,
    },

    /// Requested role does not match the resolved target slot's role.
    #[error("role dispatch target role mismatch: expected {expected:?}, got {actual:?}")]
    RoleMismatch {
        expected: SlotRole,
        actual: SlotRole,
    },

    /// Shell-joined command strings are never produced by this crate.
    #[error("role dispatch refuses shell-joined command construction")]
    ShellJoinForbidden,

    /// Task text is empty or whitespace-only.
    #[error("role dispatch task text is empty")]
    TaskTextEmpty,

    /// Task text must never become `program` or `cwd`.
    #[error("role dispatch refuses placing task text into program or cwd")]
    TaskTextInProgramOrCwd,

    /// Executable / program path is empty after resolution.
    #[error("role dispatch program is empty")]
    ProgramEmpty,

    /// LocalCli executable path is empty or blank.
    #[error("role dispatch LocalCli executable path is empty")]
    LocalCliExecutableEmpty,

    /// A launch argv element is empty (structured argv must be well-formed).
    #[error("role dispatch launch argv contains an empty element")]
    EmptyArgvElement,
}

impl ProposeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SelfDispatch { .. } => "role_dispatch_self_dispatch",
            Self::TargetMissing { .. } => "role_dispatch_target_missing",
            Self::RoleMismatch { .. } => "role_dispatch_role_mismatch",
            Self::ShellJoinForbidden => "role_dispatch_shell_join_forbidden",
            Self::TaskTextEmpty => "role_dispatch_task_empty",
            Self::TaskTextInProgramOrCwd => "role_dispatch_task_in_program_or_cwd",
            Self::ProgramEmpty => "role_dispatch_program_empty",
            Self::LocalCliExecutableEmpty => "role_dispatch_local_cli_executable_empty",
            Self::EmptyArgvElement => "role_dispatch_empty_argv_element",
        }
    }
}

/// Illegal assignment state transition.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionError {
    #[error("illegal assignment transition from {from:?} to {to:?}")]
    IllegalTransition {
        from: AssignmentState,
        to: AssignmentState,
    },
    #[error("assignment is already terminal in state {state:?}")]
    AlreadyTerminal { state: AssignmentState },
}

impl TransitionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::IllegalTransition { .. } => "role_dispatch_illegal_transition",
            Self::AlreadyTerminal { .. } => "role_dispatch_already_terminal",
        }
    }
}

/// Outcome attribution failure (e.g. governance elevation attempt).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AttributeError {
    /// Ungoverned targets cannot claim governed execution.
    #[error("role dispatch refuses elevating ungoverned outcome to governed")]
    GovernanceElevationForbidden,
    /// Initiator / target ids on the outcome must match the assignment.
    #[error("role dispatch outcome attribution ids do not match assignment")]
    AttributionMismatch,
}

impl AttributeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::GovernanceElevationForbidden => "role_dispatch_governance_elevation",
            Self::AttributionMismatch => "role_dispatch_attribution_mismatch",
        }
    }
}
