//! Transitional governed-execution port over the existing ambient registries.
//!
//! This module is the Step 4 compatibility layer. It deliberately delegates to
//! the existing free functions and global run registries; it does not change
//! ownership, inject an owner-scoped service, or complete adapter decoupling.

use std::cell::Cell;

use keeper_agent_protocol::{
    GovernedProcessReceipt, GovernedReadReceipt, GovernedWriteReceipt,
    ProcessProposalRequestedPayload, ProviderProcessProposal, ProviderToolProposal,
    ProviderWriteProposal, RunId,
};
use keeper_governance_port::{
    CancellationProbe, CheckedGovernedOutcome, GovernedExecutionPort,
    GovernedOutcomeValidationError, GovernedProcessExecutionError,
    GovernedProcessLifecycle as PortProcessLifecycle, GovernedProcessObserver,
    GovernedProcessOutcome, GovernedReadExecutionError, GovernedReadOutcome,
    GovernedWriteExecutionError, GovernedWriteLifecycle, GovernedWriteObserver,
    GovernedWriteOutcome, LifecycleObserverError,
};

use crate::{
    GovernedProcessBridgeError, GovernedProcessLifecycle as HostProcessLifecycle,
    GovernedReadBridgeError, GovernedWriteBridgeError, ProcessProposalRequestedView,
    execute_governed_process, execute_governed_read, execute_governed_write,
};

/// Step 4 compatibility implementation backed by the existing global registries.
///
/// This zero-sized handle owns no authority. Each call performs the same
/// ambient `run_id` lookup as the existing `execute_governed_*` free function,
/// then checked-converts the returned receipt into an opaque port outcome.
/// It is intentionally transitional: the global service-locator coupling
/// remains until the later owner-scoped service and lease steps.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobalGovernedExecutionPort;

impl GlobalGovernedExecutionPort {
    /// Create a compatibility handle over the existing global registries.
    pub const fn new() -> Self {
        Self
    }
}

impl GovernedExecutionPort for GlobalGovernedExecutionPort {
    fn execute_read(
        &self,
        run_id: RunId,
        proposal: ProviderToolProposal,
        cancellation: &dyn CancellationProbe,
    ) -> Result<GovernedReadOutcome, GovernedReadExecutionError> {
        let receipt =
            execute_governed_read(run_id, proposal, cancellation).map_err(map_read_bridge_error)?;
        checked_read_outcome(receipt)
    }

    fn execute_write(
        &self,
        run_id: RunId,
        proposal: ProviderWriteProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedWriteObserver,
    ) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError> {
        let observer_failure = Cell::new(None);
        let result = execute_governed_write(run_id, proposal, cancellation, |requested| {
            observer
                .observe(GovernedWriteLifecycle::Requested(requested))
                .map_err(|error| {
                    observer_failure.set(Some(error));
                    map_write_observer_error_to_bridge(error)
                })
        });

        match observer_failure.get() {
            Some(error) => Err(map_write_observer_error_to_port(error)),
            None => match result {
                Ok(receipt) => checked_write_outcome(receipt),
                Err(error) => Err(map_write_bridge_error(error)),
            },
        }
    }

    fn execute_process(
        &self,
        run_id: RunId,
        proposal: ProviderProcessProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedProcessObserver,
    ) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError> {
        let observer_failure = Cell::new(None);
        let result = execute_governed_process(run_id, proposal, cancellation, |event| {
            let observed = match event {
                HostProcessLifecycle::Requested(requested) => {
                    let payload = process_requested_payload(requested);
                    observer.observe(PortProcessLifecycle::Requested(&payload))
                }
                HostProcessLifecycle::Decided(decided) => {
                    observer.observe(PortProcessLifecycle::Decided(decided))
                }
                HostProcessLifecycle::Started(started) => {
                    observer.observe(PortProcessLifecycle::Started(started))
                }
            };
            observed.map_err(|error| {
                observer_failure.set(Some(error));
                map_process_observer_error_to_bridge(error)
            })
        });

        // Never let observer context soften the process cleanup red line.
        if matches!(&result, Err(GovernedProcessBridgeError::CleanupUnproven)) {
            return Err(GovernedProcessExecutionError::CleanupUnproven);
        }

        match observer_failure.get() {
            Some(error) => Err(map_process_observer_error_to_port(error)),
            None => match result {
                Ok(receipt) => checked_process_outcome(receipt),
                Err(error) => Err(map_process_bridge_error(error)),
            },
        }
    }
}

