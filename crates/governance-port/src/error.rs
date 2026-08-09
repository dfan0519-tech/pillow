//! Closed error sets for the governed execution port.
//!
//! Every variant is adapter-relevant and intended for **exhaustive** `match`
//! without a wildcard. There is intentionally no `Other(String)`,
//! `Box<dyn Error>`, or open-ended unavailable bag: adding a new host state must
//! force a host→port conversion update, and adding a port state must force
//! adapter provider-mapping updates.

use std::fmt;

/// Observer delivery failure. Host maps both variants to
/// `LifecycleDeliveryFailed` after durable fail-closed cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleObserverError {
    /// The active prompt/run is no longer live; fail closed.
    RunNoLongerLive,
    /// Event/sink delivery failed; fail closed.
    DeliveryFailed,
}

impl fmt::Display for LifecycleObserverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNoLongerLive => write!(f, "governed run is no longer live"),
            Self::DeliveryFailed => write!(f, "lifecycle observer delivery failed"),
        }
    }
}

impl std::error::Error for LifecycleObserverError {}

/// Closed errors from [`crate::GovernedExecutionPort::execute_read`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedReadExecutionError {
    /// No bound authority for this `run_id` on the owner-scoped service.
    RunNotBound,
    /// Provider proposal failed typed/context validation.
    InvalidProposal,
    /// Stable call identity conflicts with a different canonical request.
    ReservationConflict,
    /// A governed read is already in progress for this run.
    InProgress,
    /// Bound context is invalid for the proposal.
    InvalidContext,
    /// Host/gateway unavailable or internal control-only failure collapsed for
    /// the adapter surface (audit retained host-side).
    Unavailable,
}

impl fmt::Display for GovernedReadExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNotBound => write!(f, "governed read run is not bound"),
            Self::InvalidProposal => write!(f, "provider read proposal is invalid"),
            Self::ReservationConflict => {
                write!(
                    f,
                    "provider read call conflicts with its durable reservation"
                )
            }
            Self::InProgress => write!(f, "governed read is already in progress"),
            Self::InvalidContext => write!(f, "governed read context is invalid"),
            Self::Unavailable => write!(f, "governed read is unavailable"),
        }
    }
}

impl std::error::Error for GovernedReadExecutionError {}

/// Closed errors from [`crate::GovernedExecutionPort::execute_write`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedWriteExecutionError {
    /// No bound authority for this `run_id`.
    RunNotBound,
    /// Provider proposal failed typed/context validation.
    InvalidProposal,
    /// Stable reservation identity conflicts with a different request.
    ReservationConflict,
    /// A write proposal is already pending on this run (single-flight).
    Busy,
    /// Bound context is invalid for the proposal.
    InvalidContext,
    /// Proposal is not live in the active coordinator.
    NotLive,
    /// Host is shutting down; new work rejected.
    ShuttingDown,
    /// Observer failed; host performed fail-closed cleanup.
    LifecycleDeliveryFailed,
    /// Host/gateway unavailable or control-only failure collapsed for the
    /// adapter surface (audit retained host-side).
    Unavailable,
}

impl fmt::Display for GovernedWriteExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNotBound => write!(f, "governed write run is not bound"),
            Self::InvalidProposal => write!(f, "provider write proposal is invalid"),
            Self::ReservationConflict => {
                write!(
                    f,
                    "provider write call conflicts with its durable reservation"
                )
            }
            Self::Busy => write!(f, "write proposal is already pending on this run"),
            Self::InvalidContext => write!(f, "governed write context is invalid"),
            Self::NotLive => write!(f, "write proposal is not live in the active coordinator"),
            Self::ShuttingDown => write!(f, "host is shutting down"),
            Self::LifecycleDeliveryFailed => {
                write!(f, "write lifecycle observer delivery failed")
            }
            Self::Unavailable => write!(f, "governed write is unavailable"),
        }
    }
}

impl std::error::Error for GovernedWriteExecutionError {}

