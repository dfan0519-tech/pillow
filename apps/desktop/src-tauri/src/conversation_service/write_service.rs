//! Conversation-scoped write proposal listing, decision, and diff open.
//!
//! Write authority mirrors process: raw grants stay host-private. The renderer
//! receives only a non-authority `decision_available` bit. Decide looks up the
//! live grant on this host by proposal id + conversation ownership — never from
//! renderer-supplied grant material.
//!
//! EA-04: a truncated unified diff is not a complete review object. Approve
//! once is fail-closed until every proposed-content page has been retrieved.
//! The review-binding digest is SHA-256 of sealed proposed content.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use keeper_agent_protocol::{WriteMode, WriteProposalState, WritePublicErrorCode};
use keeper_policy::WriteDecision;
use keeper_runtime_host::{GovernedWriteBridgeError, LiveWriteProposalView};
use keeper_tool_executor::BoundedDiff;
use serde::Serialize;

use super::ConversationService;
use super::types::{ConversationCommandError, PermissionRuleAttributionDto};

/// Renderer-facing write proposal DTO. Closed wire contract (camelCase).
///
/// Authority fields (`approval_grant`, sealed content) never appear.
/// `content_digest` is the review-binding hash of sealed proposed content
/// (EA-04), not a grant and not the bounded-diff text.
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
    /// True when the host-built unified diff hit a display cap.
    pub diff_truncated: bool,
    /// SHA-256 of sealed proposed content. Absent when no live diff exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    /// True when approve is informed: not truncated, or every content page fetched.
    pub content_review_complete: bool,
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
    /// SHA-256 of sealed proposed content, not of `unified`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

/// One UTF-8-safe page of sealed proposed content. Never the full body when
/// it would exceed the display caps.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteContentPageDto {
    pub proposal_id: String,
    pub path: String,
    pub mode: String,
    pub content_digest: String,
    pub page_index: u32,
    pub page_count: u32,
    pub text: String,
    pub line_count: u32,
    pub byte_count: u64,
    pub review_complete: bool,
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
    let (diff_truncated, content_digest) = match view.diff.as_ref() {
        Some(diff) => (diff.truncated, Some(diff.content_digest.clone())),
        None => (false, None),
    };
    let content_review_complete = match view.diff.as_ref() {
        Some(diff) if diff.truncated => content_review_is_complete(
            &view.conversation_id,
            &view.proposal_id,
            &diff.content_digest,
        ),
        _ => true,
    };
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
        diff_truncated,
        content_digest,
        content_review_complete,
    }
}

struct ContentReviewSession {
    conversation_id: String,
    digest: String,
    page_count: u32,
    fetched: BTreeSet<u32>,
}

fn review_sessions() -> &'static Mutex<HashMap<String, ContentReviewSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, ContentReviewSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn review_session_key(conversation_id: &str, proposal_id: &str) -> String {
    format!("{conversation_id}:{proposal_id}")
}

fn content_review_is_complete(conversation_id: &str, proposal_id: &str, digest: &str) -> bool {
    let Ok(guard) = review_sessions().lock() else {
        return false;
    };
    let Some(session) = guard.get(&review_session_key(conversation_id, proposal_id)) else {
        return false;
    };
    session.digest == digest
        && session.conversation_id == conversation_id
        && session.page_count > 0
        && session.fetched.len() as u32 == session.page_count
        && (0..session.page_count).all(|index| session.fetched.contains(&index))
}

fn record_content_page_fetch(
    conversation_id: &str,
    proposal_id: &str,
    digest: &str,
    page_count: u32,
    page_index: u32,
) {
    let Ok(mut guard) = review_sessions().lock() else {
        return;
    };
    let key = review_session_key(conversation_id, proposal_id);
    let session = guard.entry(key).or_insert_with(|| ContentReviewSession {
        conversation_id: conversation_id.to_owned(),
        digest: digest.to_owned(),
        page_count,
        fetched: BTreeSet::new(),
    });
    if session.digest != digest
        || session.conversation_id != conversation_id
        || session.page_count != page_count
    {
        session.conversation_id = conversation_id.to_owned();
        session.digest = digest.to_owned();
        session.page_count = page_count;
        session.fetched.clear();
    }
    session.fetched.insert(page_index);
}

