//! Integration tests for the deterministic fake runtime and bounded host.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};
use keeper_runtime_host::{
    CollectingSink, FAKE_TURN_FIXTURE, FakeRuntimeAdapter, HostConfig, RuntimeHost,
    parse_fixture_jsonl, workspace_fixture_path,
};

fn start_host(honor_delays: bool) -> Arc<RuntimeHost> {
    Arc::new(
        RuntimeHost::with_fake(HostConfig {
            max_concurrent_turns: 4,
            honor_fixture_delays: honor_delays,
            event_queue_capacity: 64,
        })
        .expect("host"),
    )
}

fn kinds_and_payloads(events: &[RuntimeEvent]) -> Vec<(RuntimeEventKind, serde_json::Value)> {
    events.iter().map(|e| (e.kind, e.payload.clone())).collect()
}

fn terminal_kinds(events: &[RuntimeEvent]) -> Vec<RuntimeEventKind> {
    events
        .iter()
        .filter(|e| e.is_terminal())
        .map(|e| e.kind)
        .collect()
}

#[test]
fn fixture_file_matches_embedded_committed_fixture() {
    let from_disk = std::fs::read_to_string(workspace_fixture_path()).expect("read fixture");
    // Normalize newlines for Windows checkouts.
    let disk = from_disk.replace("\r\n", "\n");
    let embedded = FAKE_TURN_FIXTURE.replace("\r\n", "\n");
    assert_eq!(disk.trim(), embedded.trim());
}

#[test]
fn ordered_streaming_emits_monotonic_sequences() {
    let host = start_host(false);
    let session = host
        .start_session(StartSession {
            conversation_key: "conv-stream".into(),
            working_directory: None,
        })
        .unwrap();
    let run_id = RunId::new();
    let mut sink = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id,
            content: "summarize".into(),
        },
        &mut sink,
    )
    .unwrap();

    assert!(!sink.events.is_empty());
    assert_eq!(sink.events[0].kind, RuntimeEventKind::RunStarted);
    for (idx, event) in sink.events.iter().enumerate() {
        assert_eq!(event.run_id, run_id);
        assert_eq!(event.sequence, (idx as u64) + 1);
    }
    assert_eq!(
        terminal_kinds(&sink.events),
        vec![RuntimeEventKind::TurnCompleted]
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::AssistantDelta)
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::PlanUpdated)
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::ToolCallCompleted)
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::SubagentCompleted)
    );
    assert!(
        sink.events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::ArtifactProduced)
    );
}

#[test]
fn deterministic_replay_is_identical_across_two_runs() {
    let host = start_host(false);
    let session = host
        .start_session(StartSession {
            conversation_key: "conv-det".into(),
            working_directory: None,
        })
        .unwrap();

    let mut first = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "one".into(),
        },
        &mut first,
    )
    .unwrap();

    let mut second = CollectingSink::default();
    host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "two".into(),
        },
        &mut second,
    )
    .unwrap();

    assert_eq!(
        kinds_and_payloads(&first.events),
        kinds_and_payloads(&second.events),
        "fixture replay must be deterministic"
    );
    // Sequences restart per run but remain 1..n
    assert_eq!(
        first.events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        second.events.iter().map(|e| e.sequence).collect::<Vec<_>>()
    );
}

#[test]
fn cancellation_stops_emission_with_exactly_one_terminal() {
    // Slow fixture so cancel wins before turn_completed.
    let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{"prompt_preview":"slow"}}
{"kind":"assistant_delta","delay_ms":40,"payload":{"text":"a"}}
{"kind":"assistant_delta","delay_ms":40,"payload":{"text":"b"}}
{"kind":"assistant_delta","delay_ms":40,"payload":{"text":"c"}}
{"kind":"assistant_delta","delay_ms":40,"payload":{"text":"d"}}
{"kind":"assistant_delta","delay_ms":40,"payload":{"text":"e"}}
{"kind":"turn_completed","delay_ms":0,"payload":{"summary":"should not finish"}}
"#;
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: true,
            event_queue_capacity: 64,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "conv-cancel".into(),
            working_directory: None,
        })
        .unwrap();
    let run_id = RunId::new();
    let spawned = host
        .spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id,
            content: "slow".into(),
        })
        .unwrap();

    // Wait until at least the first event lands, then cancel.
    let mut saw_progress = false;
    for _ in 0..100 {
        if !spawned.events_snapshot().is_empty() {
            saw_progress = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(saw_progress, "expected stream to start before cancel");
    spawned.cancel().unwrap();

    let events = spawned.join().unwrap();
    let terminals = terminal_kinds(&events);
    assert_eq!(
        terminals,
        vec![RuntimeEventKind::RunCancelled],
        "expected sole terminal RunCancelled, got {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::TurnCompleted),
        "completion must not co-exist with cancel"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.kind == RuntimeEventKind::TurnFailed),
        "failure must not co-exist with cancel"
    );
}

