//! Deterministic in-process fake adapter (fixture replay only).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunCancelledPayload, RunId, RuntimeError, RuntimeEvent,
    RuntimeEventKind, RuntimeProbe, RuntimeSession, RuntimeSessionId, SendTurn, StartSession,
};

use crate::fixture::{
    load_fixture_file, parse_fixture_jsonl, FixtureEventLine, FAKE_TURN_FIXTURE,
};

struct ActiveTurn {
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), RuntimeError>>>,
}

/// Deterministic in-process adapter that replays a JSONL fixture.
pub struct FakeRuntimeAdapter {
    fixture: Vec<FixtureEventLine>,
    honor_delays: bool,
    shut_down: AtomicBool,
    sessions: Mutex<HashMap<RuntimeSessionId, String>>,
    active: Mutex<HashMap<RunId, ActiveTurn>>,
}

impl FakeRuntimeAdapter {
    pub fn from_fixture_text(text: &str, honor_delays: bool) -> Result<Self, RuntimeError> {
        Ok(Self {
            fixture: parse_fixture_jsonl(text)?,
            honor_delays,
            shut_down: AtomicBool::new(false),
            sessions: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn from_committed_fixture(honor_delays: bool) -> Result<Self, RuntimeError> {
        Self::from_fixture_text(FAKE_TURN_FIXTURE, honor_delays)
    }

    pub fn from_fixture_path(
        path: impl AsRef<Path>,
        honor_delays: bool,
    ) -> Result<Self, RuntimeError> {
        let fixture = load_fixture_file(path)?;
        Ok(Self {
            fixture,
            honor_delays,
            shut_down: AtomicBool::new(false),
            sessions: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
        })
    }

    fn ensure_alive(&self) -> Result<(), RuntimeError> {
        if self.shut_down.load(Ordering::SeqCst) {
            Err(RuntimeError::ShutDown)
        } else {
            Ok(())
        }
    }

    fn emit_cancelled(
        run_id: RunId,
        sequence: u64,
        sink: &mut dyn EventSink,
    ) -> Result<(), RuntimeError> {
        let event = RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::RunCancelled,
            RunCancelledPayload {
                reason: Some("user".into()),
            },
            Utc::now(),
        )?;
        sink.emit(event)
    }

    fn sleep_cooperative(total_ms: u64, cancel: &AtomicBool) {
        let total = Duration::from_millis(total_ms);
        let step = Duration::from_millis(1);
        let mut slept = Duration::ZERO;
        while slept < total {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let slice = step.min(total - slept);
            thread::sleep(slice);
            slept += slice;
        }
    }

    fn emit_fixture_turn(
        fixture: &[FixtureEventLine],
        honor_delays: bool,
        run_id: RunId,
        cancel: &AtomicBool,
        sink: &mut dyn EventSink,
    ) -> Result<(), RuntimeError> {
        let mut sequence = 0u64;
        let mut terminal_emitted = false;

        for line in fixture {
            if cancel.load(Ordering::SeqCst) {
                if !terminal_emitted {
                    sequence += 1;
                    Self::emit_cancelled(run_id, sequence, sink)?;
                }
                return Ok(());
            }

            if honor_delays && line.delay_ms > 0 {
                Self::sleep_cooperative(line.delay_ms, cancel);
                if cancel.load(Ordering::SeqCst) {
                    if !terminal_emitted {
                        sequence += 1;
                        Self::emit_cancelled(run_id, sequence, sink)?;
                    }
                    return Ok(());
                }
            }

            if line.kind.is_terminal() && terminal_emitted {
                return Err(RuntimeError::InvalidEvent {
                    message: "fixture contains multiple terminal events".into(),
                });
            }

            sequence += 1;
            let event = RuntimeEvent {
                run_id,
                sequence,
                kind: line.kind,
                payload: line.payload.clone(),
                created_at: Utc::now(),
            };
            let is_terminal = event.is_terminal();
            sink.emit(event)?;
            if is_terminal {
                terminal_emitted = true;
                break;
            }
        }

        if !terminal_emitted && cancel.load(Ordering::SeqCst) {
            sequence += 1;
            Self::emit_cancelled(run_id, sequence, sink)?;
        }

        Ok(())
    }

    /// Number of in-flight turn workers (for shutdown assertions).
    pub fn active_turn_count(&self) -> usize {
        self.active.lock().expect("active lock").len()
    }
}

impl AgentRuntimeAdapter for FakeRuntimeAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.ensure_alive()?;
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "fake".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            detail: Some("deterministic fixture replay".into()),
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.ensure_alive()?;
        let session_id = RuntimeSessionId::new();
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(session_id, request.conversation_key.clone());
        Ok(RuntimeSession {
            session_id,
            conversation_key: request.conversation_key,
        })
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        {
            let sessions = self.sessions.lock().expect("sessions lock");
            if !sessions.contains_key(&request.session_id) {
                return Err(RuntimeError::SessionNotFound {
                    session_id: request.session_id,
                });
            }
        }

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().expect("active lock");
            if active.contains_key(&request.run_id) {
                return Err(RuntimeError::RunAlreadyActive {
                    run_id: request.run_id,
                });
            }
            active.insert(
                request.run_id,
                ActiveTurn {
                    cancel: Arc::clone(&cancel),
                    join: None,
                },
            );
        }

        // Synchronous emission on the caller thread keeps shutdown simple:
        // no orphan join handles when the turn finishes before shutdown.
        let result = Self::emit_fixture_turn(
            &self.fixture,
            self.honor_delays,
            request.run_id,
            &cancel,
            sink,
        );

        {
            let mut active = self.active.lock().expect("active lock");
            active.remove(&request.run_id);
        }

        result
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        let active = self.active.lock().expect("active lock");
        if let Some(turn) = active.get(&run_id) {
            turn.cancel.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(RuntimeError::RunNotFound { run_id })
        }
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let mut active = self.active.lock().expect("active lock");
        for turn in active.values() {
            turn.cancel.store(true, Ordering::SeqCst);
        }
        let handles: Vec<JoinHandle<Result<(), RuntimeError>>> = active
            .values_mut()
            .filter_map(|t| t.join.take())
            .collect();
        active.clear();
        drop(active);

        for handle in handles {
            let _ = handle.join();
        }

        self.sessions.lock().expect("sessions lock").clear();
        Ok(())
    }
}
