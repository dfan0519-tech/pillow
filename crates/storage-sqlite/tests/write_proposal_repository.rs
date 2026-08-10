//! Durable write-proposal ledger tests (C3-B controller rejection fix).
//!
//! Covers storage-owned TTL, pre-SQL validation, typed receipts, apply-lease
//! expiry, restart reconciliation, and content/grant absence invariants.

mod common;

use chrono::{Duration, Utc};
use common::{open_pair, seed_project, table_exists};
use keeper_agent_protocol::{RunId as ProtocolRunId, WRITE_CONTENT_BYTE_CAP, WriteMode};
use keeper_policy::{WRITE_PROPOSAL_TTL_SECS, WriteDecision, WriteProposalState};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    AddPermissionRule, MAX_APPLY_LEASE_MS, PermissionRuleAction, PermissionRuleMatchSnapshot,
    PermissionRulePrefix, PermissionRuleReceiptActor, PermissionRuleReceiptDecision,
    PermissionRuleScope, SqlitePermissionRuleRepository, SqliteWriteProposalRepository,
    StoredWriteProposal, WriteApplyClaimOutcome, WriteApplyErrorCode, WriteApplyReceipt,
    WriteApplyReconcileObservation, WriteDecisionOutcome, WriteProposalReserveInput,
    WriteProposalStoreError, WriteReservationOutcome,
};
use rusqlite::Connection;
use uuid::Uuid;

/// Distinctive strings that must never appear in SQLite or repository rows.
const SECRET_PROPOSED: &str = "SECRET_PROPOSED_CONTENT_C3B_NEVER_STORE_xyz789";
const SECRET_BASE: &str = "SECRET_BASE_CONTENT_C3B_NEVER_STORE_abc123";
const SECRET_GRANT_RAW: &str = "raw-approval-grant-must-not-be-stored-c3b-99";

struct Fixture {
    store: SqliteWriteProposalRepository,
    project_id: String,
    conversation_id: String,
    run_id: ProtocolRunId,
    db_path: std::path::PathBuf,
}

#[test]
fn rule_allow_decision_persists_complete_snapshot_after_live_rule_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "rule-allow", 0xD1);
    fx.store.reserve(&input).unwrap();
    let matched = stored_rule_match(
        &fx,
        PermissionRulePrefix::Write,
        "src/**",
        PermissionRuleAction::Allow,
        &input.path,
    );
    let decided_at = Utc::now();

    let outcome = fx
        .store
        .decide_by_rule(
            &input.proposal_id,
            &fx.conversation_id,
            &matched,
            &grant_digest(0xD1),
            &input.proposal_fingerprint,
            decided_at,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        WriteDecisionOutcome::Accepted(ref row) if row.state == WriteProposalState::Approved
    ));

    SqlitePermissionRuleRepository::open(&fx.db_path)
        .unwrap()
        .delete(&matched.rule.rule_id)
        .unwrap();
    let receipt = fx
        .store
        .get_rule_receipt(&input.proposal_id)
        .unwrap()
        .expect("durable rule receipt");
    assert_eq!(receipt.proposal_id, input.proposal_id);
    assert_eq!(
        receipt.decision,
        PermissionRuleReceiptDecision::ApproveByRule
    );
    assert_eq!(receipt.actor, PermissionRuleReceiptActor::RuleEngine);
    assert_eq!(receipt.rule_id, matched.rule.rule_id);
    assert_eq!(receipt.rule_scope, PermissionRuleScope::Global);
    assert_eq!(receipt.rule_project_id, None);
    assert_eq!(receipt.rule_prefix, PermissionRulePrefix::Write);
    assert_eq!(receipt.rule_pattern, "src/**");
    assert_eq!(receipt.rule_action, PermissionRuleAction::Allow);
    assert_eq!(receipt.rule_content_digest, matched.rule.content_digest);
    assert_eq!(receipt.match_subject, input.path);
    assert_eq!(receipt.decided_at, decided_at);

    let connection = Connection::open(&fx.db_path).unwrap();
    let stored_decision: String = connection
        .query_row(
            "SELECT decision FROM write_permission_rule_receipts WHERE proposal_id = ?1",
            [&receipt.proposal_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_decision, "approve_by_rule");
}

#[test]
fn incompatible_rule_snapshot_rolls_back_before_write_decision() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "rule-wrong-prefix", 0xD2);
    fx.store.reserve(&input).unwrap();
    let matched = stored_rule_match(
        &fx,
        PermissionRulePrefix::Process,
        "cargo *",
        PermissionRuleAction::Allow,
        "cargo test",
    );

    let error = fx
        .store
        .decide_by_rule(
            &input.proposal_id,
            &fx.conversation_id,
            &matched,
            &grant_digest(0xD2),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap_err();
    assert_eq!(error, WriteProposalStoreError::InvalidRuleReceipt);
    assert_eq!(
        fx.store.get(&input.proposal_id).unwrap().unwrap().state,
        WriteProposalState::PendingApproval
    );
    assert!(
        fx.store
            .get_rule_receipt(&input.proposal_id)
            .unwrap()
            .is_none()
    );
    let connection = Connection::open(&fx.db_path).unwrap();
    let decisions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM write_approval_decisions WHERE proposal_id = ?1",
            [&input.proposal_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(decisions, 0);
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let (projects, sessions) = open_pair(temp);
    let project = seed_project(&projects, temp, "write-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("writes".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "propose a write".into(),
        })
        .unwrap();
    let db_path = temp.path().join("keeper.db");
    let store = SqliteWriteProposalRepository::open(&db_path).unwrap();
    Fixture {
        store,
        project_id: project.id.0.to_string(),
        conversation_id: conversation.id.0.to_string(),
        run_id: ProtocolRunId(run.id.0),
        db_path,
    }
}

