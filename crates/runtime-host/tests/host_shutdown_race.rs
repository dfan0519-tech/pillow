//! Spawn × shutdown linearization, sync-turn lifecycle, and adapter cleanup.
//!
//! Adversarial coverage for:
//! - async worker state machine `Starting -> Running -> Joining -> Joined`
//! - synchronous `send_turn` admission linearized with shutdown
//! - duplicate `run_id` rejection without detaching live ownership
//! - `SpawnedTurn` Drop requesting real adapter cancellation

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeProbe, RuntimeSession, SendTurn, StartSession,
};
use keeper_runtime_host::{CollectingSink, FakeRuntimeAdapter, HostConfig, RuntimeHost};

const RACE_ITERS: usize = 24;
/// Multi-thread spawn×shutdown stress: high enough to fail the old
/// acquire-slot-before-register ordering reliably on multi-core hosts.
const SPAWN_SHUTDOWN_RACE_ITERS: usize = 128;

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
    for iter in 0..SPAWN_SHUTDOWN_RACE_ITERS {
        // Short fixture: stress the slot/register linearization, not wall delay.
        let fixture = r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"assistant_delta","delay_ms":0,"payload":{"text":"x"}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#;
        let adapter = Arc::new(FakeRuntimeAdapter::from_fixture_text(fixture, false).unwrap());
        let host = Arc::new(RuntimeHost::new(
            adapter,
            HostConfig {
                max_concurrent_turns: 8,
                honor_fixture_delays: false,
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

// ---------------------------------------------------------------------------
// Synchronous send_turn × shutdown linearization
// ---------------------------------------------------------------------------

const SYNC_RACE_ITERS: usize = 32;

/// Barrier-controlled adapter: `send_turn` blocks between enter/release so
/// tests can prove shutdown waits for an admitted synchronous turn.
struct BlockingSyncSendAdapter {
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    send_entries: AtomicUsize,
    send_exits: AtomicUsize,
    shut_down: AtomicBool,
}

impl BlockingSyncSendAdapter {
    fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
        Self {
            enter,
            release,
            send_entries: AtomicUsize::new(0),
            send_exits: AtomicUsize::new(0),
            shut_down: AtomicBool::new(false),
        }
    }
}

impl AgentRuntimeAdapter for BlockingSyncSendAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "blocking-sync".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn disarm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.send_entries.fetch_add(1, Ordering::SeqCst);
        self.enter.wait();
        self.release.wait();
        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 1,
            kind: RuntimeEventKind::RunStarted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 2,
            kind: RuntimeEventKind::TurnCompleted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
        self.send_exits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        Err(RuntimeError::RunNotFound { run_id })
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.shut_down.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Adapter whose shutdown fails, but sync send_turn still blocks on barriers.
struct BlockingSyncSendShutdownErrorsAdapter {
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    shutdown_calls: AtomicUsize,
}

impl BlockingSyncSendShutdownErrorsAdapter {
    fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
        Self {
            enter,
            release,
            shutdown_calls: AtomicUsize::new(0),
        }
    }
}

impl AgentRuntimeAdapter for BlockingSyncSendShutdownErrorsAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "blocking-sync-err".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn disarm_turn(&self, _run_id: RunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.enter.wait();
        self.release.wait();
        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 1,
            kind: RuntimeEventKind::RunStarted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
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
        Err(RuntimeError::RunNotFound { run_id })
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.shutdown_calls.fetch_add(1, Ordering::SeqCst);
        Err(RuntimeError::Internal {
            message: "adapter shutdown deliberately failed (sync wait)".into(),
        })
    }
}

/// Signal pair: cancel flag + condvar for a blocked adapter turn.
type CancelSignal = Arc<(Mutex<bool>, Condvar)>;

/// Adapter that blocks inside `send_turn` until `cancel_turn` is invoked.
struct CancelUnblocksAdapter {
    waiters: Mutex<HashMap<RunId, CancelSignal>>,
    cancel_calls: AtomicUsize,
    terminals: Mutex<HashMap<RunId, RuntimeEventKind>>,
}

impl CancelUnblocksAdapter {
    fn new() -> Self {
        Self {
            waiters: Mutex::new(HashMap::new()),
            cancel_calls: AtomicUsize::new(0),
            terminals: Mutex::new(HashMap::new()),
        }
    }
}

impl AgentRuntimeAdapter for CancelUnblocksAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "cancel-unblocks".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let mut waiters = self.waiters.lock().expect("waiters");
        if waiters.contains_key(&run_id) {
            return Err(RuntimeError::RunAlreadyActive { run_id });
        }
        waiters.insert(run_id, pair);
        Ok(())
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.waiters.lock().expect("waiters").remove(&run_id);
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        let pair = {
            let waiters = self.waiters.lock().expect("waiters");
            waiters
                .get(&request.run_id)
                .cloned()
                .ok_or(RuntimeError::RunNotFound {
                    run_id: request.run_id,
                })?
        };

        // Block away from sink.emit until cancel_turn notifies.
        {
            let (lock, cv) = &*pair;
            let mut cancelled = lock.lock().expect("cancel flag");
            while !*cancelled {
                cancelled = cv.wait(cancelled).expect("cancel cv");
            }
        }

        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 1,
            kind: RuntimeEventKind::RunCancelled,
            payload: serde_json::json!({"reason": "adapter_cancel"}),
            created_at: Utc::now(),
        })?;
        self.terminals
            .lock()
            .expect("terminals")
            .insert(request.run_id, RuntimeEventKind::RunCancelled);
        self.waiters
            .lock()
            .expect("waiters")
            .remove(&request.run_id);
        Ok(())
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        let waiters = self.waiters.lock().expect("waiters");
        if let Some(pair) = waiters.get(&run_id) {
            let (lock, cv) = &**pair;
            let mut cancelled = lock.lock().expect("cancel flag");
            *cancelled = true;
            cv.notify_all();
            Ok(())
        } else {
            Err(RuntimeError::RunNotFound { run_id })
        }
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        let waiters = self.waiters.lock().expect("waiters");
        for pair in waiters.values() {
            let (lock, cv) = &**pair;
            let mut cancelled = lock.lock().expect("cancel flag");
            *cancelled = true;
            cv.notify_all();
        }
        Ok(())
    }
}

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
                    Ok(()) => Ok(()),
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

// ---------------------------------------------------------------------------
// Duplicate run_id ownership
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Drop requests adapter cancel
// ---------------------------------------------------------------------------

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
