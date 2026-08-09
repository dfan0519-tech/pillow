//! Borrowed lifecycle observer traits for write and process execute paths.
//!
//! Process **terminal** is intentionally absent: terminals return only via the
//! durable receipt. Adding a terminal observer variant would be a governance
//! timing regression (see crate-level rustdoc).

use keeper_agent_protocol::{
    ProcessProposalDecidedPayload, ProcessProposalRequestedPayload, ProcessStartedPayload,
    WriteProposalRequestedPayload,
};

use crate::LifecycleObserverError;

/// Write lifecycle events delivered synchronously on the waiter thread.
///
/// Only [`Self::Requested`] exists. Terminal resolution returns on the write
/// receipt after durable commit — never as an observer callback.
pub enum GovernedWriteLifecycle<'a> {
    /// Emitted only after durable `PendingApproval` reserve commit + live entry.
    Requested(&'a WriteProposalRequestedPayload),
}

/// Process lifecycle events delivered synchronously on the waiter thread.
///
/// Terminal outcomes are **not** included. They return only on the process
/// receipt after durable terminal commit.
pub enum GovernedProcessLifecycle<'a> {
    /// After durable reserve commit.
    Requested(&'a ProcessProposalRequestedPayload),
    /// After durable decision commit.
    Decided(&'a ProcessProposalDecidedPayload),
    /// After durable claim reaches Claimed/Running. Requires a prior Decided.
    Started(&'a ProcessStartedPayload),
}

/// Observer for write lifecycle callbacks during one blocking `execute_write`.
///
/// # Contract
///
/// - Called only on the waiter/call thread, serially.
/// - Must not be stored, sent across threads, or re-entered into `execute_*`.
/// - Invoked only after the matching durable commit; never under the coordinator lock.
/// - Never invoked on replay.
/// - Failure must be fail-closed by the host (see crate-level rustdoc).
pub trait GovernedWriteObserver {
    /// Observe one write lifecycle event.
    fn observe(&mut self, event: GovernedWriteLifecycle<'_>) -> Result<(), LifecycleObserverError>;
}

/// Observer for process lifecycle callbacks during one blocking `execute_process`.
///
/// # Contract
///
/// Same threading, lock, replay, and fail-closed rules as
/// [`GovernedWriteObserver`]. Process terminal is never delivered here.
pub trait GovernedProcessObserver {
    /// Observe one process lifecycle event.
    fn observe(
        &mut self,
        event: GovernedProcessLifecycle<'_>,
    ) -> Result<(), LifecycleObserverError>;
}
