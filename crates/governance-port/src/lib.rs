//! Adapter-facing **governed execution port** for Pillow.
//!
//! This crate freezes the shared contract between runtime adapters and the
//! host-owned governance service. It defines:
//!
//! - object-safe execute traits ([`GovernedExecutionPort`]);
//! - closed execution / observer error sets (exhaustive match, no string escape);
//! - borrowed lifecycle observer traits;
//! - level-triggered [`CancellationProbe`];
//! - opaque checked outcomes that cannot be forged as "success" by callers;
//! - a [`testkit`] module for subsequent migration steps.
//!
//! It intentionally has **no production consumer** in Step 1. Existing
//! `runtime-host` free functions and `adapter-grok-acp` call sites are unchanged.
//!
//! # Dependency boundary
//!
//! Adapters depend on this crate (and `keeper-agent-protocol` DTOs) instead of
//! `keeper-runtime-host`. The host implements the port; adapters only hold
//! `Arc<dyn GovernedExecutionPort>`.
//!
//! # Authority boundary (MUST)
//!
//! Execute signatures carry only `run_id` + a provider-normalized proposal +
//! call-scoped cancel/observer borrows. They **must not** expose project root,
//! conversation authority, capability grants, list/decide control surfaces, raw
//! approval grants, gateways, databases, or renderers. Authority stays host-side.
//!
//! # Governance timing red lines (MUST — not enforced by the type system)
//!
//! These invariants are the most durable value of this crate. Rust cannot prove
//! them; every production implementor and conformance suite must. Violating any
//! of them is a regression, not a style issue.
//!
//! ## Callback vs durable commit
//!
//! 1. **Callback must not precede durable commit.**  
//!    `WriteProposalRequested` / process Requested only after durable reserve
//!    commit; Decided only after decision commit; Started only after durable
//!    claim reaches Claimed/Running. Emitting a lifecycle callback before the
//!    matching durable commit is a **governance timing regression**.
//!
//! 2. **Do not hold the governance coordinator lock while calling observers.**  
//!    Holding a coordinator/mutex across an observer callback is a **regression**
//!    (deadlock / re-entrancy risk). Release the lock, invoke the observer on the
//!    waiter thread, then re-acquire if needed.
//!
//! 3. **No terminal callback; no replay callback.**  
//!    Process terminal outcomes return only on the post-return receipt — never as
//!    an observer event. Adding a terminal callback variant, or calling any
//!    observer again on a `Replayed` delivery, is a **governance timing
//!    regression**. Replay must perform zero side effects, zero observer calls,
//!    and zero lifecycle re-emission.
//!
//! ## Cancellation and fail-closed cleanup
//!
//! 4. **Cancellation is level-triggered and re-read every wait cycle.**  
//!    Implementations must call [`CancellationProbe::is_cancelled`] on each wait
//!    iteration. Snapshotting cancel once at execute entry and ignoring later
//!    flips is **fail-open**. Write paths must retain C3F safe-resolution
//!    arbitration: after host cancel, at most one verified
//!    cancelled/interrupted Resolved may commit before the unique `RunCancelled`.
//!    Dropping that arbitration is **fail-open**.
//!
//! 5. **Never degrade `CleanupUnproven` to cancelled or unavailable.**  
//!    Process cleanup that cannot be proven must surface
//!    [`GovernedProcessExecutionError::CleanupUnproven`]. Mapping it to a soft
//!    cancelled/unavailable provider outcome is **fail-open**.
//!
//! ## Observer delivery failure
//!
//! 6. **Observer failure is fail-closed.**  
//!    If an observer returns [`LifecycleObserverError`], the host must perform
//!    durable fail-closed cleanup (no residual write/spawn authority, no permanent
//!    Busy) and return
//!    [`GovernedWriteExecutionError::LifecycleDeliveryFailed`] /
//!    [`GovernedProcessExecutionError::LifecycleDeliveryFailed`]. Swallowing
//!    delivery failure while leaving authority live is **fail-open**.
//!
//! ## Additional MUST contracts (implementor conformance)
//!
//! 7. All three execute methods block until a durable terminal, durable replay,
//!    or a closed execution error.
//! 8. Observers run only on the current waiter/call thread, serially; do not
//!    store, cross-thread, concurrent, or nested-call them.
//! 9. The same `run_id` must not re-enter `execute_*` from an observer. Cancel
//!    and host control decisions may run on other threads.
//! 10. Host control surface and execution port must come from the same
//!     owner-scoped service instance (no split ambient registries in the
//!     completed design).
//!
//! # Opaque outcomes
//!
//! Port methods return [`GovernedReadOutcome`], [`GovernedWriteOutcome`], and
//! [`GovernedProcessOutcome`]. Callers cannot construct a successful outcome
//! from a raw receipt without [`CheckedGovernedOutcome::try_from_receipt`], which
//! rejects impossible delivery/lifecycle/provider combinations. Test doubles in
//! [`testkit`] go through the same gate.
//!
//! # What this crate does *not* do
//!
//! - No runtime-host implementation or free-function wrappers (later steps).
//! - No adapter injection or consumer wiring (later steps).
//! - No repair of known host risks (observer-failure cleanup gap; non-atomic
//!   commit→EventSink) — those are separate acceptance items, not Step 1.

#![deny(missing_docs)]

mod cancel;
mod error;
mod lifecycle;
mod outcome;
mod port;

pub mod testkit;

pub use cancel::CancellationProbe;
pub use error::{
    GovernedOutcomeValidationError, GovernedProcessExecutionError, GovernedReadExecutionError,
    GovernedWriteExecutionError, LifecycleObserverError,
};
pub use lifecycle::{
    GovernedProcessLifecycle, GovernedProcessObserver, GovernedWriteLifecycle,
    GovernedWriteObserver,
};
pub use outcome::{
    CheckedGovernedOutcome, GovernedProcessOutcome, GovernedReadOutcome, GovernedShellOutcome,
    GovernedWriteOutcome,
};
pub use port::GovernedExecutionPort;
