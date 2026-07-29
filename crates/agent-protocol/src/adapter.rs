//! Runtime-neutral adapter contract.

use crate::error::RuntimeError;
use crate::types::{RunId, RuntimeEvent, RuntimeProbe, RuntimeSession, SendTurn, StartSession};

/// Sink that receives ordered runtime events for one turn.
pub trait EventSink: Send {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError>;
}

/// Runtime-neutral adapter surface. Provider wire formats stay inside the
/// implementing crate and never appear on these method signatures.
pub trait AgentRuntimeAdapter: Send + Sync {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError>;

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError>;

    /// Mark `run_id` as cancellable before `send_turn` begins.
    ///
    /// Hosts that return a cancel handle from a concurrent spawn must call this
    /// so `cancel_turn` succeeds as soon as the handle is returned. Default is
    /// a no-op for adapters that only register inside `send_turn`.
    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let _ = run_id;
        Ok(())
    }

    /// Drop a previously armed run that will never call `send_turn` (e.g. spawn
    /// failure). Default is a no-op.
    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let _ = run_id;
        Ok(())
    }

    /// Stream events for a turn into `sink`. Implementations may block until
    /// a terminal event is emitted or return after accepting the turn.
    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError>;

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;

    /// Release all sessions, cancel in-flight turns, and join workers.
    fn shutdown(&self) -> Result<(), RuntimeError>;
}
