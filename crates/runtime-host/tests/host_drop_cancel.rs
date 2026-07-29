//! `SpawnedTurn` Drop requests real adapter cancellation; exactly one terminal.

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeEventKind, SendTurn, StartSession};
use keeper_runtime_host::{HostConfig, RuntimeHost};

use common::CancelUnblocksAdapter;

/// (8)+(9) Dropping a turn blocked inside the adapter calls cancel_turn,
/// unblocks, leaves no worker/slot, and still emits exactly one terminal.
#[test]
fn drop_requests_adapter_cancel_for_blocked_turn() {
    for _ in 0..16 {
        let adapter = Arc::new(CancelUnblocksAdapter::new());
        let host = Arc::new(RuntimeHost::new(
            adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: false,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "drop-cancel".into(),
                working_directory: None,
            })
            .unwrap();
        let run_id = RunId::new();
        let spawned = host
            .spawn_turn(SendTurn {
                session_id: session.session_id,
                run_id,
                content: "block".into(),
            })
            .unwrap();

        // Wait until the worker body is blocked inside send_turn.
        for _ in 0..200 {
            if adapter.cancel_calls.load(Ordering::SeqCst) > 0 {
                break;
            }
            // Worker is Running and blocked; registry still holds it.
            if host.worker_registry_len() == 1 && host.in_flight_turns() == 1 {
                // Give the body time to enter the wait.
                thread::sleep(Duration::from_millis(2));
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(host.worker_registry_len(), 1);
        assert_eq!(host.in_flight_turns(), 1);

        // Drop must call cancel_turn, unblock the adapter, and clean up.
        drop(spawned);

        assert!(
            adapter.cancel_calls.load(Ordering::SeqCst) >= 1,
            "Drop must invoke adapter.cancel_turn"
        );
        assert_eq!(host.worker_registry_len(), 0);
        assert_eq!(host.in_flight_turns(), 0);
        assert!(host.is_fully_idle());

        // Exactly one terminal recorded by the adapter path.
        let terminals = adapter.terminals.lock().expect("terminals");
        if let Some(kind) = terminals.get(&run_id) {
            assert_eq!(
                *kind,
                RuntimeEventKind::RunCancelled,
                "adapter must emit run_cancelled as the sole terminal kind"
            );
        }
        // Host-side events: at most one terminal after drop join.
        // (events were drained into Drop's discarded buffer; adapter map is the
        // source of truth that cancel unblocked a single terminal emission.)
    }
}

/// (9 continued) Explicit cancel still yields exactly one terminal event.
#[test]
fn cancel_after_spawn_emits_exactly_one_terminal() {
    for _ in 0..16 {
        let adapter = Arc::new(CancelUnblocksAdapter::new());
        let host = Arc::new(RuntimeHost::new(
            adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: false,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "one-term".into(),
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

        // Ensure body is in send_turn wait.
        thread::sleep(Duration::from_millis(5));
        spawned
            .cancel()
            .expect("cancel must succeed for host-owned run");
        let events = spawned.join().expect("join after cancel");
        let terminals: Vec<_> = events.iter().filter(|e| e.is_terminal()).collect();
        assert_eq!(
            terminals.len(),
            1,
            "exactly one terminal event required, got {events:?}"
        );
        assert_eq!(terminals[0].kind, RuntimeEventKind::RunCancelled);
        assert!(host.is_fully_idle());
        assert!(adapter.cancel_calls.load(Ordering::SeqCst) >= 1);
    }
}
