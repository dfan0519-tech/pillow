//! Immediate cancel must be real, including delayed adapter registration.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeProbe, RuntimeSession, RuntimeSessionId, SendTurn, StartSession,
};
use keeper_runtime_host::{HostConfig, RuntimeHost};

fn terminal_kinds(events: &[RuntimeEvent]) -> Vec<RuntimeEventKind> {
    events
        .iter()
        .filter(|e| e.is_terminal())
        .map(|e| e.kind)
        .collect()
}

/// Malicious adapter: ignores real arming, registers only after a delay inside
/// `send_turn`, and will emit `turn_completed` unless the host forces cancel.
struct DelayedRegistrationAdapter {
    registered: Mutex<std::collections::HashMap<RunId, Arc<AtomicBool>>>,
    register_delay_ms: u64,
}

impl DelayedRegistrationAdapter {
    fn new(register_delay_ms: u64) -> Self {
        Self {
            registered: Mutex::new(std::collections::HashMap::new()),
            register_delay_ms,
        }
    }
}

impl AgentRuntimeAdapter for DelayedRegistrationAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "delayed-reg".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    /// Explicit no-op: models an adapter that fails to arm before send_turn.
    fn arm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn disarm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        thread::sleep(Duration::from_millis(self.register_delay_ms));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut map = self.registered.lock().expect("registered");
            map.insert(request.run_id, Arc::clone(&cancel));
        }

        if cancel.load(Ordering::SeqCst) {
            sink.emit(RuntimeEvent {
                run_id: request.run_id,
                sequence: 1,
                kind: RuntimeEventKind::RunCancelled,
                payload: serde_json::json!({"reason": "adapter"}),
                created_at: Utc::now(),
            })?;
            return Ok(());
        }

        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 1,
            kind: RuntimeEventKind::RunStarted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
        thread::sleep(Duration::from_millis(30));
        if cancel.load(Ordering::SeqCst) {
            sink.emit(RuntimeEvent {
                run_id: request.run_id,
                sequence: 2,
                kind: RuntimeEventKind::RunCancelled,
                payload: serde_json::json!({"reason": "adapter"}),
                created_at: Utc::now(),
            })?;
            return Ok(());
        }
        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 2,
            kind: RuntimeEventKind::TurnCompleted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
        Ok(())
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let map = self.registered.lock().expect("registered");
        if let Some(flag) = map.get(&run_id) {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(RuntimeError::RunNotFound { run_id })
        }
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[test]
fn cancel_immediately_after_spawn_never_yields_turn_completed_with_delayed_registration() {
    for _ in 0..16 {
        let adapter = Arc::new(DelayedRegistrationAdapter::new(50));
        let host = Arc::new(RuntimeHost::new(
            adapter,
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: false,
                event_queue_capacity: 16,
                after_worker_registered: None,
            },
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "delayed".into(),
                working_directory: None,
            })
            .unwrap();
        let spawned = host
            .spawn_turn(SendTurn {
                session_id: session.session_id,
                run_id: RunId::new(),
                content: "go".into(),
            })
            .unwrap();

        // Cancel immediately — adapter has not registered yet (RunNotFound path).
        spawned
            .cancel()
            .expect("host-owned cancel must succeed even when adapter returns RunNotFound");

        let events = spawned.join().unwrap();
        assert_eq!(
            terminal_kinds(&events),
            vec![RuntimeEventKind::RunCancelled],
            "immediate cancel must yield exactly run_cancelled, never turn_completed; got {events:?}"
        );
        assert!(
            events
                .iter()
                .all(|e| e.kind != RuntimeEventKind::TurnCompleted),
            "turn_completed must not appear after immediate cancel: {events:?}"
        );
    }
}

/// Capacity-1 queue with terminal blocked on enqueue: cancel that returns
/// success before terminal commit must produce only `run_cancelled`.
#[test]
fn cancel_wins_over_turn_completed_blocked_on_full_queue() {
    for _ in 0..32 {
        // run_started fills capacity=1; turn_completed blocks on enqueue.
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(
            keeper_runtime_host::FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap(),
        );
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
                conversation_key: "term-block".into(),
                working_directory: None,
            })
            .unwrap();
        let spawned = host
            .spawn_turn(SendTurn {
                session_id: session.session_id,
                run_id: RunId::new(),
                content: "block".into(),
            })
            .unwrap();

        // Wait until turn_completed is blocked on the full queue.
        let mut blocked = false;
        for _ in 0..500 {
            if spawned.delivery().producer_blocked() {
                blocked = true;
                break;
            }
            if spawned.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            blocked,
            "expected turn_completed producer blocked on capacity=1"
        );
        assert!(
            !spawned.is_finished(),
            "terminal must still be blocked before cancel"
        );

        spawned
            .cancel()
            .expect("cancel must succeed before terminal commit");

        let events = spawned.join().expect("cancel+join must progress");
        assert_eq!(
            terminal_kinds(&events),
            vec![RuntimeEventKind::RunCancelled],
            "cancel before terminal commit must yield only run_cancelled; got {events:?}"
        );
        assert!(
            events
                .iter()
                .all(|e| e.kind != RuntimeEventKind::TurnCompleted),
            "blocked turn_completed must not commit after cancel: {events:?}"
        );
    }
}