fn second_run(temp: &tempfile::TempDir, conversation_id: Uuid) -> ProtocolRunId {
    let (_projects, sessions) = open_pair(temp);
    let run = sessions
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(conversation_id),
            user_message: "second run".into(),
        })
        .unwrap();
    ProtocolRunId(run.id.0)
}

/// Canonical `sha256:` + 64 lowercase hex digest helper.
fn digest(seed: u64) -> String {
    format!("sha256:{seed:064x}")
}

fn reserve_input(fx: &Fixture, call_id: &str, fingerprint_seed: u64) -> WriteProposalReserveInput {
    WriteProposalReserveInput {
        proposal_id: Uuid::new_v4().to_string(),
        reservation_key: format!(
            "wp:{}:{}:{}:{}",
            fx.project_id, fx.conversation_id, fx.run_id, call_id
        ),
        proposal_fingerprint: digest(fingerprint_seed),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: fx.run_id,
        call_id: call_id.into(),
        path: "src/example.ts".into(),
        mode: WriteMode::Create,
        proposed_digest: digest(0xA1),
        proposed_bytes: 42,
        base_digest: None,
        base_bytes: 0,
    }
}

fn modify_input(fx: &Fixture, call_id: &str, fingerprint_seed: u64) -> WriteProposalReserveInput {
    let mut input = reserve_input(fx, call_id, fingerprint_seed);
    input.mode = WriteMode::Modify;
    input.base_digest = Some(digest(0xB1));
    input.base_bytes = 17;
    input
}

fn grant_digest(seed: u64) -> String {
    digest(0x9000 + seed)
}

fn success_receipt(input: &WriteProposalReserveInput) -> WriteApplyReceipt {
    WriteApplyReceipt::Success {
        path: input.path.clone(),
        mode: input.mode,
        bytes_written: input.proposed_bytes,
        content_digest: input.proposed_digest.clone(),
        recovered: false,
    }
}

fn failure_receipt() -> WriteApplyReceipt {
    failure_receipt_with(WriteApplyErrorCode::WriteFailed)
}

fn failure_receipt_with(error_code: WriteApplyErrorCode) -> WriteApplyReceipt {
    WriteApplyReceipt::Failure {
        error_code,
        recovered: false,
    }
}

/// Closed apply-time failure vocabulary (snake_case wire values).
const APPLY_FAILURE_CODES: [WriteApplyErrorCode; 3] = [
    WriteApplyErrorCode::BaseChanged,
    WriteApplyErrorCode::PathRejected,
    WriteApplyErrorCode::WriteFailed,
];

const APPLY_FAILURE_WIRE: [&str; 3] = ["base_changed", "path_rejected", "write_failed"];

fn approve(fx: &Fixture, input: &WriteProposalReserveInput, grant_seed: u64) {
    fx.store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant_digest(grant_seed),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
}

fn claim(fx: &Fixture, proposal_id: &str, owner: &str, lease_ms: u64) {
    fx.store
        .claim_apply(proposal_id, owner, lease_ms, Utc::now())
        .unwrap();
}

fn stored_rule_match(
    fx: &Fixture,
    prefix: PermissionRulePrefix,
    pattern: &str,
    action: PermissionRuleAction,
    subject: &str,
) -> PermissionRuleMatchSnapshot {
    let rules = SqlitePermissionRuleRepository::open(&fx.db_path).unwrap();
    let rule = rules
        .add(AddPermissionRule {
            scope: PermissionRuleScope::Global,
            project_id: None,
            prefix,
            pattern: pattern.to_owned(),
            action,
        })
        .unwrap();
    PermissionRuleMatchSnapshot {
        rule,
        match_subject: subject.to_owned(),
    }
}

#[test]
fn migration_0005_upgrades_a_0004_database_without_rewriting_history() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_projects.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0002_conversations.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0003_tool_calls.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0004_resource_recents.sql"))
            .unwrap();
        assert!(!table_exists(&connection, "write_proposals"));
        assert!(!table_exists(&connection, "write_proposal_transitions"));
        assert!(!table_exists(&connection, "write_approval_decisions"));
    }

    SqliteWriteProposalRepository::open(&db).unwrap();
    let connection = Connection::open(&db).unwrap();
    assert!(table_exists(&connection, "projects"));
    assert!(table_exists(&connection, "runs"));
    assert!(table_exists(&connection, "resource_recents"));
    assert!(table_exists(&connection, "write_proposals"));
    assert!(table_exists(&connection, "write_proposal_transitions"));
    assert!(table_exists(&connection, "write_approval_decisions"));
}

#[test]
fn storage_owns_exact_ten_minute_ttl_and_caller_cannot_override() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-ttl", 1);
    let now = Utc::now();

    let reserved = fx.store.reserve_at(&input, now).unwrap();
    let WriteReservationOutcome::Reserved(row) = reserved else {
        panic!("expected Reserved");
    };
    assert_eq!(row.created_at, now);
    assert_eq!(
        row.expires_at,
        now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS)
    );
    assert_eq!(WRITE_PROPOSAL_TTL_SECS, 600);

    // reserve() uses wall clock; only assert expires_at - created_at == TTL.
    fx.store
        .transition(
            &input.proposal_id,
            WriteProposalState::Cancelled,
            Utc::now(),
        )
        .unwrap();
    let input2 = reserve_input(&fx, "call-ttl-2", 2);
    let WriteReservationOutcome::Reserved(row2) = fx.store.reserve(&input2).unwrap() else {
        panic!("expected Reserved");
    };
    assert_eq!(
        row2.expires_at - row2.created_at,
        Duration::seconds(WRITE_PROPOSAL_TTL_SECS)
    );
}

