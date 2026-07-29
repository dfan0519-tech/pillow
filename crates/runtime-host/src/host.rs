//! Bounded runtime host: concurrency limits, protocol guard, worker lifecycle.
//!
//! Spawn registration, cancel, and join are linearized through the host worker
//! map and each [`WorkerRecord`]'s phase machine (`Starting -> Running ->
//! Joining -> Joined`). There is no unregistered in-flight arm window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};

use crate::delivery::DEFAULT_EVENT_QUEUE_CAPACITY;
use crate::fake::FakeRuntimeAdapter;
use crate::sink::ProtocolGuardSink;
use crate::worker::{WorkerRecord, run_turn_worker};

/// Default concurrent turn limit for the host (process-pool style bound).
pub const DEFAULT_MAX_CONCURRENT_TURNS: u32 = 4;
const MAX_CONCURRENT_TURNS_CAP: u32 = 32;

/// Host configuration.
///
/// `after_worker_registered` is a test seam used to force the
/// register-before-handle race with barriers. Production callers leave it
/// `None`.
#[derive(Clone)]
pub struct HostConfig {
    pub max_concurrent_turns: u32,
    /// When true, fixture `delay_ms` is honored; when false, delays are zeroed.
    pub honor_fixture_delays: bool,
    /// Per-turn bounded event queue capacity (backpressure when full).
    pub event_queue_capacity: usize,
    /// Test seam: invoked after the worker is registered (`Starting`) and
    /// before `arm_turn` / thread install. Default `None`.
    #[doc(hidden)]
    pub after_worker_registered: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_concurrent_turns: DEFAULT_MAX_CONCURRENT_TURNS,
            honor_fixture_delays: true,
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            after_worker_registered: None,
        }
    }
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostConfig")
            .field("max_concurrent_turns", &self.max_concurrent_turns)
            .field("honor_fixture_delays", &self.honor_fixture_delays)
            .field("event_queue_capacity", &self.event_queue_capacity)
            .field(
                "after_worker_registered",
                &self.after_worker_registered.is_some(),
            )
            .finish()
    }
}

pub fn normalize_max_concurrent_turns(raw: u32) -> u32 {
    raw.clamp(1, MAX_CONCURRENT_TURNS_CAP)
}

/// Host that bounds concurrency and enforces the provider-neutral protocol.
pub struct RuntimeHost {
    adapter: Arc<dyn AgentRuntimeAdapter>,
    config: HostConfig,
    in_flight: AtomicU32,
    shut_down: AtomicBool,
    workers: Mutex<HashMap<RunId, Arc<WorkerRecord>>>,
}

impl RuntimeHost {
    pub fn new(adapter: Arc<dyn AgentRuntimeAdapter>, config: HostConfig) -> Self {
        Self {
            adapter,
            config: HostConfig {
                max_concurrent_turns: normalize_max_concurrent_turns(config.max_concurrent_turns),
                honor_fixture_delays: config.honor_fixture_delays,
                event_queue_capacity: config.event_queue_capacity.max(1),
                after_worker_registered: config.after_worker_registered,
            },
            in_flight: AtomicU32::new(0),
            shut_down: AtomicBool::new(false),
            workers: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_fake(config: HostConfig) -> Result<Self, RuntimeError> {
        let adapter = FakeRuntimeAdapter::from_committed_fixture(config.honor_fixture_delays)?;
        Ok(Self::new(Arc::new(adapter), config))
    }

    pub fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.ensure_alive()?;
        self.adapter.probe()
    }

    pub fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.ensure_alive()?;
        self.adapter.start_session(request)
    }

