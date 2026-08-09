//! Test doubles for subsequent migration steps.
//!
//! Production code must not depend on this module for silent no-ops. Adapters
//! that should never reach governed tools use [`PanicGovernedExecutionPort`];
//! scripted unit tests use [`CheckedScriptedGovernedExecutionPort`], which only
//! accepts outcomes already validated by [`crate::CheckedGovernedOutcome`].

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use keeper_agent_protocol::{
    ProviderProcessProposal, ProviderToolProposal, ProviderWriteProposal, RunId,
};

use crate::{
    CancellationProbe, CheckedGovernedOutcome, GovernedExecutionPort,
    GovernedProcessExecutionError, GovernedProcessLifecycle, GovernedProcessObserver,
    GovernedProcessOutcome, GovernedReadExecutionError, GovernedReadOutcome,
    GovernedWriteExecutionError, GovernedWriteLifecycle, GovernedWriteObserver,
    GovernedWriteOutcome, LifecycleObserverError,
};

/// Fixed cancel probe for tests. Live level-triggered via [`AtomicBool`].
#[derive(Debug, Default)]
pub struct FixedCancellationProbe {
    cancelled: AtomicBool,
}

impl FixedCancellationProbe {
    /// Create a probe that starts non-cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a probe with an initial cancelled state.
    pub fn with_cancelled(cancelled: bool) -> Self {
        Self {
            cancelled: AtomicBool::new(cancelled),
        }
    }

    /// Flip the live cancel flag (still level-triggered).
    pub fn set_cancelled(&self, cancelled: bool) {
        self.cancelled.store(cancelled, Ordering::SeqCst);
    }

    /// Access the underlying atomic for adapter-style wiring.
    pub fn as_atomic(&self) -> &AtomicBool {
        &self.cancelled
    }
}

impl CancellationProbe for FixedCancellationProbe {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Observer that records write lifecycle events for assertions.
#[derive(Debug, Default)]
pub struct RecordingWriteObserver {
    /// Number of Requested callbacks observed.
    pub requested_count: usize,
    /// Last error forced on the next observe call, if any.
    fail_next: Option<LifecycleObserverError>,
}

impl RecordingWriteObserver {
    /// Create an empty recording observer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fail the next `observe` with the given error, then clear.
    pub fn fail_next(&mut self, error: LifecycleObserverError) {
        self.fail_next = Some(error);
    }
}

impl GovernedWriteObserver for RecordingWriteObserver {
    fn observe(&mut self, event: GovernedWriteLifecycle<'_>) -> Result<(), LifecycleObserverError> {
        if let Some(err) = self.fail_next.take() {
            return Err(err);
        }
        match event {
            GovernedWriteLifecycle::Requested(_) => self.requested_count += 1,
        }
        Ok(())
    }
}

/// Observer that records process lifecycle events for assertions.
#[derive(Debug, Default)]
pub struct RecordingProcessObserver {
    /// Counts of each lifecycle kind, in order of delivery.
    pub events: Vec<&'static str>,
    fail_next: Option<LifecycleObserverError>,
}

impl RecordingProcessObserver {
    /// Create an empty recording observer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fail the next `observe` with the given error, then clear.
    pub fn fail_next(&mut self, error: LifecycleObserverError) {
        self.fail_next = Some(error);
    }
}

impl GovernedProcessObserver for RecordingProcessObserver {
    fn observe(
        &mut self,
        event: GovernedProcessLifecycle<'_>,
    ) -> Result<(), LifecycleObserverError> {
        if let Some(err) = self.fail_next.take() {
            return Err(err);
        }
        match event {
            GovernedProcessLifecycle::Requested(_) => self.events.push("requested"),
            GovernedProcessLifecycle::Decided(_) => self.events.push("decided"),
            GovernedProcessLifecycle::Started(_) => self.events.push("started"),
        }
        Ok(())
    }
}

/// Port double that panics on any execute call.
///
/// Use for adapter tests that must not reach governance.
#[derive(Debug, Default)]
pub struct PanicGovernedExecutionPort;

impl GovernedExecutionPort for PanicGovernedExecutionPort {
    fn execute_read(
        &self,
        _run_id: RunId,
        _proposal: ProviderToolProposal,
        _cancellation: &dyn CancellationProbe,
    ) -> Result<GovernedReadOutcome, GovernedReadExecutionError> {
        panic!("PanicGovernedExecutionPort::execute_read must not be called");
    }

    fn execute_write(
        &self,
        _run_id: RunId,
        _proposal: ProviderWriteProposal,
        _cancellation: &dyn CancellationProbe,
        _observer: &mut dyn GovernedWriteObserver,
    ) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError> {
        panic!("PanicGovernedExecutionPort::execute_write must not be called");
    }

