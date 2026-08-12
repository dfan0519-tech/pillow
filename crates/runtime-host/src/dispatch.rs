//! Host-side role-dispatch wiring into the **existing** governed process chain.
//!
//! Domain construction lives in `pillow-role-dispatch`. This module:
//! 1. Builds an assignment via [`build_assignment`]
//! 2. Converts the structured [`ProposedCommand`] into a
//!    [`ProviderProcessProposal`] (never shell-joins)
//! 3. Hands it to [`execute_governed_process`] — the same reserve / approve /
//!    spawn / receipt path used by agent-originated process.run@1
//! 4. Attributes terminal output back to initiator + target slots without
//!    elevating [`GovernanceCapability::None`]
//!
//! There is **no** second approval mechanism, no side executor, and no path
//! that spawns before human (or host rule/mode) approval.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use keeper_agent_protocol::{GovernedProcessReceipt, ProviderProcessProposal, RunId};
use keeper_governance_port::CancellationProbe;
use pillow_role_dispatch::{
    Assignment, AssignmentId, AssignmentOutcome, AssignmentState, AttributeError,
    BuildAssignmentInput, GovernanceCapability, NewAssignment, ProcessOutputRecord, ProposeError,
    ProposedCommand, SlotId, attribute_outcome_for_assignment, build_assignment,
};
use serde_json::json;
use thiserror::Error;

use crate::host::{GovernedProcessBridgeError, GovernedProcessLifecycle, execute_governed_process};

/// Tool identity shared with agent-originated process proposals.
const PROCESS_RUN_TOOL_ID: &str = "process.run";
const PROCESS_RUN_TOOL_VERSION: u16 = 1;

// ── Public types ────────────────────────────────────────────────────────────

/// Prepared dispatch ready to enter the process coordinator.
///
/// Holds a domain assignment and the structured provider proposal. Spawning
/// only happens after the existing approval path approves the proposal.
#[derive(Debug, Clone)]
pub struct PreparedDispatch {
    pub assignment: Assignment,
    pub provider: ProviderProcessProposal,
    /// Stable call id used for durable reservation (also the dispatch audit key
    /// until a proposal_id is known).
    pub call_id: String,
}

/// In-memory audit record linking an assignment to a process proposal.
#[derive(Debug, Clone)]
pub struct DispatchRecord {
    pub assignment_id: AssignmentId,
    pub initiator_slot_id: SlotId,
    pub target_slot_id: SlotId,
    pub governance: GovernanceCapability,
    pub proposal_id: Option<String>,
    pub call_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    /// Frozen proposed command (structured argv only).
    pub program: String,
    pub argv: Vec<String>,
    pub portable_cwd: Option<String>,
    pub assignment_state: AssignmentState,
    pub outcome: Option<AssignmentOutcome>,
}

/// Errors from host-side dispatch wiring.
///
/// Domain rejections preserve [`ProposeError`] so callers can surface the same
/// codes as the pure crate (`role_dispatch_*`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DispatchError {
    #[error(transparent)]
    Propose(#[from] ProposeError),
    #[error(transparent)]
    Attribute(#[from] AttributeError),
    #[error("dispatch target auth shape is unsupported")]
    UnsupportedAuth,
    #[error("dispatch process path rejected the proposal")]
    Process(GovernedProcessBridgeError),
    #[error("dispatch ledger is unavailable")]
    LedgerUnavailable,
}

impl DispatchError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Propose(error) => error.code(),
            Self::Attribute(error) => error.code(),
            Self::UnsupportedAuth => "role_dispatch_unsupported_auth",
            Self::Process(_) => "role_dispatch_process_unavailable",
            Self::LedgerUnavailable => "role_dispatch_ledger_unavailable",
        }
    }
}

// ── Prepare (domain only — no process spawn) ────────────────────────────────

/// Build a structured assignment + provider process proposal.
///
/// Failures from [`build_assignment`] / [`build_proposed_command`] are returned
/// as [`DispatchError::Propose`] with the original [`ProposeError`] codes.
/// This function never spawns a process and never opens a shell string.
pub fn prepare_dispatch(input: BuildAssignmentInput) -> Result<PreparedDispatch, DispatchError> {
    let new = build_assignment(input)?;
    let prepared = prepared_from_new_assignment(new);
    Ok(prepared)
}