    pub fn send_turn(
        &self,
        request: SendTurn,
        sink: &mut dyn EventSink,
    ) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        self.acquire_turn_slot()?;
        let run_id = request.run_id;
        let result = (|| {
            let mut guarded = ProtocolGuardSink::new(run_id, sink);
            self.adapter.send_turn(request, &mut guarded)?;
            guarded.finish()?;
            Ok(())
        })();
        self.release_turn_slot();
        result
    }

    /// Run a turn on a background thread; cancellable as soon as this returns.
    ///
    /// The worker is registered in `Starting` **before** any blocking
    /// `arm_turn` work. Shutdown either waits for that record or the spawn is
    /// rejected before registration — there is no unregistered in-flight slot.
    pub fn spawn_turn(self: &Arc<Self>, request: SendTurn) -> Result<SpawnedTurn, RuntimeError> {
        self.ensure_alive()?;
        self.acquire_turn_slot()?;

        let run_id = request.run_id;
        let record = WorkerRecord::new(self.config.event_queue_capacity);

        // Linearize with shutdown: register Starting before arm_turn/spawn.
        {
            let mut workers = self.workers.lock().expect("workers");
            if self.shut_down.load(Ordering::SeqCst) {
                drop(workers);
                self.release_turn_slot();
                return Err(RuntimeError::ShutDown);
            }
            workers.insert(run_id, Arc::clone(&record));
        }

        if let Some(hook) = self.config.after_worker_registered.as_ref() {
            hook();
        }

        // arm_turn may block; shutdown observes Starting and waits.
        if let Err(e) = self.adapter.arm_turn(run_id) {
            self.fail_spawn_cleanup(run_id, &record);
            return Err(e);
        }

        // Shutdown may have begun during arm; reject without residual work.
        if self.shut_down.load(Ordering::SeqCst) {
            let _ = self.adapter.disarm_turn(run_id);
            self.fail_spawn_cleanup(run_id, &record);
            return Err(RuntimeError::ShutDown);
        }

        let host = Arc::clone(self);
        let delivery_for_thread = Arc::clone(&record.delivery);
        let cancel_flag = Arc::clone(&record.cancel);
        let record_for_thread = Arc::clone(&record);

        let join = thread::Builder::new()
            .name(format!("keeper-turn-{run_id}"))
            .spawn(move || {
                let result = run_turn_worker(
                    &host.adapter,
                    request,
                    run_id,
                    &cancel_flag,
                    &delivery_for_thread,
                );
                delivery_for_thread.close();
                host.release_turn_slot();
                record_for_thread.mark_body_finished();
                result
            });

        let join = match join {
            Ok(handle) => handle,
            Err(e) => {
                let _ = self.adapter.disarm_turn(run_id);
                self.fail_spawn_cleanup(run_id, &record);
                return Err(RuntimeError::Internal {
                    message: format!("spawn turn worker: {e}"),
                });
            }
        };

        record.install_running(join);

        // Late shutdown: worker is Running and will be joined by shutdown.
        if self.shut_down.load(Ordering::SeqCst) {
            // Still return the handle so the caller can observe cancel; the
            // worker is fully owned. Callers that race shutdown may also see
            // ShutDown only on the pre-register / pre-thread paths above.
        }

        Ok(SpawnedTurn {
            run_id,
            host: Arc::clone(self),
            record,
        })
    }

    pub fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        let known = {
            let workers = self.workers.lock().expect("workers");
            if let Some(w) = workers.get(&run_id) {
                // Atomic with terminal commit: delivery cancel + host flag.
                w.request_cancel();
                true
            } else {
                false
            }
        };
        match self.adapter.cancel_turn(run_id) {
            Ok(()) => Ok(()),
            // Map RunNotFound → Ok only for host-owned workers, where the host
            // cancel flag already makes cancel real. Never invent success for
            // an unknown run.
            Err(RuntimeError::RunNotFound { .. }) if known => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Shut down the adapter and join all host-owned workers before returning.
    ///
    /// Host workers in `Starting`, `Running`, or `Joining` are waited on even
    /// when `adapter.shutdown()` returns an error. The adapter error is
    /// returned only after host cleanup is complete.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let already = self.shut_down.swap(true, Ordering::SeqCst);

        {
            let workers = self.workers.lock().expect("workers");
            for (run_id, worker) in workers.iter() {
                worker.request_cancel();
                let _ = self.adapter.cancel_turn(*run_id);
                worker.delivery.close();
            }
        }

        let adapter_result = self.adapter.shutdown();
        // Join host workers even when the adapter reports failure.
        self.join_all_workers();
        adapter_result?;
        if already {
            return Ok(());
        }
        if self.in_flight.load(Ordering::SeqCst) != 0 {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {} in-flight turn slot(s)",
                    self.in_flight.load(Ordering::SeqCst)
                ),
            });
        }
        Ok(())
    }

    pub fn in_flight_turns(&self) -> u32 {
        self.in_flight.load(Ordering::SeqCst)
    }

    pub fn event_queue_capacity(&self) -> usize {
        self.config.event_queue_capacity
    }

    /// Wait until every registered worker is truly `Joined`.
    fn join_all_workers(&self) {
        loop {
            let batch: Vec<(RunId, Arc<WorkerRecord>)> = {
                let workers = self.workers.lock().expect("workers");
                if workers.is_empty() {
                    return;
                }
                workers.iter().map(|(id, w)| (*id, Arc::clone(w))).collect()
            };

            for (_id, worker) in &batch {
                let mut discarded = Vec::new();
                let _ = worker.await_joined(&mut discarded);
            }

            let mut workers = self.workers.lock().expect("workers");
            for (id, _) in batch {
                workers.remove(&id);
            }
            if workers.is_empty() {
                return;
            }
            // A spawn may have inserted during join; loop until the map drains.
        }
    }

    fn fail_spawn_cleanup(&self, run_id: RunId, record: &WorkerRecord) {
        record.request_cancel();
        record.delivery.close();
        record.abandon_starting();
        {
            let mut workers = self.workers.lock().expect("workers");
            workers.remove(&run_id);
        }
        self.release_turn_slot();
    }

    fn ensure_alive(&self) -> Result<(), RuntimeError> {
        if self.shut_down.load(Ordering::SeqCst) {
            Err(RuntimeError::ShutDown)
        } else {
            Ok(())
        }
    }

    fn acquire_turn_slot(&self) -> Result<(), RuntimeError> {
        let max = self.config.max_concurrent_turns;
        loop {
            let current = self.in_flight.load(Ordering::SeqCst);
            if current >= max {
                return Err(RuntimeError::ConcurrencyLimit { max });
            }
            if self
                .in_flight
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    fn release_turn_slot(&self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Handle for a turn running on a host worker thread.
pub struct SpawnedTurn {
    pub run_id: RunId,
    host: Arc<RuntimeHost>,
    record: Arc<WorkerRecord>,
}

impl SpawnedTurn {
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        self.host.cancel_turn(self.run_id)
    }

    pub fn is_finished(&self) -> bool {
        self.record.is_body_finished()
    }

    /// Snapshot of events currently buffered (not yet consumed).
    pub fn events_snapshot(&self) -> Vec<RuntimeEvent> {
        self.record.delivery.snapshot()
    }

    /// Delivery queue for streaming consumption / backpressure tests.
    pub fn delivery(&self) -> &Arc<crate::delivery::BoundedDelivery> {
        &self.record.delivery
    }

    pub fn join(self) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let mut events = Vec::new();
        let join_result = self.record.await_joined(&mut events);
        {
            let mut workers = self.host.workers.lock().expect("workers");
            workers.remove(&self.run_id);
        }
        join_result?;
        Ok(events)
    }
}

impl Drop for SpawnedTurn {
    fn drop(&mut self) {
        if !self.record.is_joined() {
            self.record.request_cancel();
            let mut discarded = Vec::new();
            let _ = self.record.await_joined(&mut discarded);
        }
        let mut workers = self.host.workers.lock().expect("workers");
        workers.remove(&self.run_id);
    }
}

/// Resolve the workspace fixture path for tests that prefer filesystem load.
pub fn workspace_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/protocol/fake-turn.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_concurrent_turns_clamps() {
        assert_eq!(normalize_max_concurrent_turns(0), 1);
        assert_eq!(normalize_max_concurrent_turns(4), 4);
        assert_eq!(normalize_max_concurrent_turns(99), MAX_CONCURRENT_TURNS_CAP);
    }
}
