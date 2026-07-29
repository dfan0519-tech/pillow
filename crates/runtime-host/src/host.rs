//! Bounded runtime host: concurrency limits, protocol guard, worker lifecycle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};

use crate::delivery::{BoundedDelivery, DEFAULT_EVENT_QUEUE_CAPACITY};
use crate::fake::FakeRuntimeAdapter;
use crate::sink::ProtocolGuardSink;
use crate::worker::{
    JoinSlot, WorkerRecord, await_worker_slot, join_handle_draining, run_turn_worker,
};

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
    /// Test seam: invoked after the worker is registered and before the join
    /// handle is installed. Default `None`.
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
    /// Host cancel is enforced at the event ingress regardless of whether the
    /// adapter has finished registering the run. A spawn that races shutdown is
    /// either rejected with [`RuntimeError::ShutDown`] or fully owned and joined
    /// by shutdown.
    pub fn spawn_turn(self: &Arc<Self>, request: SendTurn) -> Result<SpawnedTurn, RuntimeError> {
        self.ensure_alive()?;
        self.acquire_turn_slot()?;

        let run_id = request.run_id;
        if let Err(e) = self.adapter.arm_turn(run_id) {
            self.release_turn_slot();
            return Err(e);
        }

        let delivery = BoundedDelivery::new(self.config.event_queue_capacity);
        let finished = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let record = Arc::new(WorkerRecord {
            cancel: Arc::clone(&cancel),
            finished: Arc::clone(&finished),
            delivery: Arc::clone(&delivery),
            join: Mutex::new(JoinSlot::Starting),
        });

        // Linearize with shutdown: reject if already shutting down; otherwise
        // publish the registry entry before the join handle exists.
        {
            let mut workers = self.workers.lock().expect("workers");
            if self.shut_down.load(Ordering::SeqCst) {
                drop(workers);
                let _ = self.adapter.disarm_turn(run_id);
                delivery.close();
                self.release_turn_slot();
                return Err(RuntimeError::ShutDown);
            }
            workers.insert(run_id, Arc::clone(&record));
        }

        if let Some(hook) = self.config.after_worker_registered.as_ref() {
            hook();
        }

        let host = Arc::clone(self);
        let delivery_for_thread = Arc::clone(&delivery);
        let finished_flag = Arc::clone(&finished);
        let cancel_flag = Arc::clone(&cancel);

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
                finished_flag.store(true, Ordering::SeqCst);
                result
            });

        let join = match join {
            Ok(handle) => handle,
            Err(e) => {
                let _ = self.adapter.disarm_turn(run_id);
                {
                    let mut workers = self.workers.lock().expect("workers");
                    workers.remove(&run_id);
                }
                delivery.close();
                self.release_turn_slot();
                return Err(RuntimeError::Internal {
                    message: format!("spawn turn worker: {e}"),
                });
            }
        };

        *record.join.lock().expect("join") = JoinSlot::Running(join);

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
                // Host cancel is the source of truth: ingress forces
                // `run_cancelled` so the worker cannot complete with
                // `turn_completed` after this store.
                w.cancel.store(true, Ordering::SeqCst);
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
    /// Host workers are joined even when `adapter.shutdown()` returns an error.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        let already = self.shut_down.swap(true, Ordering::SeqCst);

        {
            let workers = self.workers.lock().expect("workers");
            for (run_id, worker) in workers.iter() {
                worker.cancel.store(true, Ordering::SeqCst);
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

    /// Join every host-owned worker, including those still in `Starting`.
    fn join_all_workers(&self) {
        loop {
            let mut handles = Vec::new();
            let mut starting = 0usize;
            {
                let workers = self.workers.lock().expect("workers");
                if workers.is_empty() {
                    return;
                }
                for worker in workers.values() {
                    let mut slot = worker.join.lock().expect("join");
                    match std::mem::replace(&mut *slot, JoinSlot::Joined) {
                        JoinSlot::Running(handle) => handles.push(handle),
                        JoinSlot::Starting => {
                            *slot = JoinSlot::Starting;
                            starting += 1;
                        }
                        JoinSlot::Joined => {}
                    }
                }
            }

            if handles.is_empty() && starting == 0 {
                self.workers.lock().expect("workers").clear();
                return;
            }

            for handle in handles {
                // Deliveries were closed in shutdown; producers unblock.
                let _ = handle.join();
            }

            if starting > 0 {
                thread::yield_now();
                thread::sleep(Duration::from_millis(1));
            }
        }
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
        self.record.finished.load(Ordering::SeqCst)
    }

    /// Snapshot of events currently buffered (not yet consumed).
    pub fn events_snapshot(&self) -> Vec<RuntimeEvent> {
        self.record.delivery.snapshot()
    }

    /// Delivery queue for streaming consumption / backpressure tests.
    pub fn delivery(&self) -> &Arc<BoundedDelivery> {
        &self.record.delivery
    }

    pub fn join(self) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let mut events = Vec::new();
        let join_result = await_worker_slot(&self.record, &mut events);
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
        let handle = {
            let mut slot = self.record.join.lock().expect("join");
            match std::mem::replace(&mut *slot, JoinSlot::Joined) {
                JoinSlot::Running(h) => Some(h),
                other => {
                    *slot = other;
                    None
                }
            }
        };
        if let Some(handle) = handle {
            let _ = self.host.cancel_turn(self.run_id);
            let mut discarded = Vec::new();
            let _ = join_handle_draining(&self.record.delivery, handle, &mut discarded);
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
