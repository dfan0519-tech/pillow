//! Outcome attribution back to initiator and target slots.
//!
//! Storage is out of scope: this module only defines the audit-shaped record
//! and pure conversion helpers.

use crate::assignment::{Assignment, AssignmentId};
use crate::error::AttributeError;
use crate::types::{GovernanceCapability, SlotId};

/// Bounded process output fields for audit (no raw secret material).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcessOutputRecord {
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    /// Digest or truncated preview — callers decide encoding.
    pub stdout_digest: Option<String>,
    pub stderr_digest: Option<String>,
    pub truncated: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

/// Auditable outcome of one assignment, attributed to initiator and target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentOutcome {
    pub assignment_id: AssignmentId,
    pub initiator_slot_id: SlotId,
    pub target_slot_id: SlotId,
    pub output: ProcessOutputRecord,
    /// Frozen capability from the assignment (auth-derived).
    pub governance: GovernanceCapability,
    /// Whether execution went through the governed process path.
    ///
    /// **Must be false** when `governance == None`. Elevation is rejected.
    pub claimed_governed_execution: bool,
}

/// Inputs for attributing a process result to an assignment.
#[derive(Debug, Clone)]
pub struct AttributeInput {
    pub assignment_id: AssignmentId,
    pub initiator_slot_id: SlotId,
    pub target_slot_id: SlotId,
    /// Auth-derived capability frozen on the assignment.
    pub governance: GovernanceCapability,
    pub output: ProcessOutputRecord,
    /// Caller claim: did the host run this via the governed process chain?
    pub claimed_governed_execution: bool,
}

/// Build an attributed outcome. Fails closed on governance elevation or id mismatch.
pub fn attribute_outcome(input: AttributeInput) -> Result<AssignmentOutcome, AttributeError> {
    // Ungoverned targets (LocalCli) may only be recorded — never claimed governed.
    if input.claimed_governed_execution && !input.governance.is_governed() {
        return Err(AttributeError::GovernanceElevationForbidden);
    }

    Ok(AssignmentOutcome {
        assignment_id: input.assignment_id,
        initiator_slot_id: input.initiator_slot_id,
        target_slot_id: input.target_slot_id,
        output: input.output,
        governance: input.governance,
        claimed_governed_execution: input.claimed_governed_execution,
    })
}

/// Attribute using the assignment aggregate as the source of truth for ids and
/// governance. Rejects elevation and id mismatches.
pub fn attribute_outcome_for_assignment(
    assignment: &Assignment,
    output: ProcessOutputRecord,
    claimed_governed_execution: bool,
) -> Result<AssignmentOutcome, AttributeError> {
    attribute_outcome(AttributeInput {
        assignment_id: assignment.id,
        initiator_slot_id: assignment.initiator_slot_id,
        target_slot_id: assignment.target_slot_id,
        governance: assignment.governance,
        output,
        claimed_governed_execution,
    })
}