#[test]
fn reserve_is_atomic_idempotent_and_detects_semantic_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-1", 11);
    let mut conflicting = reserve_input(&fx, "call-1", 12);
    conflicting.proposal_id = Uuid::new_v4().to_string();
    conflicting.reservation_key = first.reservation_key.clone();

    let reserved = fx.store.reserve(&first).unwrap();
    let replay = fx.store.reserve(&first).unwrap();
    let conflict = fx.store.reserve(&conflicting).unwrap();

    assert!(matches!(
        reserved,
        WriteReservationOutcome::Reserved(ref row)
            if row.state == WriteProposalState::PendingApproval
                && row.proposal_fingerprint == first.proposal_fingerprint
    ));
    assert!(matches!(
        replay,
        WriteReservationOutcome::Replay(ref row)
            if row.proposal_fingerprint == first.proposal_fingerprint
                && row.proposal_id == first.proposal_id
    ));
    assert!(matches!(
        conflict,
        WriteReservationOutcome::Conflict(ref row)
            if row.call_id == "call-1"
                && row.proposal_fingerprint == first.proposal_fingerprint
    ));
}

#[test]
fn alias_collision_on_proposal_id_or_reservation_key_is_opaque_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-alias-a", 21);
    fx.store.reserve(&first).unwrap();
    fx.store
        .transition(
            &first.proposal_id,
            WriteProposalState::Cancelled,
            Utc::now(),
        )
        .unwrap();

    // Same proposal_id, different call/reservation identity.
    let mut id_clash = reserve_input(&fx, "call-alias-b", 22);
    id_clash.proposal_id = first.proposal_id.clone();
    let conflict_id = fx.store.reserve(&id_clash).unwrap();
    assert!(matches!(
        conflict_id,
        WriteReservationOutcome::Conflict(ref row) if row.proposal_id == first.proposal_id
    ));

    // Same reservation_key, different call identity.
    let mut key_clash = reserve_input(&fx, "call-alias-c", 23);
    key_clash.reservation_key = first.reservation_key.clone();
    let conflict_key = fx.store.reserve(&key_clash).unwrap();
    assert!(matches!(
        conflict_key,
        WriteReservationOutcome::Conflict(ref row)
            if row.reservation_key == first.reservation_key
    ));
}

#[test]
fn reserve_busy_when_another_active_proposal_exists_on_same_run() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-a", 31);
    let second = reserve_input(&fx, "call-b", 32);

    let reserved = fx.store.reserve(&first).unwrap();
    assert!(matches!(reserved, WriteReservationOutcome::Reserved(_)));

    let busy = fx.store.reserve(&second).unwrap();
    assert!(matches!(
        busy,
        WriteReservationOutcome::Busy(ref row)
            if row.call_id == "call-a" && row.proposal_id == first.proposal_id
    ));
}

#[test]
fn reserve_allows_new_call_after_prior_terminal_on_same_run() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-old", 41);
    fx.store.reserve(&first).unwrap();
    fx.store
        .transition(
            &first.proposal_id,
            WriteProposalState::Cancelled,
            Utc::now(),
        )
        .unwrap();

    let second = reserve_input(&fx, "call-new", 42);
    let outcome = fx.store.reserve(&second).unwrap();
    assert!(matches!(
        outcome,
        WriteReservationOutcome::Reserved(ref row)
            if row.call_id == "call-new"
                && row.state == WriteProposalState::PendingApproval
    ));
}

#[test]
fn reserve_rejects_missing_project_conversation_run_context() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = reserve_input(&fx, "call-x", 51);
    input.project_id = Uuid::new_v4().to_string();

    let err = fx.store.reserve(&input).unwrap_err();
    assert_eq!(err, WriteProposalStoreError::MissingContext);
}

