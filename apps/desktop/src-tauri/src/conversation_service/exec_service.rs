//! Conversation-scoped governed-process proposal and output projection.
//!
//! This module only translates the owner-scoped RuntimeHost authority into the
//! renderer's bounded DTOs. It never opens storage, recreates a proposal,
//! carries a grant, or retains an output transcript.

use keeper_policy::{ExecDecision, ExecProposalState};
use keeper_runtime_host::{
    GovernedProcessBridgeError, LiveProcessProposalView, ProcessOrigin, ProcessOutputStream,
    ProcessOutputView, ProcessRiskClass,
};
use serde::Serialize;

use super::ConversationService;
use super::types::ConversationCommandError;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecProposalDto {
    pub proposal_id: String,
    pub conversation_id: String,
    pub origin: String,
    pub state: String,
    pub program_label: String,
    pub argv: Vec<String>,
    pub portable_cwd: String,
    pub risk_class: String,
    pub summary: String,
    pub expires_at: String,
    /// A non-authority availability bit. Raw grants stay host-private.
    pub decision_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecOutputChunkDto {
    pub stream: String,
    pub sequence: u64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecOutputDto {
    pub proposal_id: String,
    pub state: String,
    pub chunks: Vec<ExecOutputChunkDto>,
    /// Presentation metadata only: true while this host retained the live transcript.
    pub live_retained: bool,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub captured_bytes: u64,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) enum ProcessOperation {
    List,
    Decision,
    Output,
}

pub(crate) fn map_process_error(
    error: GovernedProcessBridgeError,
    operation: ProcessOperation,
) -> ConversationCommandError {
    let (code, message) = match error {
        GovernedProcessBridgeError::OwnershipMismatch => (
            "process_ownership_mismatch",
            "Process proposal ownership mismatch.",
        ),
        GovernedProcessBridgeError::Expired => ("process_expired", "Process proposal expired."),
        GovernedProcessBridgeError::CleanupUnproven => (
            "process_cleanup_unproven",
            "Process cleanup could not be verified.",
        ),
        GovernedProcessBridgeError::ReservationConflict => (
            "process_conflict",
            "Process proposal conflicted with current state.",
        ),
        GovernedProcessBridgeError::InvalidProposal
        | GovernedProcessBridgeError::DecisionDenied
            if matches!(operation, ProcessOperation::Decision) =>
        {
            (
                "process_invalid_decision",
                "Process decision must be approve_once or reject.",
            )
        }
        GovernedProcessBridgeError::NotLive if matches!(operation, ProcessOperation::Output) => (
            "process_output_unavailable",
            "Process output is unavailable.",
        ),
        GovernedProcessBridgeError::NotLive
        | GovernedProcessBridgeError::Cancelled
        | GovernedProcessBridgeError::Interrupted => {
            ("process_not_live", "Process proposal is not live.")
        }
        _ => ("process_unavailable", "Process proposal is unavailable."),
    };
    ConversationCommandError::new(code, message)
}

fn parse_decision(decision: &str) -> Result<ExecDecision, ConversationCommandError> {
    match decision {
        "approve_once" => Ok(ExecDecision::ApproveOnce),
        "reject" => Ok(ExecDecision::Reject),
        _ => Err(ConversationCommandError::new(
            "process_invalid_decision",
            "Process decision must be approve_once or reject.",
        )),
    }
}

fn state_label(state: ExecProposalState) -> &'static str {
    match state {
        ExecProposalState::PendingApproval => "pending_approval",
        ExecProposalState::Approved => "approved",
        ExecProposalState::Running => "running",
        ExecProposalState::Succeeded => "succeeded",
        ExecProposalState::Failed => "failed",
        ExecProposalState::Rejected => "rejected",
        ExecProposalState::Expired => "expired",
        ExecProposalState::Cancelled => "cancelled",
        ExecProposalState::Interrupted => "interrupted",
    }
}

fn origin_label(origin: ProcessOrigin) -> &'static str {
    match origin {
        ProcessOrigin::Agent => "agent",
    }
}

fn risk_label(risk: ProcessRiskClass) -> &'static str {
    match risk {
        ProcessRiskClass::Standard => "standard",
        ProcessRiskClass::High => "high",
    }
}

fn stream_label(stream: ProcessOutputStream) -> &'static str {
    match stream {
        ProcessOutputStream::Stdout => "stdout",
        ProcessOutputStream::Stderr => "stderr",
    }
}

pub(crate) fn proposal_dto(view: LiveProcessProposalView) -> ExecProposalDto {
    ExecProposalDto {
        proposal_id: view.proposal_id,
        conversation_id: view.conversation_id,
        origin: origin_label(view.origin).into(),
        state: state_label(view.state).into(),
        program_label: view.program,
        argv: view.argv,
        // `.` is the renderer contract's portable project-root sentinel.
        portable_cwd: view.portable_cwd.unwrap_or_else(|| ".".into()),
        risk_class: risk_label(view.risk_class).into(),
        summary: view.summary,
        expires_at: view.expires_at.to_rfc3339(),
        decision_available: view.decision_available,
        error_code: None,
    }
}

pub(crate) fn output_dto(view: ProcessOutputView) -> ExecOutputDto {
    ExecOutputDto {
        proposal_id: view.proposal_id,
        state: state_label(view.state).into(),
        chunks: view
            .chunks
            .into_iter()
            .map(|chunk| ExecOutputChunkDto {
                stream: stream_label(chunk.stream).into(),
                sequence: chunk.sequence,
                text: chunk.text,
            })
            .collect(),
        live_retained: view.live_retained,
        truncated: view.truncated,
        encoding_lossy: view.encoding_lossy,
        captured_bytes: view.captured_bytes,
        exit_code: view.exit_code,
        duration_ms: view.duration_ms,
    }
}

impl ConversationService {
    /// List only this host's project- and conversation-owned process views.
    pub fn list_exec_proposals(
        &self,
        conversation_id: String,
    ) -> Result<Vec<ExecProposalDto>, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        self.runtime_host()
            .list_process_proposals(&conversation_id)
            .map_err(|error| map_process_error(error, ProcessOperation::List))
            .map(|views| views.into_iter().map(proposal_dto).collect())
    }

    /// Decide a live host-owned process proposal with an exact public verb.
    pub fn decide_exec_proposal(
        &self,
        conversation_id: String,
        proposal_id: String,
        decision: String,
    ) -> Result<ExecProposalDto, ConversationCommandError> {
        let decision = parse_decision(&decision)?;
        let _ = self.conversation_project_context(&conversation_id)?;
        self.runtime_host()
            .decide_process_proposal(&conversation_id, &proposal_id, decision)
            .map_err(|error| map_process_error(error, ProcessOperation::Decision))
            .map(proposal_dto)
    }

    /// Open host-retained bounded process output; durable-only records are not transcripts.
    pub fn open_exec_output(
        &self,
        conversation_id: String,
        proposal_id: String,
    ) -> Result<ExecOutputDto, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        self.runtime_host()
            .open_process_output(&conversation_id, &proposal_id)
            .map_err(|error| map_process_error(error, ProcessOperation::Output))
            .map(output_dto)
    }
}