fn checked_read_outcome(
    receipt: GovernedReadReceipt,
) -> Result<GovernedReadOutcome, GovernedReadExecutionError> {
    GovernedReadOutcome::try_from_receipt(receipt).map_err(map_read_outcome_error)
}

fn checked_write_outcome(
    receipt: GovernedWriteReceipt,
) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError> {
    GovernedWriteOutcome::try_from_receipt(receipt).map_err(map_write_outcome_error)
}

fn checked_process_outcome(
    receipt: GovernedProcessReceipt,
) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError> {
    GovernedProcessOutcome::try_from_receipt(receipt).map_err(map_process_outcome_error)
}

fn map_read_bridge_error(error: GovernedReadBridgeError) -> GovernedReadExecutionError {
    match error {
        GovernedReadBridgeError::RunNotRegistered => GovernedReadExecutionError::RunNotBound,
        GovernedReadBridgeError::InvalidProposal => GovernedReadExecutionError::InvalidProposal,
        GovernedReadBridgeError::ReservationConflict => {
            GovernedReadExecutionError::ReservationConflict
        }
        GovernedReadBridgeError::InProgress => GovernedReadExecutionError::InProgress,
        GovernedReadBridgeError::InvalidContext => GovernedReadExecutionError::InvalidContext,
        GovernedReadBridgeError::Gateway => GovernedReadExecutionError::Unavailable,
    }
}

fn map_write_bridge_error(error: GovernedWriteBridgeError) -> GovernedWriteExecutionError {
    match error {
        GovernedWriteBridgeError::RunNotRegistered => GovernedWriteExecutionError::RunNotBound,
        GovernedWriteBridgeError::InvalidProposal => GovernedWriteExecutionError::InvalidProposal,
        GovernedWriteBridgeError::ReservationConflict => {
            GovernedWriteExecutionError::ReservationConflict
        }
        GovernedWriteBridgeError::Busy => GovernedWriteExecutionError::Busy,
        GovernedWriteBridgeError::InvalidContext => GovernedWriteExecutionError::InvalidContext,
        GovernedWriteBridgeError::NotLive => GovernedWriteExecutionError::NotLive,
        GovernedWriteBridgeError::ShutDown => GovernedWriteExecutionError::ShuttingDown,
        GovernedWriteBridgeError::Gateway
        | GovernedWriteBridgeError::OwnershipMismatch
        | GovernedWriteBridgeError::InvalidGrant
        | GovernedWriteBridgeError::DecisionDenied
        | GovernedWriteBridgeError::Expired
        | GovernedWriteBridgeError::Cancelled
        | GovernedWriteBridgeError::Interrupted => GovernedWriteExecutionError::Unavailable,
    }
}

fn map_process_bridge_error(error: GovernedProcessBridgeError) -> GovernedProcessExecutionError {
    match error {
        GovernedProcessBridgeError::RunNotRegistered => GovernedProcessExecutionError::RunNotBound,
        GovernedProcessBridgeError::InvalidProposal => {
            GovernedProcessExecutionError::InvalidProposal
        }
        GovernedProcessBridgeError::ReservationConflict => {
            GovernedProcessExecutionError::ReservationConflict
        }
        GovernedProcessBridgeError::Busy => GovernedProcessExecutionError::Busy,
        GovernedProcessBridgeError::InvalidContext => GovernedProcessExecutionError::InvalidContext,
        GovernedProcessBridgeError::NotLive => GovernedProcessExecutionError::NotLive,
        GovernedProcessBridgeError::ShutDown => GovernedProcessExecutionError::ShuttingDown,
        GovernedProcessBridgeError::CleanupUnproven => {
            GovernedProcessExecutionError::CleanupUnproven
        }
        GovernedProcessBridgeError::Gateway
        | GovernedProcessBridgeError::OwnershipMismatch
        | GovernedProcessBridgeError::DecisionDenied
        | GovernedProcessBridgeError::Expired
        | GovernedProcessBridgeError::Cancelled
        | GovernedProcessBridgeError::Interrupted => GovernedProcessExecutionError::Unavailable,
    }
}

fn map_write_observer_error_to_bridge(error: LifecycleObserverError) -> GovernedWriteBridgeError {
    match error {
        LifecycleObserverError::RunNoLongerLive => GovernedWriteBridgeError::RunNotRegistered,
        LifecycleObserverError::DeliveryFailed => GovernedWriteBridgeError::Gateway,
    }
}

fn map_process_observer_error_to_bridge(
    error: LifecycleObserverError,
) -> GovernedProcessBridgeError {
    match error {
        LifecycleObserverError::RunNoLongerLive => GovernedProcessBridgeError::RunNotRegistered,
        LifecycleObserverError::DeliveryFailed => GovernedProcessBridgeError::Gateway,
    }
}

