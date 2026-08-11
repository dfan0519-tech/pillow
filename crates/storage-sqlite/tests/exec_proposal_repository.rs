//! Durable execution-proposal ledger tests (C4-D2A Task 6).
//!
//! Covers storage-owned TTL, reservation/replay/conflict, non-terminal
//! uniqueness per run, decision single-use, run claim, terminal receipt,
//! expiry, restart recovery, conversation listing, and privacy scans proving
//! no argv, native path, raw grant, environment or transcript bytes.

mod common;

use chrono::{Duration, Utc};
use common::{open_pair, seed_project, table_exists};
use keeper_agent_protocol::RunId as ProtocolRunId;
use keeper_policy::{EXEC_PROPOSAL_TTL_SECS, ExecDecision, ExecProposalState};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    AddPermissionRule, ExecDecisionOutcome, ExecProposalReserveInput, ExecReservationOutcome,
    ExecRunClaimOutcome, ExecTerminalReceipt, PermissionMode, PermissionRuleAction,
    PermissionRuleMatchSnapshot, PermissionRulePrefix, PermissionRuleReceiptActor,
    PermissionRuleReceiptDecision, PermissionRuleScope, PermissionRuleSegmentMatchSnapshot,
    SqliteExecProposalRepository, SqlitePermissionRuleRepository, StoredExecProposal,
};
use rusqlite::Connection;
use uuid::Uuid;

/// Distinctive strings that must never appear in SQLite or repository rows.
const SECRET_ARGV_0: &str = "SECRET_ARGV_NEVER_STORE_c4d2a_xyz789";
const SECRET_NATIVE_EXE: &str = "C:\\Users\\SECRET_NEVER_STORE\\bin\\cargo.exe";
const SECRET_NATIVE_CWD: &str = "F:\\projects\\SECRET_NEVER_STORE\\root";
const SECRET_GRANT_RAW: &str = "raw-exec-approval-grant-must-not-be-stored-c4d2a-42";
const SECRET_ENV_VALUE: &str = "SECRET_ENV_TOKEN_c4d2a_never_persist=abc123";
const SECRET_STDOUT: &str = "SECRET_STDOUT_TRANSCRIPT_c4d2a_line_one\n";
const SECRET_STDERR: &str = "SECRET_STDERR_TRANSCRIPT_c4d2a_line_two\n";

struct Fixture {
    store: SqliteExecProposalRepository,
    project_id: String,
    conversation_id: String,
    run_id: ProtocolRunId,
    db_path: std::path::PathBuf,
}