    fn execute_process(
        &self,
        _run_id: RunId,
        _proposal: ProviderProcessProposal,
        _cancellation: &dyn CancellationProbe,
        _observer: &mut dyn GovernedProcessObserver,
    ) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError> {
        panic!("PanicGovernedExecutionPort::execute_process must not be called");
    }
}

/// One scripted read response. Outcome must already be checked-constructed.
#[derive(Debug)]
pub struct ScriptedRead {
    /// When set, the call's `run_id` must match.
    pub expect_run_id: Option<RunId>,
    /// Scripted result.
    pub result: Result<GovernedReadOutcome, GovernedReadExecutionError>,
}

/// One scripted write response.
#[derive(Debug)]
pub struct ScriptedWrite {
    /// When set, the call's `run_id` must match.
    pub expect_run_id: Option<RunId>,
    /// Optional Requested payload to deliver via the observer before returning.
    /// When `Some`, the host-like double invokes the observer once (still
    /// synchronous on the call thread). Replay scripts leave this `None`.
    pub deliver_requested: Option<keeper_agent_protocol::WriteProposalRequestedPayload>,
    /// Scripted result.
    pub result: Result<GovernedWriteOutcome, GovernedWriteExecutionError>,
}

/// One scripted process response.
#[derive(Debug)]
pub struct ScriptedProcess {
    /// When set, the call's `run_id` must match.
    pub expect_run_id: Option<RunId>,
    /// Optional lifecycle events to deliver in order before returning.
    pub lifecycle: Vec<ScriptedProcessLifecycle>,
    /// Scripted result.
    pub result: Result<GovernedProcessOutcome, GovernedProcessExecutionError>,
}

/// Scripted process lifecycle payload for test doubles.
#[derive(Debug, Clone)]
pub enum ScriptedProcessLifecycle {
    /// Deliver Requested.
    Requested(keeper_agent_protocol::ProcessProposalRequestedPayload),
    /// Deliver Decided.
    Decided(keeper_agent_protocol::ProcessProposalDecidedPayload),
    /// Deliver Started.
    Started(keeper_agent_protocol::ProcessStartedPayload),
}

/// Queue-driven port double. Unexpected extra/missing calls panic.
///
/// Outcomes in the queues must be built with
/// [`CheckedGovernedOutcome::try_from_receipt`] — there is no unchecked
/// constructor.
#[derive(Debug, Default)]
pub struct CheckedScriptedGovernedExecutionPort {
    reads: Mutex<VecDeque<ScriptedRead>>,
    writes: Mutex<VecDeque<ScriptedWrite>>,
    processes: Mutex<VecDeque<ScriptedProcess>>,
}

impl CheckedScriptedGovernedExecutionPort {
    /// Empty scripted port.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a checked read response.
    pub fn push_read(&self, scripted: ScriptedRead) {
        self.reads
            .lock()
            .expect("scripted reads lock")
            .push_back(scripted);
    }

    /// Enqueue a checked write response.
    pub fn push_write(&self, scripted: ScriptedWrite) {
        self.writes
            .lock()
            .expect("scripted writes lock")
            .push_back(scripted);
    }

    /// Enqueue a checked process response.
    pub fn push_process(&self, scripted: ScriptedProcess) {
        self.processes
            .lock()
            .expect("scripted processes lock")
            .push_back(scripted);
    }

    /// Assert every scripted response was consumed.
    pub fn assert_exhausted(&self) {
        let reads = self.reads.lock().expect("scripted reads lock");
        let writes = self.writes.lock().expect("scripted writes lock");
        let processes = self.processes.lock().expect("scripted processes lock");
        assert!(
            reads.is_empty() && writes.is_empty() && processes.is_empty(),
            "scripted port still has pending responses: reads={}, writes={}, processes={}",
            reads.len(),
            writes.len(),
            processes.len()
        );
    }
}

impl GovernedExecutionPort for CheckedScriptedGovernedExecutionPort {
    fn execute_read(
        &self,
        run_id: RunId,
        _proposal: ProviderToolProposal,
        _cancellation: &dyn CancellationProbe,
    ) -> Result<GovernedReadOutcome, GovernedReadExecutionError> {
        let next = self
            .reads
            .lock()
            .expect("scripted reads lock")
            .pop_front()
            .expect("CheckedScriptedGovernedExecutionPort: unexpected execute_read");
        if let Some(expected) = next.expect_run_id {
            assert_eq!(expected, run_id, "scripted read run_id mismatch");
        }
        next.result
    }

    fn execute_write(
        &self,
        run_id: RunId,
        _proposal: ProviderWriteProposal,
        _cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedWriteObserver,
    ) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError> {
        let next = self
            .writes
            .lock()
            .expect("scripted writes lock")
            .pop_front()
            .expect("CheckedScriptedGovernedExecutionPort: unexpected execute_write");
        if let Some(expected) = next.expect_run_id {
            assert_eq!(expected, run_id, "scripted write run_id mismatch");
        }
        if let Some(requested) = next.deliver_requested.as_ref() {
            observer
                .observe(GovernedWriteLifecycle::Requested(requested))
                .map_err(|_| GovernedWriteExecutionError::LifecycleDeliveryFailed)?;
        }
        next.result
    }

