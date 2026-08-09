//! Level-triggered cancellation probe for blocking governed execute paths.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Live, level-triggered cancellation signal borrowed for one `execute_*` call.
///
/// # Contract
///
/// - Implementations **must** read the live state on every call. Production
///   adapters continue to use [`AtomicBool`] (or [`Arc<AtomicBool>`] shared
///   tokens) with [`Ordering::SeqCst`].
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

/// Shared cancel token used by adapters and multi-thread tests.
///
/// Reads the live atomic on every call (still level-triggered / SeqCst). This
/// preserves pre-Step-3 ergonomics where callers pass `&Arc<AtomicBool>` and
/// previously auto-deref'd into `&AtomicBool`.
impl CancellationProbe for Arc<AtomicBool> {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::SeqCst)
    }
}
