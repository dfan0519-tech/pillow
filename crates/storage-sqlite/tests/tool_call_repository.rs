mod common;

use common::{open_pair, seed_project, table_exists};
use keeper_agent_protocol::{
    RunId as ProtocolRunId, ToolActiveContext, ToolExecutionState, ToolProposal, ToolPublicError,
    ToolPublicErrorCode, ToolResult, canonical_arguments_digest,
};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    SqliteToolCallRepository, ToolCallReservationOutcome, ToolCallStoreError,
    ToolExecutionClaimOutcome,
};
use rusqlite::Connection;
use serde_json::{Value, json};

struct Fixture {
    store: SqliteToolCallRepository,
    context: ToolActiveContext,
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let (projects, sessions) = open_pair(temp);
    let project = seed_project(&projects, temp, "tool-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("tools".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "inspect the project".into(),
        })
        .unwrap();
    let context = ToolActiveContext::new(
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        ProtocolRunId(run.id.0),
    )
    .unwrap();
    let store = SqliteToolCallRepository::open(temp.path().join("keeper.db")).unwrap();
    Fixture { store, context }
}

fn proposal(
    context: &ToolActiveContext,
    call_id: &str,
    path: &str,
) -> keeper_agent_protocol::ValidatedToolProposal {
    let arguments = json!({"path": path, "offset_bytes": 0, "max_bytes": 128});
    ToolProposal {
        project_id: context.project_id().into(),
        conversation_id: context.conversation_id().into(),
        run_id: context.run_id(),
        call_id: call_id.into(),
        tool_id: "fs.read".into(),
        tool_version: 1,
        arguments_digest: canonical_arguments_digest(&arguments).unwrap(),
        arguments,
    }
    .validate_for_context(context)
    .unwrap()
}

fn failed_result(
    proposal: &keeper_agent_protocol::ValidatedToolProposal,
) -> keeper_agent_protocol::ValidatedToolResult {
    ToolResult {
        project_id: proposal.project_id().into(),
        conversation_id: proposal.conversation_id().into(),
        run_id: proposal.run_id(),
        call_id: proposal.call_id().into(),
        tool_id: proposal.tool_id().into(),
        tool_version: proposal.tool_version(),
        proposal_fingerprint: proposal.fingerprint().clone(),
        arguments_digest: proposal.normalized_arguments_digest().clone(),
        state: ToolExecutionState::Failed,
        output: None,
        error: Some(ToolPublicError::canonical(
            ToolPublicErrorCode::InternalError,
        )),
    }
    .validate_for_proposal(proposal.context(), proposal)
    .unwrap()
}

#[test]
fn migration_0003_upgrades_a_0002_database_without_rewriting_history() {
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
        assert!(!table_exists(&connection, "tool_calls"));
    }

    SqliteToolCallRepository::open(&db).unwrap();
    let connection = Connection::open(&db).unwrap();
    assert!(table_exists(&connection, "projects"));
    assert!(table_exists(&connection, "runs"));
    assert!(table_exists(&connection, "tool_calls"));
}

#[test]
fn reservation_is_atomic_idempotent_and_detects_semantic_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let first = proposal(&fixture.context, "call-1", "README.md");
    let conflicting = proposal(&fixture.context, "call-1", "Cargo.toml");

    let reserved = fixture.store.reserve(&first).unwrap();
    let replay = fixture.store.reserve(&first).unwrap();
    let conflict = fixture.store.reserve(&conflicting).unwrap();

    assert!(matches!(
        reserved,
        ToolCallReservationOutcome::Reserved(ref row)
            if row.state == ToolExecutionState::Proposed
    ));
    assert!(matches!(
        replay,
        ToolCallReservationOutcome::Replay(ref row)
            if row.proposal_fingerprint == first.fingerprint().as_str()
    ));
    assert!(matches!(
        conflict,
        ToolCallReservationOutcome::Conflict(ref row)
            if row.call_id == "call-1"
    ));
}

#[test]
fn lifecycle_and_terminal_receipt_are_durable_before_replay() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let proposal = proposal(&fixture.context, "call-2", "README.md");
    fixture.store.reserve(&proposal).unwrap();

    fixture
        .store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    fixture
        .store
        .transition(&proposal, ToolExecutionState::Executing, None)
        .unwrap();
    let result = failed_result(&proposal);
    let terminal = fixture
        .store
        .transition(&proposal, ToolExecutionState::Failed, Some(&result))
        .unwrap();

    assert_eq!(terminal.state, ToolExecutionState::Failed);
    assert_eq!(
        fixture.store.history(&proposal).unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
            ToolExecutionState::Failed,
        ]
    );
    let receipt = terminal.result_json.as_deref().unwrap();
    let parsed = ToolResult::parse_str(receipt).unwrap();
    parsed
        .validate_for_proposal(&fixture.context, &proposal)
        .unwrap();

    let replay = fixture.store.reserve(&proposal).unwrap();
    assert!(matches!(
        replay,
        ToolCallReservationOutcome::Replay(row)
            if row.state == ToolExecutionState::Failed
                && row.result_json.as_deref() == Some(receipt)
    ));
}