fn map_write_observer_error_to_port(error: LifecycleObserverError) -> GovernedWriteExecutionError {
    match error {
        LifecycleObserverError::RunNoLongerLive | LifecycleObserverError::DeliveryFailed => {
            GovernedWriteExecutionError::LifecycleDeliveryFailed
        }
    }
}

fn map_process_observer_error_to_port(
    error: LifecycleObserverError,
) -> GovernedProcessExecutionError {
    match error {
        LifecycleObserverError::RunNoLongerLive | LifecycleObserverError::DeliveryFailed => {
            GovernedProcessExecutionError::LifecycleDeliveryFailed
        }
    }
}

fn map_read_outcome_error(error: GovernedOutcomeValidationError) -> GovernedReadExecutionError {
    match error {
        GovernedOutcomeValidationError::InvalidDeliveryShape
        | GovernedOutcomeValidationError::InvalidLifecycleOrder
        | GovernedOutcomeValidationError::InvalidTerminalShape
        | GovernedOutcomeValidationError::ConflictingProviderOutcome => {
            GovernedReadExecutionError::Unavailable
        }
    }
}

fn map_write_outcome_error(error: GovernedOutcomeValidationError) -> GovernedWriteExecutionError {
    match error {
        GovernedOutcomeValidationError::InvalidDeliveryShape
        | GovernedOutcomeValidationError::InvalidLifecycleOrder
        | GovernedOutcomeValidationError::InvalidTerminalShape
        | GovernedOutcomeValidationError::ConflictingProviderOutcome => {
            GovernedWriteExecutionError::Unavailable
        }
    }
}

fn map_process_outcome_error(
    error: GovernedOutcomeValidationError,
) -> GovernedProcessExecutionError {
    match error {
        GovernedOutcomeValidationError::InvalidDeliveryShape
        | GovernedOutcomeValidationError::InvalidLifecycleOrder
        | GovernedOutcomeValidationError::InvalidTerminalShape
        | GovernedOutcomeValidationError::ConflictingProviderOutcome => {
            GovernedProcessExecutionError::Unavailable
        }
    }
}

