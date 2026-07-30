//! Shared types for the conversation service active-run registry and admission.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use keeper_session::{ConversationId, RunId};

/// Optional test seams for pump registration ordering and fail-closed paths.
///
/// Available only under `cfg(test)`. Production builds do not expose these
/// hooks as part of the ordinary service API.
#[cfg(test)]
#[derive(Default)]
pub struct PumpRegistrationHooks {
    /// Fires on the `send_turn` thread after the pump is spawned and before its
    /// `JoinHandle` is stored (and before the registration gate is released).
    pub after_spawn_before_register: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Fires on the pump thread after `pump_turn` returns and before
    /// `wait_until_registered` / active-entry removal.
    pub after_pump_before_registration_wait: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Pump-boundary seam: `Err` forces authoritative process failure.
    pub force_process_error: Option<Arc<dyn Fn() -> Result<(), String> + Send + Sync>>,
    /// When true after authoritative failure, terminalization returns residual.
    pub force_terminalization_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCommandError {
    pub code: &'static str,
    pub message: String,
}

impl ConversationCommandError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConversationCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ConversationCommandError {}

/// Narrow admission gate shared by `send_turn` and `shutdown`.
///
/// Once shutdown closes the gate, no new durable run/session/pump can be
/// admitted. In-flight admissions are waited on so a send that already owns
/// the gate either finishes ownership handoff or cleans up to a terminal state.
pub(super) struct ServiceAdmission {
    state: Mutex<ServiceAdmissionState>,
    cv: Condvar,
}

struct ServiceAdmissionState {
    closed: bool,
    in_flight: u32,
}

/// RAII admission for one `send_turn`. Drop releases the lifecycle slot.
pub(super) struct ServiceAdmissionGuard<'a> {
    gate: &'a ServiceAdmission,
}

impl ServiceAdmission {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(ServiceAdmissionState {
                closed: false,
                in_flight: 0,
            }),
            cv: Condvar::new(),
        }
    }

    pub(super) fn try_admit(&self) -> Result<ServiceAdmissionGuard<'_>, ConversationCommandError> {
        let mut guard = self.state.lock().map_err(|_| {
            ConversationCommandError::new(
                "conversation_state_unavailable",
                "Conversation state is temporarily unavailable.",
            )
        })?;
        if guard.closed {
            return Err(ConversationCommandError::new(
                "conversation_shut_down",
                "Conversation service is shut down.",
            ));
        }
        guard.in_flight = guard.in_flight.saturating_add(1);
        Ok(ServiceAdmissionGuard { gate: self })
    }

    pub(super) fn close(&self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.closed = true;
            self.cv.notify_all();
        }
    }

    /// Wait until every admitted `send_turn` has released, or until `timeout`.
    pub(super) fn wait_until_idle(
        &self,
        timeout: Duration,
    ) -> Result<(), ConversationCommandError> {
        let deadline = Instant::now() + timeout;
        let Ok(mut guard) = self.state.lock() else {
            return Err(ConversationCommandError::new(
                "conversation_state_unavailable",
                "Conversation state is temporarily unavailable.",
            ));
        };
        while guard.in_flight > 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ConversationCommandError::new(
                    "shutdown_incomplete",
                    format!(
                        "Timed out waiting for {} admitted send_turn(s) during shutdown.",
                        guard.in_flight
                    ),
                ));
            }
            let (next, wait_result) =
                self.cv
                    .wait_timeout(guard, remaining)
                    .unwrap_or_else(|poisoned| {
                        let (g, r) = poisoned.into_inner();
                        (g, r)
                    });
            guard = next;
            if wait_result.timed_out() && guard.in_flight > 0 {
                return Err(ConversationCommandError::new(
                    "shutdown_incomplete",
                    format!(
                        "Timed out waiting for {} admitted send_turn(s) during shutdown.",
                        guard.in_flight
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl Drop for ServiceAdmissionGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.gate.state.lock() {
            guard.in_flight = guard.in_flight.saturating_sub(1);
            self.gate.cv.notify_all();
        }
    }
}

pub(super) struct ActiveRun {
    pub(super) conversation_id: ConversationId,
    /// Join handle for the persistence/emit pump (not the host worker).
    pub(super) pump: Option<thread::JoinHandle<()>>,
}

pub(super) type ActiveMap = Arc<Mutex<HashMap<RunId, ActiveRun>>>;

/// Handshake so a pump never removes its active entry before the `JoinHandle`
/// is authoritatively stored.
///
/// Without this gate, a fast pump can finish and `remove` the pre-insert entry
/// while `send_turn` still holds the handle. Re-inserting that completed handle
/// then leaves a zombie entry (`active_run_count() == 1`) with no pump left to
/// clean it up, and `drain`/shutdown can time out.
pub(super) struct PumpHandleRegistration {
    ready: Mutex<bool>,
    cv: Condvar,
}

impl PumpHandleRegistration {
    pub(super) fn new() -> Self {
        Self {
            ready: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    pub(super) fn wait_until_registered(&self) {
        let mut ready = self
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*ready {
            ready = self
                .cv
                .wait(ready)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    pub(super) fn mark_registered(&self) {
        let mut ready = self
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *ready = true;
        self.cv.notify_one();
    }
}

pub(super) fn remove_active_run(active: &ActiveMap, run_id: RunId) {
    if let Ok(mut guard) = active.lock() {
        guard.remove(&run_id);
    }
}