fn prepared_from_new_assignment(new: NewAssignment) -> PreparedDispatch {
    let call_id = format!("dispatch-{}", new.id.0);
    let provider = provider_proposal_from_command(&new.proposed_command, call_id.clone());
    PreparedDispatch {
        assignment: Assignment::from(new),
        provider,
        call_id,
    }
}

/// Convert a domain [`ProposedCommand`] into a structured process.run@1 proposal.
///
/// - `program` / `argv` / `cwd` are copied as structured JSON fields
/// - argv is **never** joined into a shell string
/// - task text remains whatever argv element role-dispatch already placed
///   after `--pillow-task` (never moved into program/cwd here)
pub fn provider_proposal_from_command(
    command: &ProposedCommand,
    call_id: String,
) -> ProviderProcessProposal {
    let mut arguments = json!({
        "program": command.program,
        "argv": command.argv,
    });
    if let Some(cwd) = command.cwd.as_ref() {
        arguments
            .as_object_mut()
            .expect("json object")
            .insert("cwd".into(), json!(cwd));
    }
    ProviderProcessProposal {
        call_id,
        tool_id: PROCESS_RUN_TOOL_ID.into(),
        tool_version: PROCESS_RUN_TOOL_VERSION,
        arguments,
    }
}

// ── Process handoff (same chain as agent process.run@1) ─────────────────────

/// Register and wait on a prepared dispatch through [`execute_governed_process`].
///
/// Unapproved proposals remain in `pending_approval` and **never** reach
/// `Started` / process spawn — identical to agent-originated proposals.
///
/// Callers that need a non-blocking Tauri return should run this on a worker
/// thread and surface the [`GovernedProcessLifecycle::Requested`] view.
pub fn execute_prepared_dispatch(
    run_id: RunId,
    prepared: &PreparedDispatch,
    cancellation: &dyn CancellationProbe,
    on_lifecycle: impl FnMut(GovernedProcessLifecycle<'_>) -> Result<(), GovernedProcessBridgeError>,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    execute_governed_process(
        run_id,
        prepared.provider.clone(),
        cancellation,
        on_lifecycle,
    )
}

/// Bind a prepared dispatch into the in-memory ledger when the process
/// proposal is requested (after durable reserve).
pub fn register_dispatch_on_requested(
    prepared: &PreparedDispatch,
    conversation_id: impl Into<String>,
    run_id: RunId,
    proposal_id: impl Into<String>,
) -> Result<DispatchRecord, DispatchError> {
    let proposal_id = proposal_id.into();
    let record = DispatchRecord {
        assignment_id: prepared.assignment.id,
        initiator_slot_id: prepared.assignment.initiator_slot_id,
        target_slot_id: prepared.assignment.target_slot_id,
        governance: prepared.assignment.governance,
        proposal_id: Some(proposal_id.clone()),
        call_id: prepared.call_id.clone(),
        conversation_id: conversation_id.into(),
        run_id,
        program: prepared.assignment.proposed_command.program.clone(),
        argv: prepared.assignment.proposed_command.argv.clone(),
        portable_cwd: prepared.assignment.proposed_command.cwd.clone(),
        assignment_state: AssignmentState::Proposed,
        outcome: None,
    };
    ledger_insert(record.clone())?;
    Ok(record)
}

// ── Attribution (T2) ────────────────────────────────────────────────────────

/// Whether the host may claim governed execution for this capability.
///
/// LocalCli (`None`) must always yield `false` — record only.
pub fn claimed_governed_execution_for(governance: GovernanceCapability) -> bool {
    governance.is_governed()
}

/// Build a process output record from a terminal process receipt (no secrets).
pub fn output_record_from_receipt(receipt: &GovernedProcessReceipt) -> ProcessOutputRecord {
    let terminal = receipt.terminal.as_ref();
    ProcessOutputRecord {
        exit_code: terminal.and_then(|t| t.exit_code),
        duration_ms: terminal.and_then(|t| t.duration_ms),
        stdout_digest: None,
        stderr_digest: None,
        truncated: terminal.and_then(|t| t.truncated).unwrap_or(false),
        cancelled: receipt.cancelled,
        timed_out: receipt.timed_out,
    }
}

