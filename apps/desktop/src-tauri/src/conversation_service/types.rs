//! Shared types for the conversation service active-run registry.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use keeper_session::{ConversationId, RunId};

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
