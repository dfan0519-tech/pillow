//! Controller role-dispatch DTOs and mapping helpers.
//!
//! The propose path lives on [`ConversationService`] (needs run admission and
//! host process registration). This module owns closed wire shapes only.

use keeper_runtime_host::{LiveProcessProposalView, PreparedDispatch};
use pillow_role_dispatch::Assignment;
use serde::Serialize;

use super::exec_service::{ExecProposalDto, proposal_dto};

/// Closed DTO returned when a role-dispatch proposal is reserved for approval.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDispatchProposalDto {
    pub assignment_id: String,
    pub initiator_slot_id: String,
    pub target_slot_id: String,
    pub target_role: String,
    pub target_role_custom_name: Option<String>,
    /// Auth-derived capability frozen at propose time: `full` | `none`.
    pub governance: String,
    pub task_text: String,
    pub program: String,
    pub argv: Vec<String>,
    pub portable_cwd: Option<String>,
    pub call_id: String,
    pub run_id: String,
    /// Same shape as list/decide process proposals (decision_available, etc.).
    pub proposal: ExecProposalDto,
}

/// In-memory dispatch audit row (host ledger projection).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDispatchRecordDto {
    pub assignment_id: String,
    pub initiator_slot_id: String,
    pub target_slot_id: String,
    pub governance: String,
    pub proposal_id: Option<String>,
    pub call_id: String,
    pub claimed_governed_execution: Option<bool>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
}

pub(super) fn dispatch_proposal_dto(
    prepared: &PreparedDispatch,
    run_id: String,
    view: LiveProcessProposalView,
) -> RoleDispatchProposalDto {
    let (target_role, target_role_custom_name) = role_wire(&prepared.assignment);
    RoleDispatchProposalDto {
        assignment_id: prepared.assignment.id.0.to_string(),
        initiator_slot_id: prepared.assignment.initiator_slot_id.0.to_string(),
        target_slot_id: prepared.assignment.target_slot_id.0.to_string(),
        target_role,
        target_role_custom_name,
        governance: prepared.assignment.governance.as_str().into(),
        task_text: prepared.assignment.task_text.clone(),
        program: prepared.assignment.proposed_command.program.clone(),
        argv: prepared.assignment.proposed_command.argv.clone(),
        portable_cwd: prepared.assignment.proposed_command.cwd.clone(),
        call_id: prepared.call_id.clone(),
        run_id,
        proposal: proposal_dto(view),
    }
}

fn role_wire(assignment: &Assignment) -> (String, Option<String>) {
    match &assignment.target_role {
        pillow_role_dispatch::SlotRole::Controller => ("controller".into(), None),
        pillow_role_dispatch::SlotRole::Librarian => ("librarian".into(), None),
        pillow_role_dispatch::SlotRole::Custom(name) => ("custom".into(), Some(name.clone())),
    }
}
