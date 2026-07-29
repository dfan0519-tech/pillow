//! Spawn × shutdown linearization and adapter-shutdown error cleanup.
//!
//! Adversarial coverage for the explicit worker state machine:
//! `Starting -> Running -> Joining -> Joined`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeProbe, RuntimeSession, SendTurn,
    StartSession,
};
use keeper_runtime_host::{FakeRuntimeAdapter, HostConfig, RuntimeHost};

const RACE_ITERS: usize = 24;

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
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: true,
            event_queue_capacity: 8,
            after_worker_registered: Some(Arc::new(move || {
                enter_h.wait();
                release_h.wait();
            })),
        },
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
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: false,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
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
    for _ in 0..RACE_ITERS {
        let slow = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":50,"payload":{"text":"x"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(slow, true).unwrap());
        let host = Arc::new(RuntimeHost::new(
            adapter,
            HostConfig {
                max_concurrent_turns: 8,
                honor_fixture_delays: true,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
        ));
        let session = host
            .start_session(StartSession {
                conversation_key: "race".into(),
                working_directory: None,
            })
            .unwrap();
        let session_id = session.session_id;

        let start = Arc::new(Barrier::new(5));
        let mut handles = Vec::new();
        for i in 0..4 {
            let host = Arc::clone(&host);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                if i == 3 {
                    host.shutdown()
                } else {
                    match host.spawn_turn(SendTurn {
                        session_id,
                        run_id: RunId::new(),
                        content: format!("r{i}"),
                    }) {
                        Ok(s) => {
                            drop(s);
                            Ok(())
                        }
                        Err(RuntimeError::ShutDown) => Ok(()),
                        Err(e) => Err(e),
                    }
                }
            }));
        }
        start.wait();
        for h in handles {
            h.join().expect("race thread").expect("spawn/shutdown race");
        }
        assert_eq!(
            host.in_flight_turns(),
            0,
            "all workers must be joined after spawn×shutdown race"
        );
        let err = match host.spawn_turn(SendTurn {
            session_id,
            run_id: RunId::new(),
            content: "after".into(),
        }) {
            Ok(_) => panic!("spawn after shutdown must fail"),
            Err(e) => e,
        };
        assert!(matches!(err, RuntimeError::ShutDown));
    }
}

struct BlockingArmAdapter {
    inner: FakeRuntimeAdapter,
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    arm_entries: AtomicUsize,
    arm_exits: AtomicUsize,
}

impl BlockingArmAdapter {
    fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        Self {
            inner: FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap(),
            enter,
            release,
            arm_entries: AtomicUsize::new(0),
            arm_exits: AtomicUsize::new(0),
        }
    }
}

impl AgentRuntimeAdapter for BlockingArmAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.inner.probe()
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.inner.start_session(request)
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.arm_entries.fetch_add(1, Ordering::SeqCst);
        self.enter.wait();
        self.release.wait();
        let result = self.inner.arm_turn(run_id);
        self.arm_exits.fetch_add(1, Ordering::SeqCst);
        result
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.disarm_turn(run_id)
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.inner.send_turn(request, sink)
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.cancel_turn(run_id)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.inner.shutdown()
    }
}

struct ShutdownErrorsAdapter {
    inner: FakeRuntimeAdapter,
    shutdown_calls: AtomicUsize,
}

impl ShutdownErrorsAdapter {
    fn new() -> Self {
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":80,"payload":{"text":"x"}}
{"kind":"assistant_delta","delay_ms":80,"payload":{"text":"y"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        Self {
            inner: FakeRuntimeAdapter::from_fixture_text(fixture, true).unwrap(),
            shutdown_calls: AtomicUsize::new(0),
        }
    }
}

impl AgentRuntimeAdapter for ShutdownErrorsAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.inner.probe()
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.inner.start_session(request)
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.arm_turn(run_id)
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.disarm_turn(run_id)
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.inner.send_turn(request, sink)
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.cancel_turn(run_id)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.shutdown_calls.fetch_add(1, Ordering::SeqCst);
        let _ = self.inner.shutdown();
        Err(RuntimeError::Internal {
            message: "adapter shutdown deliberately failed".into(),
        })
    }
}

#[test]
fn host_joins_workers_even_when_adapter_shutdown_errors() {
    let adapter = Arc::new(ShutdownErrorsAdapter::new());
    let host = Arc::new(RuntimeHost::new(
        adapter.clone() as Arc<dyn AgentRuntimeAdapter>,
        HostConfig {
            max_concurrent_turns: 2,
            honor_fixture_delays: true,
            event_queue_capacity: 8,
            after_worker_registered: None,
        },
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
            HostConfig {
                max_concurrent_turns: 2,
                honor_fixture_delays: true,
                event_queue_capacity: 8,
                after_worker_registered: None,
            },
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
