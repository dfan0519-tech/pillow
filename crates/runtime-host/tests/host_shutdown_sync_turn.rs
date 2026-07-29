//! Synchronous `send_turn` admission linearized with shutdown.
//!
//! Coverage:
//! - shutdown waits for admitted sync turns
//! - sync × shutdown races leave zero slots / admissions
//! - adapter shutdown errors still wait for admitted sync turns

mod common;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{AgentRuntimeAdapter, RunId, RuntimeError, SendTurn, StartSession};
use keeper_runtime_host::{CollectingSink, FakeRuntimeAdapter, HostConfig, RuntimeHost};

use common::{BlockingSyncSendAdapter, BlockingSyncSendShutdownErrorsAdapter};

const SYNC_RACE_ITERS: usize = 32;

/// (1)+(2) Shutdown blocks while an admitted sync send_turn is blocked; after
/// release both finish and the host is fully idle.
#[test]
fn shutdown_waits_for_admitted_synchronous_send_turn() {
    let enter = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let adapter = Arc::new(BlockingSyncSendAdapter::new(
        Arc::clone(&enter),
        Arc::clone(&release),
    ));
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
            conversation_key: "sync-wait".into(),
            working_directory: None,
        })
        .unwrap();
    let session_id = session.session_id;
    let host_send = Arc::clone(&host);
    let send_thread = thread::spawn(move || {
        let mut sink = CollectingSink::default();
        host_send.send_turn(
            SendTurn {
                session_id,
                run_id: RunId::new(),
                content: "sync".into(),
            },
            &mut sink,
        )
    });

    // Sync turn admitted and blocked inside adapter.
    enter.wait();
    assert_eq!(host.sync_turns_in_flight(), 1);
    assert_eq!(host.in_flight_turns(), 1);

    let host_shutdown = Arc::clone(&host);
    let shutdown_thread = thread::spawn(move || host_shutdown.shutdown());

    // Prove shutdown is blocked: give it time; sync turn still holds the slot.
    thread::sleep(Duration::from_millis(30));
    assert_eq!(
        host.sync_turns_in_flight(),
        1,
        "shutdown must not finish while sync turn is admitted"
    );
    assert!(!shutdown_thread.is_finished());

    // Release the sync turn; both sides must complete cleanly.
    release.wait();
    send_thread
        .join()
        .expect("send thread")
        .expect("send_turn must succeed");
    shutdown_thread
        .join()
        .expect("shutdown thread")
        .expect("shutdown must succeed after sync turn releases");
    assert!(host.is_fully_idle());
    assert_eq!(host.worker_registry_len(), 0);
    assert_eq!(host.in_flight_turns(), 0);
    assert_eq!(
        adapter.send_entries.load(Ordering::SeqCst),
        adapter.send_exits.load(Ordering::SeqCst)
    );
}

/// (3)+(4) Sync send_turn racing shutdown is admitted-and-awaited or ShutDown;
/// repeated races leave zero slots and no registry entries.
#[test]
fn synchronous_send_turn_racing_shutdown_is_admitted_or_rejected() {
    for iter in 0..SYNC_RACE_ITERS {
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap());
        let host = Arc::new(RuntimeHost::new(
            adapter,
            HostConfig {
                max_concurrent_turns: 4,
                honor_fixture_delays: false,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "sync-race".into(),
                working_directory: None,
            })
            .unwrap();
        let session_id = session.session_id;

        const SENDERS: usize = 4;
        let parties = SENDERS + 2; // senders + shutdown + driver
        let start = Arc::new(Barrier::new(parties));
        let mut handles = Vec::new();
        for i in 0..SENDERS {
            let host = Arc::clone(&host);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                let mut sink = CollectingSink::default();
                match host.send_turn(
                    SendTurn {
                        session_id,
                        run_id: RunId::new(),
                        content: format!("s{i}"),
                    },
                    &mut sink,
                ) {
                    // Admitted and completed, or rejected at host admission.
                    // SessionNotFound is not a normal shutdown race result:
                    // shutdown waits for admitted sync turns before adapter
                    // teardown clears sessions.
                    Ok(()) | Err(RuntimeError::ShutDown) => Ok(()),
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
                .expect("sync race thread")
                .unwrap_or_else(|e| panic!("iter {iter}: sync/shutdown race: {e}"));
        }
        assert_eq!(
            host.worker_registry_len(),
            0,
            "iter {iter}: registry empty after sync race"
        );
        assert_eq!(
            host.sync_turns_in_flight(),
            0,
            "iter {iter}: no residual sync admissions"
        );
        assert_eq!(
            host.in_flight_turns(),
            0,
            "iter {iter}: zero slots after sync race"
        );
        assert!(
            host.is_fully_idle(),
            "iter {iter}: fully idle after sync race"
        );
    }
}

/// (5) Adapter shutdown error still waits for an admitted synchronous turn.
#[test]
fn adapter_shutdown_error_waits_for_admitted_synchronous_turn() {
    let enter = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let adapter = Arc::new(BlockingSyncSendShutdownErrorsAdapter::new(
        Arc::clone(&enter),
        Arc::clone(&release),
    ));
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
            conversation_key: "sync-err".into(),
            working_directory: None,
        })
        .unwrap();
    let session_id = session.session_id;
    let host_send = Arc::clone(&host);
    let send_thread = thread::spawn(move || {
        let mut sink = CollectingSink::default();
        host_send.send_turn(
            SendTurn {
                session_id,
                run_id: RunId::new(),
                content: "sync".into(),
            },
            &mut sink,
        )
    });

    enter.wait();
    assert_eq!(host.sync_turns_in_flight(), 1);

    let host_shutdown = Arc::clone(&host);
    let shutdown_thread = thread::spawn(move || host_shutdown.shutdown());

    thread::sleep(Duration::from_millis(30));
    assert!(
        !shutdown_thread.is_finished(),
        "shutdown must wait for sync turn even when adapter will error"
    );

    release.wait();
    send_thread
        .join()
        .expect("send thread")
        .expect("send_turn must succeed");
    let err = shutdown_thread
        .join()
        .expect("shutdown thread")
        .expect_err("adapter shutdown error must surface after sync wait");
    assert!(
        matches!(err, RuntimeError::Internal { .. }),
        "expected adapter internal error, got {err}"
    );
    assert!(host.is_fully_idle());
    assert!(adapter.shutdown_calls.load(Ordering::SeqCst) >= 1);
}