/// Closed errors from [`crate::GovernedExecutionPort::execute_process`].
///
/// [`Self::CleanupUnproven`] is retained as a distinct fail-closed signal. Mapping
/// it to cancelled/unavailable is **fail-open** (crate-level red line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedProcessExecutionError {
    /// No bound authority for this `run_id`.
    RunNotBound,
    /// Provider proposal failed typed/context validation.
    InvalidProposal,
    /// Stable reservation identity conflicts with a different request.
    ReservationConflict,
    /// A process proposal is already pending on this run (single-flight).
    Busy,
    /// Bound context is invalid for the proposal.
    InvalidContext,
    /// Proposal is not live in the active coordinator.
    NotLive,
    /// Host is shutting down; new work rejected.
    ShuttingDown,
    /// Child cleanup could not be proven; must not be softened to cancelled.
    CleanupUnproven,
    /// Observer failed; host performed fail-closed cleanup.
    LifecycleDeliveryFailed,
    /// Host/gateway unavailable or control-only failure collapsed for the
    /// adapter surface (audit retained host-side).
    Unavailable,
}

impl fmt::Display for GovernedProcessExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNotBound => write!(f, "governed process run is not bound"),
            Self::InvalidProposal => write!(f, "provider process proposal is invalid"),
            Self::ReservationConflict => {
                write!(
                    f,
                    "provider process call conflicts with its durable reservation"
                )
            }
            Self::Busy => write!(f, "process proposal is already pending on this run"),
            Self::InvalidContext => write!(f, "governed process context is invalid"),
            Self::NotLive => {
                write!(f, "process proposal is not live in the active coordinator")
            }
            Self::ShuttingDown => write!(f, "host is shutting down"),
            Self::CleanupUnproven => write!(f, "process cleanup could not be proven"),
            Self::LifecycleDeliveryFailed => {
                write!(f, "process lifecycle observer delivery failed")
            }
            Self::Unavailable => write!(f, "governed process is unavailable"),
        }
    }
}

impl std::error::Error for GovernedProcessExecutionError {}

/// Why a raw receipt was rejected as an opaque port outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedOutcomeValidationError {
    /// Delivery marker disagrees with lifecycle payload presence (e.g. Replayed
    /// carrying Requested, or Executed missing required lifecycle).
    InvalidDeliveryShape,
    /// Lifecycle ordering is impossible (e.g. Started without Decided).
    InvalidLifecycleOrder,
    /// Terminal shape is incomplete or non-terminal (missing terminal payload,
    /// non-terminal state, etc.).
    InvalidTerminalShape,
    /// Provider success/error flags or cancelled/timed_out bits conflict with
    /// each other or with the terminal state.
    ConflictingProviderOutcome,
}

impl fmt::Display for GovernedOutcomeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeliveryShape => {
                write!(f, "governed receipt delivery shape is invalid")
            }
            Self::InvalidLifecycleOrder => {
                write!(f, "governed receipt lifecycle order is invalid")
            }
            Self::InvalidTerminalShape => {
                write!(f, "governed receipt terminal shape is invalid")
            }
            Self::ConflictingProviderOutcome => {
                write!(f, "governed receipt provider outcome is conflicting")
            }
        }
    }
}