#[test]
fn invalid_digests_and_byte_counts_reject_before_sql() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    let mut upper = reserve_input(&fx, "call-up", 61);
    upper.proposed_digest = format!(
        "sha256:{}",
        "A".repeat(64) // uppercase hex
    );
    assert_eq!(
        fx.store.reserve(&upper).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut short = reserve_input(&fx, "call-short", 62);
    short.proposed_digest = format!("sha256:{}", "a".repeat(63));
    assert_eq!(
        fx.store.reserve(&short).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut long = reserve_input(&fx, "call-long", 63);
    long.proposed_digest = format!("sha256:{}", "a".repeat(65));
    assert_eq!(
        fx.store.reserve(&long).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut non_hex = reserve_input(&fx, "call-nh", 64);
    non_hex.proposed_digest = format!("sha256:{}", "g".repeat(64));
    assert_eq!(
        fx.store.reserve(&non_hex).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut zero_content = reserve_input(&fx, "call-z", 65);
    zero_content.proposed_bytes = 0;
    assert_eq!(
        fx.store.reserve(&zero_content).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut oversize = reserve_input(&fx, "call-os", 66);
    oversize.proposed_bytes = WRITE_CONTENT_BYTE_CAP + 1;
    assert_eq!(
        fx.store.reserve(&oversize).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut create_with_base = reserve_input(&fx, "call-cb", 67);
    create_with_base.base_bytes = 1;
    assert_eq!(
        fx.store.reserve(&create_with_base).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut modify_no_base = modify_input(&fx, "call-mb", 68);
    modify_no_base.base_digest = None;
    assert_eq!(
        fx.store.reserve(&modify_no_base).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let mut modify_over_base = modify_input(&fx, "call-mob", 69);
    modify_over_base.base_bytes = WRITE_CONTENT_BYTE_CAP + 1;
    assert_eq!(
        fx.store.reserve(&modify_over_base).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    // Empty existing text is allowed for modify (base_bytes == 0).
    let mut empty_base = modify_input(&fx, "call-eb", 70);
    empty_base.base_bytes = 0;
    assert!(matches!(
        fx.store.reserve(&empty_base).unwrap(),
        WriteReservationOutcome::Reserved(_)
    ));

    // Bad proposal_id (not UUID).
    let mut bad_id = reserve_input(&fx, "call-bi", 71);
    bad_id.proposal_id = "not-a-uuid".into();
    assert_eq!(
        fx.store.reserve(&bad_id).unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );
}

#[test]
fn ownership_mismatch_on_decide_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-own", 81);
    fx.store.reserve(&input).unwrap();

    let err = fx
        .store
        .decide(
            &input.proposal_id,
            "foreign-conversation-id",
            WriteDecision::ApproveOnce,
            &grant_digest(81),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap_err();
    assert_eq!(err, WriteProposalStoreError::OwnershipMismatch);
}

#[test]
fn expiry_boundary_is_true_at_exact_expires_at() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-exp", 91);
    let now = Utc::now();
    fx.store.reserve_at(&input, now).unwrap();
    let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);

    let before = fx
        .store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant_digest(91),
            &input.proposal_fingerprint,
            expires_at - Duration::milliseconds(1),
        )
        .unwrap();
    assert!(matches!(before, WriteDecisionOutcome::Accepted(_)));

    // Interrupt so a second proposal can reserve on the same run.
    fx.store
        .transition(
            &input.proposal_id,
            WriteProposalState::Interrupted,
            Utc::now(),
        )
        .unwrap();

    let input2 = reserve_input(&fx, "call-exp2", 92);
    fx.store.reserve_at(&input2, now).unwrap();
    let outcome_at = fx
        .store
        .decide(
            &input2.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant_digest(92),
            &input2.proposal_fingerprint,
            expires_at,
        )
        .unwrap();

    assert!(matches!(
        outcome_at,
        WriteDecisionOutcome::Expired(ref row)
            if row.state == WriteProposalState::Expired
                && row.proposal_id == input2.proposal_id
    ));
}

#[test]
fn decision_accepts_approve_and_reject_once() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let approve = reserve_input(&fx, "call-dec-a", 101);
    fx.store.reserve(&approve).unwrap();

    let accepted = fx
        .store
        .decide(
            &approve.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant_digest(101),
            &approve.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        accepted,
        WriteDecisionOutcome::Accepted(ref row)
            if row.state == WriteProposalState::Approved
    ));

    let replay = fx
        .store
        .decide(
            &approve.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant_digest(101),
            &approve.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(replay, WriteDecisionOutcome::Accepted(_)));

    let conflict = fx
        .store
        .decide(
            &approve.proposal_id,
            &fx.conversation_id,
            WriteDecision::Reject,
            &grant_digest(102),
            &approve.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(conflict, WriteDecisionOutcome::Conflict(_)));

    fx.store
        .transition(
            &approve.proposal_id,
            WriteProposalState::Interrupted,
            Utc::now(),
        )
        .unwrap();
    let reject = reserve_input(&fx, "call-dec-r", 103);
    fx.store.reserve(&reject).unwrap();
    let rejected = fx
        .store
        .decide(
            &reject.proposal_id,
            &fx.conversation_id,
            WriteDecision::Reject,
            &grant_digest(103),
            &reject.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        rejected,
        WriteDecisionOutcome::Accepted(ref row)
            if row.state == WriteProposalState::Rejected
    ));
    let history = fx.store.history(&reject.proposal_id).unwrap();
    assert_eq!(
        history,
        vec![
            WriteProposalState::PendingApproval,
            WriteProposalState::Rejected,
        ]
    );
}

#[test]
fn approval_grant_digest_is_single_use_globally() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-g1", 111);
    fx.store.reserve(&first).unwrap();
    let shared_grant = grant_digest(111);
    fx.store
        .decide(
            &first.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &shared_grant,
            &first.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    fx.store
        .transition(
            &first.proposal_id,
            WriteProposalState::Interrupted,
            Utc::now(),
        )
        .unwrap();

    let second = reserve_input(&fx, "call-g2", 112);
    fx.store.reserve(&second).unwrap();
    let replay_grant = fx
        .store
        .decide(
            &second.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &shared_grant,
            &second.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        replay_grant,
        WriteDecisionOutcome::Conflict(ref row)
            if row.proposal_id == second.proposal_id
                && row.state == WriteProposalState::PendingApproval
    ));
}

#[test]
fn apply_lease_claim_and_typed_terminal_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = modify_input(&fx, "call-apply", 121);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 121);

    let claimed = fx
        .store
        .claim_apply(&input.proposal_id, "owner-a", 5_000, Utc::now())
        .unwrap();
    assert!(matches!(
        claimed,
        WriteApplyClaimOutcome::Claimed(ref row)
            if row.state == WriteProposalState::Applying
    ));

    let busy = fx
        .store
        .claim_apply(&input.proposal_id, "owner-b", 5_000, Utc::now())
        .unwrap();
    assert!(matches!(busy, WriteApplyClaimOutcome::Busy(_)));

    let same_owner = fx
        .store
        .claim_apply(&input.proposal_id, "owner-a", 5_000, Utc::now())
        .unwrap();
    assert!(matches!(same_owner, WriteApplyClaimOutcome::Claimed(_)));

    let receipt = success_receipt(&input);
    let terminal = fx
        .store
        .complete_apply(&input.proposal_id, "owner-a", &receipt, Utc::now())
        .unwrap();
    assert_eq!(terminal.state, WriteProposalState::Succeeded);
    let expected_json = serde_json::to_string(&receipt).unwrap();
    assert_eq!(
        terminal.result_json.as_deref(),
        Some(expected_json.as_str())
    );

    let claim_after = fx
        .store
        .claim_apply(&input.proposal_id, "owner-a", 5_000, Utc::now())
        .unwrap();
    assert!(matches!(
        claim_after,
        WriteApplyClaimOutcome::Terminal(ref row)
            if row.state == WriteProposalState::Succeeded
    ));

    let again = fx
        .store
        .complete_apply(&input.proposal_id, "owner-a", &receipt, Utc::now())
        .unwrap();
    assert_eq!(again.state, WriteProposalState::Succeeded);

    let err = fx
        .store
        .transition(&input.proposal_id, WriteProposalState::Failed, Utc::now())
        .unwrap_err();
    assert_eq!(err, WriteProposalStoreError::InvalidTransition);

    let history = fx.store.history(&input.proposal_id).unwrap();
    assert_eq!(
        history,
        vec![
            WriteProposalState::PendingApproval,
            WriteProposalState::Approved,
            WriteProposalState::Applying,
            WriteProposalState::Succeeded,
        ]
    );
}

