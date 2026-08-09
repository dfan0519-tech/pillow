//! Object-safe governed execution port.
//!
//! Synchronous by design: the current host model blocks the worker until a
//! durable terminal, durable replay, or closed execution error. Introducing
//! `async` would change cancel/decision/terminal timing and is out of scope.

use keeper_agent_protocol::{
    ProviderProcessProposal, ProviderToolProposal, ProviderWriteProposal, RunId,
};

use crate::{
    CancellationProbe, GovernedProcessExecutionError, GovernedProcessObserver,
    GovernedProcessOutcome, GovernedReadExecutionError, GovernedReadOutcome,
    GovernedWriteExecutionError, GovernedWriteObserver, GovernedWriteOutcome,
};

/// Adapter-facing governed tool execution surface.
///
/// # Object safety
///
/// No generic methods, associated types, `impl Trait`, or `Self` returns. Intended
/// storage is `Arc<dyn GovernedExecutionPort + 'static>`.
///
/// # Call-period borrows
///
/// Proposals move by value. Cancel probes and observers are borrowed only for the
/// duration of the blocking call. Implementors **must not** retain those
/// references after return.
///
/// # Timing
///
/// See the crate-level rustdoc for durable-commit barriers, lock release before
/// observer calls, replay zero-callback, level-triggered cancel, and
/// `CleanupUnproven` fail-closed rules. Those contracts are part of this trait
/// even though the compiler cannot enforce them.
pub trait GovernedExecutionPort: Send + Sync + 'static {
    /// Execute one governed read for a bound run.
    ///
    /// Blocks until durable receipt (executed or replayed) or a closed error.
    /// Read has no lifecycle observer.
    fn execute_read(
        &self,
        run_id: RunId,
        proposal: ProviderToolProposal,
        cancellation: &dyn CancellationProbe,
    ) -> Result<GovernedReadOutcome, GovernedReadExecutionError>;

    /// Execute one governed write for a bound run.
    ///
    /// Blocks until durable terminal/replay receipt or a closed error. May invoke
    /// `observer` synchronously with [`crate::GovernedWriteLifecycle::Requested`]
    /// only after durable reserve commit, never on replay, never under the
    /// coordinator lock.
    fn execute_write(
        &self,
        run_id: RunId,
        proposal: ProviderWriteProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedWriteObserver,
    ) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError>;

    /// Execute one governed process for a bound run.
    ///
    /// Blocks until durable terminal/replay receipt or a closed error. May invoke
    /// `observer` with Requested → Decided → Started (reject/expire omit Started),
    /// never terminal, never on replay, never under the coordinator lock.
    ///
    /// Cleanup that cannot be proven must return
    /// [`GovernedProcessExecutionError::CleanupUnproven`] — not cancelled/unavailable.
    fn execute_process(
        &self,
        run_id: RunId,
        proposal: ProviderProcessProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedProcessObserver,
    ) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError>;
}
