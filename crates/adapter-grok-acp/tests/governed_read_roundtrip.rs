use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::{
    EventSink, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn, StartSession,
    ToolActiveContext, ToolExecutionState, ToolProposal, canonical_arguments_digest,
};
use keeper_project::CreateProject;
use keeper_runtime_host::{HostConfig, PreparedRuntimeHost};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteProjectRepository, SqliteToolCallRepository,
};
use serde_json::json;

#[derive(Default)]
struct CollectingSink {
    events: Vec<RuntimeEvent>,
}

impl EventSink for CollectingSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.events.push(event);
        Ok(())
    }
}

fn fake_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_acp_agent"))
}

#[test]
fn grok_reverse_read_executes_once_replays_and_emits_one_normalized_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.txt"), "governed content").unwrap();

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
            title: Some("Round trip".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "read note".into(),
        })
        .unwrap();

    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(5);
    config.extra_env = vec![(
        "KEEPER_FAKE_ACP_SCRIPT".into(),
        "governed-read-replay".into(),
    )];
    let prepared =
        PreparedRuntimeHost::open(&db, HostConfig::new(1, false, 64)).expect("prepared host");
    let adapter =
        Arc::new(GrokAcpAdapter::new(config, prepared.governed_execution_port()).unwrap());
    let host = prepared.finish(adapter);
    let session = host
        .start_session(StartSession {
            conversation_key: conversation.id.0.to_string(),
            working_directory: Some(root.to_string_lossy().into_owned()),
        })
        .unwrap();
    let protocol_run_id = keeper_agent_protocol::RunId(run.id.0);
    host.register_governed_read_run(
        protocol_run_id,
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    let mut sink = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: protocol_run_id,
            content: "read note".into(),
        },
        &mut sink,
    )
    .unwrap();

    let kinds = sink
        .events
        .iter()
        .map(|event| event.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == RuntimeEventKind::ToolCallStarted)
            .count(),
        1
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == RuntimeEventKind::ToolCallOutput)
            .count(),
        1
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == RuntimeEventKind::ToolCallCompleted)
            .count(),
        1
    );
    assert!(sink.events.iter().any(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event.payload["text"] == "governed-read-replay-ok"
    }));
    let output = sink
        .events
        .iter()
        .find(|event| event.kind == RuntimeEventKind::ToolCallOutput)
        .unwrap()
        .payload["chunk"]
        .as_str()
        .unwrap();
    assert!(output.contains("governed content"));
    let call_id = sink
        .events
        .iter()
        .find(|event| event.kind == RuntimeEventKind::ToolCallStarted)
        .unwrap()
        .payload["tool_call_id"]
        .as_str()
        .unwrap();
    assert_ne!(call_id, "grok-acp:41");
    assert!(call_id.contains(&protocol_run_id.to_string()));

    let context = ToolActiveContext::new(
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        protocol_run_id,
    )
    .unwrap();
    let arguments = json!({
        "path": "note.txt",
        "offset_bytes": 0,
        "max_bytes": 65536
    });
    let proposal = ToolProposal {
        project_id: context.project_id().into(),
        conversation_id: context.conversation_id().into(),
        run_id: context.run_id(),
        call_id: call_id.into(),
        tool_id: "fs.read".into(),
        tool_version: 1,
        arguments_digest: canonical_arguments_digest(&arguments).unwrap(),
        arguments,
    }
    .validate_for_context(&context)
    .unwrap();
    let store = SqliteToolCallRepository::open(&db).unwrap();
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

#[test]
fn reused_transport_id_with_different_request_fails_closed_before_execution() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.txt"), "governed content").unwrap();
    fs::write(root.join("other.txt"), "must not execute").unwrap();

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
            title: Some("Conflict".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "reuse transport id".into(),
        })
        .unwrap();

    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(5);
    config.extra_env = vec![(
        "KEEPER_FAKE_ACP_SCRIPT".into(),
        "governed-read-id-conflict".into(),
    )];
    let prepared =
        PreparedRuntimeHost::open(&db, HostConfig::new(1, false, 64)).expect("prepared host");
    let adapter =
        Arc::new(GrokAcpAdapter::new(config, prepared.governed_execution_port()).unwrap());
    let host = prepared.finish(adapter);
    let session = host
        .start_session(StartSession {
            conversation_key: conversation.id.0.to_string(),
            working_directory: Some(root.to_string_lossy().into_owned()),
        })
        .unwrap();
    let run_id = keeper_agent_protocol::RunId(run.id.0);
    host.register_governed_read_run(
        run_id,
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    let mut sink = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id,
            content: "reuse transport id".into(),
        },
        &mut sink,
    )
    .unwrap();

    assert!(sink.events.iter().any(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event.payload["text"] == "governed-read-id-conflict-ok"
    }));
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| event.kind == RuntimeEventKind::ToolCallStarted)
            .count(),
        1
    );
    host.shutdown().unwrap();
}

#[test]
fn reused_transport_id_in_new_run_gets_new_durable_identity_and_executes() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.txt"), "governed content").unwrap();

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
            title: Some("New run".into()),
        })
        .unwrap();

    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(5);
    config.extra_env = vec![(
        "KEEPER_FAKE_ACP_SCRIPT".into(),
        "governed-read-replay".into(),
    )];
    let prepared =
        PreparedRuntimeHost::open(&db, HostConfig::new(1, false, 64)).expect("prepared host");
    let adapter =
        Arc::new(GrokAcpAdapter::new(config, prepared.governed_execution_port()).unwrap());
    let host = prepared.finish(adapter);
    let session = host
        .start_session(StartSession {
            conversation_key: conversation.id.0.to_string(),
            working_directory: Some(root.to_string_lossy().into_owned()),
        })
        .unwrap();

    let mut call_ids = Vec::new();
    for user_message in ["first run", "second run"] {
        let run = conversations
            .create_run(NewRun {
                conversation_id: conversation.id,
                user_message: user_message.into(),
            })
            .unwrap();
        let run_id = keeper_agent_protocol::RunId(run.id.0);
        host.register_governed_read_run(
            run_id,
            project.id.0.to_string(),
            conversation.id.0.to_string(),
            &root,
        )
        .unwrap();
        let mut sink = CollectingSink::default();
        host.send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: user_message.into(),
            },
            &mut sink,
        )
        .unwrap();
        let started = sink
            .events
            .iter()
            .filter(|event| event.kind == RuntimeEventKind::ToolCallStarted)
            .collect::<Vec<_>>();
        assert_eq!(started.len(), 1);
        let call_id = started[0].payload["tool_call_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(call_id.contains(&run_id.to_string()));
        call_ids.push(call_id);
    }

    assert_ne!(call_ids[0], call_ids[1]);
    host.shutdown().unwrap();
}