#[test]
fn bypass_decision_persists_mode_receipt_and_approves_process() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "mode-bypass", 0xC3);
    fx.store.reserve(&input).unwrap();
    let decided_at = Utc::now();

    let outcome = fx
        .store
        .decide_by_permission_mode(
            &input.proposal_id,
            &fx.conversation_id,
            PermissionMode::Bypass,
            &grant_digest(0xC3),
            &input.proposal_fingerprint,
            decided_at,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        ExecDecisionOutcome::Accepted(ref row) if row.state == ExecProposalState::Approved
    ));

    let receipt = fx
        .store
        .get_permission_mode_receipt(&input.proposal_id)
        .unwrap()
        .expect("durable process mode receipt");
    assert_eq!(receipt.proposal_id, input.proposal_id);
    assert_eq!(receipt.permission_mode, PermissionMode::Bypass);
    assert_eq!(receipt.decided_at, decided_at);
    assert!(
        fx.store
            .get_rule_receipt(&receipt.proposal_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn default_and_accept_edits_cannot_create_process_approval_receipts() {
    for permission_mode in [PermissionMode::Default, PermissionMode::AcceptEdits] {
        let temp = tempfile::tempdir().unwrap();
        let fx = fixture(&temp);
        let input = reserve_input(&fx, permission_mode.as_str(), 0xC4);
        fx.store.reserve(&input).unwrap();

        let error = fx
            .store
            .decide_by_permission_mode(
                &input.proposal_id,
                &fx.conversation_id,
                permission_mode,
                &grant_digest(0xC4),
                &input.proposal_fingerprint,
                Utc::now(),
            )
            .expect_err("only bypass may auto-approve a process");
        assert_eq!(
            error,
            keeper_storage_sqlite::ExecProposalStoreError::InvalidPermissionModeReceipt
        );
        assert_eq!(
            fx.store.get(&input.proposal_id).unwrap().unwrap().state,
            ExecProposalState::PendingApproval
        );
        assert!(
            fx.store
                .get_permission_mode_receipt(&input.proposal_id)
                .unwrap()
                .is_none()
        );
        let connection = Connection::open(&fx.db_path).unwrap();
        let decisions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM exec_approval_decisions WHERE proposal_id = ?1",
                [&input.proposal_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(decisions, 0);
    }
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let (projects, sessions) = open_pair(temp);
    let project = seed_project(&projects, temp, "exec-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("execs".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "propose a process".into(),
        })
        .unwrap();
    let db_path = temp.path().join("keeper.db");
    let store = SqliteExecProposalRepository::open(&db_path).unwrap();
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

fn reserve_input(fx: &Fixture, call_id: &str, fingerprint_seed: u64) -> ExecProposalReserveInput {
    ExecProposalReserveInput {
        proposal_id: Uuid::new_v4().to_string(),
        reservation_key: format!(
            "ep:{}:{}:{}:{}",
            fx.project_id, fx.conversation_id, fx.run_id, call_id
        ),
        proposal_fingerprint: digest(fingerprint_seed),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: fx.run_id,
        call_id: call_id.into(),
        program: "cargo".into(),
        argv_digest: digest(0xA100 + fingerprint_seed),
        portable_cwd: Some("crates/storage-sqlite".into()),
    }
}

fn grant_digest(seed: u64) -> String {
    digest(0xE000 + seed)
}

fn success_receipt() -> ExecTerminalReceipt {
    ExecTerminalReceipt {
        exit_code: Some(0),
        duration_ms: 42,
        stdout_sha256: digest(0x50),
        stderr_sha256: digest(0x51),
        captured_bytes: 128,
        truncated: false,
        encoding_lossy: false,
        cancelled: false,
        timed_out: false,
    }
}

fn failure_receipt() -> ExecTerminalReceipt {
    ExecTerminalReceipt {
        exit_code: Some(1),
        duration_ms: 17,
        stdout_sha256: digest(0xF0),
        stderr_sha256: digest(0xF1),
        captured_bytes: 64,
        truncated: true,
        encoding_lossy: true,
        cancelled: false,
        timed_out: false,
    }
}

fn cancelled_receipt() -> ExecTerminalReceipt {
    ExecTerminalReceipt {
        exit_code: None,
        duration_ms: 9,
        stdout_sha256: digest(0xC0),
        stderr_sha256: digest(0xC1),
        captured_bytes: 16,
        truncated: false,
        encoding_lossy: false,
        cancelled: true,
        timed_out: false,
    }
}

fn approve(fx: &Fixture, input: &ExecProposalReserveInput, grant_seed: u64) {
    fx.store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant_digest(grant_seed),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
}

fn claim(fx: &Fixture, proposal_id: &str, owner: &str, lease_ms: u64) {
    fx.store
        .claim_run(proposal_id, owner, lease_ms, Utc::now())
        .unwrap();
}

fn stored_rule_match(
    fx: &Fixture,
    scope: PermissionRuleScope,
    action: PermissionRuleAction,
    pattern: &str,
    subject: &str,
) -> PermissionRuleMatchSnapshot {
    let rules = SqlitePermissionRuleRepository::open(&fx.db_path).unwrap();
    let rule = rules
        .add(AddPermissionRule {
            scope,
            project_id: (scope == PermissionRuleScope::Project).then(|| fx.project_id.clone()),
            prefix: PermissionRulePrefix::Process,
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
fn project_rule_deny_persists_reject_by_rule_with_complete_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "rule-deny", 0xD3);
    fx.store.reserve(&input).unwrap();
    let matched = stored_rule_match(
        &fx,
        PermissionRuleScope::Project,
        PermissionRuleAction::Deny,
        "cargo publish*",
        "cargo publish --dry-run",
    );
    let decided_at = Utc::now();

    let outcome = fx
        .store
        .decide_by_rule(
            &input.proposal_id,
            &fx.conversation_id,
            &matched,
            &grant_digest(0xD3),
            &input.proposal_fingerprint,
            decided_at,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        ExecDecisionOutcome::Accepted(ref row) if row.state == ExecProposalState::Rejected
    ));

    let receipt = fx
        .store
        .get_rule_receipt(&input.proposal_id)
        .unwrap()
        .expect("durable process rule receipt");
    assert_eq!(
        receipt.decision,
        PermissionRuleReceiptDecision::RejectByRule
    );
    assert_eq!(receipt.actor, PermissionRuleReceiptActor::RuleEngine);
    assert_eq!(receipt.rule_id, matched.rule.rule_id);
    assert_eq!(receipt.rule_scope, PermissionRuleScope::Project);
    assert_eq!(
        receipt.rule_project_id.as_deref(),
        Some(fx.project_id.as_str())
    );
    assert_eq!(receipt.rule_prefix, PermissionRulePrefix::Process);
    assert_eq!(receipt.rule_pattern, "cargo publish*");
    assert_eq!(receipt.rule_action, PermissionRuleAction::Deny);
    assert_eq!(receipt.rule_content_digest, matched.rule.content_digest);
    assert_eq!(receipt.match_subject, "cargo publish --dry-run");
    assert_eq!(receipt.decided_at, decided_at);

    let connection = Connection::open(&fx.db_path).unwrap();
    let stored_decision: String = connection
        .query_row(
            "SELECT decision FROM exec_permission_rule_receipts WHERE proposal_id = ?1",
            [&receipt.proposal_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_decision, "reject_by_rule");
}

#[test]
fn conjunctive_shell_allow_persists_an_ordered_complete_snapshot_for_every_segment() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = reserve_input(&fx, "rule-conjunction", 0xA11);
    input.program = "shell".into();
    fx.store.reserve(&input).unwrap();
    let cargo = stored_rule_match(
        &fx,
        PermissionRuleScope::Global,
        PermissionRuleAction::Allow,
        "cargo test",
        "cargo test",
    );
    let rustc = stored_rule_match(
        &fx,
        PermissionRuleScope::Global,
        PermissionRuleAction::Allow,
        "rustc --version",
        "rustc --version",
    );
    let decided_at = Utc::now();

    let outcome = fx
        .store
        .decide_by_rules(
            &input.proposal_id,
            &fx.conversation_id,
            &[cargo.clone(), rustc.clone()],
            &grant_digest(0xA11),
            &input.proposal_fingerprint,
            decided_at,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        ExecDecisionOutcome::Accepted(ref row) if row.state == ExecProposalState::Approved
    ));

    let receipts = fx.store.get_rule_receipts(&input.proposal_id).unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].segment_index, 0);
    assert_eq!(receipts[0].receipt.rule_id, cargo.rule.rule_id);
    assert_eq!(receipts[0].receipt.rule_pattern, "cargo test");
    assert_eq!(
        receipts[0].receipt.rule_content_digest,
        cargo.rule.content_digest
    );
    assert_eq!(receipts[0].receipt.match_subject, "cargo test");
    assert_eq!(receipts[0].receipt.decided_at, decided_at);
    assert_eq!(receipts[1].segment_index, 1);
    assert_eq!(receipts[1].receipt.rule_id, rustc.rule.rule_id);
    assert_eq!(receipts[1].receipt.rule_pattern, "rustc --version");
    assert_eq!(
        receipts[1].receipt.rule_content_digest,
        rustc.rule.content_digest
    );
    assert_eq!(receipts[1].receipt.match_subject, "rustc --version");

    let representative = fx
        .store
        .get_rule_receipt(&input.proposal_id)
        .unwrap()
        .expect("backward-compatible representative receipt");
    assert_eq!(representative.rule_id, cargo.rule.rule_id);
    assert_eq!(representative.match_subject, "cargo test");

    let rules = SqlitePermissionRuleRepository::open(&fx.db_path).unwrap();
    assert!(rules.delete(&cargo.rule.rule_id).unwrap());
    assert!(rules.delete(&rustc.rule.rule_id).unwrap());
    let retained = fx.store.get_rule_receipts(&input.proposal_id).unwrap();
    assert_eq!(
        retained, receipts,
        "rule deletion must not erase audit history"
    );

    let connection = Connection::open(&fx.db_path).unwrap();
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM exec_permission_rule_segment_receipts WHERE proposal_id = ?1",
            [&input.proposal_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(row_count, 2);
}

#[test]
fn mixed_rule_actions_cannot_create_a_partial_conjunctive_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = reserve_input(&fx, "rule-mixed-actions", 0xA12);
    input.program = "shell".into();
    fx.store.reserve(&input).unwrap();
    let allow = stored_rule_match(
        &fx,
        PermissionRuleScope::Global,
        PermissionRuleAction::Allow,
        "cargo test",
        "cargo test",
    );
    let deny = stored_rule_match(
        &fx,
        PermissionRuleScope::Project,
        PermissionRuleAction::Deny,
        "rm -rf",
        "rm -rf x",
    );

    assert_eq!(
        fx.store.decide_by_rules(
            &input.proposal_id,
            &fx.conversation_id,
            &[allow, deny],
            &grant_digest(0xA12),
            &input.proposal_fingerprint,
            Utc::now(),
        ),
        Err(keeper_storage_sqlite::ExecProposalStoreError::InvalidRuleReceipt)
    );
    assert!(
        fx.store
            .get_rule_receipts(&input.proposal_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fx.store.get(&input.proposal_id).unwrap().unwrap().state,
        ExecProposalState::PendingApproval
    );
}

#[test]
fn denied_shell_segment_receipt_retains_its_original_command_index() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = reserve_input(&fx, "rule-deny-index", 0xA13);
    input.program = "shell".into();
    fx.store.reserve(&input).unwrap();
    let deny = stored_rule_match(
        &fx,
        PermissionRuleScope::Project,
        PermissionRuleAction::Deny,
        "rm -rf",
        "rm -rf x",
    );

    let outcome = fx
        .store
        .decide_by_rule_segments(
            &input.proposal_id,
            &fx.conversation_id,
            &[PermissionRuleSegmentMatchSnapshot {
                segment_index: 2,
                matched: deny.clone(),
            }],
            &grant_digest(0xA13),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        outcome,
        ExecDecisionOutcome::Accepted(ref row) if row.state == ExecProposalState::Rejected
    ));
    let receipts = fx.store.get_rule_receipts(&input.proposal_id).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].segment_index, 2);
    assert_eq!(receipts[0].receipt.rule_id, deny.rule.rule_id);
    assert_eq!(receipts[0].receipt.match_subject, "rm -rf x");
}

