//! Level-triggered cancellation probe for blocking governed execute paths.

use std::sync::atomic::{AtomicBool, Ordering};

/// Live, level-triggered cancellation signal borrowed for one `execute_*` call.
///
/// # Contract
///
/// - Implementations **must** read the live state on every call. Production
///   adapters continue to use [`AtomicBool`] with [`Ordering::SeqCst`].
/// - Host waiters **must** re-probe on each wait cycle. Snapshotting cancel once
///   at execute entry is **fail-open** (see crate-level timing red lines).
/// - The port implementor must not store this reference beyond the blocking call.
pub trait CancellationProbe: Send + Sync {
    /// Returns whether cancellation is currently asserted.
    fn is_cancelled(&self) -> bool;
}

impl CancellationProbe for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::SeqCst)
    }
}

impl CancellationProbe for &AtomicBool {
    fn is_cancelled(&self) -> bool {
        (*self).load(Ordering::SeqCst)
    }
}
