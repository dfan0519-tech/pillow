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
    /// **Required** for any adapter used with concurrent host spawn
    /// (`RuntimeHost::spawn_turn` or equivalent). Implementations must register
    /// a cancel token so `cancel_turn` can succeed as soon as the host returns
    /// a handle. There is intentionally **no default no-op**: spawn-capable
    /// adapters must opt in explicitly.
    ///
    /// Synchronous `send_turn`-only adapters may implement this as an explicit
    /// empty `Ok(())` if they never expose a pre-`send_turn` cancel handle.
    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;

    /// Drop a previously armed run that will never call `send_turn` (e.g. spawn
    /// failure). Spawn-capable adapters must free any token created by
    /// `arm_turn`. Synchronous-only adapters may return `Ok(())`.
    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;

    /// Stream events for a turn into `sink`. Implementations may block until
    /// a terminal event is emitted or return after accepting the turn.
    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError>;

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError>;

    /// Release all sessions, cancel in-flight turns, and join workers.
    fn shutdown(&self) -> Result<(), RuntimeError>;
}