/// Attribute a terminal process result to the assignment.
///
/// Uses [`claimed_governed_execution_for`] so LocalCli outcomes cannot be
/// labeled as governed execution.
pub fn attribute_dispatch_receipt(
    assignment: &Assignment,
    receipt: &GovernedProcessReceipt,
) -> Result<AssignmentOutcome, DispatchError> {
    let output = output_record_from_receipt(receipt);
    let claimed = claimed_governed_execution_for(assignment.governance);
    let outcome = attribute_outcome_for_assignment(assignment, output, claimed)?;
    Ok(outcome)
}

/// Advance assignment state after a durable process decision / terminal, then
/// store the outcome on the ledger entry for `proposal_id`.
pub fn complete_dispatch_on_receipt(
    proposal_id: &str,
    assignment: &mut Assignment,
    receipt: &GovernedProcessReceipt,
) -> Result<AssignmentOutcome, DispatchError> {
    // Mirror process terminal into assignment lifecycle (best-effort; illegal
    // edges surface as attribute/ledger errors only after we have an outcome).
    match receipt.state {
        keeper_policy::ExecProposalState::Rejected => {
            let _ = assignment.transition(AssignmentState::Rejected);
        }
        keeper_policy::ExecProposalState::Cancelled
        | keeper_policy::ExecProposalState::Interrupted
        | keeper_policy::ExecProposalState::Expired => {
            let _ = assignment.transition(AssignmentState::Cancelled);
        }
        keeper_policy::ExecProposalState::Succeeded => {
            let _ = assignment.transition(AssignmentState::Approved);
            let _ = assignment.transition(AssignmentState::Running);
            let _ = assignment.transition(AssignmentState::Succeeded);
        }
        keeper_policy::ExecProposalState::Failed => {
            let _ = assignment.transition(AssignmentState::Approved);
            let _ = assignment.transition(AssignmentState::Running);
            let _ = assignment.transition(AssignmentState::Failed);
        }
        keeper_policy::ExecProposalState::Approved
        | keeper_policy::ExecProposalState::Running
        | keeper_policy::ExecProposalState::PendingApproval => {
            // Non-terminal for attribution purposes — still build a record.
        }
    }

    let outcome = attribute_dispatch_receipt(assignment, receipt)?;
    ledger_complete(proposal_id, assignment.state, outcome.clone())?;
    Ok(outcome)
}

// ── Ledger (in-memory, project-scoped lookups by conversation / proposal) ───

struct DispatchLedger {
    by_proposal: HashMap<String, DispatchRecord>,
    by_call: HashMap<String, String>,
}

fn ledger() -> &'static Mutex<DispatchLedger> {
    static LEDGER: OnceLock<Mutex<DispatchLedger>> = OnceLock::new();
    LEDGER.get_or_init(|| {
        Mutex::new(DispatchLedger {
            by_proposal: HashMap::new(),
            by_call: HashMap::new(),
        })
    })
}

fn ledger_insert(record: DispatchRecord) -> Result<(), DispatchError> {
    let mut guard = ledger()
        .lock()
        .map_err(|_| DispatchError::LedgerUnavailable)?;
    if let Some(proposal_id) = record.proposal_id.as_ref() {
        guard
            .by_call
            .insert(record.call_id.clone(), proposal_id.clone());
        guard.by_proposal.insert(proposal_id.clone(), record);
    } else {
        // Proposal id not yet known — index by call only after request.
        let _ = record;
    }
    Ok(())
}

fn ledger_complete(
    proposal_id: &str,
    state: AssignmentState,
    outcome: AssignmentOutcome,
) -> Result<(), DispatchError> {
    let mut guard = ledger()
        .lock()
        .map_err(|_| DispatchError::LedgerUnavailable)?;
    if let Some(entry) = guard.by_proposal.get_mut(proposal_id) {
        entry.assignment_state = state;
        entry.outcome = Some(outcome);
    }
    Ok(())
}

/// Look up a dispatch record by process proposal id.
pub fn get_dispatch_record(proposal_id: &str) -> Result<Option<DispatchRecord>, DispatchError> {
    let guard = ledger()
        .lock()
        .map_err(|_| DispatchError::LedgerUnavailable)?;
    Ok(guard.by_proposal.get(proposal_id).cloned())
}