#[test]
fn apply_failure_error_codes_serialize_round_trip_closed_snake_case() {
    for (code, wire) in APPLY_FAILURE_CODES.iter().zip(APPLY_FAILURE_WIRE.iter()) {
        let receipt = failure_receipt_with(*code);
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(
            json.contains(&format!(r#""error_code":"{wire}""#)),
            "expected snake_case wire value {wire} in {json}"
        );
        assert!(
            !json.contains("BaseChanged")
                && !json.contains("PathRejected")
                && !json.contains("WriteFailed"),
            "PascalCase variant names must not appear on the wire: {json}"
        );
        let back: WriteApplyReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back, receipt);

        // Direct enum round-trip.
        let code_json = serde_json::to_string(code).unwrap();
        assert_eq!(code_json, format!("\"{wire}\""));
        let code_back: WriteApplyErrorCode = serde_json::from_str(&code_json).unwrap();
        assert_eq!(code_back, *code);
    }

    // Closed set: approval / provider / terminal / git / free text rejected.
    for bad in [
        r#""approval_required""#,
        r#""approval_rejected""#,
        r#""write_conflict""#,
        r#""git_error""#,
        r#""io_error""#,
        r#""something_went_wrong""#,
        r#""WRITE_FAILED""#,
        r#""WriteFailed""#,
    ] {
        assert!(
            serde_json::from_str::<WriteApplyErrorCode>(bad).is_err(),
            "unexpected code accepted: {bad}"
        );
    }

    // Failure receipt with unknown error_code or free-form message field fails closed.
    assert!(
        serde_json::from_str::<WriteApplyReceipt>(
            r#"{"outcome":"failure","error_code":"approval_required","recovered":false}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<WriteApplyReceipt>(
            r#"{"outcome":"failure","error_code":"write_failed","recovered":false,"message":"disk full"}"#
        )
        .is_err()
    );
}

#[test]
fn complete_apply_accepts_all_three_apply_failure_codes() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    for (index, code) in APPLY_FAILURE_CODES.iter().enumerate() {
        let call_id = format!("call-fail-code-{index}");
        let mut input = reserve_input(&fx, &call_id, 300 + index as u64);
        if index > 0 {
            let run = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
            input.run_id = run;
            input.reservation_key = format!(
                "wp:{}:{}:{}:{}",
                fx.project_id, fx.conversation_id, run, call_id
            );
        }
        fx.store.reserve(&input).unwrap();
        approve(&fx, &input, 300 + index as u64);
        let owner = format!("owner-fail-{index}");
        claim(&fx, &input.proposal_id, &owner, 5_000);

        let receipt = failure_receipt_with(*code);
        let terminal = fx
            .store
            .complete_apply(&input.proposal_id, &owner, &receipt, Utc::now())
            .unwrap();
        assert_eq!(terminal.state, WriteProposalState::Failed);
        let expected = serde_json::to_string(&receipt).unwrap();
        assert_eq!(terminal.result_json.as_deref(), Some(expected.as_str()));
        assert!(
            terminal
                .result_json
                .as_deref()
                .unwrap()
                .contains(APPLY_FAILURE_WIRE[index])
        );

        // Replay of the same typed receipt is idempotent.
        let again = fx
            .store
            .complete_apply(&input.proposal_id, &owner, &receipt, Utc::now())
            .unwrap();
        assert_eq!(again.state, WriteProposalState::Failed);
        assert_eq!(again.result_json.as_deref(), Some(expected.as_str()));
    }
}

