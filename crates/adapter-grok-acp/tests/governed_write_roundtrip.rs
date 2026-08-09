//! Real fake-ACP / provider roundtrip for governed writes.
//!
//! Proves durable pending precedes the requested event, an exact native
//! approve/reject wakes the provider request, resolved precedes provider
//! completion, and provider-native write permission remains denied.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::{
    EventSink, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn, StartSession,
    WriteProposalState,
};
use keeper_policy::WriteDecision;
use keeper_project::CreateProject;
use keeper_runtime_host::{HostConfig, PreparedRuntimeHost, RuntimeHost};
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

/// Thread-safe sink so a concurrent decider can observe requested events.
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
    db: PathBuf,
    root: PathBuf,
    project_id: String,
    conversation_id: String,
    run_id: keeper_agent_protocol::RunId,
    host: Arc<RuntimeHost>,
    session_id: keeper_agent_protocol::RuntimeSessionId,
}

fn harness(script: &str) -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("out")).unwrap();

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
            title: Some("Write roundtrip".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "write".into(),
        })
        .unwrap();

    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(15);
    config.extra_env = vec![("KEEPER_FAKE_ACP_SCRIPT".into(), script.into())];
    let prepared =
        PreparedRuntimeHost::open(&db, HostConfig::new(1, false, 64)).expect("prepared host");
    let adapter =
        Arc::new(GrokAcpAdapter::new(config, prepared.governed_execution_port()).unwrap());
    let host = Arc::new(prepared.finish(adapter));
    let session = host
        .start_session(StartSession {
            conversation_key: conversation.id.0.to_string(),
            working_directory: Some(root.to_string_lossy().into_owned()),
        })
        .unwrap();
    let protocol_run_id = keeper_agent_protocol::RunId(run.id.0);
    host.register_governed_write_run(
        protocol_run_id,
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    Harness {
        _temp: temp,
        db,
        root,
        project_id: project.id.0.to_string(),
        conversation_id: conversation.id.0.to_string(),
        run_id: protocol_run_id,
        host,
        session_id: session.session_id,
    }
}