fn clear_content_review(conversation_id: &str, proposal_id: &str) {
    if let Ok(mut guard) = review_sessions().lock() {
        guard.remove(&review_session_key(conversation_id, proposal_id));
    }
}

fn live_diff_for_proposal<'a>(
    views: &'a [LiveWriteProposalView],
    proposal_id: &str,
) -> Option<&'a BoundedDiff> {
    views
        .iter()
        .find(|view| view.proposal_id == proposal_id)
        .and_then(|view| view.diff.as_ref())
}

fn write_not_live() -> ConversationCommandError {
    ConversationCommandError::new("write_not_live", "Write proposal is not live")
}

/// EA-04 gate: approve-once is refused until a truncated diff has been read in
/// full. Checked before the host consumes the one-shot grant, so a refusal
/// costs the user nothing.
///
/// Split out from `decide_write_proposal` so its behaviour can be called
/// directly. The original guard was covered only by a test asserting that
/// certain identifiers appear in this file's source, which cannot tell a live
/// guard from dead code — verified by mutation: replacing the truncation
/// condition with `false` left that test, and the whole suite, green.
fn ensure_approve_is_informed(
    decision: WriteDecision,
    diff: Option<&BoundedDiff>,
    conversation_id: &str,
    proposal_id: &str,
) -> Result<(), ConversationCommandError> {
    // Reject is always available. Being unable to finish reading a proposal is
    // never a reason to trap someone into keeping it.
    if decision != WriteDecision::ApproveOnce {
        return Ok(());
    }
    let Some(diff) = diff else { return Ok(()) };
    if !diff.truncated {
        return Ok(());
    }
    if content_review_is_complete(conversation_id, proposal_id, &diff.content_digest) {
        return Ok(());
    }
    Err(write_truncated_unreviewed())
}

