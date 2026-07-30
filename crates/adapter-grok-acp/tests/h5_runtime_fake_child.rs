//! H5 regression coverage for ACP's one-sink/one-prompt runtime constraints.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};

struct CollectingSink {
    events: Arc<Mutex<Vec<RuntimeEvent>>>,
}

impl EventSink for CollectingSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.events.lock().expect("events lock").push(event);
        Ok(())
    }
}

fn fake_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_acp_agent"))
}

fn config(script: &str) -> GrokAcpConfig {
    let mut config = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    config.request_timeout = Duration::from_secs(5);
    config.cancel_grace = Duration::from_millis(100);
    config.extra_env = vec![
        ("KEEPER_FAKE_ACP_SCRIPT".into(), script.into()),
        ("KEEPER_FAKE_ACP_DELAY_MS".into(), "0".into()),
    ];
    config
}

fn config_with_cleanup_failure(script: &str) -> GrokAcpConfig {
    let mut config = config(script);
    config
        .extra_env
        .push(("KEEPER_ACP_TEST_FAIL_KILL_TREE".into(), "1".into()));
    config
}

fn cwd() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn start(adapter: &GrokAcpAdapter) -> keeper_agent_protocol::RuntimeSession {
    adapter
        .start_session(StartSession {
            conversation_key: "h5-conversation".into(),
            working_directory: Some(cwd()),
        })
        .expect("start session")
}

fn wait_for_text(events: &Arc<Mutex<Vec<RuntimeEvent>>>, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if events.lock().expect("events lock").iter().any(|event| {
            event.kind == RuntimeEventKind::AssistantDelta
                && event.payload["text"]
                    .as_str()
                    .is_some_and(|value| value == text)
        }) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {text}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn concurrent_session_operation_keeps_active_prompt_events_attached() {
    let adapter = Arc::new(GrokAcpAdapter::new(config("race-session-operation")).unwrap());
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));

    let prompt_adapter = Arc::clone(&adapter);
    let prompt_events = Arc::clone(&events);
    let prompt = thread::spawn(move || {
        let mut sink = CollectingSink {
            events: prompt_events,
        };
        prompt_adapter.send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "race".into(),
            },
            &mut sink,
        )
    });
    wait_for_text(&events, "before-session-operation");

    let (entered_tx, entered_rx) = mpsc::channel();
    let session_adapter = Arc::clone(&adapter);
    let session_operation = thread::spawn(move || {
        entered_tx.send(()).unwrap();
        session_adapter.start_session(StartSession {
            conversation_key: "concurrent-session".into(),
            working_directory: Some(cwd()),
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    wait_for_text(&events, "after-session-operation");
    adapter.cancel_turn(run_id).unwrap();
    prompt.join().unwrap().unwrap();
    session_operation
        .join()
        .unwrap()
        .expect("session operation");

    let recorded = events.lock().unwrap();
    assert_eq!(
        recorded
            .iter()
            .filter(|event| event.kind.is_terminal())
            .count(),
        1
    );
    assert_eq!(
        recorded.last().map(|event| event.kind),
        Some(RuntimeEventKind::RunCancelled)
    );
    adapter.shutdown().unwrap();
}

#[test]
fn ignored_cancel_escalates_within_grace_and_leaves_adapter_unavailable() {
    let adapter = Arc::new(GrokAcpAdapter::new(config("ignore-cancel")).unwrap());
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let cancel_adapter = Arc::clone(&adapter);
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        cancel_adapter.cancel_turn(run_id)
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut sink = CollectingSink {
        events: Arc::clone(&events),
    };

    let started = Instant::now();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "cancel me".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancellation escalation exceeded its bounded deadline"
    );
    canceller.join().unwrap().unwrap();

    let recorded = events.lock().unwrap();
    assert_eq!(
        recorded
            .iter()
            .filter(|event| event.kind == RuntimeEventKind::RunCancelled)
            .count(),
        1
    );
    assert_eq!(
        recorded
            .iter()
            .filter(|event| event.kind.is_terminal())
            .count(),
        1
    );
    drop(recorded);
    assert_eq!(
        adapter.probe().unwrap().detail.as_deref(),
        Some("unavailable")
    );

    adapter
        .arm_turn(run_id)
        .expect("escalation clears active turn");
    adapter.disarm_turn(run_id).unwrap();
    adapter.shutdown().unwrap();
}

#[test]
fn ignored_cancel_cleanup_failure_is_failed_terminal_and_clears_runtime_state() {
    let adapter =
        Arc::new(GrokAcpAdapter::new(config_with_cleanup_failure("ignore-cancel")).unwrap());
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let cancel_adapter = Arc::clone(&adapter);
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        cancel_adapter.cancel_turn(run_id)
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut sink = CollectingSink {
        events: Arc::clone(&events),
    };

    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "cancel me".into(),
            },
            &mut sink,
        )
        .unwrap();
    canceller.join().unwrap().unwrap();

    let recorded = events.lock().unwrap();
    let terminals = recorded
        .iter()
        .filter(|event| event.kind.is_terminal())
        .collect::<Vec<_>>();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].kind, RuntimeEventKind::TurnFailed);
    assert_eq!(
        terminals[0].payload["code"].as_str(),
        Some("acp_kill_tree_failed")
    );
    assert_eq!(
        recorded
            .iter()
            .filter(|event| event.kind == RuntimeEventKind::RunCancelled)
            .count(),
        0
    );
    drop(recorded);
    assert_eq!(
        adapter.probe().unwrap().detail.as_deref(),
        Some("unavailable")
    );
    let next_run_id = RunId::new();
    adapter
        .arm_turn(next_run_id)
        .expect("cleanup clears the active turn");
    assert!(matches!(
        adapter.send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: next_run_id,
                content: "stale session".into(),
            },
            &mut sink,
        ),
        Err(RuntimeError::SessionNotFound { .. })
    ));
    adapter.disarm_turn(next_run_id).unwrap();
    adapter.shutdown().unwrap();
}
