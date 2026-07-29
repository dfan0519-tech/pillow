//! Bounded runtime host: concurrency limits, protocol guard, worker lifecycle.
//!
//! Spawn registration, cancel, and join are linearized through the host worker
//! map and each [`WorkerRecord`]'s phase machine (`Starting -> Running ->
//! Joining -> Joined`). Async turn slots are RAII [`TurnSlotPermit`]s owned by
//! the worker record — no unregistered in-flight window and no manual
//! double/missed release on the async path.
//!
//! Synchronous `send_turn` shares the same concurrency counter but is not an
//! OS worker record. Admission is linearized with shutdown via
//! [`SyncTurnLifecycle`]: the gate is closed before workers are cancelled, and
//! shutdown waits on a condition variable until every admitted sync turn
//! releases its permit.

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
use crate::worker::{SyncTurnLifecycle, TurnSlotPermit, WorkerRecord, run_turn_worker};

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
    /// Shared turn-slot counter (async permits + sync `send_turn` guards).
    slots: Arc<AtomicU32>,
    shut_down: AtomicBool,
    workers: Mutex<HashMap<RunId, Arc<WorkerRecord>>>,
    /// Sync `send_turn` admission linearized with shutdown (mutex + condvar).
    sync_turns: SyncTurnLifecycle,
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
            slots: Arc::new(AtomicU32::new(0)),
            shut_down: AtomicBool::new(false),
            workers: Mutex::new(HashMap::new()),
            sync_turns: SyncTurnLifecycle::new(),
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
        // Lifecycle admission first: linearizes with shutdown close/wait so an
        // admitted sync turn is never invisible to join. Drop order is reverse
        // of declaration — permit releases before admission, so wait_until_idle
        // implies this turn's concurrency slot is already free.
        let _admission = self.sync_turns.try_admit()?;
        let _permit = TurnSlotPermit::try_acquire(&self.slots, self.config.max_concurrent_turns)?;
        let run_id = request.run_id;
        let mut guarded = ProtocolGuardSink::new(run_id, sink);
        self.adapter.send_turn(request, &mut guarded)?;
        guarded.finish()?;
        Ok(())
    }

    /// Run a turn on a background thread; cancellable as soon as this returns.
    ///
    /// Shutdown check, turn-slot permit acquisition, and insertion of the
    /// `Starting` worker record (with the permit moved into the record) are one
    /// critical section under the workers map. Shutdown either waits for that
    /// record or the spawn is rejected before any slot is held.
    pub fn spawn_turn(self: &Arc<Self>, request: SendTurn) -> Result<SpawnedTurn, RuntimeError> {
        self.ensure_alive()?;

        let run_id = request.run_id;

        // Linearize with shutdown: re-check shut_down, reject duplicate run_id
        // (never replace a live record), acquire the permit, and publish
        // Starting with permit ownership under the same workers lock.
        let record = {
            let mut workers = self.workers.lock().expect("workers");
            if self.shut_down.load(Ordering::SeqCst) {
                return Err(RuntimeError::ShutDown);
            }
            if workers.contains_key(&run_id) {
                return Err(RuntimeError::RunAlreadyActive { run_id });
            }
            let permit =
                TurnSlotPermit::try_acquire(&self.slots, self.config.max_concurrent_turns)?;
            let record = WorkerRecord::new(self.config.event_queue_capacity, permit);
            workers.insert(run_id, Arc::clone(&record));
            record
        };

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
                // Free concurrency when the body finishes; await_joined also
                // takes the permit (idempotent) before publishing Joined.
                record_for_thread.release_slot_permit();
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
    /// Host workers in `Starting`, `Running`, or `Joining`, and every admitted
    /// synchronous `send_turn`, are waited on even when `adapter.shutdown()`
    /// returns an error. The adapter error is returned only after host cleanup
    /// is complete.
    ///
    /// Order:
    /// 1. Close the sync-turn gate (no new admissions; existing holds remain).
    /// 2. Publish `shut_down` under the workers lock and cancel async workers.
    /// 3. `adapter.shutdown()` (error retained).
    /// 4. Join every async worker record.
    /// 5. Wait (condvar) until every admitted sync turn releases.
    /// 6. Empty registry must imply zero turn slots.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        // Close first so try_admit cannot race past AtomicBool shut_down alone.
        self.sync_turns.close();

        let already = {
            let workers = self.workers.lock().expect("workers");
            let already = self.shut_down.swap(true, Ordering::SeqCst);
            for (run_id, worker) in workers.iter() {
                worker.request_cancel();
                let _ = self.adapter.cancel_turn(*run_id);
                worker.delivery.close();
            }
            already
        };

        let adapter_result = self.adapter.shutdown();
        // Join host workers even when the adapter reports failure.
        self.join_all_workers();
        // Wait for every admitted synchronous send_turn to release its slot.
        self.sync_turns.wait_until_idle();
        adapter_result?;
        if already {
            return Ok(());
        }
        // Empty registry + idle sync gate ⇒ no outstanding turn slots.
        self.ensure_registry_empty_implies_no_slots()?;
        Ok(())
    }

    pub fn in_flight_turns(&self) -> u32 {
        self.slots.load(Ordering::SeqCst)
    }

    /// True when the worker registry is empty, no sync admissions remain, and
    /// no turn-slot permits remain.
    ///
    /// After a successful shutdown this must hold. It also documents the
    /// invariant that an empty async worker map cannot leave a leaked permit.
    pub fn is_fully_idle(&self) -> bool {
        let workers = self.workers.lock().expect("workers");
        workers.is_empty()
            && self.sync_turns.in_flight() == 0
            && self.slots.load(Ordering::SeqCst) == 0
    }

    /// Admitted synchronous turns still holding the lifecycle gate (tests).
    pub fn sync_turns_in_flight(&self) -> u32 {
        self.sync_turns.in_flight()
    }

    /// Number of worker records still registered (test / invariant helper).
    pub fn worker_registry_len(&self) -> usize {
        self.workers.lock().expect("workers").len()
    }

    /// Count of registered workers that still own their async slot permit.
    pub fn workers_holding_slot_permits(&self) -> usize {
        let workers = self.workers.lock().expect("workers");
        workers.values().filter(|w| w.has_slot_permit()).count()
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

    /// Abandon a Starting record: permit is released inside `abandon_starting`
    /// before `Joined` is published, then the map entry is removed **only if it
    /// still points at this exact record** (never detach another owner).
    fn fail_spawn_cleanup(&self, run_id: RunId, record: &Arc<WorkerRecord>) {
        record.request_cancel();
        record.delivery.close();
        record.abandon_starting();
        let mut workers = self.workers.lock().expect("workers");
        match workers.get(&run_id) {
            Some(current) if Arc::ptr_eq(current, record) => {
                workers.remove(&run_id);
            }
            _ => {
                // Already removed, or a different record owns this run_id —
                // do not remove foreign ownership.
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

    /// Cancel a host-owned worker without requiring the host to still be alive.
    ///
    /// Used by [`SpawnedTurn`] Drop so a blocked adapter is unblocked even when
    /// `shut_down` is already set. Tolerates `RunNotFound` only because the
    /// caller holds the host worker record.
    fn cancel_owned_worker_best_effort(&self, run_id: RunId, record: &WorkerRecord) {
        record.request_cancel();
        match self.adapter.cancel_turn(run_id) {
            Ok(()) => {}
            Err(RuntimeError::RunNotFound { .. }) => {}
            Err(_) => {}
        }
    }

    /// Remove `run_id` from the registry only when it still points at `record`.
    fn remove_worker_if_owner(&self, run_id: RunId, record: &Arc<WorkerRecord>) {
        let mut workers = self.workers.lock().expect("workers");
        match workers.get(&run_id) {
            Some(current) if Arc::ptr_eq(current, record) => {
                workers.remove(&run_id);
            }
            _ => {}
        }
    }

    /// Post-shutdown invariant: empty worker registry + idle sync gate ⇒ no
    /// turn-slot permits.
    ///
    /// Async permits live on worker records and are dropped before `Joined` is
    /// published. Sync admissions are closed and drained before this check, so
    /// a non-zero counter is always a bookkeeping leak.
    fn ensure_registry_empty_implies_no_slots(&self) -> Result<(), RuntimeError> {
        let workers = self.workers.lock().expect("workers");
        if !workers.is_empty() {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {} worker record(s) still registered",
                    workers.len()
                ),
            });
        }
        let sync = self.sync_turns.in_flight();
        if sync != 0 {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {sync} admitted synchronous turn(s) still in flight"
                ),
            });
        }
        let slots = self.slots.load(Ordering::SeqCst);
        if slots != 0 {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {slots} in-flight turn slot(s) and empty worker registry"
                ),
            });
        }
        Ok(())
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
        self.host.remove_worker_if_owner(self.run_id, &self.record);
        join_result?;
        Ok(events)
    }
}

impl Drop for SpawnedTurn {
    fn drop(&mut self) {
        if !self.record.is_joined() {
            // Request real adapter cancellation before waiting so a blocked
            // adapter (away from sink.emit) can unblock. Host cancel flag alone
            // is not sufficient for every provider. Exactly-one-terminal is
            // preserved by ProtocolGuardSink + delivery cancel arbitration.
            self.host
                .cancel_owned_worker_best_effort(self.run_id, &self.record);
            let mut discarded = Vec::new();
            let _ = self.record.await_joined(&mut discarded);
        }
        self.host.remove_worker_if_owner(self.run_id, &self.record);
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