fn wait_live_grant(host: &RuntimeHost, conversation_id: &str) -> (String, String) {
    for _ in 0..500 {
        if let Ok(list) = host.list_write_proposals(conversation_id)
            && let Some(p) = list.into_iter().find(|p| {
                p.state == WriteProposalState::PendingApproval && p.approval_grant.is_some()
            })
        {
            return (p.proposal_id, p.approval_grant.unwrap());
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for live grant");
}

#[test]
fn approve_roundtrip_pending_before_requested_resolved_before_completion() {
    let h = harness("governed-write-create");
    let conversation_id = h.conversation_id.clone();
    let host = Arc::clone(&h.host);
    let sink_events = Arc::new(Mutex::new(CollectingSink::default()));
    let decided = Arc::new(AtomicBool::new(false));

    let decide = {
        let host = Arc::clone(&host);
        let conversation_id = conversation_id.clone();
        let decided = Arc::clone(&decided);
        let db = h.db.clone();
        thread::spawn(move || {
            let (proposal_id, grant) = wait_live_grant(&host, &conversation_id);
            // Durable pending must already exist before/at decision (requested emits after reserve).
            let store = keeper_storage_sqlite::SqliteWriteProposalRepository::open(&db).unwrap();
            let row = store.get(&proposal_id).unwrap().unwrap();
            assert_eq!(row.state, WriteProposalState::PendingApproval);
            host.decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::ApproveOnce,
            )
            .unwrap();
            decided.store(true, Ordering::SeqCst);
        })
    };

    let mut sink = SharedSink {
        inner: Arc::clone(&sink_events),
    };
    host.send_turn(
        SendTurn {
            session_id: h.session_id,
            run_id: h.run_id,
            content: "create file".into(),
        },
        &mut sink,
    )
    .unwrap();
    decide.join().unwrap();
    assert!(decided.load(Ordering::SeqCst));

    let events = sink_events.lock().unwrap().events.clone();
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();

    let requested_idx = kinds
        .iter()
        .position(|k| *k == RuntimeEventKind::WriteProposalRequested)
        .expect("requested event");
    let resolved_idx = kinds
        .iter()
        .position(|k| *k == RuntimeEventKind::WriteProposalResolved)
        .expect("resolved event");
    assert!(
        requested_idx < resolved_idx,
        "requested must precede resolved: {kinds:?}"
    );

    // Resolved must precede provider-visible completion text.
    let completion_idx = events.iter().position(|e| {
        e.kind == RuntimeEventKind::AssistantDelta
            && e.payload
                .get("text")
                .and_then(|t| t.as_str())
                .is_some_and(|t| t.contains("governed-write-create-ok"))
    });
    if let Some(ci) = completion_idx {
        assert!(
            resolved_idx < ci,
            "resolved must precede provider completion text"
        );
    }

    // No raw grant or content in event payloads.
    for event in &events {
        let text = format!("{:?}", event.payload);
        assert!(!text.contains("created by governed write"));
        assert!(!text.contains("approval_grant"));
        assert!(!text.contains("raw_grant"));
    }

    assert_eq!(
        fs::read_to_string(h.root.join("out/created.txt")).unwrap(),
        "created by governed write\n"
    );
    host.shutdown().unwrap();
    let _ = h.project_id;
}

#[test]
fn reject_roundtrip_wakes_provider_without_write() {
    let h = harness("governed-write-create");
    let conversation_id = h.conversation_id.clone();
    let host = Arc::clone(&h.host);

    let decide = {
        let host = Arc::clone(&host);
        let conversation_id = conversation_id.clone();
        thread::spawn(move || {
            let (proposal_id, grant) = wait_live_grant(&host, &conversation_id);
            host.decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
        })
    };

    let events = Arc::new(Mutex::new(CollectingSink::default()));
    let mut sink = SharedSink {
        inner: Arc::clone(&events),
    };
    host.send_turn(
        SendTurn {
            session_id: h.session_id,
            run_id: h.run_id,
            content: "reject write".into(),
        },
        &mut sink,
    )
    .unwrap();
    decide.join().unwrap();

    let events = events.lock().unwrap().events.clone();
    assert!(
        events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::WriteProposalRequested)
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::WriteProposalResolved)
    );
    // Provider saw a governed_write error path (pending-or-denied text).
    assert!(events.iter().any(|e| {
        e.kind == RuntimeEventKind::AssistantDelta
            && e.payload
                .get("text")
                .and_then(|t| t.as_str())
                .is_some_and(|t| {
                    t.contains("governed-write-create-pending-or-denied")
                        || t.contains("governed-write-create-failed")
                })
    }));
    assert!(!h.root.join("out/created.txt").exists());
    host.shutdown().unwrap();
}

#[test]
fn provider_native_write_permission_remains_denied() {
    let h = harness("permission-write");
    let mut sink = CollectingSink::default();
    h.host
        .send_turn(
            SendTurn {
                session_id: h.session_id,
                run_id: h.run_id,
                content: "try native write permission".into(),
            },
            &mut sink,
        )
        .unwrap();

    // Permission-write script ends cancelled / permission rejected — never
    // emits write_proposal_requested and never creates files via native path.
    assert!(
        !sink
            .events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::WriteProposalRequested)
    );
    assert!(!h.root.join("out/created.txt").exists());
    // Stop reason / cancellation should surface without applying a write.
    let kinds: Vec<_> = sink.events.iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&RuntimeEventKind::RunCancelled)
            || kinds.contains(&RuntimeEventKind::TurnCompleted)
            || kinds.contains(&RuntimeEventKind::TurnFailed)
            || kinds.contains(&RuntimeEventKind::AssistantDelta),
        "expected terminal/progress evidence: {kinds:?}"
    );
    // Native write permission is rejected: tool call may complete as failed/
    // cancelled without ever opening a governed write proposal.
    h.host.shutdown().unwrap();
}

