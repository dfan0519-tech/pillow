//! Full-queue join / cancel+join / Drop must make progress without a consumer.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{RunId, RuntimeEvent, RuntimeEventKind, SendTurn, StartSession};
use keeper_runtime_host::{FakeRuntimeAdapter, HostConfig, RuntimeHost, SpawnedTurn};

fn terminal_kinds(events: &[RuntimeEvent]) -> Vec<RuntimeEventKind> {
    events
        .iter()
        .filter(|e| e.is_terminal())
        .map(|e| e.kind)
        .collect()
}

fn many_event_fixture(deltas: usize) -> String {
    let mut lines = String::from(r#"{"kind":"run_started","delay_ms":0,"payload":{}}"#);
    lines.push('\n');
    for i in 0..deltas {
        lines.push_str(&format!(
            r#"{{"kind":"assistant_delta","delay_ms":0,"payload":{{"text":"{i}"}}}}"#
        ));
        lines.push('\n');
    }
    lines.push_str(r#"{"kind":"turn_completed","delay_ms":0,"payload":{}}"#);
    lines.push('\n');
    lines
}

fn wait_producer_blocked(spawned: &SpawnedTurn) {
    for _ in 0..500 {
        if spawned.delivery().producer_blocked() {
            return;
        }
        if spawned.is_finished() {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn host_with_capacity(capacity: usize, fixture: &str) -> Arc<RuntimeHost> {
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap());
    Arc::new(RuntimeHost::new(
        adapter,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: false,
            event_queue_capacity: capacity,
            after_worker_registered: None,
        },
    ))
}

#[test]
fn join_progresses_when_event_count_exceeds_capacity() {
    let capacity = 2usize;
    let host = host_with_capacity(capacity, &many_event_fixture(20));
    let session = host
        .start_session(StartSession {
            conversation_key: "join-full".into(),
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

    let events = spawned
        .join()
        .expect("join must not deadlock on full queue");
    assert!(
        events.len() > capacity,
        "expected more events than capacity ({capacity}), got {}",
        events.len()
    );
    assert_eq!(
        terminal_kinds(&events),
        vec![RuntimeEventKind::TurnCompleted]
    );
}

#[test]
fn cancel_join_progresses_when_queue_is_full() {
    let capacity = 1usize;
    let host = host_with_capacity(capacity, &many_event_fixture(30));
    let session = host
        .start_session(StartSession {
            conversation_key: "cancel-full".into(),
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

    wait_producer_blocked(&spawned);
    spawned.cancel().unwrap();
    let events = spawned
        .join()
        .expect("cancel+join must not deadlock on full queue");
    assert_eq!(
        terminal_kinds(&events),
        vec![RuntimeEventKind::RunCancelled],
        "cancel+join terminal: {events:?}"
    );
}

#[test]
fn drop_progresses_when_queue_is_full() {
    let capacity = 1usize;
    let host = host_with_capacity(capacity, &many_event_fixture(30));
    let session = host
        .start_session(StartSession {
            conversation_key: "drop-full".into(),
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

    wait_producer_blocked(&spawned);
    drop(spawned);

    for _ in 0..500 {
        if host.in_flight_turns() == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        host.in_flight_turns(),
        0,
        "Drop must join the worker and release the concurrency slot"
    );
}