fn write_truncated_unreviewed() -> ConversationCommandError {
    ConversationCommandError::new(
        "write_truncated_unreviewed",
        "Truncated write content cannot be approved until the full proposed content is reviewed",
    )
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
        let views = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .map_err(map_write_error)?;
        let live = views
            .iter()
            .find(|view| view.proposal_id == proposal_id)
            .ok_or_else(write_not_live)?;

        ensure_approve_is_informed(decision, live.diff.as_ref(), &conversation_id, &proposal_id)?;

        let grant = live.approval_grant.clone().ok_or_else(write_not_live)?;

        let view = self
            .runtime_host()
            .decide_write_proposal(&conversation_id, &proposal_id, &grant, decision)
            .map_err(map_write_error)?;
        clear_content_review(&conversation_id, &proposal_id);
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
        let content_digest = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .ok()
            .as_deref()
            .and_then(|views| live_diff_for_proposal(views, &proposal_id))
            .map(|live| live.content_digest.clone());
        Ok(WriteDiffDto {
            proposal_id: diff.proposal_id,
            path: diff.path,
            mode: mode_label(diff.mode).into(),
            unified: diff.unified,
            truncated: diff.truncated,
            line_count: diff.line_count,
            byte_count: diff.byte_count,
            content_digest,
        })
    }

    /// Page sealed proposed content for informed approval (EA-04).
    ///
    /// Retrieving every page binds the session to `content_digest` and is the
    /// only way a truncated proposal becomes approvable.
    pub fn open_write_proposal_content_page(
        &self,
        conversation_id: String,
        proposal_id: String,
        page_index: u32,
    ) -> Result<WriteContentPageDto, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let views = self
            .runtime_host()
            .list_write_proposals(&conversation_id)
            .map_err(map_write_error)?;
        let live = views
            .iter()
            .find(|view| view.proposal_id == proposal_id)
            .ok_or_else(write_not_live)?;
        let diff = live.diff.as_ref().ok_or_else(write_not_live)?;
        let page = diff.content_page(page_index).ok_or_else(|| {
            ConversationCommandError::new(
                "write_invalid_content_page",
                "Write content page is not available",
            )
        })?;
        record_content_page_fetch(
            &conversation_id,
            &proposal_id,
            &diff.content_digest,
            page.page_count,
            page.page_index,
        );
        let review_complete =
            content_review_is_complete(&conversation_id, &proposal_id, &diff.content_digest);
        Ok(WriteContentPageDto {
            proposal_id: live.proposal_id.clone(),
            path: live.path.clone(),
            mode: mode_label(live.mode).into(),
            content_digest: diff.content_digest.clone(),
            page_index: page.page_index,
            page_count: page.page_count,
            text: page.text,
            line_count: page.line_count,
            byte_count: page.byte_count,
            review_complete,
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
        // Normalise line endings first: these assertions match exact byte
        // sequences, and git checks this file out CRLF on Windows. Without it
        // the contract holds while the test reports it broken.
        let source = include_str!("write_service.rs").replace("\r\n", "\n");
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

    /// EA-04: digest is of sealed proposed content, never of the bounded unified text.
    #[test]
    fn truncated_proposal_dto_binds_digest_to_proposed_not_unified() {
        use keeper_tool_executor::{build_bounded_unified_diff, content_sha256_digest};

        let mut proposed = String::new();
        for i in 0..4_080 {
            proposed.push_str(&format!("keep-{i:05}\n"));
        }
        proposed.push_str("HIDDEN_TAIL_MUST_NOT_BE_IN_UNIFIED\n");
        let diff = build_bounded_unified_diff(None, &proposed);
        assert!(diff.truncated);
        assert!(!diff.unified.contains("HIDDEN_TAIL_MUST_NOT_BE_IN_UNIFIED"));
        assert_eq!(diff.content_digest, content_sha256_digest(&proposed));
        assert_ne!(diff.content_digest, content_sha256_digest(&diff.unified));

        let mut view = sample_view(Some("abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG"));
        view.diff = Some(diff.clone());
        let dto = proposal_dto(view);
        assert!(dto.diff_truncated);
        assert_eq!(
            dto.content_digest.as_deref(),
            Some(diff.content_digest.as_str())
        );
        assert!(
            !dto.content_review_complete,
            "truncated content is unreviewed until every page is fetched"
        );

        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(json.get("diffTruncated"), Some(&serde_json::json!(true)));
        assert_eq!(
            json.get("contentDigest"),
            Some(&serde_json::json!(diff.content_digest))
        );
        assert_eq!(
            json.get("contentReviewComplete"),
            Some(&serde_json::json!(false))
        );
        assert!(json.get("proposedContent").is_none());
        assert!(json.get("unified").is_none());
        assert!(json.get("approvalGrant").is_none());
    }

    /// EA-04: approve is blocked until every proposed-content page is retrieved.
    #[test]
    fn approve_once_is_blocked_until_every_content_page_is_fetched() {
        use keeper_tool_executor::build_bounded_unified_diff;

        let mut proposed = String::new();
        for i in 0..4_050 {
            proposed.push_str(&format!("page-line-{i:05}\n"));
        }
        let diff = build_bounded_unified_diff(None, &proposed);
        assert!(diff.truncated);
        let first = diff.content_page(0).expect("page 0");
        assert!(first.page_count >= 2, "need more than one review page");

        let conversation = "conv-ea04-review";
        let proposal = "prop-ea04-review";
        assert!(!content_review_is_complete(
            conversation,
            proposal,
            &diff.content_digest
        ));
        record_content_page_fetch(
            conversation,
            proposal,
            &diff.content_digest,
            first.page_count,
            0,
        );
        assert!(
            !content_review_is_complete(conversation, proposal, &diff.content_digest),
            "a single page must not unlock approve"
        );
        for index in 1..first.page_count {
            record_content_page_fetch(
                conversation,
                proposal,
                &diff.content_digest,
                first.page_count,
                index,
            );
        }
        assert!(content_review_is_complete(
            conversation,
            proposal,
            &diff.content_digest
        ));
        clear_content_review(conversation, proposal);
        assert!(!content_review_is_complete(
            conversation,
            proposal,
            &diff.content_digest
        ));
    }

    /// EA-04: decide production path fail-closes on truncated unreviewed content.
    #[test]
    fn decide_write_proposal_source_fail_closes_truncated_unreviewed() {
        let source = include_str!("write_service.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production write_service source");
        assert!(
            production.contains("write_truncated_unreviewed"),
            "decide must fail closed when the diff is truncated and unreviewed"
        );
        assert!(
            production.contains("content_review_is_complete"),
            "approve must consult the content-review session"
        );
        assert!(
            production.contains("open_write_proposal_content_page"),
            "host must expose paged proposed-content retrieval"
        );
        assert!(
            !production.contains("pub proposed_content:"),
            "full sealed content must not be a renderer DTO field"
        );
    }
}

/// Behavioural cover for the EA-04 approve gate.
///
/// The pre-existing `decide_write_proposal_source_fail_closes_truncated_unreviewed`
/// asserts that identifiers appear in this file's source. That is worth having
/// as a wiring check, but it cannot tell a live guard from dead code: replacing
/// the truncation condition with `false` left it — and the entire suite — green.
/// These call the gate instead.
#[cfg(test)]
mod ea04_approve_gate_tests {
    use super::*;
    use keeper_tool_executor::build_bounded_unified_diff;

    const CONVERSATION: &str = "00000000-0000-0000-0000-0000000000c4";

    fn truncated_diff() -> BoundedDiff {
        // Comfortably past the 4,000-line display cap.
        let proposed = "line\n".repeat(5_000);
        let diff = build_bounded_unified_diff(None, &proposed);
        assert!(diff.truncated, "fixture must exercise the truncated path");
        diff
    }

    fn short_diff() -> BoundedDiff {
        let diff = build_bounded_unified_diff(None, "one line\n");
        assert!(
            !diff.truncated,
            "fixture must exercise the untruncated path"
        );
        diff
    }

    fn approve(
        diff: Option<&BoundedDiff>,
        proposal_id: &str,
    ) -> Result<(), ConversationCommandError> {
        ensure_approve_is_informed(WriteDecision::ApproveOnce, diff, CONVERSATION, proposal_id)
    }

    #[test]
    fn approving_a_truncated_proposal_is_refused_until_every_page_is_read() {
        let proposal = "ea04-unread";
        let diff = truncated_diff();
        clear_content_review(CONVERSATION, proposal);

        let refused = approve(Some(&diff), proposal).expect_err("unread truncated approve");
        assert_eq!(refused.code, "write_truncated_unreviewed");

        // One page of three is not a review.
        record_content_page_fetch(CONVERSATION, proposal, &diff.content_digest, 3, 0);
        let still_refused = approve(Some(&diff), proposal).expect_err("partial review");
        assert_eq!(still_refused.code, "write_truncated_unreviewed");

        record_content_page_fetch(CONVERSATION, proposal, &diff.content_digest, 3, 1);
        record_content_page_fetch(CONVERSATION, proposal, &diff.content_digest, 3, 2);
        approve(Some(&diff), proposal).expect("every page read");
        clear_content_review(CONVERSATION, proposal);
    }

    #[test]
    fn an_untruncated_proposal_needs_no_paged_review() {
        let proposal = "ea04-short";
        clear_content_review(CONVERSATION, proposal);
        approve(Some(&short_diff()), proposal).expect("nothing was hidden");
        approve(None, proposal).expect("no diff to hide anything in");
    }

    #[test]
    fn reject_is_never_gated_on_finishing_the_review() {
        // Refusing to read is not a reason to be stuck holding a proposal.
        let proposal = "ea04-reject";
        let diff = truncated_diff();
        clear_content_review(CONVERSATION, proposal);
        ensure_approve_is_informed(WriteDecision::Reject, Some(&diff), CONVERSATION, proposal)
            .expect("reject must stay available while the review is incomplete");
    }

    #[test]
    fn a_review_of_different_content_does_not_unlock_this_one() {
        // The session binds to the digest of what will actually be written.
        // Reading some other proposal's pages must not count.
        let proposal = "ea04-swap";
        let diff = truncated_diff();
        clear_content_review(CONVERSATION, proposal);
        record_content_page_fetch(CONVERSATION, proposal, "sha256:0000", 1, 0);

        let refused = approve(Some(&diff), proposal).expect_err("digest mismatch");
        assert_eq!(refused.code, "write_truncated_unreviewed");
        clear_content_review(CONVERSATION, proposal);
    }
}