#[test]
fn migration_0007_upgrades_a_0006_database_without_rewriting_history() {
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
        connection
            .execute_batch(include_str!("../migrations/0005_write_proposals.sql"))
            .unwrap();
        // 0006 is conditional rebuild; presence of write tables is enough base.
        assert!(!table_exists(&connection, "exec_proposals"));
        assert!(!table_exists(&connection, "exec_proposal_transitions"));
        assert!(!table_exists(&connection, "exec_approval_decisions"));
    }

    SqliteExecProposalRepository::open(&db).unwrap();
    let connection = Connection::open(&db).unwrap();
    assert!(table_exists(&connection, "projects"));
    assert!(table_exists(&connection, "runs"));
    assert!(table_exists(&connection, "write_proposals"));
    assert!(table_exists(&connection, "exec_proposals"));
    assert!(table_exists(&connection, "exec_proposal_transitions"));
    assert!(table_exists(&connection, "exec_approval_decisions"));

    // Partial unique index for one non-terminal per run must exist.
    let index_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_exec_proposals_one_active_per_run'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(index_sql.contains("pending_approval"));
    assert!(index_sql.contains("approved"));
    assert!(index_sql.contains("running"));
}

#[test]
fn storage_owns_exact_ten_minute_ttl_and_caller_cannot_override() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-ttl", 1);
    let now = Utc::now();

    let reserved = fx.store.reserve_at(&input, now).unwrap();
    let ExecReservationOutcome::Reserved(row) = reserved else {
        panic!("expected Reserved");
    };
    assert_eq!(row.created_at, now);
    assert_eq!(
        row.expires_at,
        now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS)
    );
    assert_eq!(EXEC_PROPOSAL_TTL_SECS, 600);

    fx.store
        .transition(&input.proposal_id, ExecProposalState::Cancelled, Utc::now())
        .unwrap();
    let input2 = reserve_input(&fx, "call-ttl-2", 2);
    let ExecReservationOutcome::Reserved(row2) = fx.store.reserve(&input2).unwrap() else {
        panic!("expected Reserved");
    };
    assert_eq!(
        row2.expires_at - row2.created_at,
        Duration::seconds(EXEC_PROPOSAL_TTL_SECS)
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
        ExecReservationOutcome::Reserved(ref row)
            if row.state == ExecProposalState::PendingApproval
                && row.proposal_fingerprint == first.proposal_fingerprint
                && row.program == "cargo"
                && row.argv_digest == first.argv_digest
                && row.portable_cwd.as_deref() == Some("crates/storage-sqlite")
    ));
    assert!(matches!(
        replay,
        ExecReservationOutcome::Replay(ref row)
            if row.proposal_fingerprint == first.proposal_fingerprint
                && row.proposal_id == first.proposal_id
    ));
    assert!(matches!(
        conflict,
        ExecReservationOutcome::Conflict(ref row)
            if row.call_id == "call-1"
                && row.proposal_fingerprint == first.proposal_fingerprint
    ));
}

