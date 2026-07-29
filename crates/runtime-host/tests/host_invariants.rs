//! Lifecycle regressions: immediate cancel, shutdown join, backpressure, slot rollback.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};
use keeper_runtime_host::{FakeRuntimeAdapter, HostConfig, RuntimeHost};

fn terminal_kinds(events: &[RuntimeEvent]) -> Vec<RuntimeEventKind> {
    events
        .iter()
        .filter(|e| e.is_terminal())
        .map(|e| e.kind)
        .collect()
}

#[test]
fn spawn_turn_is_cancellable_immediately_after_return() {
    // Long delays so the worker would take a long time if cancel is lost.
    let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":200,"payload":{"text":"a"}}
{"kind":"assistant_delta","delay_ms":200,"payload":{"text":"b"}}
{"kind":"assistant_delta","delay_ms":200,"payload":{"text":"c"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: true,
            event_queue_capacity: 64,
            after_worker_registered: None,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "imm".into(),
            working_directory: None,
        })
        .unwrap();
    let spawned = host
        .spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "slow".into(),
        })
        .unwrap();
    // Cancel immediately — must succeed without waiting for first emit.
    spawned
        .cancel()
        .expect("cancel must work as soon as spawn returns");
    let events = spawned.join().unwrap();
    assert_eq!(
        terminal_kinds(&events),
        vec![RuntimeEventKind::RunCancelled],
        "immediate cancel must yield run_cancelled, got {events:?}"
    );
}

#[test]
fn shutdown_joins_workers_without_later_join_masking() {
    let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":80,"payload":{"text":"x"}}
{"kind":"assistant_delta","delay_ms":80,"payload":{"text":"y"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: true,
            event_queue_capacity: 64,
            after_worker_registered: None,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "join".into(),
            working_directory: None,
        })
        .unwrap();
    let spawned = host
        .spawn_turn(SendTurn {
            session_id: session.session_id,
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

    host.shutdown().unwrap();
    // Prove cleanup at return: finished + no slots, without calling join first.
    assert!(spawned.is_finished());
    assert_eq!(host.in_flight_turns(), 0);
    // Drop without join must not hang or leave residual work.
    drop(spawned);
}

#[test]
fn slow_consumer_experiences_bounded_backpressure() {
    // Many small events, capacity 1: producer must block until consumer drains.
    let mut lines = String::from(r#"{"kind":"run_started","delay_ms":0,"payload":{}}"#);
    lines.push('\n');
    for i in 0..20 {
        lines.push_str(&format!(
            r#"{{"kind":"assistant_delta","delay_ms":0,"payload":{{"text":"{i}"}}}}"#
        ));
        lines.push('\n');
    }
    lines.push_str(r#"{"kind":"turn_completed","delay_ms":0,"payload":{}}"#);
    lines.push('\n');

    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(&lines, false).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: false,
            event_queue_capacity: 1,
            after_worker_registered: None,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "bp".into(),
            working_directory: None,
        })
        .unwrap();
    let spawned = host
        .spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "many".into(),
        })
        .unwrap();

    // Wait until the queue is full and the producer is blocked.
    let mut saw_block = false;
    for _ in 0..200 {
        if spawned.delivery().producer_blocked() {
            saw_block = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        saw_block,
        "expected producer to block on capacity=1 with a slow consumer"
    );
    assert!(
        !spawned.is_finished(),
        "turn must not finish while backpressured"
    );
    assert!(spawned.delivery().len() <= 1);

    // Slow drain: consume one-by-one.
    let mut collected = Vec::new();
    while !spawned.is_finished() || !spawned.delivery().is_empty() {
        if let Some(ev) = spawned.delivery().try_pop() {
            collected.push(ev);
        } else {
            thread::sleep(Duration::from_millis(1));
        }
        if collected.len() > 100 {
            break;
        }
    }
    let rest = spawned.join().unwrap();
    collected.extend(rest);
    assert!(
        collected
            .iter()
            .any(|e| e.kind == RuntimeEventKind::TurnCompleted)
            || collected
                .iter()
                .any(|e| e.kind == RuntimeEventKind::RunCancelled),
        "expected a terminal after draining, got {collected:?}"
    );
    assert!(
        collected.len() >= 3,
        "expected streamed events under backpressure"
    );
}

/// Prove slot rollback when `arm_turn` fails after slot acquisition.
struct ArmFailsAdapter;

impl AgentRuntimeAdapter for ArmFailsAdapter {
    fn probe(&self) -> Result<keeper_agent_protocol::RuntimeProbe, RuntimeError> {
        Ok(keeper_agent_protocol::RuntimeProbe {
            healthy: true,
            adapter_name: "arm-fail".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(
        &self,
        request: StartSession,
    ) -> Result<keeper_agent_protocol::RuntimeSession, RuntimeError> {
        Ok(keeper_agent_protocol::RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        Err(RuntimeError::Internal {
            message: format!("arm failed for {run_id}"),
        })
    }

    fn disarm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, _request: SendTurn, _sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        unreachable!("send_turn must not run when arm fails")
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        Err(RuntimeError::RunNotFound { run_id })
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[test]
fn slot_is_released_when_arm_turn_fails() {
    let host = Arc::new(RuntimeHost::new(
        Arc::new(ArmFailsAdapter),
        HostConfig {
            max_concurrent_turns: 1,
            honor_fixture_delays: false,
            event_queue_capacity: 8,
            after_worker_registered: None,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "slot".into(),
            working_directory: None,
        })
        .unwrap();
    let err = match host.spawn_turn(SendTurn {
        session_id: session.session_id,
        run_id: RunId::new(),
        content: "x".into(),
    }) {
        Ok(_) => panic!("expected arm_turn failure"),
        Err(e) => e,
    };
    assert!(matches!(err, RuntimeError::Internal { .. }));
    assert_eq!(
        host.in_flight_turns(),
        0,
        "failed spawn must not leak a concurrency slot"
    );
    // A subsequent spawn attempt should not hit ConcurrencyLimit from a leak.
    let err2 = match host.spawn_turn(SendTurn {
        session_id: session.session_id,
        run_id: RunId::new(),
        content: "y".into(),
    }) {
        Ok(_) => panic!("expected arm_turn failure again"),
        Err(e) => e,
    };
    assert!(
        matches!(err2, RuntimeError::Internal { .. }),
        "expected arm failure again, not concurrency leak: {err2}"
    );
}
