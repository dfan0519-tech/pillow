//! Conversation-scoped write proposal listing, decision, and diff open.
//!
//! Write authority mirrors process: raw grants stay host-private. The renderer
//! receives only a non-authority `decision_available` bit. Decide looks up the
//! live grant on this host by proposal id + conversation ownership — never from
//! renderer-supplied grant material.

use chrono::Utc;
use keeper_agent_protocol::{WriteMode, WriteProposalState, WritePublicErrorCode};
use keeper_policy::WriteDecision;
use keeper_runtime_host::{GovernedWriteBridgeError, LiveWriteProposalView};
use serde::Serialize;

use super::ConversationService;
use super::types::{ConversationCommandError, PermissionRuleAttributionDto};

/// Renderer-facing write proposal DTO. Closed wire contract (camelCase).
///
/// Authority fields (`approval_grant`, digests, sealed content) never appear.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProposalDto {
    pub proposal_id: String,
    pub conversation_id: String,
    pub path: String,
    pub mode: String,
    pub state: String,
    pub summary: String,
    pub expires_at: String,
    /// A non-authority availability bit. Raw grants stay host-private.
    pub decision_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub rule_subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approved_by_rule: Option<PermissionRuleAttributionDto>,
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

/// Map host live view → renderer DTO. Never copies raw grant material.
fn proposal_dto(view: LiveWriteProposalView) -> WriteProposalDto {
    WriteProposalDto {
        proposal_id: view.proposal_id,
        conversation_id: view.conversation_id,
        path: view.path,
        mode: mode_label(view.mode).into(),
        state: state_label(view.state).into(),
        summary: view.summary,
        expires_at: view.expires_at.to_rfc3339(),
        // Availability only: grant presence on the host live entry.
        decision_available: view.approval_grant.is_some(),
        error_code: view.error_code.map(error_label).map(str::to_owned),
        rule_subject: view.rule_subject,
        auto_approved_by_rule: view.auto_approved_by_rule.map(Into::into),
    }
}

impl ConversationService {
    /// List write proposals for a conversation.
    /// Routes only through this service's host instance (owner-scoped).
    /// Live pending exposes `decision_available`; raw grants stay host-private.
    pub fn list_write_proposals(
        &self,
        conversation_id: String,
    ) -> Result<Vec<WriteProposalDto>, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let views = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .map_err(map_write_error)?;
        Ok(views.into_iter().map(proposal_dto).collect())
    }

    /// Decide a live write proposal.
    ///
    /// Callers supply conversation id, proposal id, and decision only. The host
    /// looks up the one-shot grant from its own live proposal table — renderer
    /// grant material is never accepted.
    pub fn decide_write_proposal(
        &self,
        conversation_id: String,
        proposal_id: String,
        decision: String,
    ) -> Result<WriteProposalDto, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let decision = parse_decision(&decision)?;

        // Host-private grant resolution: list is owner-scoped; grant never
        // crossed the renderer boundary.
        let grant = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .map_err(map_write_error)?
            .into_iter()
            .find(|view| view.proposal_id == proposal_id)
            .and_then(|view| view.approval_grant)
            .ok_or_else(|| {
                ConversationCommandError::new("write_not_live", "Write proposal is not live")
            })?;

        let view = self
            .runtime_host()
            .decide_write_proposal(&conversation_id, &proposal_id, &grant, decision)
            .map_err(map_write_error)?;
        Ok(proposal_dto(view))
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

#[cfg(test)]
mod ea03_write_grant_boundary_tests {
    use super::*;
    use keeper_agent_protocol::WriteMode;
    use keeper_runtime_host::LiveWriteProposalView;

    fn sample_view(grant: Option<&str>) -> LiveWriteProposalView {
        LiveWriteProposalView {
            proposal_id: "00000000-0000-0000-0000-0000000000p1".into(),
            conversation_id: "00000000-0000-0000-0000-0000000000c1".into(),
            path: "src/example.ts".into(),
            mode: WriteMode::Create,
            state: WriteProposalState::PendingApproval,
            summary: "Create src/example.ts".into(),
            expires_at: Utc::now(),
            approval_grant: grant.map(str::to_owned),
            error_code: None,
            diff: None,
            rule_subject: "src/example.ts".into(),
            auto_approved_by_rule: None,
        }
    }

    /// EA-03 sequence A: renderer DTO must never carry raw grant material.
    #[test]
    fn write_proposal_dto_exposes_decision_available_not_raw_grant() {
        let with_grant = proposal_dto(sample_view(Some(
            "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
        )));
        assert!(with_grant.decision_available);

        let json = serde_json::to_value(&with_grant).expect("serialize");
        assert_eq!(
            json.get("decisionAvailable"),
            Some(&serde_json::json!(true))
        );
        assert!(
            json.get("approvalGrant").is_none(),
            "raw grant must not cross renderer boundary: {json}"
        );
        assert!(
            json.get("approval_grant").is_none(),
            "snake_case grant must not appear: {json}"
        );

        let debug = format!("{with_grant:?}");
        assert!(
            !debug.contains("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG"),
            "debug must not leak grant: {debug}"
        );

        let without = proposal_dto(sample_view(None));
        assert!(!without.decision_available);
        let json_without = serde_json::to_value(&without).expect("serialize");
        assert_eq!(
            json_without.get("decisionAvailable"),
            Some(&serde_json::json!(false))
        );
        assert!(json_without.get("approvalGrant").is_none());
    }

    /// EA-03 sequence A: decide service API takes no grant argument.
    #[test]
    fn decide_write_proposal_service_source_does_not_accept_renderer_grant() {
        // Strip this test module so source tokens in asserts cannot self-match.
        let source = include_str!("write_service.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production write_service source");
        // Signature: conversation_id, proposal_id, decision only.
        assert!(
            production.contains(
                "pub fn decide_write_proposal(\n        &self,\n        conversation_id: String,\n        proposal_id: String,\n        decision: String,"
            ),
            "decide_write_proposal must not take approval_grant from callers"
        );
        assert!(
            production.contains("Raw grants stay host-private"),
            "host-private grant contract comment must remain"
        );
        // Mapping must never assign host grant into a DTO approval_grant field.
        let forbidden = format!("{}: {}", "approval_grant", "view.approval_grant");
        assert!(
            !production.contains(&forbidden),
            "must not forward raw grant into WriteProposalDto"
        );
        assert!(
            production.contains("decision_available: view.approval_grant.is_some()"),
            "decision_available must derive from host grant presence"
        );
        // DTO struct must declare decision_available, not approval_grant.
        assert!(
            production.contains("pub decision_available: bool"),
            "WriteProposalDto must expose decision_available"
        );
        assert!(
            !production.contains("pub approval_grant:"),
            "WriteProposalDto must not declare approval_grant"
        );
    }
}
