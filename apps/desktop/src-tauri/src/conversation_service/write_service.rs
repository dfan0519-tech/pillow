//! Conversation-scoped write proposal listing, decision, and diff open.

use chrono::Utc;
use keeper_agent_protocol::{WriteMode, WriteProposalState, WritePublicErrorCode};
use keeper_policy::WriteDecision;
use keeper_runtime_host::GovernedWriteBridgeError;
use serde::Serialize;

use super::ConversationService;
use super::types::ConversationCommandError;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProposalDto {
    pub proposal_id: String,
    pub conversation_id: String,
    pub path: String,
    pub mode: String,
    pub state: String,
    pub summary: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_grant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl std::fmt::Debug for WriteProposalDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteProposalDto")
            .field("proposal_id", &self.proposal_id)
            .field("conversation_id", &self.conversation_id)
            .field("path", &self.path)
            .field("mode", &self.mode)
            .field("state", &self.state)
            .field("summary", &self.summary)
            .field("expires_at", &self.expires_at)
            .field(
                "approval_grant",
                &self.approval_grant.as_ref().map(|_| "<redacted>"),
            )
            .field("error_code", &self.error_code)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteDiffDto {
    pub proposal_id: String,
    pub path: String,
    pub mode: String,
    pub unified: String,
    pub truncated: bool,
    pub line_count: u32,
    pub byte_count: u64,
}

fn mode_label(mode: WriteMode) -> &'static str {
    match mode {
        WriteMode::Create => "create",
        WriteMode::Modify => "modify",
    }
}

fn state_label(state: WriteProposalState) -> &'static str {
    match state {
        WriteProposalState::PendingApproval => "pending_approval",
        WriteProposalState::Approved => "approved",
        WriteProposalState::Applying => "applying",
        WriteProposalState::Succeeded => "succeeded",
        WriteProposalState::Rejected => "rejected",
        WriteProposalState::Expired => "expired",
        WriteProposalState::Cancelled => "cancelled",
        WriteProposalState::Interrupted => "interrupted",
        WriteProposalState::Failed => "failed",
    }
}

fn error_label(code: WritePublicErrorCode) -> &'static str {
    match code {
        WritePublicErrorCode::ApprovalRequired => "approval_required",
        WritePublicErrorCode::ApprovalRejected => "approval_rejected",
        WritePublicErrorCode::ApprovalExpired => "approval_expired",
        WritePublicErrorCode::BaseChanged => "base_changed",
        WritePublicErrorCode::WriteConflict => "write_conflict",
        WritePublicErrorCode::PathRejected => "path_rejected",
        WritePublicErrorCode::WriteFailed => "write_failed",
        WritePublicErrorCode::Cancelled => "cancelled",
        WritePublicErrorCode::Interrupted => "interrupted",
    }
}

fn map_write_error(error: GovernedWriteBridgeError) -> ConversationCommandError {
    match error {
        GovernedWriteBridgeError::OwnershipMismatch => ConversationCommandError::new(
            "write_ownership_mismatch",
            "Write proposal ownership mismatch",
        ),
        GovernedWriteBridgeError::InvalidGrant => {
            ConversationCommandError::new("write_invalid_grant", "Approval grant was rejected")
        }
        GovernedWriteBridgeError::NotLive => {
            ConversationCommandError::new("write_not_live", "Write proposal is not live")
        }
        GovernedWriteBridgeError::DecisionDenied => {
            ConversationCommandError::new("write_decision_denied", "Write decision was denied")
        }
        GovernedWriteBridgeError::Expired => {
            ConversationCommandError::new("write_expired", "Write proposal expired")
        }
        GovernedWriteBridgeError::Cancelled => {
            ConversationCommandError::new("write_cancelled", "Write proposal was cancelled")
        }
        GovernedWriteBridgeError::Interrupted => {
            ConversationCommandError::new("write_interrupted", "Write proposal was interrupted")
        }
        _ => ConversationCommandError::new("write_unavailable", "Write proposal is unavailable"),
    }
}

fn parse_decision(decision: &str) -> Result<WriteDecision, ConversationCommandError> {
    match decision {
        "approve_once" => Ok(WriteDecision::ApproveOnce),
        "reject" => Ok(WriteDecision::Reject),
        _ => Err(ConversationCommandError::new(
            "write_invalid_decision",
            "Decision must be approve_once or reject",
        )),
    }
}

impl ConversationService {
    /// List write proposals for a conversation. Live pending includes the grant.
    /// Routes only through this service's host instance (owner-scoped).
    pub fn list_write_proposals(
        &self,
        conversation_id: String,
    ) -> Result<Vec<WriteProposalDto>, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let views = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .map_err(map_write_error)?;
        Ok(views
            .into_iter()
            .map(|view| WriteProposalDto {
                proposal_id: view.proposal_id,
                conversation_id: view.conversation_id,
                path: view.path,
                mode: mode_label(view.mode).into(),
                state: state_label(view.state).into(),
                summary: view.summary,
                expires_at: view.expires_at.to_rfc3339(),
                approval_grant: view.approval_grant,
                error_code: view.error_code.map(error_label).map(str::to_owned),
            })
            .collect())
    }

    /// Decide a live write proposal. Callers supply only conversation, id, grant, decision.
    pub fn decide_write_proposal(
        &self,
        conversation_id: String,
        proposal_id: String,
        approval_grant: String,
        decision: String,
    ) -> Result<WriteProposalDto, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let decision = parse_decision(&decision)?;
        let view = self
            .runtime_host()
            .decide_write_proposal(&conversation_id, &proposal_id, &approval_grant, decision)
            .map_err(map_write_error)?;
        Ok(WriteProposalDto {
            proposal_id: view.proposal_id,
            conversation_id: view.conversation_id,
            path: view.path,
            mode: mode_label(view.mode).into(),
            state: state_label(view.state).into(),
            summary: view.summary,
            expires_at: view.expires_at.to_rfc3339(),
            approval_grant: None,
            error_code: view.error_code.map(error_label).map(str::to_owned),
        })
    }

    /// Open the host-built diff only when the proposal is live in memory.
    pub fn open_write_proposal_diff(
        &self,
        conversation_id: String,
        proposal_id: String,
    ) -> Result<WriteDiffDto, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let diff = self
            .runtime_host()
            .open_write_proposal_diff(&conversation_id, &proposal_id)
            .map_err(map_write_error)?;
        Ok(WriteDiffDto {
            proposal_id: diff.proposal_id,
            path: diff.path,
            mode: mode_label(diff.mode).into(),
            unified: diff.unified,
            truncated: diff.truncated,
            line_count: diff.line_count,
            byte_count: diff.byte_count,
        })
    }

    /// Restart recovery for write proposals: interrupt non-terminals and
    /// reconcile orphaned `applying` rows via read-only digest observation.
    ///
    /// Recovered rows have no live grant and no host diff. Never reconstructs
    /// content, diff, or grant from disk; never applies.
    pub(super) fn recover_write_proposals_on_startup(
        &self,
    ) -> Result<usize, ConversationCommandError> {
        let now = Utc::now();
        let recovered = self
            .runtime_host()
            .recover_write_proposals(now, |project_id| self.project_root_path(project_id))
            .map_err(map_write_error)?;
        Ok(recovered.len())
    }
}