#[test]
fn completion_failure_and_cancellation_are_mutually_exclusive() {
    let completed = parse_fixture_jsonl(FAKE_TURN_FIXTURE).unwrap();
    assert_eq!(completed.iter().filter(|l| l.kind.is_terminal()).count(), 1);
    assert_eq!(
        completed.last().unwrap().kind,
        RuntimeEventKind::TurnCompleted
    );

    let failed = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"turn_failed","delay_ms":0,"payload":{"code":"x","message":"y"}}
"#;
    let adapter = FakeRuntimeAdapter::from_fixture_text(failed, false).unwrap();
    let session = adapter
        .start_session(StartSession {
            conversation_key: "f".into(),
            working_directory: None,
        })
        .unwrap();
    let mut sink = CollectingSink::default();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: RunId::new(),
                content: "x".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(
        terminal_kinds(&sink.events),
        vec![RuntimeEventKind::TurnFailed]
    );
}

#[test]
fn shutdown_rejects_new_work_and_leaves_no_active_turns() {
    let host = start_host(true);
    let session = host
        .start_session(StartSession {
            conversation_key: "conv-shutdown".into(),
            working_directory: None,
        })
        .unwrap();

    // Start a slow turn so shutdown must cancel it.
    let slow_host = {
        let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":50,"payload":{"text":"x"}}
{"kind":"assistant_delta","delay_ms":50,"payload":{"text":"y"}}
{"kind":"assistant_delta","delay_ms":50,"payload":{"text":"z"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
        Arc::new(RuntimeHost::new(
            adapter.clone(),
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: true,
                event_queue_capacity: 64,
            },
        ))
    };

    let session2 = slow_host
        .start_session(StartSession {
            conversation_key: "s2".into(),
            working_directory: None,
        })
        .unwrap();
    let spawned = slow_host
        .spawn_turn(SendTurn {
            session_id: session2.session_id,
            run_id: RunId::new(),
            content: "slow".into(),
        })
        .unwrap();

    for _ in 0..100 {
        if !spawned.events_snapshot().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }

    slow_host.shutdown().unwrap();
    // Worker must already be finished without requiring join() first.
    assert!(
        spawned.is_finished(),
        "shutdown must join host workers before returning"
    );
    assert_eq!(slow_host.in_flight_turns(), 0);
    let events = spawned.join().unwrap();
    assert!(
        terminal_kinds(&events).len() <= 1,
        "at most one terminal after shutdown cancel"
    );

    let err = slow_host
        .start_session(StartSession {
            conversation_key: "after".into(),
            working_directory: None,
        })
        .unwrap_err();
    assert!(matches!(err, RuntimeError::ShutDown));

    // Original host without in-flight work also shuts down cleanly.
    host.shutdown().unwrap();
    let err = host
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: RunId::new(),
                content: "nope".into(),
            },
            &mut CollectingSink::default(),
        )
        .unwrap_err();
    assert!(matches!(err, RuntimeError::ShutDown));
}

#[test]
fn concurrency_limit_is_enforced() {
    let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":30,"payload":{"text":"x"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 1,
            honor_fixture_delays: true,
            event_queue_capacity: 64,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "lim".into(),
            working_directory: None,
        })
        .unwrap();

    let first = host
        .spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "a".into(),
        })
        .unwrap();

    // Second turn should hit the bound while first is in flight.
    let mut hit_limit = false;
    for _ in 0..20 {
        match host.spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "b".into(),
        }) {
            Ok(spawned) => {
                // Rare race if first finished; join and retry once.
                let _ = spawned.join();
            }
            Err(RuntimeError::ConcurrencyLimit { max }) => {
                assert_eq!(max, 1);
                hit_limit = true;
                break;
            }
            Err(other) => panic!("unexpected error: {other}"),
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(hit_limit, "expected concurrency limit");
    let _ = first.join();
    host.shutdown().unwrap();
}

#[test]
fn probe_reports_fake_adapter_without_credentials() {
    let host = start_host(false);
    let probe = host.probe().unwrap();
    assert!(probe.healthy);
    assert_eq!(probe.adapter_name, "fake");
    let dump = serde_json::to_string(&probe).unwrap().to_ascii_lowercase();
    assert!(!dump.contains("api_key"));
    assert!(!dump.contains("token"));
    assert!(!dump.contains("password"));
}
