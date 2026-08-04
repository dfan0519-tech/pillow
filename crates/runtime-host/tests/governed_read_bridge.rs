use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId as ProtocolRunId, RuntimeError, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession, ToolActiveContext, ToolExecutionState, ToolProposal,
    canonical_arguments_digest,
};
use keeper_project::CreateProject;
use keeper_runtime_host::{
    GovernedReadBridgeError, GovernedReadDelivery, HostConfig, ProviderToolProposal, RuntimeHost,
    execute_governed_read,
};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteProjectRepository, SqliteToolCallRepository,
};
use serde_json::json;

struct IdleAdapter;

impl AgentRuntimeAdapter for IdleAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "idle".into(),
            version: "1".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn disarm_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, _: SendTurn, _: &mut dyn EventSink) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn cancel_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    context: ToolActiveContext,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
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
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Governed read".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "read note".into(),
        })
        .unwrap();
    let context = ToolActiveContext::new(
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        ProtocolRunId(run.id.0),
    )
    .unwrap();
    Fixture {
        _temp: temp,
        db,
        root,
        context,
    }
}

fn provider_read(call_id: &str, max_bytes: u64) -> ProviderToolProposal {
    ProviderToolProposal {
        call_id: call_id.into(),
        tool_id: "fs.read".into(),
        tool_version: 1,
        arguments: json!({
            "path": "note.txt",
            "offset_bytes": 0,
            "max_bytes": max_bytes
        }),
    }
}

fn validated(
    context: &ToolActiveContext,
    call_id: &str,
    max_bytes: u64,
) -> keeper_agent_protocol::ValidatedToolProposal {
    let proposal = provider_read(call_id, max_bytes);
    ToolProposal {
        project_id: context.project_id().into(),
        conversation_id: context.conversation_id().into(),
        run_id: context.run_id(),
        call_id: proposal.call_id,
        tool_id: proposal.tool_id,
        tool_version: proposal.tool_version,
        arguments_digest: canonical_arguments_digest(&proposal.arguments).unwrap(),
        arguments: proposal.arguments,
    }
    .validate_for_context(context)
    .unwrap()
}

#[test]
fn exact_terminal_receipt_replays_without_reexecution() {
    let fixture = fixture();
    fs::write(fixture.root.join("note.txt"), "first").unwrap();
    let host = RuntimeHost::new_with_governed_reads(
        Arc::new(IdleAdapter),
        HostConfig::default(),
        &fixture.db,
    )
    .unwrap();
    host.register_governed_read_run(
        fixture.context.run_id(),
        fixture.context.project_id(),
        fixture.context.conversation_id(),
        &fixture.root,
    )
    .unwrap();

    let cancelled = AtomicBool::new(false);
    let first = execute_governed_read(
        fixture.context.run_id(),
        provider_read("grok-acp:41", 64),
        &cancelled,
    )
    .unwrap();
    assert_eq!(first.delivery, GovernedReadDelivery::Executed);
    assert_eq!(first.result.state(), ToolExecutionState::Succeeded);
    let first_wire = serde_json::to_value(first.result.result()).unwrap();

    fs::write(fixture.root.join("note.txt"), "second").unwrap();
    let replay = execute_governed_read(
        fixture.context.run_id(),
        provider_read("grok-acp:41", 64),
        &cancelled,
    )
    .unwrap();
    assert_eq!(replay.delivery, GovernedReadDelivery::Replayed);
    assert_eq!(
        serde_json::to_value(replay.result.result()).unwrap(),
        first_wire
    );
    assert_eq!(
        replay.result.output().unwrap()["content"].as_str(),
        Some("first")
    );

    let store = SqliteToolCallRepository::open(&fixture.db).unwrap();
    assert_eq!(
        store
            .history(&validated(&fixture.context, "grok-acp:41", 64))
            .unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
            ToolExecutionState::Succeeded,
        ]
    );
    host.shutdown().unwrap();
}

