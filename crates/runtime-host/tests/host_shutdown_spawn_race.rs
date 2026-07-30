//! Async spawn × shutdown linearization and adapter-error join behavior.
//!
//! Coverage:
//! - worker state machine `Starting -> Running -> Joining -> Joined`
//! - spawn/shutdown races leave no residual slots or registry entries
//! - host joins workers even when `adapter.shutdown()` errors

mod common;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeError, SendTurn, StartSession};
use keeper_runtime_host::{FakeRuntimeAdapter, HostConfig, RuntimeHost};

use common::{BlockingArmAdapter, ShutdownErrorsAdapter};

const RACE_ITERS: usize = 24;
/// Multi-thread spawn×shutdown stress: high enough to fail the old
/// acquire-slot-before-register ordering reliably on multi-core hosts.
const SPAWN_SHUTDOWN_RACE_ITERS: usize = 128;

// `shutdown_joins_worker_still_in_starting_state` lives as a unit test under
// `src/host/registration_race_tests.rs` so the registration seam stays off
// the release API while race coverage is preserved.

/// Barrier inside blocking `arm_turn`: shutdown must wait for Starting or the
/// spawn must be rejected with no residual work.
#[test]
fn shutdown_waits_for_worker_blocked_in_arm_turn() {
    for _ in 0..RACE_ITERS {
        let enter = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let adapter = Arc::new(BlockingArmAdapter::new(
            Arc::clone(&enter),
            Arc::clone(&release),
        ));
        let host = Arc::new(RuntimeHost::new(
            adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
            HostConfig::new(2, false, 8),
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "arm-block".into(),
                working_directory: None,
            })
            .unwrap();
        let session_id = session.session_id;
        let host_spawn = Arc::clone(&host);
        let spawner = thread::spawn(move || {
            host_spawn.spawn_turn(SendTurn {
                session_id,
                run_id: RunId::new(),
                content: "arm".into(),
            })
        });

        // arm_turn entered — worker must already be registered as Starting.
        enter.wait();
        assert_eq!(
            host.in_flight_turns(),
            1,
            "spawn slot held while arm_turn blocks"
        );

        let host_shutdown = Arc::clone(&host);
        let shutdown_thread = thread::spawn(move || host_shutdown.shutdown());

        // Unblock arm; shutdown must observe Starting and finish cleanly.
        release.wait();

        let spawned = spawner.join().expect("spawn thread");
        let shutdown_result = shutdown_thread.join().expect("shutdown thread");
        // Adapter shutdown succeeds; host cleanup always completes first.
        shutdown_result.expect("shutdown must complete after arm_turn race");
        assert_eq!(host.in_flight_turns(), 0, "no residual slot after arm race");

        match spawned {
            Ok(s) => {
                assert!(s.is_finished());
                drop(s);
            }
            Err(RuntimeError::ShutDown) => {}
            Err(e) => panic!("unexpected spawn error: {e}"),
        }
        assert_eq!(
            adapter.arm_entries.load(Ordering::SeqCst),
            adapter.arm_exits.load(Ordering::SeqCst),
            "no residual arm_turn work"
        );
    }
}