#[test]
fn typed_receipt_rejects_arbitrary_json_secrets_and_mismatched_fields() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-receipt", 131);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 131);
    claim(&fx, &input.proposal_id, "owner-r", 5_000);

    // Mismatched success path cannot reach SQLite.
    let bad_path = WriteApplyReceipt::Success {
        path: "other/path.ts".into(),
        mode: input.mode,
        bytes_written: input.proposed_bytes,
        content_digest: input.proposed_digest.clone(),
        recovered: false,
    };
    assert_eq!(
        fx.store
            .complete_apply(&input.proposal_id, "owner-r", &bad_path, Utc::now())
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let bad_bytes = WriteApplyReceipt::Success {
        path: input.path.clone(),
        mode: input.mode,
        bytes_written: input.proposed_bytes + 1,
        content_digest: input.proposed_digest.clone(),
        recovered: false,
    };
    assert_eq!(
        fx.store
            .complete_apply(&input.proposal_id, "owner-r", &bad_bytes, Utc::now())
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    let bad_digest = WriteApplyReceipt::Success {
        path: input.path.clone(),
        mode: input.mode,
        bytes_written: input.proposed_bytes,
        content_digest: digest(0xDEAD),
        recovered: false,
    };
    assert_eq!(
        fx.store
            .complete_apply(&input.proposal_id, "owner-r", &bad_digest, Utc::now())
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    // recovered=true is reserved for reconcile_applying, not live complete.
    let recovered_live = WriteApplyReceipt::Success {
        path: input.path.clone(),
        mode: input.mode,
        bytes_written: input.proposed_bytes,
        content_digest: input.proposed_digest.clone(),
        recovered: true,
    };
    assert_eq!(
        fx.store
            .complete_apply(&input.proposal_id, "owner-r", &recovered_live, Utc::now())
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    // Secret-bearing / unknown-field wire shapes fail closed at deserialize.
    let secret_wire = format!(
        r#"{{"outcome":"success","path":"{}","mode":"create","bytes_written":42,"content_digest":"{}","recovered":false,"content":"{}"}}"#,
        input.path, input.proposed_digest, SECRET_PROPOSED
    );
    assert!(serde_json::from_str::<WriteApplyReceipt>(&secret_wire).is_err());

    let failed = fx
        .store
        .complete_apply(
            &input.proposal_id,
            "owner-r",
            &failure_receipt(),
            Utc::now(),
        )
        .unwrap();
    assert_eq!(failed.state, WriteProposalState::Failed);
    let expected = serde_json::to_string(&failure_receipt()).unwrap();
    assert_eq!(failed.result_json.as_deref(), Some(expected.as_str()));
    assert!(
        !failed
            .result_json
            .as_deref()
            .unwrap()
            .contains(SECRET_PROPOSED)
    );
    assert!(!failed.result_json.as_deref().unwrap().contains("content"));
}

#[test]
fn zero_and_oversize_lease_reject_without_silent_clamp() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-lease", 141);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 141);

    assert_eq!(
        fx.store
            .claim_apply(&input.proposal_id, "owner-z", 0, Utc::now())
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );
    assert_eq!(
        fx.store
            .claim_apply(
                &input.proposal_id,
                "owner-z",
                MAX_APPLY_LEASE_MS + 1,
                Utc::now()
            )
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );

    // Boundary max is accepted.
    assert!(matches!(
        fx.store
            .claim_apply(
                &input.proposal_id,
                "owner-z",
                MAX_APPLY_LEASE_MS,
                Utc::now()
            )
            .unwrap(),
        WriteApplyClaimOutcome::Claimed(_)
    ));
}

#[test]
fn complete_apply_requires_live_lease_and_exact_owner() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-lease-exp", 151);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 151);

    let claim_at = Utc::now();
    let lease_ms = 5_000u64;
    fx.store
        .claim_apply(&input.proposal_id, "owner-old", lease_ms, claim_at)
        .unwrap();
    let lease_expires = claim_at + Duration::milliseconds(lease_ms as i64);

    // Exact boundary: lease expires_at is not live (expires_at > now required).
    let err_at = fx
        .store
        .complete_apply(
            &input.proposal_id,
            "owner-old",
            &success_receipt(&input),
            lease_expires,
        )
        .unwrap_err();
    assert_eq!(err_at, WriteProposalStoreError::ReservationConflict);

    // Still applying after failed complete.
    let still = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still.state, WriteProposalState::Applying);

    // New owner reclaims after expiry.
    let reclaim_at = lease_expires + Duration::milliseconds(1);
    let reclaimed = fx
        .store
        .claim_apply(&input.proposal_id, "owner-new", lease_ms, reclaim_at)
        .unwrap();
    assert!(matches!(reclaimed, WriteApplyClaimOutcome::Claimed(_)));

    // Stale owner cannot complete after reclaim.
    let stale = fx
        .store
        .complete_apply(
            &input.proposal_id,
            "owner-old",
            &success_receipt(&input),
            reclaim_at + Duration::milliseconds(1),
        )
        .unwrap_err();
    assert_eq!(stale, WriteProposalStoreError::ReservationConflict);

    // Live new owner can complete just before their lease ends.
    let new_lease_end = reclaim_at + Duration::milliseconds(lease_ms as i64);
    let ok = fx
        .store
        .complete_apply(
            &input.proposal_id,
            "owner-new",
            &success_receipt(&input),
            new_lease_end - Duration::milliseconds(1),
        )
        .unwrap();
    assert_eq!(ok.state, WriteProposalState::Succeeded);
}

#[test]
fn live_interrupt_requires_exact_owner_and_live_lease() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-live-int", 191);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 191);

    let claim_at = Utc::now();
    let lease_ms = 5_000u64;
    fx.store
        .claim_apply(&input.proposal_id, "owner-live", lease_ms, claim_at)
        .unwrap();
    let lease_expires = claim_at + Duration::milliseconds(lease_ms as i64);

    // Wrong owner cannot interrupt while the live lease is held.
    let wrong = fx
        .store
        .interrupt_apply(
            &input.proposal_id,
            "owner-other",
            claim_at + Duration::milliseconds(1),
        )
        .unwrap_err();
    assert_eq!(wrong, WriteProposalStoreError::ReservationConflict);
    let still = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still.state, WriteProposalState::Applying);
    let history_after_wrong = fx.store.history(&input.proposal_id).unwrap();
    assert_eq!(
        history_after_wrong,
        vec![
            WriteProposalState::PendingApproval,
            WriteProposalState::Approved,
            WriteProposalState::Applying,
        ]
    );
    assert_apply_lease(&fx.db_path, &input.proposal_id, Some("owner-live"), true);

    // Expired owner (exact boundary: expires_at is not live) cannot interrupt.
    let expired = fx
        .store
        .interrupt_apply(&input.proposal_id, "owner-live", lease_expires)
        .unwrap_err();
    assert_eq!(expired, WriteProposalStoreError::ReservationConflict);
    let still_expired = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still_expired.state, WriteProposalState::Applying);
    assert_eq!(
        fx.store.history(&input.proposal_id).unwrap(),
        history_after_wrong
    );
    assert_apply_lease(&fx.db_path, &input.proposal_id, Some("owner-live"), true);

    // New owner reclaims after expiry; stale owner still cannot interrupt.
    let reclaim_at = lease_expires + Duration::milliseconds(1);
    let reclaimed = fx
        .store
        .claim_apply(&input.proposal_id, "owner-new", lease_ms, reclaim_at)
        .unwrap();
    assert!(matches!(reclaimed, WriteApplyClaimOutcome::Claimed(_)));
    assert_apply_lease(&fx.db_path, &input.proposal_id, Some("owner-new"), true);

    let stale = fx
        .store
        .interrupt_apply(
            &input.proposal_id,
            "owner-live",
            reclaim_at + Duration::milliseconds(1),
        )
        .unwrap_err();
    assert_eq!(stale, WriteProposalStoreError::ReservationConflict);
    let still_new = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still_new.state, WriteProposalState::Applying);
    assert_eq!(
        fx.store.history(&input.proposal_id).unwrap(),
        history_after_wrong
    );
    assert_apply_lease(&fx.db_path, &input.proposal_id, Some("owner-new"), true);

    // Correct live owner interrupts: terminal interrupted, history exact, lease cleared.
    let live_at = reclaim_at + Duration::milliseconds(2);
    let interrupted = fx
        .store
        .interrupt_apply(&input.proposal_id, "owner-new", live_at)
        .unwrap();
    assert_eq!(interrupted.state, WriteProposalState::Interrupted);
    assert!(interrupted.result_json.is_none());
    assert_eq!(
        fx.store.history(&input.proposal_id).unwrap(),
        vec![
            WriteProposalState::PendingApproval,
            WriteProposalState::Approved,
            WriteProposalState::Applying,
            WriteProposalState::Interrupted,
        ]
    );
    assert_apply_lease(&fx.db_path, &input.proposal_id, None, false);

    // Already interrupted: further interrupt fails closed without history growth.
    let again = fx
        .store
        .interrupt_apply(
            &input.proposal_id,
            "owner-new",
            live_at + Duration::milliseconds(1),
        )
        .unwrap_err();
    assert_eq!(again, WriteProposalStoreError::ReservationConflict);
    assert_eq!(
        fx.store
            .history(&input.proposal_id)
            .unwrap()
            .last()
            .copied(),
        Some(WriteProposalState::Interrupted)
    );
    assert_eq!(fx.store.history(&input.proposal_id).unwrap().len(), 4);
}

