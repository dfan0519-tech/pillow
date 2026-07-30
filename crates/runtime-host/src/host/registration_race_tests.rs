//! Deterministic race coverage for the post-registration test seam.
//!
//! Lives as a unit test so `HostConfig::with_after_worker_registered` is
//! available via `cfg(test)` without shipping that API in release builds.

use std::sync::{Arc, Barrier};
use std::thread;

use keeper_agent_protocol::{RunId, RuntimeError, SendTurn, StartSession};

use crate::fake::FakeRuntimeAdapter;
use crate::host::{HostConfig, RuntimeHost};

/// Worker is registered (`Starting`) before the join handle is installed;
/// shutdown must wait and join (or the spawn is rejected cleanly).
#[test]
fn shutdown_joins_worker_still_in_starting_state() {
    let enter = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let enter_h = Arc::clone(&enter);
    let release_h = Arc::clone(&release);

    let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":100,"payload":{"text":"x"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
    let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
    let host = Arc::new(RuntimeHost::new(
        adapter,
        HostConfig::new(2, true, 8).with_after_worker_registered(Arc::new(move || {
            enter_h.wait();
            release_h.wait();
        })),
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "starting".into(),
            working_directory: None,
        })
        .unwrap();
    let session_id = session.session_id;
    let host_spawn = Arc::clone(&host);
    let spawner = thread::spawn(move || {
        host_spawn.spawn_turn(SendTurn {
            session_id,
            run_id: RunId::new(),
            content: "race".into(),
        })
    });

    // Worker registered; join handle not installed yet.
    enter.wait();

    let host_shutdown = Arc::clone(&host);
    let shutdown_thread = thread::spawn(move || host_shutdown.shutdown());

    // Allow spawn to install the handle; shutdown must wait and join it.
    release.wait();

    let spawned = spawner.join().expect("spawn thread");
    let shutdown_result = shutdown_thread.join().expect("shutdown thread");
    shutdown_result.expect("shutdown must succeed after joining Starting worker");
    assert_eq!(host.in_flight_turns(), 0);

    match spawned {
        Ok(s) => {
            assert!(s.is_finished());
            drop(s);
        }
        Err(RuntimeError::ShutDown) => {}
        Err(e) => panic!("unexpected spawn error: {e}"),
    }
}
