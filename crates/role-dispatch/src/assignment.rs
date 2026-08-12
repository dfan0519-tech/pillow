//! Assignment aggregate: initiator, target, task, proposed command, state.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::command::{ProposedCommand, TargetSlotRecord, build_proposed_command};
use crate::error::{ProposeError, TransitionError};
use crate::transition::apply_assignment_transition;
use crate::types::{AssignmentState, GovernanceCapability, SlotId, SlotRole};

/// Stable assignment identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssignmentId(pub Uuid);

impl AssignmentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AssignmentId {
    fn default() -> Self {
        Self::new()
    }
}

/// How the controller addressed the target (slot id and/or role).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentTarget {
    Slot(SlotId),
    Role(SlotRole),
    /// Both role and slot were specified (role must match the slot).
    SlotAndRole {
        slot_id: SlotId,
        role: SlotRole,
    },
}

/// Fresh assignment ready to enter the host process-governance path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAssignment {
    pub id: AssignmentId,
    pub initiator_slot_id: SlotId,
    pub target_slot_id: SlotId,
    pub target_role: SlotRole,
    pub target_address: AssignmentTarget,
    pub task_text: String,
    pub proposed_command: ProposedCommand,
    pub governance: GovernanceCapability,
    pub state: AssignmentState,
}

/// Durable-shaped assignment record (types only; storage is out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub id: AssignmentId,
    pub initiator_slot_id: SlotId,
    pub target_slot_id: SlotId,
    pub target_role: SlotRole,
    pub target_address: AssignmentTarget,
    pub task_text: String,
    pub proposed_command: ProposedCommand,
    /// Frozen from target auth at propose time.
    pub governance: GovernanceCapability,
    pub state: AssignmentState,
}

impl From<NewAssignment> for Assignment {
    fn from(value: NewAssignment) -> Self {
        Self {
            id: value.id,
            initiator_slot_id: value.initiator_slot_id,
            target_slot_id: value.target_slot_id,
            target_role: value.target_role,
            target_address: value.target_address,
            task_text: value.task_text,
            proposed_command: value.proposed_command,
            governance: value.governance,
            state: value.state,
        }
    }
}

impl Assignment {
    /// Advance the assignment through a validated state edge.
    pub fn transition(&mut self, to: AssignmentState) -> Result<(), TransitionError> {
        self.state = apply_assignment_transition(self.state, to)?;
        Ok(())
    }
}

/// Inputs for building a proposed assignment (pure; caller supplies resolved target).
#[derive(Debug, Clone)]
pub struct BuildAssignmentInput {
    pub initiator_slot_id: SlotId,
    /// `None` means the target could not be resolved (missing).
    pub target: Option<TargetSlotRecord>,
    /// When addressing by role (or role+slot), the role that was requested.
    pub requested_role: Option<SlotRole>,
    /// When addressing by slot id, the id that was requested (for missing reports).
    pub requested_slot_id: Option<SlotId>,
    pub task_text: String,
    pub cwd: Option<String>,
}

/// Construct a new assignment from initiator + resolved target + task text.
///
/// Rejections are always explicit ([`ProposeError`]): self-dispatch, missing
/// target, role mismatch, empty task, empty program, shell-join (via separate
/// API), etc.
pub fn build_assignment(input: BuildAssignmentInput) -> Result<NewAssignment, ProposeError> {
    let Some(target) = input.target.as_ref() else {
        return Err(ProposeError::TargetMissing {
            slot_id: input.requested_slot_id,
            role: input.requested_role.clone(),
        });
    };

    if input.initiator_slot_id == target.slot_id {
        return Err(ProposeError::SelfDispatch {
            slot_id: target.slot_id,
        });
    }

    if let Some(expected) = input.requested_role.as_ref()
        && expected != &target.role
    {
        return Err(ProposeError::RoleMismatch {
            expected: expected.clone(),
            actual: target.role.clone(),
        });
    }

    let proposed_command =
        build_proposed_command(&target.auth, &input.task_text, input.cwd.as_deref())?;

    // Governance on the assignment is the auth-derived tag — never upgraded.
    let governance = target.auth.governance();
    debug_assert_eq!(governance, proposed_command.governance);

    let target_address = match (input.requested_slot_id, input.requested_role.clone()) {
        (Some(slot_id), Some(role)) => AssignmentTarget::SlotAndRole { slot_id, role },
        (Some(slot_id), None) => AssignmentTarget::Slot(slot_id),
        (None, Some(role)) => AssignmentTarget::Role(role),
        (None, None) => AssignmentTarget::Slot(target.slot_id),
    };

    Ok(NewAssignment {
        id: AssignmentId::new(),
        initiator_slot_id: input.initiator_slot_id,
        target_slot_id: target.slot_id,
        target_role: target.role.clone(),
        target_address,
        task_text: input.task_text.trim().to_owned(),
        proposed_command,
        governance,
        state: AssignmentState::Proposed,
    })
}