/// Assert apply lease columns on the proposal row.
fn assert_apply_lease(
    db_path: &std::path::Path,
    proposal_id: &str,
    expected_owner: Option<&str>,
    expect_lease_present: bool,
) {
    let conn = Connection::open(db_path).unwrap();
    let (owner, lease): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT apply_owner, apply_lease_expires_at FROM write_proposals WHERE proposal_id = ?1",
            [proposal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(owner.as_deref(), expected_owner);
    if expect_lease_present {
        assert!(lease.is_some(), "expected live lease timestamp");
    } else {
        assert!(lease.is_none(), "lease must be cleared, got {lease:?}");
        assert!(owner.is_none(), "owner must be cleared, got {owner:?}");
    }
}

#[test]
fn reconcile_applying_three_outcomes_replay_and_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    // --- MatchesProposed -> recovered success ---
    let success_input = reserve_input(&fx, "call-rec-s", 161);
    fx.store.reserve(&success_input).unwrap();
    approve(&fx, &success_input, 161);
    claim(&fx, &success_input.proposal_id, "owner-s", 30_000);

    let recovered = fx
        .store
        .reconcile_applying(
            &success_input.proposal_id,
            WriteApplyReconcileObservation::MatchesProposed,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(recovered.state, WriteProposalState::Succeeded);
    let expected_success = WriteApplyReceipt::Success {
        path: success_input.path.clone(),
        mode: success_input.mode,
        bytes_written: success_input.proposed_bytes,
        content_digest: success_input.proposed_digest.clone(),
        recovered: true,
    };
    assert_eq!(
        recovered.result_json.as_deref(),
        Some(serde_json::to_string(&expected_success).unwrap().as_str())
    );

    // Exact replay is deterministic.
    let replay = fx
        .store
        .reconcile_applying(
            &success_input.proposal_id,
            WriteApplyReconcileObservation::MatchesProposed,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(replay.state, WriteProposalState::Succeeded);

    // Conflicting observation fails closed.
    let conflict = fx
        .store
        .reconcile_applying(
            &success_input.proposal_id,
            WriteApplyReconcileObservation::Neither,
            Utc::now(),
        )
        .unwrap_err();
    assert_eq!(conflict, WriteProposalStoreError::InvalidResult);

    // --- StillBaseOrAbsent -> interrupted, no receipt ---
    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut interrupt_input = reserve_input(&fx, "call-rec-i", 162);
    interrupt_input.run_id = run2;
    interrupt_input.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-rec-i"
    );
    fx.store.reserve(&interrupt_input).unwrap();
    approve(&fx, &interrupt_input, 162);
    claim(&fx, &interrupt_input.proposal_id, "owner-i", 30_000);

    let interrupted = fx
        .store
        .reconcile_applying(
            &interrupt_input.proposal_id,
            WriteApplyReconcileObservation::StillBaseOrAbsent,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(interrupted.state, WriteProposalState::Interrupted);
    assert!(interrupted.result_json.is_none());

    let interrupt_replay = fx
        .store
        .reconcile_applying(
            &interrupt_input.proposal_id,
            WriteApplyReconcileObservation::StillBaseOrAbsent,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(interrupt_replay.state, WriteProposalState::Interrupted);

    // --- Neither -> recovered opaque failure ---
    let run3 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut fail_input = reserve_input(&fx, "call-rec-f", 163);
    fail_input.run_id = run3;
    fail_input.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run3, "call-rec-f"
    );
    fx.store.reserve(&fail_input).unwrap();
    approve(&fx, &fail_input, 163);
    claim(&fx, &fail_input.proposal_id, "owner-f", 30_000);

    let failed = fx
        .store
        .reconcile_applying(
            &fail_input.proposal_id,
            WriteApplyReconcileObservation::Neither,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(failed.state, WriteProposalState::Failed);
    let expected_fail = WriteApplyReceipt::Failure {
        error_code: WriteApplyErrorCode::WriteFailed,
        recovered: true,
    };
    assert_eq!(
        failed.result_json.as_deref(),
        Some(serde_json::to_string(&expected_fail).unwrap().as_str())
    );

    // Owner-independent: no owner parameter required; works without live lease.
    // Conflicting replay of MatchesProposed after Neither fails.
    assert_eq!(
        fx.store
            .reconcile_applying(
                &fail_input.proposal_id,
                WriteApplyReconcileObservation::MatchesProposed,
                Utc::now(),
            )
            .unwrap_err(),
        WriteProposalStoreError::InvalidResult
    );
}

#[test]
fn list_applying_returns_only_applying_rows() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    let pending = reserve_input(&fx, "call-list-p", 201);
    fx.store.reserve(&pending).unwrap();

    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut applying = reserve_input(&fx, "call-list-a", 202);
    applying.run_id = run2;
    applying.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-list-a"
    );
    fx.store.reserve(&applying).unwrap();
    approve(&fx, &applying, 202);
    claim(&fx, &applying.proposal_id, "owner-list", 30_000);

    let listed = fx.store.list_applying().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].proposal_id, applying.proposal_id);
    assert_eq!(listed[0].state, WriteProposalState::Applying);
    // Enumeration is read-only — row stays applying.
    assert_eq!(
        fx.store.get(&applying.proposal_id).unwrap().unwrap().state,
        WriteProposalState::Applying
    );
}