    fn execute_process(
        &self,
        run_id: RunId,
        _proposal: ProviderProcessProposal,
        _cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedProcessObserver,
    ) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError> {
        let next = self
            .processes
            .lock()
            .expect("scripted processes lock")
            .pop_front()
            .expect("CheckedScriptedGovernedExecutionPort: unexpected execute_process");
        if let Some(expected) = next.expect_run_id {
            assert_eq!(expected, run_id, "scripted process run_id mismatch");
        }
        for event in &next.lifecycle {
            let result = match event {
                ScriptedProcessLifecycle::Requested(p) => {
                    observer.observe(GovernedProcessLifecycle::Requested(p))
                }
                ScriptedProcessLifecycle::Decided(p) => {
                    observer.observe(GovernedProcessLifecycle::Decided(p))
                }
                ScriptedProcessLifecycle::Started(p) => {
                    observer.observe(GovernedProcessLifecycle::Started(p))
                }
            };
            result.map_err(|_| GovernedProcessExecutionError::LifecycleDeliveryFailed)?;
        }
        next.result
    }
}

/// Build a checked write outcome from a receipt, panicking on validation failure.
///
/// Test-only convenience so script authors fail loud at construction time.
pub fn checked_write_outcome(
    receipt: keeper_agent_protocol::GovernedWriteReceipt,
) -> GovernedWriteOutcome {
    GovernedWriteOutcome::try_from_receipt(receipt)
        .expect("testkit: write receipt failed outcome validation")
}

/// Build a checked process outcome from a receipt, panicking on validation failure.
pub fn checked_process_outcome(
    receipt: keeper_agent_protocol::GovernedProcessReceipt,
) -> GovernedProcessOutcome {
    GovernedProcessOutcome::try_from_receipt(receipt)
        .expect("testkit: process receipt failed outcome validation")
}

/// Build a checked read outcome from a receipt, panicking on validation failure.
pub fn checked_read_outcome(
    receipt: keeper_agent_protocol::GovernedReadReceipt,
) -> GovernedReadOutcome {
    GovernedReadOutcome::try_from_receipt(receipt)
        .expect("testkit: read receipt failed outcome validation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use keeper_agent_protocol::{
        FsWriteOutput, GovernedWriteDelivery, GovernedWriteReceipt, ProviderWriteProposal,
        WriteMode, WriteProposalRequestedPayload, WriteProposalResolvedPayload, WriteProposalState,
    };
    use uuid::Uuid;

    fn fixed_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
    }

    fn sample_write_outcome() -> GovernedWriteOutcome {
        checked_write_outcome(GovernedWriteReceipt {
            delivery: GovernedWriteDelivery::Executed,
            requested: Some(WriteProposalRequestedPayload {
                proposal_id: "wp-1".into(),
                path: "src/a.ts".into(),
                mode: WriteMode::Create,
                summary: "Create src/a.ts".into(),
                expires_at: fixed_time(),
            }),
            resolved: WriteProposalResolvedPayload {
                proposal_id: "wp-1".into(),
                path: "src/a.ts".into(),
                mode: WriteMode::Create,
                state: WriteProposalState::Succeeded,
                error_code: None,
            },
            provider_ok: Some(FsWriteOutput {
                path: "src/a.ts".into(),
                mode: WriteMode::Create,
                bytes_written: 1,
                content_digest:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            }),
            provider_error: None,
        })
    }

    #[test]
    fn scripted_write_delivers_requested_and_returns_checked_outcome() {
        let port = CheckedScriptedGovernedExecutionPort::new();
        let run_id = RunId(Uuid::nil());
        let requested = WriteProposalRequestedPayload {
            proposal_id: "wp-1".into(),
            path: "src/a.ts".into(),
            mode: WriteMode::Create,
            summary: "Create src/a.ts".into(),
            expires_at: fixed_time(),
        };
        port.push_write(ScriptedWrite {
            expect_run_id: Some(run_id),
            deliver_requested: Some(requested),
            result: Ok(sample_write_outcome()),
        });

        let mut observer = RecordingWriteObserver::new();
        let cancel = FixedCancellationProbe::new();
        let proposal = ProviderWriteProposal {
            call_id: "c1".into(),
            tool_id: "fs.write".into(),
            tool_version: 1,
            arguments: serde_json::json!({}),
        };
        let outcome = port
            .execute_write(run_id, proposal, &cancel, &mut observer)
            .expect("scripted write");
        assert_eq!(observer.requested_count, 1);
        assert_eq!(
            outcome.receipt().resolved.state,
            WriteProposalState::Succeeded
        );
        port.assert_exhausted();
    }

    #[test]
    fn cancel_probe_is_level_triggered() {
        let probe = FixedCancellationProbe::new();
        assert!(!probe.is_cancelled());
        probe.set_cancelled(true);
        assert!(probe.is_cancelled());
        probe.set_cancelled(false);
        assert!(!probe.is_cancelled());
    }

    #[test]
    fn atomic_bool_implements_cancellation_probe() {
        let flag = AtomicBool::new(false);
        assert!(!CancellationProbe::is_cancelled(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(CancellationProbe::is_cancelled(&flag));
    }
}
