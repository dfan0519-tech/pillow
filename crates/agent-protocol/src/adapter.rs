//! Runtime-neutral adapter contract.

use crate::error::RuntimeError;
use crate::types::{
    RunId, RuntimeEvent, RuntimeProbe, RuntimeSession, SendTurn, StartSession,
};

/// Sink that receives ordered runtime events for one turn.
pub trait EventSink: Send {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError>;
}

/// Runtime-neutral adapter surface. Provider wire formats stay inside the
/// implementing crate and never appear on these method signatures.
pub trait AgentRuntimeAdapter: Send + Sync {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError>;

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError>;

    /// Stream events for a turn into `sink`. Implementations may block until
    /// a terminal event is emitted or return after accepting the turn.
    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError>;

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;

    /// Release all sessions, cancel in-flight turns, and join workers.
    fn shutdown(&self) -> Result<(), RuntimeError>;
}