#[test]
fn same_identity_different_fingerprint_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-fp", 21);
    fx.store.reserve(&first).unwrap();

    let mut second = first.clone();
    second.proposal_fingerprint = digest(99);
    // Keep proposal_id and reservation_key identical for pure fingerprint conflict.
    let outcome = fx.store.reserve(&second).unwrap();
    assert!(matches!(
        outcome,
        ExecReservationOutcome::Conflict(ref row)
            if row.proposal_fingerprint == first.proposal_fingerprint
    ));
}

#[test]
fn alias_collision_on_proposal_id_or_reservation_key_is_opaque_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-alias-a", 31);
    fx.store.reserve(&first).unwrap();
    fx.store
        .transition(&first.proposal_id, ExecProposalState::Cancelled, Utc::now())
        .unwrap();

    let mut id_clash = reserve_input(&fx, "call-alias-b", 32);
    id_clash.proposal_id = first.proposal_id.clone();
    let conflict_id = fx.store.reserve(&id_clash).unwrap();
    assert!(matches!(
        conflict_id,
        ExecReservationOutcome::Conflict(ref row) if row.proposal_id == first.proposal_id
    ));

    let mut key_clash = reserve_input(&fx, "call-alias-c", 33);
    key_clash.reservation_key = first.reservation_key.clone();
    let conflict_key = fx.store.reserve(&key_clash).unwrap();
    assert!(matches!(
        conflict_key,
        ExecReservationOutcome::Conflict(ref row)
            if row.reservation_key == first.reservation_key
    ));
}

#[test]
fn reserve_busy_when_another_non_terminal_proposal_exists_on_same_run() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-a", 41);
    let second = reserve_input(&fx, "call-b", 42);

    let reserved = fx.store.reserve(&first).unwrap();
    assert!(matches!(reserved, ExecReservationOutcome::Reserved(_)));

    let busy = fx.store.reserve(&second).unwrap();
    assert!(matches!(
        busy,
        ExecReservationOutcome::Busy(ref row)
            if row.call_id == "call-a" && row.proposal_id == first.proposal_id
    ));
}

#[test]
fn reserve_allows_new_call_after_prior_terminal_on_same_run() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-old", 51);
    fx.store.reserve(&first).unwrap();
    fx.store
        .transition(&first.proposal_id, ExecProposalState::Cancelled, Utc::now())
        .unwrap();

    let second = reserve_input(&fx, "call-new", 52);
    let outcome = fx.store.reserve(&second).unwrap();
    assert!(matches!(
        outcome,
        ExecReservationOutcome::Reserved(ref row)
            if row.call_id == "call-new"
                && row.state == ExecProposalState::PendingApproval
    ));
}

#[test]
fn reserve_rejects_missing_project_conversation_run_context() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let mut input = reserve_input(&fx, "call-x", 61);
    input.project_id = Uuid::new_v4().to_string();

    let err = fx.store.reserve(&input).unwrap_err();
    assert!(matches!(
        err,
        keeper_storage_sqlite::ExecProposalStoreError::MissingContext
    ));
}