#[test]
fn manifest_conflicts_unknown_tools_and_unregistered_runs_fail_closed() {
    let fixture = fixture();
    fs::write(fixture.root.join("note.txt"), "content").unwrap();
    let host = RuntimeHost::new_with_governed_reads(
        Arc::new(IdleAdapter),
        HostConfig::default(),
        &fixture.db,
    )
    .unwrap();
    host.register_governed_read_run(
        fixture.context.run_id(),
        fixture.context.project_id(),
        fixture.context.conversation_id(),
        &fixture.root,
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);

    execute_governed_read(
        fixture.context.run_id(),
        provider_read("grok-acp:42", 8),
        &cancelled,
    )
    .unwrap();
    assert!(matches!(
        execute_governed_read(
            fixture.context.run_id(),
            provider_read("grok-acp:42", 16),
            &cancelled
        ),
        Err(GovernedReadBridgeError::ReservationConflict)
    ));
    assert!(matches!(
        execute_governed_read(
            fixture.context.run_id(),
            ProviderToolProposal {
                call_id: "grok-acp:43".into(),
                tool_id: "fs.write".into(),
                tool_version: 1,
                arguments: json!({"path":"note.txt","content":"no"}),
            },
            &cancelled
        ),
        Err(GovernedReadBridgeError::InvalidProposal)
    ));

    host.release_governed_read_run(fixture.context.run_id());
    assert!(matches!(
        execute_governed_read(
            fixture.context.run_id(),
            provider_read("grok-acp:44", 64),
            &cancelled
        ),
        Err(GovernedReadBridgeError::RunNotRegistered)
    ));
    host.shutdown().unwrap();
}

#[test]
fn cancellation_is_a_durable_terminal_result() {
    let fixture = fixture();
    fs::write(fixture.root.join("note.txt"), "content").unwrap();
    let host = RuntimeHost::new_with_governed_reads(
        Arc::new(IdleAdapter),
        HostConfig::default(),
        &fixture.db,
    )
    .unwrap();
    host.register_governed_read_run(
        fixture.context.run_id(),
        fixture.context.project_id(),
        fixture.context.conversation_id(),
        &fixture.root,
    )
    .unwrap();

    let cancelled = AtomicBool::new(true);
    let receipt = execute_governed_read(
        fixture.context.run_id(),
        provider_read("grok-acp:cancelled", 64),
        &cancelled,
    )
    .unwrap();
    assert_eq!(receipt.result.state(), ToolExecutionState::Cancelled);
    assert_eq!(receipt.delivery, GovernedReadDelivery::Executed);
    host.shutdown().unwrap();
}

#[test]
fn interrupted_execution_is_recovered_through_the_same_bridge() {
    let fixture = fixture();
    fs::write(fixture.root.join("note.txt"), "recovered").unwrap();
    let proposal = validated(&fixture.context, "grok-acp:recovery", 64);
    let store = SqliteToolCallRepository::open(&fixture.db).unwrap();
    store.reserve(&proposal).unwrap();
    store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    store
        .transition(&proposal, ToolExecutionState::Executing, None)
        .unwrap();

    let host = RuntimeHost::new_with_governed_reads(
        Arc::new(IdleAdapter),
        HostConfig::default(),
        &fixture.db,
    )
    .unwrap();
    host.register_governed_read_run(
        fixture.context.run_id(),
        fixture.context.project_id(),
        fixture.context.conversation_id(),
        &fixture.root,
    )
    .unwrap();
    let receipt = execute_governed_read(
        fixture.context.run_id(),
        provider_read("grok-acp:recovery", 64),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(receipt.delivery, GovernedReadDelivery::Executed);
    assert_eq!(receipt.result.state(), ToolExecutionState::Succeeded);
    assert_eq!(
        store.history(&proposal).unwrap(),
        vec![
            ToolExecutionState::Proposed,
            ToolExecutionState::Validated,
            ToolExecutionState::Executing,
            ToolExecutionState::Succeeded,
        ]
    );
    host.shutdown().unwrap();
}
