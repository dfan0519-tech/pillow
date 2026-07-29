//! Runtime adapter and host errors.

use thiserror::Error;

use crate::types::{RunId, RuntimeSessionId};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("runtime is shut down")]
    ShutDown,
    #[error("session not found: {session_id}")]
    SessionNotFound { session_id: RuntimeSessionId },
    #[error("run not found: {run_id}")]
    RunNotFound { run_id: RunId },
    #[error("run already active: {run_id}")]
    RunAlreadyActive { run_id: RunId },
    #[error("concurrency limit reached (max {max})")]
    ConcurrencyLimit { max: u32 },
    #[error("invalid fixture: {message}")]
    InvalidFixture { message: String },
    #[error("invalid event: {message}")]
    InvalidEvent { message: String },
    #[error("event queue full (capacity {capacity})")]
    EventQueueFull { capacity: usize },
    #[error("cancelled")]
    Cancelled,
    #[error("internal error: {message}")]
    Internal { message: String },
}