/// C3-F product fix: cancel while `pillow/write_tool` is pending must complete
/// the provider RPC (fixed error), finish prompt-loop cleanup, and allow a
/// fresh same-process turn without waiting for the 180s request timeout.
#[test]
fn cancel_while_pending_write_completes_provider_rpc_and_next_turn() {
    use std::time::Instant;

    let h = harness("governed-write-create");
    let host = Arc::clone(&h.host);
    let conversation_id = h.conversation_id.clone();

    // Host-owned worker path: ProtocolGuardSink commits safe resolution then
    // returns RuntimeError::Cancelled to the adapter (the hang trigger).
    let spawned = host
        .spawn_turn(SendTurn {
            session_id: h.session_id,
            run_id: h.run_id,
            content: "create file while pending then cancel".into(),
        })
        .expect("spawn cancel-while-pending turn");

    // Wait until the governed write is live/pending (approval bar would show).
    let _ = wait_live_grant(&host, &conversation_id);

    let cancel_started = Instant::now();
    spawned.cancel().expect("host cancel while write pending");
    let events = spawned
        .join()
        .expect("cancelled write turn must finish without 180s hang");
    let cancel_elapsed = cancel_started.elapsed();
    assert!(
        cancel_elapsed < Duration::from_secs(5),
        "cancel-while-pending must complete within tight budget, took {cancel_elapsed:?}"
    );

    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    let resolved_idx = kinds
        .iter()
        .position(|k| *k == RuntimeEventKind::WriteProposalResolved)
        .expect("exactly one write_proposal_resolved after durable cancel");
    let cancelled_idx = kinds
        .iter()
        .position(|k| *k == RuntimeEventKind::RunCancelled)
        .expect("exactly one run_cancelled after safe resolution");
    assert!(
        resolved_idx < cancelled_idx,
        "write_proposal_resolved must precede run_cancelled: {kinds:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == RuntimeEventKind::WriteProposalResolved)
            .count(),
        1,
        "exactly one write_proposal_resolved"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == RuntimeEventKind::RunCancelled)
            .count(),
        1,
        "exactly one run_cancelled"
    );
    let resolved = &events[resolved_idx];
    assert_eq!(
        resolved.payload.get("state").and_then(|v| v.as_str()),
        Some("cancelled")
    );
    assert_eq!(
        resolved.payload.get("error_code").and_then(|v| v.as_str()),
        Some("cancelled")
    );
    // No path/content/grant/debug leakage in the resolved payload dump.
    let dump = format!("{:?}", resolved.payload);
    assert!(!dump.contains("created by governed write"));
    assert!(!dump.contains("approval_grant"));
    assert!(!dump.contains("raw_grant"));
    // No target mutation on cancel.
    assert!(
        !h.root.join("out/created.txt").exists(),
        "cancel must not write the target file"
    );
    assert!(
        host.is_fully_idle(),
        "host must be idle after cancel cleanup (gate released, no live turn)"
    );

    // Same host + same adapter instance: fresh run/session turn must complete.
    let conversations = SqliteConversationRepository::open(&h.db).unwrap();
    let run2 = conversations
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(
                uuid::Uuid::parse_str(&conversation_id).expect("conversation uuid"),
            ),
            user_message: "second turn after cancel".into(),
        })
        .unwrap();
    let run_id2 = keeper_agent_protocol::RunId(run2.id.0);
    host.register_governed_write_run(
        run_id2,
        h.project_id.clone(),
        conversation_id.clone(),
        &h.root,
    )
    .unwrap();

    let decide = {
        let host = Arc::clone(&host);
        let conversation_id = conversation_id.clone();
        thread::spawn(move || {
            let (proposal_id, grant) = wait_live_grant(&host, &conversation_id);
            host.decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::ApproveOnce,
            )
            .unwrap();
        })
    };

    let next_started = Instant::now();
    let mut sink = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: h.session_id,
            run_id: run_id2,
            content: "create file after cancel recovery".into(),
        },
        &mut sink,
    )
    .expect("next same-process turn must admit and complete after cancel recovery");
    let next_elapsed = next_started.elapsed();
    decide.join().unwrap();
    assert!(
        next_elapsed < Duration::from_secs(5),
        "next turn after cancel must complete within tight budget, took {next_elapsed:?}"
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::WriteProposalResolved),
        "recovered turn must complete governed write resolution"
    );
    assert_eq!(
        fs::read_to_string(h.root.join("out/created.txt")).unwrap(),
        "created by governed write\n"
    );

    host.shutdown().unwrap();
}
