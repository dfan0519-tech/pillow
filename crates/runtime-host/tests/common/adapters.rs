//! Adversarial / barrier-controlled adapters for host concurrency tests.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex};

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeProbe, RuntimeSession, SendTurn, StartSession,
};
use keeper_runtime_host::FakeRuntimeAdapter;

/// Blocks inside `arm_turn` between enter/release barriers.
pub struct BlockingArmAdapter {
    inner: FakeRuntimeAdapter,
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    pub arm_entries: AtomicUsize,
    pub arm_exits: AtomicUsize,
}

impl BlockingArmAdapter {
    pub fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
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

/// Adapter whose `shutdown` always fails after calling the inner fake.
pub struct ShutdownErrorsAdapter {
    inner: FakeRuntimeAdapter,
    pub shutdown_calls: AtomicUsize,
}

impl ShutdownErrorsAdapter {
    pub fn new() -> Self {
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

impl Default for ShutdownErrorsAdapter {
    fn default() -> Self {
        Self::new()
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

/// Barrier-controlled adapter: `send_turn` blocks between enter/release so
/// tests can prove shutdown waits for an admitted synchronous turn.
pub struct BlockingSyncSendAdapter {
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    pub send_entries: AtomicUsize,
    pub send_exits: AtomicUsize,
    shut_down: AtomicBool,
}

impl BlockingSyncSendAdapter {
    pub fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
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
pub struct BlockingSyncSendShutdownErrorsAdapter {
    enter: Arc<Barrier>,
    release: Arc<Barrier>,
    pub shutdown_calls: AtomicUsize,
}

impl BlockingSyncSendShutdownErrorsAdapter {
    pub fn new(enter: Arc<Barrier>, release: Arc<Barrier>) -> Self {
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
pub struct CancelUnblocksAdapter {
    waiters: Mutex<HashMap<RunId, CancelSignal>>,
    pub cancel_calls: AtomicUsize,
    pub terminals: Mutex<HashMap<RunId, RuntimeEventKind>>,
}

impl CancelUnblocksAdapter {
    pub fn new() -> Self {
        Self {
            waiters: Mutex::new(HashMap::new()),
            cancel_calls: AtomicUsize::new(0),
            terminals: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for CancelUnblocksAdapter {
    fn default() -> Self {
        Self::new()
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
