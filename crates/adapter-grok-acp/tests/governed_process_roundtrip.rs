//! Real fake-ACP / provider roundtrip for governed processes.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::{
    EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn, StartSession,
};
use keeper_policy::ExecDecision;
use keeper_project::CreateProject;
use keeper_runtime_host::{HostConfig, RuntimeHost};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{SqliteConversationRepository, SqliteProjectRepository};

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

struct SharedSink {
    inner: Arc<Mutex<CollectingSink>>,
}

impl EventSink for SharedSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.inner.lock().expect("sink").emit(event)
    }
}

fn fake_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_acp_agent"))
}

struct Harness {
    _temp: tempfile::TempDir,
    conversation_id: String,
    run_id: RunId,
    host: Arc<RuntimeHost>,
    session_id: keeper_agent_protocol::RuntimeSessionId,
}

fn harness(script: &str) -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();

    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "Process project".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Process roundtrip".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "run process".into(),
        })
        .unwrap();

    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(15);
    config.extra_env = vec![("KEEPER_FAKE_ACP_SCRIPT".into(), script.into())];
    let adapter = Arc::new(GrokAcpAdapter::new(config).unwrap());
    let host = Arc::new(
        RuntimeHost::new_with_governed_tools(adapter, HostConfig::new(1, false, 64), &db).unwrap(),
    );
    let session = host
        .start_session(StartSession {
            conversation_key: conversation.id.0.to_string(),
            working_directory: Some(root.to_string_lossy().into_owned()),
        })
        .unwrap();
    let run_id = RunId(run.id.0);
    host.register_governed_process_run(
        run_id,
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    Harness {
        _temp: temp,
        conversation_id: conversation.id.0.to_string(),
        run_id,
        host,
        session_id: session.session_id,
    }
}

fn wait_pending(host: &RuntimeHost, conversation_id: &str) -> String {
    for _ in 0..500 {
        if let Ok(proposals) = host.list_process_proposals(conversation_id)
            && let Some(proposal) = proposals
                .into_iter()
                .find(|proposal| proposal.decision_available)
        {
            return proposal.proposal_id;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for governed process proposal")
}

fn send_with_decision(script: &str, decision: ExecDecision) -> Vec<RuntimeEvent> {
    let harness = harness(script);
    let host = Arc::clone(&harness.host);
    let conversation_id = harness.conversation_id.clone();
    let decide = thread::spawn(move || {
        let proposal_id = wait_pending(&host, &conversation_id);
        host.decide_process_proposal(&conversation_id, &proposal_id, decision)
            .unwrap();
    });
    let events = Arc::new(Mutex::new(CollectingSink::default()));
    let mut sink = SharedSink {
        inner: Arc::clone(&events),
    };
    harness
        .host
        .send_turn(
            SendTurn {
                session_id: harness.session_id,
                run_id: harness.run_id,
                content: "run process".into(),
            },
            &mut sink,
        )
        .unwrap();
    decide.join().unwrap();
    harness.host.shutdown().unwrap();
    events.lock().unwrap().events.clone()
}

fn process_lifecycle(events: &[RuntimeEvent]) -> Vec<RuntimeEventKind> {
    events
        .iter()
        .map(|event| event.kind)
        .filter(|kind| {
            matches!(
                kind,
                RuntimeEventKind::ProcessProposalRequested
                    | RuntimeEventKind::ProcessProposalDecided
                    | RuntimeEventKind::ProcessStarted
                    | RuntimeEventKind::ProcessTerminalOutcome
            )
        })
        .collect()
}

#[test]
fn approved_provider_process_emits_the_governed_lifecycle_before_completion() {
    let events = send_with_decision("governed-process-approve", ExecDecision::ApproveOnce);
    let lifecycle = process_lifecycle(&events);
    assert_eq!(
        lifecycle,
        vec![
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessStarted,
            RuntimeEventKind::ProcessTerminalOutcome,
        ]
    );
    let terminal = events
        .iter()
        .position(|event| event.kind == RuntimeEventKind::ProcessTerminalOutcome)
        .expect("process terminal event");
    let completion = events.iter().position(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
                == Some("governed-process-approve-ok")
    });
    assert!(completion.is_some_and(|completion| terminal < completion));
}

#[test]
fn rejected_provider_process_omits_started_and_completes_after_terminal() {
    let events = send_with_decision("governed-process-reject", ExecDecision::Reject);
    assert_eq!(
        process_lifecycle(&events),
        vec![
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessTerminalOutcome,
        ]
    );
    assert!(events.iter().any(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
                == Some("governed-process-reject-ok")
    }));
}

#[test]
fn exact_replay_and_conflict_do_not_duplicate_the_lifecycle() {
    for (script, completion) in [
        ("governed-process-replay", "governed-process-replay-ok"),
        ("governed-process-conflict", "governed-process-conflict-ok"),
    ] {
        let events = send_with_decision(script, ExecDecision::ApproveOnce);
        assert_eq!(
            process_lifecycle(&events),
            vec![
                RuntimeEventKind::ProcessProposalRequested,
                RuntimeEventKind::ProcessProposalDecided,
                RuntimeEventKind::ProcessStarted,
                RuntimeEventKind::ProcessTerminalOutcome,
            ],
            "{script} must not emit a second lifecycle"
        );
        assert!(
            events.iter().any(|event| {
                event.kind == RuntimeEventKind::AssistantDelta
                    && event
                        .payload
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        == Some(completion)
            }),
            "{script} events: {events:?}"
        );
    }
}

#[test]
fn nonzero_exit_script_emits_one_lifecycle_and_provider_completion() {
    let events = send_with_decision("governed-process-nonzero", ExecDecision::ApproveOnce);
    assert_eq!(
        process_lifecycle(&events),
        vec![
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessStarted,
            RuntimeEventKind::ProcessTerminalOutcome,
        ]
    );
    let terminal = events
        .iter()
        .find(|event| event.kind == RuntimeEventKind::ProcessTerminalOutcome)
        .expect("process terminal event");
    let exit = terminal
        .payload
        .get("exit_code")
        .and_then(serde_json::Value::as_i64);
    assert_eq!(
        exit,
        Some(7),
        "nonzero script must surface exit 7: {terminal:?}"
    );
    assert!(events.iter().any(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
                == Some("governed-process-nonzero-ok")
    }));
}

#[test]
fn output_bounds_script_emits_one_lifecycle_with_bound_flags() {
    let events = send_with_decision("governed-process-output-bounds", ExecDecision::ApproveOnce);
    assert_eq!(
        process_lifecycle(&events),
        vec![
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessStarted,
            RuntimeEventKind::ProcessTerminalOutcome,
        ]
    );
    let terminal = events
        .iter()
        .find(|event| event.kind == RuntimeEventKind::ProcessTerminalOutcome)
        .expect("process terminal event");
    let truncated = terminal
        .payload
        .get("truncated")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let lossy = terminal
        .payload
        .get("encoding_lossy")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    assert!(
        truncated || lossy,
        "output-bounds must mark truncated and/or encoding_lossy: {terminal:?}"
    );
    assert!(events.iter().any(|event| {
        event.kind == RuntimeEventKind::AssistantDelta
            && event
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
                == Some("governed-process-output-bounds-ok")
    }));
}