#[test]
fn invalid_digests_program_and_cwd_reject_before_sql() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    let mut upper = reserve_input(&fx, "call-up", 71);
    upper.argv_digest = format!("sha256:{}", "A".repeat(64));
    assert!(matches!(
        fx.store.reserve(&upper).unwrap_err(),
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));

    let mut bad_program = reserve_input(&fx, "call-prog", 72);
    bad_program.program = "Cargo".into(); // uppercase denied
    assert!(matches!(
        fx.store.reserve(&bad_program).unwrap_err(),
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));

    let mut shell = reserve_input(&fx, "call-shell", 73);
    shell.program = "cmd.exe".into();
    assert!(matches!(
        fx.store.reserve(&shell).unwrap_err(),
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));

    let mut bad_cwd = reserve_input(&fx, "call-cwd", 74);
    bad_cwd.portable_cwd = Some("C:\\absolute\\path".into());
    assert!(matches!(
        fx.store.reserve(&bad_cwd).unwrap_err(),
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));

    let mut bad_id = reserve_input(&fx, "call-bi", 75);
    bad_id.proposal_id = "not-a-uuid".into();
    assert!(matches!(
        fx.store.reserve(&bad_id).unwrap_err(),
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));
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
            ExecDecision::ApproveOnce,
            &grant_digest(81),
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        keeper_storage_sqlite::ExecProposalStoreError::OwnershipMismatch
    ));
}

#[test]
fn expiry_boundary_is_true_at_exact_expires_at() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-exp", 91);
    let now = Utc::now();
    fx.store.reserve_at(&input, now).unwrap();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);

    let before = fx
        .store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant_digest(91),
            &input.proposal_fingerprint,
            expires_at - Duration::milliseconds(1),
        )
        .unwrap();
    assert!(matches!(before, ExecDecisionOutcome::Accepted(_)));

    fx.store
        .transition(
            &input.proposal_id,
            ExecProposalState::Interrupted,
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
            ExecDecision::ApproveOnce,
            &grant_digest(92),
            &input2.proposal_fingerprint,
            expires_at,
        )
        .unwrap();

    assert!(matches!(
        outcome_at,
        ExecDecisionOutcome::Expired(ref row)
            if row.state == ExecProposalState::Expired
                && row.proposal_id == input2.proposal_id
    ));
}

#[test]
fn decision_accepts_approve_and_reject_once() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let approve_input = reserve_input(&fx, "call-dec-a", 101);
    fx.store.reserve(&approve_input).unwrap();

    let accepted = fx
        .store
        .decide(
            &approve_input.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant_digest(101),
            &approve_input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        accepted,
        ExecDecisionOutcome::Accepted(ref row)
            if row.state == ExecProposalState::Approved
    ));

    // Identical re-decision is accepted (idempotent single decision record).
    let replay = fx
        .store
        .decide(
            &approve_input.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant_digest(101),
            &approve_input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(replay, ExecDecisionOutcome::Accepted(_)));

    // Different decision after approve is conflict.
    let conflict = fx
        .store
        .decide(
            &approve_input.proposal_id,
            &fx.conversation_id,
            ExecDecision::Reject,
            &grant_digest(102),
            &approve_input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(conflict, ExecDecisionOutcome::Conflict(_)));

    // Reject path on a separate run.
    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut reject_input = reserve_input(&fx, "call-dec-r", 103);
    reject_input.run_id = run2;
    reject_input.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-dec-r"
    );
    fx.store.reserve(&reject_input).unwrap();
    let rejected = fx
        .store
        .decide(
            &reject_input.proposal_id,
            &fx.conversation_id,
            ExecDecision::Reject,
            &grant_digest(103),
            &reject_input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(
        rejected,
        ExecDecisionOutcome::Terminal(ref row) | ExecDecisionOutcome::Accepted(ref row)
            if row.state == ExecProposalState::Rejected
    ));
}

#[test]
fn grant_digest_is_single_use_across_proposals() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let first = reserve_input(&fx, "call-g1", 111);
    fx.store.reserve(&first).unwrap();
    approve(&fx, &first, 111);

    fx.store
        .transition(
            &first.proposal_id,
            ExecProposalState::Interrupted,
            Utc::now(),
        )
        .unwrap();

    let second = reserve_input(&fx, "call-g2", 112);
    fx.store.reserve(&second).unwrap();
    let reused = fx
        .store
        .decide(
            &second.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant_digest(111), // same grant
            &second.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    assert!(matches!(reused, ExecDecisionOutcome::Conflict(_)));
}

#[test]
fn claim_run_is_one_shot_approved_to_running_and_busy_while_running() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-claim", 121);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 121);

    let now = Utc::now();
    // First Approved -> Running is the only Claimed outcome.
    let claimed = fx
        .store
        .claim_run(&input.proposal_id, "owner-a", 5_000, now)
        .unwrap();
    assert!(matches!(
        claimed,
        ExecRunClaimOutcome::Claimed(ref row)
            if row.state == ExecProposalState::Running
    ));

    // Same owner while still running is Busy — not a renewable lease reclaim.
    let same_owner = fx
        .store
        .claim_run(&input.proposal_id, "owner-a", 5_000, now)
        .unwrap();
    assert!(matches!(
        same_owner,
        ExecRunClaimOutcome::Busy(ref row) if row.state == ExecProposalState::Running
    ));

    // Different owner while still running is Busy.
    let other_owner = fx
        .store
        .claim_run(&input.proposal_id, "owner-b", 5_000, now)
        .unwrap();
    assert!(matches!(
        other_owner,
        ExecRunClaimOutcome::Busy(ref row) if row.state == ExecProposalState::Running
    ));

    // Expired run lease never reassigns spawn authority; still Busy.
    let after_lease = now + Duration::milliseconds(6_000);
    let expired_lease = fx
        .store
        .claim_run(&input.proposal_id, "owner-c", 5_000, after_lease)
        .unwrap();
    assert!(matches!(
        expired_lease,
        ExecRunClaimOutcome::Busy(ref row) if row.state == ExecProposalState::Running
    ));
    // Owner/lease must be unchanged by the failed claim attempt.
    let still_running = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still_running.state, ExecProposalState::Running);
}