/// List dispatch records for a conversation (in-memory, this host instance).
pub fn list_dispatch_records(conversation_id: &str) -> Result<Vec<DispatchRecord>, DispatchError> {
    let guard = ledger()
        .lock()
        .map_err(|_| DispatchError::LedgerUnavailable)?;
    Ok(guard
        .by_proposal
        .values()
        .filter(|r| r.conversation_id == conversation_id)
        .cloned()
        .collect())
}

/// Test-only: drop all in-memory dispatch records.
#[cfg(any(test, feature = "test-support"))]
pub fn clear_dispatch_ledger_for_tests() {
    if let Ok(mut guard) = ledger().lock() {
        guard.by_proposal.clear();
        guard.by_call.clear();
    }
}

// ── Auth mapping helpers (fail-closed on unknown variants) ──────────────────

/// Map a storage-shaped auth tag string into a role-dispatch capability without
/// elevating. Unknown tags refuse dispatch.
///
/// Prefer the typed [`map_slot_auth_shape`] path when the full auth enum is
/// available. This helper is for closed wire labels only.
pub fn governance_from_wire_label(label: &str) -> Result<GovernanceCapability, DispatchError> {
    match label {
        "full" => Ok(GovernanceCapability::Full),
        "none" => Ok(GovernanceCapability::None),
        _ => Err(DispatchError::UnsupportedAuth),
    }
}

// ── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_policy::ExecProposalState;
    use pillow_role_dispatch::{
        AssignmentTarget, GOVERNED_CATALOG_PROGRAM, SlotAuthShape, SlotRole, TASK_ARGV_FLAG,
        TargetSlotRecord,
    };
    use uuid::Uuid;

    fn local_cli_target(slot: SlotId, program: &str) -> TargetSlotRecord {
        TargetSlotRecord {
            slot_id: slot,
            role: SlotRole::Custom("worker".into()),
            auth: SlotAuthShape::LocalCli {
                executable_path: program.into(),
                launch_args: vec!["--version".into()],
                status_args: vec![],
            },
        }
    }

    fn governed_target(slot: SlotId) -> TargetSlotRecord {
        TargetSlotRecord {
            slot_id: slot,
            role: SlotRole::Librarian,
            auth: SlotAuthShape::ApiKey {
                model_id: "grok-4".into(),
            },
        }
    }

    #[test]
    fn prepare_builds_structured_provider_proposal_without_shell_join() {
        let initiator = SlotId::new();
        let target_id = SlotId::new();
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: initiator,
            target: Some(local_cli_target(target_id, "node")),
            requested_role: None,
            requested_slot_id: Some(target_id),
            task_text: "run unit tests; echo done".into(),
            cwd: Some(".".into()),
        })
        .unwrap();

        assert_eq!(prepared.assignment.governance, GovernanceCapability::None);
        assert_eq!(prepared.provider.tool_id, PROCESS_RUN_TOOL_ID);
        assert_eq!(prepared.provider.tool_version, PROCESS_RUN_TOOL_VERSION);
        let program = prepared.provider.arguments["program"].as_str().unwrap();
        let argv = prepared.provider.arguments["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(program, "node");
        assert_eq!(argv[0], "--version");
        assert_eq!(argv[1], TASK_ARGV_FLAG);
        assert_eq!(argv[2], "run unit tests; echo done");
        // Never a single shell-joined command string.
        assert!(!argv.iter().any(|a| a.contains("node --version")));
        assert_ne!(program, "run unit tests; echo done");
    }

    #[test]
    fn prepare_keeps_governed_catalog_program_and_full_capability() {
        let initiator = SlotId::new();
        let target_id = SlotId::new();
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: initiator,
            target: Some(governed_target(target_id)),
            requested_role: Some(SlotRole::Librarian),
            requested_slot_id: Some(target_id),
            task_text: "summarize vault".into(),
            cwd: None,
        })
        .unwrap();
        assert_eq!(prepared.assignment.governance, GovernanceCapability::Full);
        assert_eq!(
            prepared.assignment.proposed_command.program,
            GOVERNED_CATALOG_PROGRAM
        );
        assert!(claimed_governed_execution_for(
            prepared.assignment.governance
        ));
    }

    #[test]
    fn propose_errors_surface_original_codes() {
        let id = SlotId::new();
        let cases: Vec<(&str, BuildAssignmentInput)> = vec![
            (
                "role_dispatch_self_dispatch",
                BuildAssignmentInput {
                    initiator_slot_id: id,
                    target: Some(local_cli_target(id, "node")),
                    requested_role: None,
                    requested_slot_id: Some(id),
                    task_text: "x".into(),
                    cwd: None,
                },
            ),
            (
                "role_dispatch_target_missing",
                BuildAssignmentInput {
                    initiator_slot_id: SlotId::new(),
                    target: None,
                    requested_role: None,
                    requested_slot_id: Some(SlotId::new()),
                    task_text: "x".into(),
                    cwd: None,
                },
            ),
            (
                "role_dispatch_role_mismatch",
                BuildAssignmentInput {
                    initiator_slot_id: SlotId::new(),
                    target: Some(TargetSlotRecord {
                        slot_id: SlotId::new(),
                        role: SlotRole::Librarian,
                        auth: SlotAuthShape::GrokLogin { model_id: None },
                    }),
                    requested_role: Some(SlotRole::Controller),
                    requested_slot_id: None,
                    task_text: "x".into(),
                    cwd: None,
                },
            ),
            (
                "role_dispatch_task_empty",
                BuildAssignmentInput {
                    initiator_slot_id: SlotId::new(),
                    target: Some(local_cli_target(SlotId::new(), "node")),
                    requested_role: None,
                    requested_slot_id: None,
                    task_text: "   ".into(),
                    cwd: None,
                },
            ),
            (
                "role_dispatch_task_in_program_or_cwd",
                BuildAssignmentInput {
                    initiator_slot_id: SlotId::new(),
                    target: Some(local_cli_target(SlotId::new(), "node")),
                    requested_role: None,
                    requested_slot_id: None,
                    task_text: "same".into(),
                    cwd: Some("same".into()),
                },
            ),
            (
                "role_dispatch_local_cli_executable_empty",
                BuildAssignmentInput {
                    initiator_slot_id: SlotId::new(),
                    target: Some(TargetSlotRecord {
                        slot_id: SlotId::new(),
                        role: SlotRole::Custom("w".into()),
                        auth: SlotAuthShape::LocalCli {
                            executable_path: "  ".into(),
                            launch_args: vec![],
                            status_args: vec![],
                        },
                    }),
                    requested_role: None,
                    requested_slot_id: None,
                    task_text: "work".into(),
                    cwd: None,
                },
            ),
        ];

        for (code, input) in cases {
            let err = prepare_dispatch(input).unwrap_err();
            assert_eq!(err.code(), code, "expected {code}, got {:?}", err);
            assert!(matches!(err, DispatchError::Propose(_)));
        }

        // Shell-join is always refused via the domain helper (not build path).
        let shell = pillow_role_dispatch::reject_shell_joined_command("bash", "echo hi");
        assert_eq!(shell.code(), "role_dispatch_shell_join_forbidden");
        let wrapped = DispatchError::Propose(shell);
        assert_eq!(wrapped.code(), "role_dispatch_shell_join_forbidden");
    }

    #[test]
    fn empty_argv_element_surfaces_domain_code() {
        let err = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: SlotId::new(),
            target: Some(TargetSlotRecord {
                slot_id: SlotId::new(),
                role: SlotRole::Custom("w".into()),
                auth: SlotAuthShape::LocalCli {
                    executable_path: "node".into(),
                    launch_args: vec!["".into()],
                    status_args: vec![],
                },
            }),
            requested_role: None,
            requested_slot_id: None,
            task_text: "work".into(),
            cwd: None,
        })
        .unwrap_err();
        assert_eq!(err.code(), "role_dispatch_empty_argv_element");
    }

    #[test]
    fn local_cli_attribution_never_claims_governed_execution() {
        let initiator = SlotId::new();
        let target_id = SlotId::new();
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: initiator,
            target: Some(local_cli_target(target_id, "node")),
            requested_role: None,
            requested_slot_id: Some(target_id),
            task_text: "probe".into(),
            cwd: None,
        })
        .unwrap();

        assert!(!claimed_governed_execution_for(
            prepared.assignment.governance
        ));

        let receipt = GovernedProcessReceipt {
            delivery: keeper_agent_protocol::GovernedProcessDelivery::Executed,
            proposal_id: "p1".into(),
            state: ExecProposalState::Succeeded,
            requested: None,
            decided: None,
            started: None,
            terminal: Some(keeper_agent_protocol::ProcessTerminalOutcomePayload {
                proposal_id: "p1".into(),
                program: "node".into(),
                state: ExecProposalState::Succeeded,
                exit_code: Some(0),
                duration_ms: Some(12),
                truncated: Some(false),
                encoding_lossy: Some(false),
                captured_bytes: Some(0),
                cancelled: Some(false),
                timed_out: Some(false),
            }),
            provider_ok: None,
            cancelled: false,
            timed_out: false,
        };

        let outcome = attribute_dispatch_receipt(&prepared.assignment, &receipt).unwrap();
        assert_eq!(outcome.governance, GovernanceCapability::None);
        assert!(!outcome.claimed_governed_execution);
        assert_eq!(outcome.initiator_slot_id, initiator);
        assert_eq!(outcome.target_slot_id, target_id);
        assert_eq!(outcome.output.exit_code, Some(0));

        // Elevating manually is still forbidden by the domain helper.
        let elevated = attribute_outcome_for_assignment(
            &prepared.assignment,
            ProcessOutputRecord::default(),
            true,
        );
        assert!(matches!(
            elevated,
            Err(AttributeError::GovernanceElevationForbidden)
        ));
    }

    #[test]
    fn full_governance_may_claim_governed_execution() {
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: SlotId::new(),
            target: Some(governed_target(SlotId::new())),
            requested_role: None,
            requested_slot_id: None,
            task_text: "work".into(),
            cwd: None,
        })
        .unwrap();
        assert!(claimed_governed_execution_for(
            prepared.assignment.governance
        ));
        let receipt = GovernedProcessReceipt {
            delivery: keeper_agent_protocol::GovernedProcessDelivery::Executed,
            proposal_id: "p2".into(),
            state: ExecProposalState::Failed,
            requested: None,
            decided: None,
            started: None,
            terminal: Some(keeper_agent_protocol::ProcessTerminalOutcomePayload {
                proposal_id: "p2".into(),
                program: GOVERNED_CATALOG_PROGRAM.into(),
                state: ExecProposalState::Failed,
                exit_code: Some(1),
                duration_ms: Some(3),
                truncated: Some(false),
                encoding_lossy: Some(false),
                captured_bytes: Some(0),
                cancelled: Some(false),
                timed_out: Some(false),
            }),
            provider_ok: None,
            cancelled: false,
            timed_out: false,
        };
        let outcome = attribute_dispatch_receipt(&prepared.assignment, &receipt).unwrap();
        assert!(outcome.claimed_governed_execution);
        assert_eq!(outcome.governance, GovernanceCapability::Full);
    }

    #[test]
    fn task_text_never_enters_program_or_cwd_fields_on_provider_proposal() {
        let task = "unique-task-text-xyz";
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: SlotId::new(),
            target: Some(local_cli_target(SlotId::new(), "node")),
            requested_role: None,
            requested_slot_id: None,
            task_text: task.into(),
            cwd: Some(".".into()),
        })
        .unwrap();
        let program = prepared.provider.arguments["program"].as_str().unwrap();
        let cwd = prepared.provider.arguments["cwd"].as_str().unwrap();
        assert_ne!(program, task);
        assert_ne!(cwd, task);
    }

    #[test]
    fn assignment_target_address_is_preserved() {
        let target_id = SlotId::new();
        let prepared = prepare_dispatch(BuildAssignmentInput {
            initiator_slot_id: SlotId::new(),
            target: Some(local_cli_target(target_id, "node")),
            requested_role: Some(SlotRole::Custom("worker".into())),
            requested_slot_id: Some(target_id),
            task_text: "t".into(),
            cwd: None,
        })
        .unwrap();
        assert!(matches!(
            prepared.assignment.target_address,
            AssignmentTarget::SlotAndRole { .. }
        ));
        // call_id embeds assignment id for audit correlation.
        assert!(
            prepared
                .call_id
                .contains(&prepared.assignment.id.0.to_string())
        );
        let _ = Uuid::parse_str(
            prepared
                .call_id
                .strip_prefix("dispatch-")
                .expect("call id prefix"),
        )
        .unwrap();
    }

    #[test]
    fn unsupported_wire_governance_fails_closed() {
        assert!(matches!(
            governance_from_wire_label("maybe"),
            Err(DispatchError::UnsupportedAuth)
        ));
        assert_eq!(
            governance_from_wire_label("full").unwrap(),
            GovernanceCapability::Full
        );
    }
}
