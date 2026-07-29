//! Bounded runtime host: concurrency limits and terminal enforcement.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};

use crate::fake::FakeRuntimeAdapter;
use crate::sink::TerminalGuardSink;

/// Default concurrent turn limit for the host (process-pool style bound).
pub const DEFAULT_MAX_CONCURRENT_TURNS: u32 = 4;
const MAX_CONCURRENT_TURNS_CAP: u32 = 32;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub max_concurrent_turns: u32,
    /// When true, fixture `delay_ms` is honored; when false, delays are zeroed.
    pub honor_fixture_delays: bool,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_concurrent_turns: DEFAULT_MAX_CONCURRENT_TURNS,
            honor_fixture_delays: true,
        }
    }
}

pub fn normalize_max_concurrent_turns(raw: u32) -> u32 {
    raw.clamp(1, MAX_CONCURRENT_TURNS_CAP)
}

/// Host that bounds concurrency and enforces single-terminal emission.
pub struct RuntimeHost {
    adapter: Arc<dyn AgentRuntimeAdapter>,
    config: HostConfig,
    in_flight: AtomicU32,
    shut_down: AtomicBool,
}

impl RuntimeHost {
    pub fn new(adapter: Arc<dyn AgentRuntimeAdapter>, config: HostConfig) -> Self {
        Self {
            adapter,
            config: HostConfig {
                max_concurrent_turns: normalize_max_concurrent_turns(config.max_concurrent_turns),
                honor_fixture_delays: config.honor_fixture_delays,
            },
            in_flight: AtomicU32::new(0),
            shut_down: AtomicBool::new(false),
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
        let mut guarded = TerminalGuardSink::new(sink);
        let result = self.adapter.send_turn(request, &mut guarded);
        self.release_turn_slot();
        result
    }

    /// Run a turn on a background thread; returns a cancel handle and join.
    pub fn spawn_turn(self: &Arc<Self>, request: SendTurn) -> Result<SpawnedTurn, RuntimeError> {
        self.ensure_alive()?;
        self.acquire_turn_slot()?;

        let host = Arc::clone(self);
        let run_id = request.run_id;
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_for_thread = Arc::clone(&events);
        let finished = Arc::new(AtomicBool::new(false));
        let finished_flag = Arc::clone(&finished);

        let join = thread::Builder::new()
            .name(format!("keeper-turn-{run_id}"))
            .spawn(move || {
                struct SharedSink {
                    events: Arc<Mutex<Vec<RuntimeEvent>>>,
                }
                impl EventSink for SharedSink {
                    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
                        self.events.lock().expect("events").push(event);
                        Ok(())
                    }
                }
                let mut sink = SharedSink {
                    events: events_for_thread,
                };
                let mut guarded = TerminalGuardSink::new(&mut sink);
                let result = host.adapter.send_turn(request, &mut guarded);
                host.release_turn_slot();
                finished_flag.store(true, Ordering::SeqCst);
                result
            })
            .map_err(|e| RuntimeError::Internal {
                message: format!("spawn turn worker: {e}"),
            })?;

        Ok(SpawnedTurn {
            run_id,
            host: Arc::clone(self),
            events,
            finished,
            join: Some(join),
        })
    }

    pub fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        self.adapter.cancel_turn(run_id)
    }

    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return self.adapter.shutdown();
        }
        self.adapter.shutdown()?;
        // Wait briefly for in-flight counters to drain (spawned turns release slots).
        for _ in 0..50 {
            if self.in_flight.load(Ordering::SeqCst) == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }

    pub fn in_flight_turns(&self) -> u32 {
        self.in_flight.load(Ordering::SeqCst)
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
    events: Arc<Mutex<Vec<RuntimeEvent>>>,
    finished: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), RuntimeError>>>,
}

impl SpawnedTurn {
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        self.host.cancel_turn(self.run_id)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    pub fn events_snapshot(&self) -> Vec<RuntimeEvent> {
        self.events.lock().expect("events").clone()
    }

    pub fn join(mut self) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        if let Some(handle) = self.join.take() {
            handle
                .join()
                .map_err(|_| RuntimeError::Internal {
                    message: "turn worker panicked".into(),
                })??;
        }
        Ok(self.events.lock().expect("events").clone())
    }
}

impl Drop for SpawnedTurn {
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            // Best-effort cancel + join so Drop leaves no thread residual.
            let _ = self.host.adapter.cancel_turn(self.run_id);
            let _ = handle.join();
        }
    }
}

/// Resolve the workspace fixture path for tests that prefer filesystem load.
pub fn workspace_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/protocol/fake-turn.jsonl")
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