#[test]
fn claim_run_rejects_pending_and_expired_approved() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let pending = reserve_input(&fx, "call-pend", 131);
    fx.store.reserve(&pending).unwrap();
    let conflict = fx
        .store
        .claim_run(&pending.proposal_id, "owner-p", 5_000, Utc::now())
        .unwrap();
    assert!(matches!(conflict, ExecRunClaimOutcome::Conflict(_)));

    fx.store
        .transition(
            &pending.proposal_id,
            ExecProposalState::Cancelled,
            Utc::now(),
        )
        .unwrap();

    let input = reserve_input(&fx, "call-exp-claim", 132);
    let now = Utc::now();
    fx.store.reserve_at(&input, now).unwrap();
    approve(&fx, &input, 132);
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    let expired = fx
        .store
        .claim_run(&input.proposal_id, "owner-e", 5_000, expires_at)
        .unwrap();
    assert!(matches!(
        expired,
        ExecRunClaimOutcome::Expired(ref row)
            if row.state == ExecProposalState::Expired
    ));
}

#[test]
fn complete_run_stores_terminal_receipt_and_replays_identical() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-term", 141);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 141);
    claim(&fx, &input.proposal_id, "owner-t", 5_000);

    let receipt = success_receipt();
    let terminal = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-t",
            ExecProposalState::Succeeded,
            &receipt,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(terminal.state, ExecProposalState::Succeeded);
    assert!(terminal.result_json.is_some());
    assert_eq!(terminal.exit_code, Some(0));
    assert_eq!(terminal.duration_ms, Some(42));
    assert_eq!(terminal.captured_bytes, Some(128));
    assert_eq!(terminal.truncated, Some(false));
    assert_eq!(terminal.encoding_lossy, Some(false));
    assert_eq!(terminal.timed_out, Some(false));
    assert_eq!(terminal.cancelled, Some(false));
    assert!(terminal.started_at.is_some());
    assert!(terminal.completed_at.is_some());

    // Identical complete is a durable replay.
    let replay = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-t",
            ExecProposalState::Succeeded,
            &receipt,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(replay.result_json, terminal.result_json);
    assert_eq!(replay.state, ExecProposalState::Succeeded);

    // Conflicting receipt fails closed.
    let err = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-t",
            ExecProposalState::Failed,
            &failure_receipt(),
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));
}

#[test]
fn complete_run_failure_receipt_path() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-fail", 151);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 151);
    claim(&fx, &input.proposal_id, "owner-f", 5_000);

    let receipt = failure_receipt();
    let terminal = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-f",
            ExecProposalState::Failed,
            &receipt,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(terminal.state, ExecProposalState::Failed);
    assert_eq!(terminal.exit_code, Some(1));
    assert_eq!(terminal.truncated, Some(true));
    assert_eq!(terminal.encoding_lossy, Some(true));
}

#[test]
fn complete_run_cancelled_receipt_persists_replays_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-cancel-term", 152);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 152);
    claim(&fx, &input.proposal_id, "owner-c", 5_000);

    let receipt = cancelled_receipt();
    let terminal = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-c",
            ExecProposalState::Cancelled,
            &receipt,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(terminal.state, ExecProposalState::Cancelled);
    assert!(terminal.result_json.is_some());
    assert_eq!(terminal.cancelled, Some(true));
    assert_eq!(terminal.duration_ms, Some(9));
    assert_eq!(terminal.captured_bytes, Some(16));
    assert_eq!(
        terminal.stdout_digest.as_deref(),
        Some(receipt.stdout_sha256.as_str())
    );
    assert_eq!(
        terminal.stderr_digest.as_deref(),
        Some(receipt.stderr_sha256.as_str())
    );
    assert!(terminal.completed_at.is_some());
    assert!(terminal.started_at.is_some());

    // Identical complete is a durable replay.
    let replay = fx
        .store
        .complete_run(
            &input.proposal_id,
            "owner-c",
            ExecProposalState::Cancelled,
            &receipt,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(replay.result_json, terminal.result_json);
    assert_eq!(replay.state, ExecProposalState::Cancelled);
    assert_eq!(replay.cancelled, Some(true));

    let history = fx.store.history(&input.proposal_id).unwrap();
    assert_eq!(
        history,
        vec![
            ExecProposalState::PendingApproval,
            ExecProposalState::Approved,
            ExecProposalState::Running,
            ExecProposalState::Cancelled,
        ]
    );
    // Transition history for post-spawn cancelled carries the same receipt.
    let connection = Connection::open(&fx.db_path).unwrap();
    let hist_json: Option<String> = connection
        .query_row(
            "SELECT result_json FROM exec_proposal_transitions
             WHERE proposal_id = ?1 AND state = 'cancelled'",
            [&input.proposal_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hist_json.as_deref(), terminal.result_json.as_deref());
}

#[test]
fn complete_run_cancellation_flag_mismatch_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    // Cancelled state with cancelled=false receipt fails closed.
    let input_a = reserve_input(&fx, "call-flag-a", 153);
    fx.store.reserve(&input_a).unwrap();
    approve(&fx, &input_a, 153);
    claim(&fx, &input_a.proposal_id, "owner-a", 5_000);
    let err_a = fx
        .store
        .complete_run(
            &input_a.proposal_id,
            "owner-a",
            ExecProposalState::Cancelled,
            &success_receipt(), // cancelled: false
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err_a,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));
    assert_eq!(
        fx.store.get(&input_a.proposal_id).unwrap().unwrap().state,
        ExecProposalState::Running
    );

    // Succeeded state with cancelled=true receipt fails closed.
    let run_b = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut input_b = reserve_input(&fx, "call-flag-b", 154);
    input_b.run_id = run_b;
    input_b.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run_b, "call-flag-b"
    );
    fx.store.reserve(&input_b).unwrap();
    approve(&fx, &input_b, 154);
    claim(&fx, &input_b.proposal_id, "owner-b", 5_000);
    let err_b = fx
        .store
        .complete_run(
            &input_b.proposal_id,
            "owner-b",
            ExecProposalState::Succeeded,
            &cancelled_receipt(), // cancelled: true
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err_b,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));
    assert_eq!(
        fx.store.get(&input_b.proposal_id).unwrap().unwrap().state,
        ExecProposalState::Running
    );

    // Failed state with cancelled=true also fails closed.
    let err_c = fx
        .store
        .complete_run(
            &input_b.proposal_id,
            "owner-b",
            ExecProposalState::Failed,
            &cancelled_receipt(),
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err_c,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidResult
    ));
}