#[test]
fn spawn_racing_shutdown_is_rejected_or_fully_joined() {
    for iter in 0..SPAWN_SHUTDOWN_RACE_ITERS {
        // Short fixture: stress the slot/register linearization, not wall delay.
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":0,"payload":{"text":"x"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap());
        let host = Arc::new(RuntimeHost::new(adapter, HostConfig::new(8, false, 8)));
        let session = host
            .start_session(StartSession {
                conversation_key: "race".into(),
                working_directory: None,
            })
            .unwrap();
        let session_id = session.session_id;

        // 8 spawns + 1 shutdown racing from a shared barrier.
        const SPAWNERS: usize = 8;
        let parties = SPAWNERS + 2; // spawns + shutdown + driver
        let start = Arc::new(Barrier::new(parties));
        let mut handles = Vec::new();
        for i in 0..SPAWNERS {
            let host = Arc::clone(&host);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                match host.spawn_turn(SendTurn {
                    session_id,
                    run_id: RunId::new(),
                    content: format!("r{i}"),
                }) {
                    Ok(s) => {
                        // Accepted: fully joined via Drop (or already finished).
                        drop(s);
                        Ok(())
                    }
                    Err(RuntimeError::ShutDown) => Ok(()),
                    Err(e) => Err(e),
                }
            }));
        }
        {
            let host = Arc::clone(&host);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                host.shutdown()
            }));
        }
        start.wait();
        for h in handles {
            h.join()
                .expect("race thread")
                .unwrap_or_else(|e| panic!("iter {iter}: spawn/shutdown race: {e}"));
        }
        assert_eq!(
            host.worker_registry_len(),
            0,
            "iter {iter}: worker registry must be empty after race"
        );
        assert_eq!(
            host.workers_holding_slot_permits(),
            0,
            "iter {iter}: no worker may still own an async slot permit"
        );
        assert_eq!(
            host.in_flight_turns(),
            0,
            "iter {iter}: in_flight must be 0 after every accepted/rejected race"
        );
        assert!(
            host.is_fully_idle(),
            "iter {iter}: empty registry must imply no turn-slot permits remain"
        );
        let err = match host.spawn_turn(SendTurn {
            session_id,
            run_id: RunId::new(),
            content: "after".into(),
        }) {
            Ok(_) => panic!("iter {iter}: spawn after shutdown must fail"),
            Err(e) => e,
        };
        assert!(
            matches!(err, RuntimeError::ShutDown),
            "iter {iter}: expected ShutDown after race, got {err}"
        );
        assert!(
            host.is_fully_idle(),
            "iter {iter}: post-shutdown spawn rejection must leave host fully idle"
        );
    }
}

#[test]
fn host_joins_workers_even_when_adapter_shutdown_errors() {
    let adapter = Arc::new(ShutdownErrorsAdapter::new());
    let host = Arc::new(RuntimeHost::new(
        adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
        HostConfig::new(2, true, 8),
    ));
    let session = host
        .start_session(StartSession {
            conversation_key: "err-shutdown".into(),
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

    let err = host.shutdown().expect_err("adapter shutdown must surface");
    assert!(
        matches!(err, RuntimeError::Internal { .. }),
        "expected adapter internal error, got {err}"
    );
    assert!(
        spawned.is_finished(),
        "host must join workers before returning even when adapter.shutdown errors"
    );
    assert_eq!(host.in_flight_turns(), 0);
    assert!(adapter.shutdown_calls.load(Ordering::SeqCst) >= 1);
    drop(spawned);
}

/// Concurrent external join + shutdown where adapter shutdown errors: shutdown
/// must not return until the external join finishes and the worker is Joined.
#[test]
fn shutdown_waits_for_external_join_while_adapter_errors() {
    for _ in 0..RACE_ITERS {
        let adapter = Arc::new(ShutdownErrorsAdapter::new());
        let host = Arc::new(RuntimeHost::new(
            adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
            HostConfig::new(2, true, 8),
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "join-shutdown".into(),
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

        // Ensure the worker is Running before racing join with shutdown.
        for _ in 0..200 {
            if !spawned.events_snapshot().is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        let host_shutdown = Arc::clone(&host);
        // Three parties: join thread, shutdown thread, and this driver.
        let start = Arc::new(Barrier::new(3));
        let start_j = Arc::clone(&start);
        let start_s = Arc::clone(&start);

        let join_thread = thread::spawn(move || {
            start_j.wait();
            spawned.join()
        });
        let shutdown_thread = thread::spawn(move || {
            start_s.wait();
            host_shutdown.shutdown()
        });

        start.wait();

        let join_result = join_thread.join().expect("join thread");
        let shutdown_result = shutdown_thread.join().expect("shutdown thread");

        // External join may observe cancel or completion; must not hang.
        let _ = join_result;
        let err = shutdown_result.expect_err("adapter shutdown must surface after host join");
        assert!(
            matches!(err, RuntimeError::Internal { .. }),
            "expected adapter internal error after full host cleanup, got {err}"
        );
        assert_eq!(host.in_flight_turns(), 0);
        assert!(adapter.shutdown_calls.load(Ordering::SeqCst) >= 1);
    }
}
