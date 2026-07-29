//! Bounded runtime host: concurrency limits, protocol guard, worker lifecycle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};

use crate::delivery::{BoundedDelivery, DEFAULT_EVENT_QUEUE_CAPACITY};
use crate::fake::FakeRuntimeAdapter;
use crate::sink::{BoundedDeliverySink, ProtocolGuardSink};

/// Default concurrent turn limit for the host (process-pool style bound).
pub const DEFAULT_MAX_CONCURRENT_TURNS: u32 = 4;
const MAX_CONCURRENT_TURNS_CAP: u32 = 32;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub max_concurrent_turns: u32,
    /// When true, fixture `delay_ms` is honored; when false, delays are zeroed.
    pub honor_fixture_delays: bool,
    /// Per-turn bounded event queue capacity (backpressure when full).
    pub event_queue_capacity: usize,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_concurrent_turns: DEFAULT_MAX_CONCURRENT_TURNS,
            honor_fixture_delays: true,
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
        }
    }
}

pub fn normalize_max_concurrent_turns(raw: u32) -> u32 {
    raw.clamp(1, MAX_CONCURRENT_TURNS_CAP)
}

struct WorkerRecord {
    cancel: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    delivery: Arc<BoundedDelivery>,
    join: Mutex<Option<JoinHandle<Result<(), RuntimeError>>>>,
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
            join: Mutex::new(None),
        });

        {
            let mut workers = self.workers.lock().expect("workers");
            workers.insert(run_id, Arc::clone(&record));
        }

        let host = Arc::clone(self);
        let delivery_for_thread = Arc::clone(&delivery);
        let finished_flag = Arc::clone(&finished);

        let join = thread::Builder::new()
            .name(format!("keeper-turn-{run_id}"))
            .spawn(move || {
                let result = (|| {
                    let mut sink = BoundedDeliverySink::new(Arc::clone(&delivery_for_thread));
                    let mut guarded = ProtocolGuardSink::new(run_id, &mut sink);
                    host.adapter.send_turn(request, &mut guarded)?;
                    guarded.finish()?;
                    Ok(())
                })();
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

        *record.join.lock().expect("join") = Some(join);

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
                w.cancel.store(true, Ordering::SeqCst);
                true
            } else {
                false
            }
        };
        match self.adapter.cancel_turn(run_id) {
            Ok(()) => Ok(()),
            Err(RuntimeError::RunNotFound { .. }) if known => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Shut down the adapter and join all host-owned workers before returning.
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            // Already shut down: still ensure no residual host workers.
            self.join_all_workers();
            return self.adapter.shutdown();
        }

        // Cancel every tracked worker, close deliveries so blocked producers
        // unblock, then shut down the adapter and join threads.
        {
            let workers = self.workers.lock().expect("workers");
            for (run_id, worker) in workers.iter() {
                worker.cancel.store(true, Ordering::SeqCst);
                let _ = self.adapter.cancel_turn(*run_id);
                // Unblock push_blocking waiters (backpressured producers).
                worker.delivery.close();
            }
        }

        self.adapter.shutdown()?;
        self.join_all_workers();

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

    fn join_all_workers(&self) {
        let handles: Vec<JoinHandle<Result<(), RuntimeError>>> = {
            let workers = self.workers.lock().expect("workers");
            workers
                .values()
                .filter_map(|w| w.join.lock().expect("join").take())
                .collect()
        };
        for handle in handles {
            let _ = handle.join();
        }
        // Workers remove themselves on exit; clear any stragglers after join.
        self.workers.lock().expect("workers").clear();
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
        // Prefer taking the join handle so Drop does not double-join.
        let handle = self.record.join.lock().expect("join").take();
        let join_result = if let Some(handle) = handle {
            handle.join().map_err(|_| RuntimeError::Internal {
                message: "turn worker panicked".into(),
            })?
        } else {
            // Host shutdown may have joined already; wait for finished flag.
            while !self.record.finished.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        };
        {
            let mut workers = self.host.workers.lock().expect("workers");
            workers.remove(&self.run_id);
        }
        join_result?;
        Ok(self.record.delivery.drain())
    }
}

impl Drop for SpawnedTurn {
    fn drop(&mut self) {
        if let Some(handle) = self.record.join.lock().expect("join").take() {
            // Best-effort cancel + join so Drop leaves no thread residual.
            let _ = self.host.cancel_turn(self.run_id);
            let _ = handle.join();
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