#[test]
fn generic_transition_rejects_all_mutations_from_running() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-run-guard", 155);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 155);
    claim(&fx, &input.proposal_id, "owner-g", 5_000);

    for next in [
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Approved,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::PendingApproval,
        ExecProposalState::Running,
    ] {
        let err = fx
            .store
            .transition(&input.proposal_id, next, Utc::now())
            .unwrap_err();
        assert!(
            matches!(
                err,
                keeper_storage_sqlite::ExecProposalStoreError::InvalidTransition
            ),
            "Running -> {next:?} must be owner-aware API only, got {err:?}"
        );
    }

    let still = fx.store.get(&input.proposal_id).unwrap().unwrap();
    assert_eq!(still.state, ExecProposalState::Running);

    // Pre-spawn cancel remains receipt-free via generic transition.
    let pre = reserve_input(&fx, "call-pre-cancel", 156);
    // Different run so the active partial unique index allows it.
    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut pre = pre;
    pre.run_id = run2;
    pre.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-pre-cancel"
    );
    fx.store.reserve(&pre).unwrap();
    let cancelled = fx
        .store
        .transition(&pre.proposal_id, ExecProposalState::Cancelled, Utc::now())
        .unwrap();
    assert_eq!(cancelled.state, ExecProposalState::Cancelled);
    assert!(cancelled.result_json.is_none());
    assert!(cancelled.cancelled.is_none());
    assert!(cancelled.completed_at.is_none());
}

#[test]
fn recover_interrupted_marks_all_non_terminal_without_replaying_authority() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);

    let pending = reserve_input(&fx, "call-rec-p", 161);
    fx.store.reserve(&pending).unwrap();

    // Second run for approved.
    let run2 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut approved = reserve_input(&fx, "call-rec-a", 162);
    approved.run_id = run2;
    approved.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run2, "call-rec-a"
    );
    fx.store.reserve(&approved).unwrap();
    approve(&fx, &approved, 162);

    // Third run for running.
    let run3 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut running = reserve_input(&fx, "call-rec-r", 163);
    running.run_id = run3;
    running.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run3, "call-rec-r"
    );
    fx.store.reserve(&running).unwrap();
    approve(&fx, &running, 163);
    claim(&fx, &running.proposal_id, "owner-r", 5_000);

    // Terminal must stay terminal.
    let run4 = second_run(&temp, Uuid::parse_str(&fx.conversation_id).unwrap());
    let mut rejected = reserve_input(&fx, "call-rec-x", 164);
    rejected.run_id = run4;
    rejected.reservation_key = format!(
        "ep:{}:{}:{}:{}",
        fx.project_id, fx.conversation_id, run4, "call-rec-x"
    );
    fx.store.reserve(&rejected).unwrap();
    fx.store
        .decide(
            &rejected.proposal_id,
            &fx.conversation_id,
            ExecDecision::Reject,
            &grant_digest(164),
            &rejected.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();

    let recovered = fx.store.recover_interrupted(Utc::now()).unwrap();
    assert_eq!(recovered.len(), 3);
    assert!(
        recovered
            .iter()
            .all(|r| r.state == ExecProposalState::Interrupted)
    );

    let still_rejected = fx.store.get(&rejected.proposal_id).unwrap().unwrap();
    assert_eq!(still_rejected.state, ExecProposalState::Rejected);

    // Recovery never re-opens terminal or re-runs: second recover is empty.
    let second = fx.store.recover_interrupted(Utc::now()).unwrap();
    assert!(second.is_empty());
}

#[test]
fn list_for_conversation_is_conversation_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-list", 171);
    fx.store.reserve(&input).unwrap();

    let listed = fx.store.list_for_conversation(&fx.conversation_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].proposal_id, input.proposal_id);

    let foreign = fx
        .store
        .list_for_conversation(&Uuid::new_v4().to_string())
        .unwrap();
    assert!(foreign.is_empty());
}