#[test]
fn recovery_interrupts_pending_and_approved_but_keeps_applying() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    let pending = reserve_input(&fx, "call-rec-p", 171);
    fx.store.reserve(&pending).unwrap();

    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut approved = reserve_input(&fx, "call-rec-a", 172);
    approved.run_id = run2;
    approved.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-rec-a"
    );
    fx.store.reserve(&approved).unwrap();
    approve(&fx, &approved, 172);

    let run3 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut applying = reserve_input(&fx, "call-rec-x", 173);
    applying.run_id = run3;
    applying.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run3, "call-rec-x"
    );
    fx.store.reserve(&applying).unwrap();
    approve(&fx, &applying, 173);
    claim(&fx, &applying.proposal_id, "owner-x", 30_000);

    let recovered = fx.store.recover_interrupted(Utc::now()).unwrap();
    assert_eq!(recovered.len(), 2);
    assert!(
        recovered
            .iter()
            .all(|row| row.state == WriteProposalState::Interrupted)
    );

    let pending_row = fx.store.get(&pending.proposal_id).unwrap().unwrap();
    assert_eq!(pending_row.state, WriteProposalState::Interrupted);
    let approved_row = fx.store.get(&approved.proposal_id).unwrap().unwrap();
    assert_eq!(approved_row.state, WriteProposalState::Interrupted);
    let applying_row = fx.store.get(&applying.proposal_id).unwrap().unwrap();
    assert_eq!(applying_row.state, WriteProposalState::Applying);

    let listed = fx.store.list_for_conversation(&fx.conversation_id).unwrap();
    assert!(
        listed
            .iter()
            .any(|row| row.proposal_id == applying.proposal_id
                && row.state == WriteProposalState::Applying)
    );
}

#[test]
fn terminal_decision_outcome_when_already_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-term", 181);
    fx.store.reserve(&input).unwrap();
    fx.store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::Reject,
            &grant_digest(181),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();

    let terminal = fx
        .store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::Reject,
            &grant_digest(181),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        terminal,
        WriteDecisionOutcome::Terminal(ref row)
            if row.state == WriteProposalState::Rejected
    ));
}

#[test]
fn proposed_and_base_content_and_raw_grant_never_enter_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = modify_input(&fx, "call-secret", 191);
    input.proposed_digest = digest(0xCA);
    input.base_digest = Some(digest(0xCB));
    assert!(!input.proposed_digest.contains(SECRET_PROPOSED));
    assert!(!input.base_digest.as_deref().unwrap().contains(SECRET_BASE));

    fx.store.reserve(&input).unwrap();
    let grant = grant_digest(191);
    assert!(!grant.contains(SECRET_GRANT_RAW));
    fx.store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant,
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    claim(&fx, &input.proposal_id, "owner-s", 5_000);
    let receipt = success_receipt(&input);
    fx.store
        .complete_apply(&input.proposal_id, "owner-s", &receipt, Utc::now())
        .unwrap();

    let stored: StoredWriteProposal = fx.store.get(&input.proposal_id).unwrap().unwrap();
    let row_blob = format!("{stored:?}");
    assert!(
        !row_blob.contains(SECRET_PROPOSED) && !row_blob.contains(SECRET_BASE),
        "repository DTO must not carry proposed/base content"
    );
    assert!(!row_blob.contains(SECRET_GRANT_RAW));

    let history = fx.store.history(&input.proposal_id).unwrap();
    let history_blob = format!("{history:?}");
    assert!(!history_blob.contains(SECRET_PROPOSED));
    assert!(!history_blob.contains(SECRET_BASE));

    let bytes = std::fs::read(&fx.db_path).unwrap();
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(
        !haystack.contains(SECRET_PROPOSED),
        "proposed content leaked into SQLite file"
    );
    assert!(
        !haystack.contains(SECRET_BASE),
        "base content leaked into SQLite file"
    );
    assert!(
        !haystack.contains(SECRET_GRANT_RAW),
        "raw approval grant leaked into SQLite file"
    );

    let connection = Connection::open(&fx.db_path).unwrap();
    let mut stmt = connection
        .prepare("SELECT result_json FROM write_proposal_transitions")
        .unwrap();
    let rows: Vec<Option<String>> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    for json in rows.into_iter().flatten() {
        assert!(!json.contains(SECRET_PROPOSED));
        assert!(!json.contains(SECRET_BASE));
        assert!(!json.contains(SECRET_GRANT_RAW));
        assert!(!json.contains("native_path"));
        assert!(!json.contains("debug"));
    }
}

#[test]
fn list_for_conversation_is_conversation_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-list", 201);
    fx.store.reserve(&input).unwrap();

    let listed = fx.store.list_for_conversation(&fx.conversation_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].proposal_id, input.proposal_id);

    let foreign = fx
        .store
        .list_for_conversation("00000000-0000-0000-0000-000000000099")
        .unwrap();
    assert!(foreign.is_empty());
}