#[test]
fn terminal_receipts_are_listed_in_stable_run_created_reservation_order() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let first = proposal(&fixture.context, "call-z", "README.md");
    let second = proposal(&fixture.context, "call-a", "Cargo.toml");
    let non_terminal = proposal(&fixture.context, "call-pending", "src/lib.rs");

    for proposal in [&first, &second] {
        fixture.store.reserve(proposal).unwrap();
        fixture
            .store
            .transition(proposal, ToolExecutionState::Validated, None)
            .unwrap();
        fixture
            .store
            .transition(proposal, ToolExecutionState::Executing, None)
            .unwrap();
        let result = failed_result(proposal);
        fixture
            .store
            .transition(proposal, ToolExecutionState::Failed, Some(&result))
            .unwrap();
    }
    fixture.store.reserve(&non_terminal).unwrap();

    let receipts = fixture.store.list_terminal_calls().unwrap();

    assert_eq!(receipts.len(), 2);
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.state == ToolExecutionState::Failed)
    );
    assert!(receipts.windows(2).all(|rows| {
        (
            rows[0].run_id.as_str(),
            rows[0].created_at,
            rows[0].reservation_key.as_str(),
        ) <= (
            rows[1].run_id.as_str(),
            rows[1].created_at,
            rows[1].reservation_key.as_str(),
        )
    }));
}

#[test]
fn illegal_transition_leaves_the_last_durable_state_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let proposal = proposal(&fixture.context, "call-3", "README.md");
    fixture.store.reserve(&proposal).unwrap();
    let result = failed_result(&proposal);

    let error = fixture
        .store
        .transition(&proposal, ToolExecutionState::Failed, Some(&result))
        .unwrap_err();
    assert_eq!(error, ToolCallStoreError::InvalidTransition);

    let replay = fixture.store.reserve(&proposal).unwrap();
    assert!(matches!(
        replay,
        ToolCallReservationOutcome::Replay(row)
            if row.state == ToolExecutionState::Proposed && row.result_json.is_none()
    ));
}

#[test]
fn reservation_rejects_a_context_missing_from_the_database() {
    let temp = tempfile::tempdir().unwrap();
    let store = SqliteToolCallRepository::open(temp.path().join("keeper.db")).unwrap();
    let context = ToolActiveContext::new(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        ProtocolRunId(uuid::Uuid::new_v4()),
    )
    .unwrap();
    let proposal = proposal(&context, "call-missing", "README.md");

    assert_eq!(
        store.reserve(&proposal).unwrap_err(),
        ToolCallStoreError::MissingContext
    );
}

#[test]
fn one_execution_lease_owner_wins_and_only_that_owner_can_complete() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let proposal = proposal(&fixture.context, "call-lease", "README.md");
    fixture.store.reserve(&proposal).unwrap();
    fixture
        .store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();

    let claimed = fixture
        .store
        .claim_execution(&proposal, "owner-a", 60_000)
        .unwrap();
    let busy = fixture
        .store
        .claim_execution(&proposal, "owner-b", 60_000)
        .unwrap();
    assert!(matches!(
        claimed,
        ToolExecutionClaimOutcome::Claimed(ref row)
            if row.execution_owner.as_deref() == Some("owner-a")
    ));
    assert!(matches!(
        busy,
        ToolExecutionClaimOutcome::Busy(ref row)
            if row.execution_owner.as_deref() == Some("owner-a")
    ));

    let result = failed_result(&proposal);
    assert_eq!(
        fixture
            .store
            .complete_execution(&proposal, "owner-b", &result)
            .unwrap_err(),
        ToolCallStoreError::ReservationConflict
    );
    let terminal = fixture
        .store
        .complete_execution(&proposal, "owner-a", &result)
        .unwrap();
    assert_eq!(terminal.state, ToolExecutionState::Failed);
    assert!(terminal.execution_owner.is_none());
    assert!(terminal.lease_expires_at.is_none());
}

#[test]
fn an_expired_execution_lease_can_be_recovered() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let proposal = proposal(&fixture.context, "call-expired", "README.md");
    fixture.store.reserve(&proposal).unwrap();
    fixture
        .store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    fixture
        .store
        .claim_execution(&proposal, "owner-a", 0)
        .unwrap();

    let recovered = fixture
        .store
        .claim_execution(&proposal, "owner-b", 60_000)
        .unwrap();
    assert!(matches!(
        recovered,
        ToolExecutionClaimOutcome::Claimed(ref row)
            if row.execution_owner.as_deref() == Some("owner-b")
    ));
}

#[allow(dead_code)]
fn _value_type_anchor(_: Value) {}