#[test]
fn history_records_lifecycle_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-hist", 181);
    fx.store.reserve(&input).unwrap();
    approve(&fx, &input, 181);
    claim(&fx, &input.proposal_id, "owner-h", 5_000);
    fx.store
        .complete_run(
            &input.proposal_id,
            "owner-h",
            ExecProposalState::Succeeded,
            &success_receipt(),
            Utc::now(),
        )
        .unwrap();

    let history = fx.store.history(&input.proposal_id).unwrap();
    assert_eq!(
        history,
        vec![
            ExecProposalState::PendingApproval,
            ExecProposalState::Approved,
            ExecProposalState::Running,
            ExecProposalState::Succeeded,
        ]
    );
}

#[test]
fn illegal_transition_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-ill", 191);
    fx.store.reserve(&input).unwrap();

    // Pending cannot jump to Running.
    let err = fx
        .store
        .transition(&input.proposal_id, ExecProposalState::Running, Utc::now())
        .unwrap_err();
    assert!(matches!(
        err,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidTransition
    ));

    // Terminal cannot reopen.
    fx.store
        .transition(&input.proposal_id, ExecProposalState::Rejected, Utc::now())
        .unwrap();
    let err2 = fx
        .store
        .transition(
            &input.proposal_id,
            ExecProposalState::PendingApproval,
            Utc::now(),
        )
        .unwrap_err();
    assert!(matches!(
        err2,
        keeper_storage_sqlite::ExecProposalStoreError::InvalidTransition
    ));
}

#[test]
fn argv_native_path_grant_env_and_transcript_never_enter_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let input = reserve_input(&fx, "call-secret", 201);
    // Digests must not embed secrets; secrets live only in host memory.
    assert!(!input.argv_digest.contains(SECRET_ARGV_0));
    assert!(!input.proposal_fingerprint.contains(SECRET_NATIVE_EXE));

    fx.store.reserve(&input).unwrap();
    let grant = grant_digest(201);
    assert!(!grant.contains(SECRET_GRANT_RAW));
    fx.store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant,
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
    claim(&fx, &input.proposal_id, "owner-s", 5_000);
    let receipt = success_receipt();
    // Receipt digests must not contain transcript bytes.
    assert!(!receipt.stdout_sha256.contains(SECRET_STDOUT));
    assert!(!receipt.stderr_sha256.contains(SECRET_STDERR));
    fx.store
        .complete_run(
            &input.proposal_id,
            "owner-s",
            ExecProposalState::Succeeded,
            &receipt,
            Utc::now(),
        )
        .unwrap();

    let stored: StoredExecProposal = fx.store.get(&input.proposal_id).unwrap().unwrap();
    let row_blob = format!("{stored:?}");
    for secret in [
        SECRET_ARGV_0,
        SECRET_NATIVE_EXE,
        SECRET_NATIVE_CWD,
        SECRET_GRANT_RAW,
        SECRET_ENV_VALUE,
        SECRET_STDOUT,
        SECRET_STDERR,
    ] {
        assert!(
            !row_blob.contains(secret),
            "repository DTO leaked secret material: {secret}"
        );
    }

    let history = fx.store.history(&input.proposal_id).unwrap();
    let history_blob = format!("{history:?}");
    assert!(!history_blob.contains(SECRET_ARGV_0));
    assert!(!history_blob.contains(SECRET_STDOUT));

    let bytes = std::fs::read(&fx.db_path).unwrap();
    let haystack = String::from_utf8_lossy(&bytes);
    for secret in [
        SECRET_ARGV_0,
        SECRET_NATIVE_EXE,
        SECRET_NATIVE_CWD,
        SECRET_GRANT_RAW,
        SECRET_ENV_VALUE,
        SECRET_STDOUT,
        SECRET_STDERR,
    ] {
        assert!(
            !haystack.contains(secret),
            "secret leaked into SQLite file bytes: {secret}"
        );
    }

    // Column inventory: no argv/env/grant/transcript/native columns.
    let connection = Connection::open(&fx.db_path).unwrap();
    let mut stmt = connection
        .prepare("PRAGMA table_info(exec_proposals)")
        .unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let forbidden = [
        "argv",
        "executable",
        "native_cwd",
        "native_root",
        "environment",
        "env",
        "stdout",
        "stderr",
        "transcript",
        "approval_grant",
        "raw_grant",
    ];
    for col in &columns {
        let lower = col.to_ascii_lowercase();
        for bad in forbidden {
            assert!(
                lower != bad && !lower.ends_with(&format!("_{bad}")),
                "forbidden column present: {col}"
            );
        }
    }
    // Required metadata columns.
    for required in [
        "program",
        "argv_digest",
        "portable_cwd",
        "proposal_fingerprint",
        "state",
        "result_json",
        "stdout_digest",
        "stderr_digest",
        "captured_bytes",
        "truncated",
        "encoding_lossy",
        "exit_code",
        "duration_ms",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "missing required column: {required}"
        );
    }

    // result_json must be bounded metadata JSON only.
    let mut stmt = connection
        .prepare("SELECT result_json FROM exec_proposals WHERE result_json IS NOT NULL")
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(!rows.is_empty());
    for json in rows {
        assert!(json.len() <= 8192);
        for secret in [
            SECRET_ARGV_0,
            SECRET_NATIVE_EXE,
            SECRET_GRANT_RAW,
            SECRET_ENV_VALUE,
            SECRET_STDOUT,
            SECRET_STDERR,
        ] {
            assert!(!json.contains(secret));
        }
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("argv").is_none());
        assert!(parsed.get("environment").is_none());
        assert!(parsed.get("stdout").is_none());
        assert!(parsed.get("stderr").is_none());
        assert!(parsed.get("approval_grant").is_none());
    }
}