impl std::error::Error for GovernedOutcomeValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_execution_errors_are_closed_without_wildcard() {
        for variant in [
            GovernedReadExecutionError::RunNotBound,
            GovernedReadExecutionError::InvalidProposal,
            GovernedReadExecutionError::ReservationConflict,
            GovernedReadExecutionError::InProgress,
            GovernedReadExecutionError::InvalidContext,
            GovernedReadExecutionError::Unavailable,
        ] {
            let label = match variant {
                GovernedReadExecutionError::RunNotBound => "RunNotBound",
                GovernedReadExecutionError::InvalidProposal => "InvalidProposal",
                GovernedReadExecutionError::ReservationConflict => "ReservationConflict",
                GovernedReadExecutionError::InProgress => "InProgress",
                GovernedReadExecutionError::InvalidContext => "InvalidContext",
                GovernedReadExecutionError::Unavailable => "Unavailable",
            };
            assert!(!label.is_empty());
            let _ = variant.to_string();
        }
    }

    #[test]
    fn write_execution_errors_are_closed_without_wildcard() {
        for variant in [
            GovernedWriteExecutionError::RunNotBound,
            GovernedWriteExecutionError::InvalidProposal,
            GovernedWriteExecutionError::ReservationConflict,
            GovernedWriteExecutionError::Busy,
            GovernedWriteExecutionError::InvalidContext,
            GovernedWriteExecutionError::NotLive,
            GovernedWriteExecutionError::ShuttingDown,
            GovernedWriteExecutionError::LifecycleDeliveryFailed,
            GovernedWriteExecutionError::Unavailable,
        ] {
            let label = match variant {
                GovernedWriteExecutionError::RunNotBound => "RunNotBound",
                GovernedWriteExecutionError::InvalidProposal => "InvalidProposal",
                GovernedWriteExecutionError::ReservationConflict => "ReservationConflict",
                GovernedWriteExecutionError::Busy => "Busy",
                GovernedWriteExecutionError::InvalidContext => "InvalidContext",
                GovernedWriteExecutionError::NotLive => "NotLive",
                GovernedWriteExecutionError::ShuttingDown => "ShuttingDown",
                GovernedWriteExecutionError::LifecycleDeliveryFailed => "LifecycleDeliveryFailed",
                GovernedWriteExecutionError::Unavailable => "Unavailable",
            };
            assert!(!label.is_empty());
            let _ = variant.to_string();
        }
    }

    #[test]
    fn process_execution_errors_are_closed_without_wildcard() {
        for variant in [
            GovernedProcessExecutionError::RunNotBound,
            GovernedProcessExecutionError::InvalidProposal,
            GovernedProcessExecutionError::ReservationConflict,
            GovernedProcessExecutionError::Busy,
            GovernedProcessExecutionError::InvalidContext,
            GovernedProcessExecutionError::NotLive,
            GovernedProcessExecutionError::ShuttingDown,
            GovernedProcessExecutionError::CleanupUnproven,
            GovernedProcessExecutionError::LifecycleDeliveryFailed,
            GovernedProcessExecutionError::Unavailable,
        ] {
            let label = match variant {
                GovernedProcessExecutionError::RunNotBound => "RunNotBound",
                GovernedProcessExecutionError::InvalidProposal => "InvalidProposal",
                GovernedProcessExecutionError::ReservationConflict => "ReservationConflict",
                GovernedProcessExecutionError::Busy => "Busy",
                GovernedProcessExecutionError::InvalidContext => "InvalidContext",
                GovernedProcessExecutionError::NotLive => "NotLive",
                GovernedProcessExecutionError::ShuttingDown => "ShuttingDown",
                GovernedProcessExecutionError::CleanupUnproven => "CleanupUnproven",
                GovernedProcessExecutionError::LifecycleDeliveryFailed => "LifecycleDeliveryFailed",
                GovernedProcessExecutionError::Unavailable => "Unavailable",
            };
            assert!(!label.is_empty());
            let _ = variant.to_string();
        }
    }

    #[test]
    fn observer_and_validation_errors_are_closed_without_wildcard() {
        for variant in [
            LifecycleObserverError::RunNoLongerLive,
            LifecycleObserverError::DeliveryFailed,
        ] {
            let _ = match variant {
                LifecycleObserverError::RunNoLongerLive => "RunNoLongerLive",
                LifecycleObserverError::DeliveryFailed => "DeliveryFailed",
            };
        }
        for variant in [
            GovernedOutcomeValidationError::InvalidDeliveryShape,
            GovernedOutcomeValidationError::InvalidLifecycleOrder,
            GovernedOutcomeValidationError::InvalidTerminalShape,
            GovernedOutcomeValidationError::ConflictingProviderOutcome,
        ] {
            let _ = match variant {
                GovernedOutcomeValidationError::InvalidDeliveryShape => "InvalidDeliveryShape",
                GovernedOutcomeValidationError::InvalidLifecycleOrder => "InvalidLifecycleOrder",
                GovernedOutcomeValidationError::InvalidTerminalShape => "InvalidTerminalShape",
                GovernedOutcomeValidationError::ConflictingProviderOutcome => {
                    "ConflictingProviderOutcome"
                }
            };
        }
    }
}
