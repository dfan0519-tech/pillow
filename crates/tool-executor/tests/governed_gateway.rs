use std::fs;

use keeper_agent_protocol::{
    FsReadOutput, RunId as ProtocolRunId, ToolActiveContext, ToolExecutionState, ToolProposal,
    canonical_arguments_digest,
};
use keeper_project::CreateProject;
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteProjectRepository, SqliteToolCallRepository,
    ToolExecutionClaimOutcome,
};
use keeper_tool_executor::{
    GovernedReadToolGateway, GovernedToolError, GovernedToolOutcome, NeverCancelled,
    ProjectCapability,
};
use serde_json::json;

struct Fixture {
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    context: ToolActiveContext,
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "Project".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Gateway".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "read".into(),
        })
        .unwrap();
    Fixture {
        db,
        root,
        context: ToolActiveContext::new(
            project.id.0.to_string(),
            conversation.id.0.to_string(),
            ProtocolRunId(run.id.0),
        )
        .unwrap(),
    }
}

fn proposal(
    context: &ToolActiveContext,
    call_id: &str,
    max_bytes: u64,
) -> keeper_agent_protocol::ValidatedToolProposal {
    let arguments = json!({
        "path": "note.txt",
        "offset_bytes": 0,
        "max_bytes": max_bytes
    });
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

#[test]
fn terminal_receipt_is_persisted_before_return_and_replayed_without_reexecution() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    fs::write(fixture.root.join("note.txt"), "first").unwrap();
    let capability = ProjectCapability::new(fixture.context.project_id(), &fixture.root).unwrap();
    let proposal = proposal(&fixture.context, "call-gateway-1", 64);
    let gateway = GovernedReadToolGateway::open(&fixture.db).unwrap();

    let first = gateway
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let GovernedToolOutcome::Executed(first) = first else {
        panic!("first call must execute");
    };
    let first_output: FsReadOutput =
        serde_json::from_value(first.output().unwrap().clone()).unwrap();
    assert_eq!(first_output.content.as_deref(), Some("first"));

    fs::write(fixture.root.join("note.txt"), "second").unwrap();
    let reopened = GovernedReadToolGateway::open(&fixture.db).unwrap();
    let replay = reopened
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let GovernedToolOutcome::Replayed(replay) = replay else {
        panic!("terminal call must replay");
    };
    let replay_output: FsReadOutput =
        serde_json::from_value(replay.output().unwrap().clone()).unwrap();
    assert_eq!(replay_output.content.as_deref(), Some("first"));

    let store = SqliteToolCallRepository::open(&fixture.db).unwrap();
    assert_eq!(
        store.history(&proposal).unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
            ToolExecutionState::Succeeded,
        ]
    );
}

#[test]
fn an_interrupted_executing_read_can_resume_to_one_terminal_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    fs::write(fixture.root.join("note.txt"), "resumed").unwrap();
    let capability = ProjectCapability::new(fixture.context.project_id(), &fixture.root).unwrap();
    let proposal = proposal(&fixture.context, "call-gateway-2", 64);
    let store = SqliteToolCallRepository::open(&fixture.db).unwrap();
    store.reserve(&proposal).unwrap();
    store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    store
        .transition(&proposal, ToolExecutionState::Executing, None)
        .unwrap();

    let gateway = GovernedReadToolGateway::open(&fixture.db).unwrap();
    let result = gateway
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    assert!(matches!(result, GovernedToolOutcome::Executed(_)));
    assert_eq!(
        store.history(&proposal).unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
            ToolExecutionState::Succeeded,
        ]
    );
}

#[test]
fn call_identity_cannot_be_reused_for_different_arguments() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    fs::write(fixture.root.join("note.txt"), "content").unwrap();
    let capability = ProjectCapability::new(fixture.context.project_id(), &fixture.root).unwrap();
    let first = proposal(&fixture.context, "call-gateway-3", 8);
    let conflict = proposal(&fixture.context, "call-gateway-3", 16);
    let gateway = GovernedReadToolGateway::open(&fixture.db).unwrap();
    gateway
        .execute(&first, &capability, &NeverCancelled)
        .unwrap();

    assert!(matches!(
        gateway.execute(&conflict, &capability, &NeverCancelled),
        Err(GovernedToolError::ReservationConflict)
    ));
}

#[test]
fn an_active_execution_lease_prevents_duplicate_work() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    fs::write(fixture.root.join("note.txt"), "busy").unwrap();
    let capability = ProjectCapability::new(fixture.context.project_id(), &fixture.root).unwrap();
    let proposal = proposal(&fixture.context, "call-gateway-busy", 64);
    let store = SqliteToolCallRepository::open(&fixture.db).unwrap();
    store.reserve(&proposal).unwrap();
    store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    assert!(matches!(
        store
            .claim_execution(&proposal, "another-controller", 60_000)
            .unwrap(),
        ToolExecutionClaimOutcome::Claimed(_)
    ));

    let gateway = GovernedReadToolGateway::open(&fixture.db).unwrap();
    assert!(matches!(
        gateway
            .execute(&proposal, &capability, &NeverCancelled)
            .unwrap(),
        GovernedToolOutcome::InProgress
    ));
    assert_eq!(
        store.history(&proposal).unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
        ]
    );
}
