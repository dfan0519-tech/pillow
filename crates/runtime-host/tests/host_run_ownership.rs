//! Unique run ownership: duplicate `run_id` never detaches a live worker.

mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeError, SendTurn, StartSession};
use keeper_runtime_host::{HostConfig, RuntimeHost};

use common::BlockingArmAdapter;

/// (6)+(7) Duplicate run_id is rejected without replacing the first worker;
/// cleanup cannot remove the original registry entry; shutdown joins the first.
#[test]
fn duplicate_run_id_rejected_without_replacing_live_worker() {
    let enter = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let adapter = Arc::new(BlockingArmAdapter::new(
        Arc::clone(&enter),
        Arc::clone(&release),
    ));
    let host = Arc::new(RuntimeHost::new(
        adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
        HostConfig {
            max_concurrent_turns: 4,
            honor_fixture_delays: false,
            event_queue_capacity: 8,
            after_worker_registered: None,
        },
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "dup".into(),
            working_directory: None,
        })
        .unwrap();
    let session_id = session.session_id;
    let run_id = RunId::new();

    let host_first = Arc::clone(&host);
    let first = thread::spawn(move || {
        host_first.spawn_turn(SendTurn {
            session_id,
            run_id,
            content: "first".into(),
        })
    });

    // First worker registered (Starting) and blocked in arm_turn.
    enter.wait();
    assert_eq!(host.worker_registry_len(), 1);
    assert_eq!(host.in_flight_turns(), 1);

    // Second spawn with the same run_id must fail without map replacement.
    let err = match host.spawn_turn(SendTurn {
        session_id,
        run_id,
        content: "second".into(),
    }) {
        Ok(_) => panic!("duplicate run_id must be rejected"),
        Err(e) => e,
    };
    assert!(
        matches!(err, RuntimeError::RunAlreadyActive { run_id: id } if id == run_id),
        "expected RunAlreadyActive, got {err}"
    );
    assert_eq!(
        host.worker_registry_len(),
        1,
        "rejected duplicate must not remove or replace the original entry"
    );
    assert_eq!(
        host.in_flight_turns(),
        1,
        "duplicate reject must not steal the first worker's slot"
    );

    let host_shutdown = Arc::clone(&host);
    let shutdown_thread = thread::spawn(move || host_shutdown.shutdown());

    // Unblock first arm; shutdown must join the original worker.
    release.wait();
    let first_result = first.join().expect("first spawn thread");
    let shutdown_result = shutdown_thread.join().expect("shutdown thread");
    shutdown_result.expect("shutdown must join the original worker");

    match first_result {
        Ok(s) => drop(s),
        Err(RuntimeError::ShutDown) => {}
        Err(e) => panic!("unexpected first spawn error: {e}"),
    }
    assert!(host.is_fully_idle());
    assert_eq!(host.in_flight_turns(), 0);
    assert_eq!(host.worker_registry_len(), 0);
}