fn process_requested_payload(
    requested: &ProcessProposalRequestedView,
) -> ProcessProposalRequestedPayload {
    ProcessProposalRequestedPayload {
        proposal_id: requested.proposal_id.clone(),
        program: requested.program.clone(),
        argv: requested.argv.clone(),
        portable_cwd: requested.portable_cwd.clone(),
        summary: requested.summary.clone(),
        risk_class: requested.risk_class.into(),
        expires_at: requested.expires_at,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use keeper_agent_protocol::ProcessRiskClass as ProtocolRiskClass;

    use super::*;
    use crate::ProcessRiskClass;

    #[test]
    fn bridge_error_mappings_are_exhaustive() {
        for (source, expected) in [
            (
                GovernedReadBridgeError::RunNotRegistered,
                GovernedReadExecutionError::RunNotBound,
            ),
            (
                GovernedReadBridgeError::InvalidProposal,
                GovernedReadExecutionError::InvalidProposal,
            ),
            (
                GovernedReadBridgeError::ReservationConflict,
                GovernedReadExecutionError::ReservationConflict,
            ),
            (
                GovernedReadBridgeError::InProgress,
                GovernedReadExecutionError::InProgress,
            ),
            (
                GovernedReadBridgeError::InvalidContext,
                GovernedReadExecutionError::InvalidContext,
            ),
            (
                GovernedReadBridgeError::Gateway,
                GovernedReadExecutionError::Unavailable,
            ),
        ] {
            assert_eq!(map_read_bridge_error(source), expected);
        }

        for (source, expected) in [
            (
                GovernedWriteBridgeError::RunNotRegistered,
                GovernedWriteExecutionError::RunNotBound,
            ),
            (
                GovernedWriteBridgeError::InvalidProposal,
                GovernedWriteExecutionError::InvalidProposal,
            ),
            (
                GovernedWriteBridgeError::ReservationConflict,
                GovernedWriteExecutionError::ReservationConflict,
            ),
            (
                GovernedWriteBridgeError::Busy,
                GovernedWriteExecutionError::Busy,
            ),
            (
                GovernedWriteBridgeError::InvalidContext,
                GovernedWriteExecutionError::InvalidContext,
            ),
            (
                GovernedWriteBridgeError::NotLive,
                GovernedWriteExecutionError::NotLive,
            ),
            (
                GovernedWriteBridgeError::ShutDown,
                GovernedWriteExecutionError::ShuttingDown,
            ),
            (
                GovernedWriteBridgeError::Gateway,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::OwnershipMismatch,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::InvalidGrant,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::DecisionDenied,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::Expired,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::Cancelled,
                GovernedWriteExecutionError::Unavailable,
            ),
            (
                GovernedWriteBridgeError::Interrupted,
                GovernedWriteExecutionError::Unavailable,
            ),
        ] {
            assert_eq!(map_write_bridge_error(source), expected);
        }

        for (source, expected) in [
            (
                GovernedProcessBridgeError::RunNotRegistered,
                GovernedProcessExecutionError::RunNotBound,
            ),
            (
                GovernedProcessBridgeError::InvalidProposal,
                GovernedProcessExecutionError::InvalidProposal,
            ),
            (
                GovernedProcessBridgeError::ReservationConflict,
                GovernedProcessExecutionError::ReservationConflict,
            ),
            (
                GovernedProcessBridgeError::Busy,
                GovernedProcessExecutionError::Busy,
            ),
            (
                GovernedProcessBridgeError::InvalidContext,
                GovernedProcessExecutionError::InvalidContext,
            ),
            (
                GovernedProcessBridgeError::NotLive,
                GovernedProcessExecutionError::NotLive,
            ),
            (
                GovernedProcessBridgeError::ShutDown,
                GovernedProcessExecutionError::ShuttingDown,
            ),
            (
                GovernedProcessBridgeError::CleanupUnproven,
                GovernedProcessExecutionError::CleanupUnproven,
            ),
            (
                GovernedProcessBridgeError::Gateway,
                GovernedProcessExecutionError::Unavailable,
            ),
            (
                GovernedProcessBridgeError::OwnershipMismatch,
                GovernedProcessExecutionError::Unavailable,
            ),
            (
                GovernedProcessBridgeError::DecisionDenied,
                GovernedProcessExecutionError::Unavailable,
            ),
            (
                GovernedProcessBridgeError::Expired,
                GovernedProcessExecutionError::Unavailable,
            ),
            (
                GovernedProcessBridgeError::Cancelled,
                GovernedProcessExecutionError::Unavailable,
            ),
            (
                GovernedProcessBridgeError::Interrupted,
                GovernedProcessExecutionError::Unavailable,
            ),
        ] {
            assert_eq!(map_process_bridge_error(source), expected);
        }
    }

    #[test]
    fn outcome_validation_errors_are_exhaustive_and_unavailable() {
        for source in [
            GovernedOutcomeValidationError::InvalidDeliveryShape,
            GovernedOutcomeValidationError::InvalidLifecycleOrder,
            GovernedOutcomeValidationError::InvalidTerminalShape,
            GovernedOutcomeValidationError::ConflictingProviderOutcome,
        ] {
            assert_eq!(
                map_read_outcome_error(source),
                GovernedReadExecutionError::Unavailable
            );
            assert_eq!(
                map_write_outcome_error(source),
                GovernedWriteExecutionError::Unavailable
            );
            assert_eq!(
                map_process_outcome_error(source),
                GovernedProcessExecutionError::Unavailable
            );
        }
    }

    #[test]
    fn observer_errors_are_exhaustive_and_distinct_at_the_port() {
        for source in [
            LifecycleObserverError::RunNoLongerLive,
            LifecycleObserverError::DeliveryFailed,
        ] {
            assert_eq!(
                map_write_observer_error_to_port(source),
                GovernedWriteExecutionError::LifecycleDeliveryFailed
            );
            assert_eq!(
                map_process_observer_error_to_port(source),
                GovernedProcessExecutionError::LifecycleDeliveryFailed
            );
        }
    }

    #[test]
    fn process_requested_view_converts_to_protocol_payload() {
        let expires_at = Utc::now();
        let view = ProcessProposalRequestedView {
            proposal_id: "proposal-1".into(),
            program: "node".into(),
            argv: vec!["--version".into()],
            portable_cwd: Some(".".into()),
            summary: "Run node --version".into(),
            risk_class: ProcessRiskClass::High,
            expires_at,
        };

        let payload = process_requested_payload(&view);
        assert_eq!(payload.proposal_id, view.proposal_id);
        assert_eq!(payload.program, view.program);
        assert_eq!(payload.argv, view.argv);
        assert_eq!(payload.portable_cwd, view.portable_cwd);
        assert_eq!(payload.summary, view.summary);
        assert_eq!(payload.risk_class, ProtocolRiskClass::High);
        assert_eq!(payload.expires_at, expires_at);
    }
}
