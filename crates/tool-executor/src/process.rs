//! Host-side governed process gateway: seal → decide → claim → execute → receipt.
//!
//! Provider input is only a catalog program label, structured argv and optional
//! portable cwd. Native executable/cwd material and the clean environment stay
//! memory-only inside this gateway/runner boundary. SQLite receives digests and
//! typed terminal receipts only.
//!
//! Production construction always selects [`SupervisedProcessRunner`]. This crate
//! never spawns OS children itself. Arbitrary runner injection, counting doubles,
//! mutable ledger access, and post-seal timeout mutation are available only under
//! `cfg(test)` (unit-test surface).

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{PROCESS_ARGV_BYTE_CAP, PROCESS_ARGV_COUNT_CAP, RunId};
use keeper_policy::is_exec_terminal;
use keeper_policy::{ExecDecision, ExecProposalState};
use keeper_process_executor::{
    CancellationSignal as ProcessCancellationSignal, OutputChunk as RunnerOutputChunk,
    OutputObserver, OutputStreamTag, ProcessError, ProcessReceipt, ProcessSpec,
    SupervisedProcessRunner, build_output_chunks, neutralize_control_sequences, sensitive_name,
};
use keeper_storage_sqlite::{
    ExecDecisionOutcome, ExecProposalReserveInput, ExecProposalStoreError, ExecReservationOutcome,
    ExecRunClaimOutcome, ExecTerminalReceipt, MAX_RUN_LEASE_MS, SqliteExecProposalRepository,
    StoredExecProposal,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::ProjectCapability;

/// Fixed C4.0 public process catalog labels (host registry only).
pub const PROCESS_CATALOG: &[&str] = &["cargo", "rustc", "node", "python", "python3"];

/// Re-export process.run@1 argv bounds so callers and tests share one contract.
pub use keeper_agent_protocol::{
    PROCESS_ARGV_BYTE_CAP as PROCESS_RUN_ARGV_BYTE_CAP,
    PROCESS_ARGV_COUNT_CAP as PROCESS_RUN_ARGV_COUNT_CAP,
};

/// Labels and class markers that must never enter `process.run@1`.
const DENIED_LABELS: &[&str] = &[
    "git",
    "cmd",
    "powershell",
    "pwsh",
    "bash",
    "sh",
    "zsh",
    "fish",
    "wscript",
    "cscript",
    "mshta",
    "npm",
    "pnpm",
    "yarn",
    "npx",
];

const DENIED_EXTENSIONS: &[&str] = &[".cmd", ".bat", ".ps1", ".vbs", ".js", ".wsf", ".vbe"];

/// Default process timeout (matches process.run@1 manifest).
pub const PROCESS_TIMEOUT: Duration = Duration::from_millis(60_000);

/// Combined stdout/stderr capture budget (matches process.run@1 manifest).
pub const PROCESS_OUTPUT_BYTE_CAP: usize = 262_144;

/// Run-lease margin above the sealed timeout (milliseconds).
const RUN_LEASE_MARGIN_MS: u64 = 5_000;

/// Fixed public error token when cleanup cannot be proven.
pub const CLEANUP_UNPROVEN_PUBLIC_ERROR: &str = "process_cleanup_unproven";

/// Runner contract used inside the gateway.
///
/// Production construction always installs [`SupervisedProcessRunner`]. The
/// trait is never part of the ordinary public production surface.
trait ProcessRunner: Send + Sync {
    fn run(
        &self,
        spec: ProcessSpec,
        cancellation: ProcessCancellationSignal,
    ) -> Result<ProcessReceipt, ProcessError>;

    fn run_with_observer(
        &self,
        spec: ProcessSpec,
        cancellation: ProcessCancellationSignal,
        _observer: Option<OutputObserver>,
    ) -> Result<ProcessReceipt, ProcessError> {
        self.run(spec, cancellation)
    }
}

impl ProcessRunner for SupervisedProcessRunner {
    fn run(
        &self,
        spec: ProcessSpec,
        cancellation: ProcessCancellationSignal,
    ) -> Result<ProcessReceipt, ProcessError> {
        SupervisedProcessRunner::run(self, spec, cancellation)
    }

    fn run_with_observer(
        &self,
        spec: ProcessSpec,
        cancellation: ProcessCancellationSignal,
        observer: Option<OutputObserver>,
    ) -> Result<ProcessReceipt, ProcessError> {
        SupervisedProcessRunner::run_with_observer(self, spec, cancellation, observer)
    }
}

/// Counting wrapper used in spawn-authority unit tests (not a production export).
#[cfg(test)]
#[derive(Debug, Default)]
struct CountingProcessRunner {
    inner: SupervisedProcessRunner,
    spawns: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl CountingProcessRunner {
    fn new() -> Self {
        Self::default()
    }

    fn spawn_count(&self) -> usize {
        self.spawns.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl ProcessRunner for CountingProcessRunner {
    fn run(
        &self,
        spec: ProcessSpec,
        cancellation: ProcessCancellationSignal,
    ) -> Result<ProcessReceipt, ProcessError> {
        self.spawns
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.run(spec, cancellation)
    }
}

/// Fault-injecting runner for cleanup-unproven closed-path unit tests.
#[cfg(test)]
#[derive(Debug, Default)]
struct CleanupUnprovenRunner {
    spawns: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl CleanupUnprovenRunner {
    fn new() -> Self {
        Self::default()
    }

    fn spawn_count(&self) -> usize {
        self.spawns.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl ProcessRunner for CleanupUnprovenRunner {
    fn run(
        &self,
        _spec: ProcessSpec,
        _cancellation: ProcessCancellationSignal,
    ) -> Result<ProcessReceipt, ProcessError> {
        self.spawns
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(ProcessError::CleanupUnproven)
    }
}

/// Memory-only sealed process material.
///
/// Native executable and native cwd never leave this type's private fields.
/// Safe program/argv/portable-cwd display data and digests remain readable.
/// Debug redacts native paths and argv payloads.
#[derive(Clone)]
pub struct SealedProcessSpec {
    program: String,
    argv: Vec<String>,
    portable_cwd: Option<String>,
    argv_digest: String,
    proposal_fingerprint: String,
    /// Resolved absolute executable — gateway/runner memory only.
    executable: PathBuf,
    /// Resolved absolute cwd — gateway/runner memory only.
    native_cwd: PathBuf,
    timeout: Duration,
    output_byte_cap: usize,
}

impl SealedProcessSpec {
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Sanitized structured argv retained for review display (not native OS paths).
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    pub fn portable_cwd(&self) -> Option<&str> {
        self.portable_cwd.as_deref()
    }

    pub fn argv_digest(&self) -> &str {
        &self.argv_digest
    }

    pub fn proposal_fingerprint(&self) -> &str {
        &self.proposal_fingerprint
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn output_byte_cap(&self) -> usize {
        self.output_byte_cap
    }

    /// Private unit-test seam only: accelerate timeout without a production
    /// mutator. Not present on the ordinary public surface (integration tests
    /// and normal builds cannot call this). Production seals always retain
    /// [`PROCESS_TIMEOUT`].
    #[cfg(test)]
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl std::fmt::Debug for SealedProcessSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedProcessSpec")
            .field("program", &self.program)
            .field("argv", &"<redacted>")
            .field("portable_cwd", &self.portable_cwd)
            .field("argv_digest", &self.argv_digest)
            .field("proposal_fingerprint", &self.proposal_fingerprint)
            .field("executable", &"<redacted>")
            .field("native_cwd", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("output_byte_cap", &self.output_byte_cap)
            .finish()
    }
}

/// Durable metadata plus memory-only sealed material for review/execution.
#[derive(Clone)]
pub struct PreparedExecProposal {
    pub metadata: StoredExecProposal,
    pub sealed: SealedProcessSpec,
}

impl std::fmt::Debug for PreparedExecProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedExecProposal")
            .field("metadata", &self.metadata)
            .field("sealed", &self.sealed)
            .finish()
    }
}

/// Outcome of a governed claim/execute attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernedProcessOutcome {
    Succeeded(StoredExecProposal),
    Failed(StoredExecProposal),
    Cancelled(StoredExecProposal),
    Busy(StoredExecProposal),
    Terminal(StoredExecProposal),
    Expired(StoredExecProposal),
    Conflict(StoredExecProposal),
    Rejected(StoredExecProposal),
}

impl GovernedProcessOutcome {
    pub fn metadata(&self) -> &StoredExecProposal {
        match self {
            Self::Succeeded(row)
            | Self::Failed(row)
            | Self::Cancelled(row)
            | Self::Busy(row)
            | Self::Terminal(row)
            | Self::Expired(row)
            | Self::Conflict(row)
            | Self::Rejected(row) => row,
        }
    }
}

/// Memory-only bounded process output retained after a fresh spawn.
///
/// Never written to SQLite. After host restart this is absent; durable metadata
/// remains and callers must report that live output was not retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessOutput {
    pub chunks: Vec<ProcessOutputChunk>,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub captured_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutputChunk {
    pub stream: ProcessOutputStream,
    pub sequence: u64,
    pub lossy_utf8: bool,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputStream {
    Stdout,
    Stderr,
}

/// Fresh execute result: durable outcome plus optional memory-only live output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedProcessExecution {
    pub outcome: GovernedProcessOutcome,
    /// Present only after a fresh spawn that produced capture. Absent on
    /// busy/replay/pre-spawn terminals and after restart.
    pub live_output: Option<LiveProcessOutput>,
}

impl GovernedProcessExecution {
    fn without_output(outcome: GovernedProcessOutcome) -> Self {
        Self {
            outcome,
            live_output: None,
        }
    }

    fn with_output(outcome: GovernedProcessOutcome, live_output: LiveProcessOutput) -> Self {
        Self {
            outcome,
            live_output: Some(live_output),
        }
    }

    pub fn metadata(&self) -> &StoredExecProposal {
        self.outcome.metadata()
    }
}

#[derive(Debug, Error)]
pub enum GovernedProcessError {
    #[error("process proposal store error")]
    Store(#[from] ExecProposalStoreError),
    #[error("project capability root could not be established")]
    InvalidProjectRoot,
    #[error("process.run@1 program/argv bounds or character contract violated")]
    InvalidArguments,
    #[error("process program label is not in the public catalog")]
    UnknownProgram,
    #[error("process program label is denied by the public catalog")]
    DeniedProgram,
    #[error("process program executable is unavailable")]
    ProgramUnavailable,
    #[error("process cwd was rejected")]
    CwdRejected,
    #[error("sealed process material does not match durable proposal metadata")]
    FingerprintMismatch,
    #[error("sealed executable or cwd drifted before execution")]
    DriftDetected,
    #[error("process proposal is not available for execution")]
    NotApplicable,
    #[error("process reservation identity does not match project capability")]
    CapabilityMismatch,
    #[error("process environment construction rejected sensitive names")]
    EnvironmentRejected,
    #[error("process cleanup could not be proven")]
    CleanupUnproven,
}

/// In-process governed process gateway.
pub struct GovernedProcessToolGateway {
    store: SqliteExecProposalRepository,
    runner: Arc<dyn ProcessRunner>,
}

impl GovernedProcessToolGateway {
    /// Open a production gateway bound to [`SupervisedProcessRunner`].
    ///
    /// There is no public production path that accepts an arbitrary
    /// [`ProcessRunner`] implementor.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GovernedProcessError> {
        Ok(Self {
            store: SqliteExecProposalRepository::open(path)?,
            runner: Arc::new(SupervisedProcessRunner::new()),
        })
    }

    /// Unit-test-only construction with a runner double. Not present on the
    /// production public surface (integration tests and normal builds cannot
    /// call this).
    #[cfg(test)]
    fn open_with_runner(
        path: impl AsRef<Path>,
        runner: Arc<dyn ProcessRunner>,
    ) -> Result<Self, GovernedProcessError> {
        Ok(Self {
            store: SqliteExecProposalRepository::open(path)?,
            runner,
        })
    }

    /// Unit-test-only mutable ledger access. Production callers never receive
    /// the repository (or any mutation-capable trait/object) through the
    /// gateway. Inspection for normal consumers is [`Self::get_proposal`].
    #[cfg(test)]
    fn store(&self) -> &SqliteExecProposalRepository {
        &self.store
    }

    /// Narrow read-only proposal inspection: metadata-only DTO, no reserve /
    /// decide / claim / complete surface.
    ///
    /// Prefer [`Self::get_for_owner`] when a project capability is available so
    /// foreign project rows fail closed.
    pub fn get_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredExecProposal>, GovernedProcessError> {
        Ok(self.store.get(proposal_id)?)
    }

    /// Capability- and conversation-bound proposal read. Foreign project or
    /// conversation identity fails closed (`None` / ownership error path).
    pub fn get_for_owner(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<Option<StoredExecProposal>, GovernedProcessError> {
        let Some(row) = self.store.get(proposal_id)? else {
            return Ok(None);
        };
        if row.project_id != capability.project_id() || row.conversation_id != conversation_id {
            return Err(GovernedProcessError::CapabilityMismatch);
        }
        Ok(Some(row))
    }

    /// Capability- and conversation-bound proposal list (metadata only).
    pub fn list_for_conversation(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
    ) -> Result<Vec<StoredExecProposal>, GovernedProcessError> {
        let rows = self.store.list_for_conversation(conversation_id)?;
        Ok(rows
            .into_iter()
            .filter(|row| row.project_id == capability.project_id())
            .collect())
    }

    /// Fixed startup recovery: every non-terminal proposal → interrupted.
    /// Never replays authority. Does not expose the repository or path.
    pub fn recover_interrupted(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> Result<Vec<StoredExecProposal>, GovernedProcessError> {
        Ok(self.store.recover_interrupted(now)?)
    }

    /// Capability-bound expire of a pending proposal.
    pub fn expire_proposal(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<StoredExecProposal, GovernedProcessError> {
        self.transition_owned(
            capability,
            conversation_id,
            proposal_id,
            ExecProposalState::Expired,
        )
    }

    /// Capability-bound cancel of a non-running proposal (pending/approved).
    pub fn cancel_proposal(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<StoredExecProposal, GovernedProcessError> {
        self.transition_owned(
            capability,
            conversation_id,
            proposal_id,
            ExecProposalState::Cancelled,
        )
    }

    /// Capability-bound interrupt of a non-running proposal (pending/approved).
    /// Running rows must use the execute cancellation path / run owner.
    pub fn interrupt_proposal(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<StoredExecProposal, GovernedProcessError> {
        self.transition_owned(
            capability,
            conversation_id,
            proposal_id,
            ExecProposalState::Interrupted,
        )
    }

    fn transition_owned(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
        next: ExecProposalState,
    ) -> Result<StoredExecProposal, GovernedProcessError> {
        let Some(current) = self.store.get(proposal_id)? else {
            return Err(GovernedProcessError::NotApplicable);
        };
        if current.project_id != capability.project_id() {
            return Err(GovernedProcessError::CapabilityMismatch);
        }
        if current.conversation_id != conversation_id {
            return Err(GovernedProcessError::NotApplicable);
        }
        Ok(self.store.transition(proposal_id, next, Utc::now())?)
    }

    /// Resolve the catalog label, portable cwd and native material; seal in memory.
    ///
    /// Enforces the full `process.run@1` argument contract at this production
    /// boundary: program label grammar, ≤64 argv entries, ≤32 KiB combined
    /// program/argv text, no NUL/control characters, and portable cwd rules.
    /// There is no alternate raw `&[String]` entry that skips these checks.
    pub fn seal(
        capability: &ProjectCapability,
        program: &str,
        argv: &[String],
        portable_cwd: Option<&str>,
    ) -> Result<SealedProcessSpec, GovernedProcessError> {
        validate_process_run_arguments(program, argv, portable_cwd)?;
        validate_program_catalog(program)?;
        let root = revalidate_root(capability)?;
        let executable = resolve_program_executable(program)?;
        let native_cwd = resolve_portable_cwd(&root, portable_cwd)?;
        let argv_digest = argv_digest(argv);
        let proposal_fingerprint =
            process_fingerprint(program, argv, portable_cwd, &executable, &native_cwd);

        Ok(SealedProcessSpec {
            program: program.to_owned(),
            argv: argv.to_vec(),
            portable_cwd: portable_cwd.map(str::to_owned),
            argv_digest,
            proposal_fingerprint,
            executable,
            native_cwd,
            timeout: PROCESS_TIMEOUT,
            output_byte_cap: PROCESS_OUTPUT_BYTE_CAP,
        })
    }

    /// Persist digests/metadata only and return the prepared proposal.
    ///
    /// Reservation is bound to `capability`: project/conversation/run/call
    /// identities are derived from the capability and the explicit scoped
    /// arguments. Callers cannot inject a foreign project id.
    pub fn reserve(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        run_id: RunId,
        call_id: &str,
        sealed: &SealedProcessSpec,
    ) -> Result<PreparedExecProposal, GovernedProcessError> {
        if capability.project_id().is_empty() {
            return Err(GovernedProcessError::InvalidProjectRoot);
        }
        if conversation_id.is_empty() || call_id.is_empty() {
            return Err(GovernedProcessError::InvalidArguments);
        }
        // Re-seal check: catalog still holds and fingerprints stay stable.
        let again = Self::seal(
            capability,
            sealed.program(),
            sealed.argv(),
            sealed.portable_cwd(),
        )?;
        if again.proposal_fingerprint != sealed.proposal_fingerprint
            || again.argv_digest != sealed.argv_digest
            || again.executable != sealed.executable
            || again.native_cwd != sealed.native_cwd
        {
            return Err(GovernedProcessError::DriftDetected);
        }

        let input = ExecProposalReserveInput {
            proposal_id: Uuid::new_v4().to_string(),
            reservation_key: format!(
                "ep:{}:{}:{}:{}",
                capability.project_id(),
                conversation_id,
                run_id,
                call_id
            ),
            proposal_fingerprint: sealed.proposal_fingerprint.clone(),
            project_id: capability.project_id().to_owned(),
            conversation_id: conversation_id.to_owned(),
            run_id,
            call_id: call_id.to_owned(),
            program: sealed.program.clone(),
            argv_digest: sealed.argv_digest.clone(),
            portable_cwd: sealed.portable_cwd.clone(),
        };

        match self.store.reserve(&input)? {
            ExecReservationOutcome::Reserved(metadata) => Ok(PreparedExecProposal {
                metadata,
                sealed: sealed.clone(),
            }),
            ExecReservationOutcome::Replay(metadata)
                if metadata.proposal_fingerprint == sealed.proposal_fingerprint
                    && metadata.project_id == capability.project_id()
                    && metadata.conversation_id == conversation_id =>
            {
                Ok(PreparedExecProposal {
                    metadata,
                    sealed: sealed.clone(),
                })
            }
            // Deterministic reservation key + matching fingerprint: terminal
            // identity replay. Store returns Conflict when a new proposal_id is
            // minted for an existing (project, conversation, run, call).
            ExecReservationOutcome::Conflict(metadata)
                if metadata.proposal_fingerprint == sealed.proposal_fingerprint
                    && metadata.project_id == capability.project_id()
                    && metadata.conversation_id == conversation_id
                    && metadata.run_id == run_id
                    && metadata.call_id == call_id
                    && metadata.reservation_key == input.reservation_key
                    && is_exec_terminal(metadata.state) =>
            {
                Ok(PreparedExecProposal {
                    metadata,
                    sealed: sealed.clone(),
                })
            }
            ExecReservationOutcome::Replay(_)
            | ExecReservationOutcome::Conflict(_)
            | ExecReservationOutcome::Busy(_) => Err(GovernedProcessError::NotApplicable),
        }
    }

    pub fn decide(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        decision: ExecDecision,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
    ) -> Result<ExecDecisionOutcome, GovernedProcessError> {
        Ok(self.store.decide(
            proposal_id,
            conversation_id,
            decision,
            approval_grant_digest,
            proposal_fingerprint,
            Utc::now(),
        )?)
    }

    /// Capability-bound decide: verifies project ownership before the durable
    /// conversation-scoped decision.
    pub fn decide_for_owner(
        &self,
        capability: &ProjectCapability,
        conversation_id: &str,
        proposal_id: &str,
        decision: ExecDecision,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
    ) -> Result<ExecDecisionOutcome, GovernedProcessError> {
        let Some(current) = self.store.get(proposal_id)? else {
            return Err(GovernedProcessError::NotApplicable);
        };
        if current.project_id != capability.project_id() {
            return Err(GovernedProcessError::CapabilityMismatch);
        }
        if current.conversation_id != conversation_id {
            return Err(GovernedProcessError::NotApplicable);
        }
        self.decide(
            proposal_id,
            conversation_id,
            decision,
            approval_grant_digest,
            proposal_fingerprint,
        )
    }

    /// Claim once, re-resolve for drift, execute through the supervised runner,
    /// and persist a typed terminal receipt. Non-`Claimed` outcomes never spawn.
    ///
    /// Fresh spawn paths return memory-only [`LiveProcessOutput`]; SQLite keeps
    /// digests/metadata only.
    pub fn execute(
        &self,
        capability: &ProjectCapability,
        proposal_id: &str,
        sealed: &SealedProcessSpec,
        cancellation: ProcessCancellationSignal,
    ) -> Result<GovernedProcessExecution, GovernedProcessError> {
        self.execute_with_observer(capability, proposal_id, sealed, cancellation, None)
    }

    /// Execute using a data-only observer of retained display chunks.
    ///
    /// Claiming, spawning, cancellation and terminal persistence are identical
    /// to [`Self::execute`]; the observer has no authority over any outcome.
    pub fn execute_with_observer(
        &self,
        capability: &ProjectCapability,
        proposal_id: &str,
        sealed: &SealedProcessSpec,
        cancellation: ProcessCancellationSignal,
        observer: Option<OutputObserver>,
    ) -> Result<GovernedProcessExecution, GovernedProcessError> {
        if capability.project_id().is_empty() {
            return Err(GovernedProcessError::InvalidProjectRoot);
        }

        let Some(current) = self.store.get(proposal_id)? else {
            return Err(GovernedProcessError::NotApplicable);
        };
        if current.project_id != capability.project_id() {
            return Err(GovernedProcessError::CapabilityMismatch);
        }
        if current.proposal_fingerprint != sealed.proposal_fingerprint
            || current.argv_digest != sealed.argv_digest
            || current.program != sealed.program
            || current.portable_cwd != sealed.portable_cwd
        {
            return Err(GovernedProcessError::FingerprintMismatch);
        }

        let owner = format!("process-gateway-{}", Uuid::new_v4());
        let lease_ms = run_lease_ms(sealed.timeout);
        let now = Utc::now();
        let claim = self.store.claim_run(proposal_id, &owner, lease_ms, now)?;

        let _claimed = match claim {
            ExecRunClaimOutcome::Claimed(row) => row,
            ExecRunClaimOutcome::Busy(row) => {
                return Ok(GovernedProcessExecution::without_output(
                    GovernedProcessOutcome::Busy(row),
                ));
            }
            ExecRunClaimOutcome::Terminal(row) => {
                // Classify durable terminals so callers do not re-spawn.
                // Terminal replay has no retained transcript.
                return Ok(GovernedProcessExecution::without_output(match row.state {
                    ExecProposalState::Rejected => GovernedProcessOutcome::Rejected(row),
                    ExecProposalState::Expired => GovernedProcessOutcome::Expired(row),
                    _ => GovernedProcessOutcome::Terminal(row),
                }));
            }
            ExecRunClaimOutcome::Expired(row) => {
                return Ok(GovernedProcessExecution::without_output(
                    GovernedProcessOutcome::Expired(row),
                ));
            }
            ExecRunClaimOutcome::Conflict(row) => {
                if row.state == ExecProposalState::Rejected {
                    return Ok(GovernedProcessExecution::without_output(
                        GovernedProcessOutcome::Rejected(row),
                    ));
                }
                return Ok(GovernedProcessExecution::without_output(
                    GovernedProcessOutcome::Conflict(row),
                ));
            }
        };

        // Only the first Claimed path may build a ProcessSpec or invoke the runner.
        if let Err(error) = self.revalidate_sealed(capability, sealed) {
            return self
                .fail_claimed(proposal_id, &owner, empty_failure_receipt(0), error)
                .map(GovernedProcessExecution::without_output);
        }

        if cancellation.is_cancelled() {
            let receipt = ExecTerminalReceipt {
                exit_code: None,
                duration_ms: 0,
                stdout_sha256: empty_digest(),
                stderr_sha256: empty_digest(),
                captured_bytes: 0,
                truncated: false,
                encoding_lossy: false,
                cancelled: true,
                timed_out: false,
            };
            let row = self.store.complete_run(
                proposal_id,
                &owner,
                ExecProposalState::Cancelled,
                &receipt,
                Utc::now(),
            )?;
            return Ok(GovernedProcessExecution::without_output(
                GovernedProcessOutcome::Cancelled(row),
            ));
        }

        let env = match build_clean_environment(&sealed.executable) {
            Ok(env) => env,
            Err(error) => {
                return self
                    .fail_claimed(proposal_id, &owner, empty_failure_receipt(0), error)
                    .map(GovernedProcessExecution::without_output);
            }
        };

        let spec = ProcessSpec {
            executable: sealed.executable.clone(),
            argv: sealed.argv.iter().map(OsString::from).collect(),
            cwd: sealed.native_cwd.clone(),
            env,
            timeout: sealed.timeout,
            output_byte_cap: sealed.output_byte_cap,
        };

        match self.runner.run_with_observer(spec, cancellation, observer) {
            Ok(receipt) => self.complete_from_receipt(proposal_id, &owner, receipt),
            Err(ProcessError::CleanupUnproven) => {
                let terminal = self.store.complete_run(
                    proposal_id,
                    &owner,
                    ExecProposalState::Failed,
                    &empty_failure_receipt(0),
                    Utc::now(),
                )?;
                // Typed failure is durable; surface the fixed public error to callers.
                let _ = terminal;
                let _ = CLEANUP_UNPROVEN_PUBLIC_ERROR;
                Err(GovernedProcessError::CleanupUnproven)
            }
            Err(ProcessError::EnvironmentRejected) => self
                .fail_claimed(
                    proposal_id,
                    &owner,
                    empty_failure_receipt(0),
                    GovernedProcessError::EnvironmentRejected,
                )
                .map(GovernedProcessExecution::without_output),
            Err(_) => self
                .fail_claimed(
                    proposal_id,
                    &owner,
                    empty_failure_receipt(0),
                    GovernedProcessError::ProgramUnavailable,
                )
                .map(GovernedProcessExecution::without_output),
        }
    }

    fn revalidate_sealed(
        &self,
        capability: &ProjectCapability,
        sealed: &SealedProcessSpec,
    ) -> Result<(), GovernedProcessError> {
        let again = Self::seal(
            capability,
            sealed.program(),
            sealed.argv(),
            sealed.portable_cwd(),
        )?;
        if again.executable != sealed.executable || again.native_cwd != sealed.native_cwd {
            return Err(GovernedProcessError::DriftDetected);
        }
        if again.proposal_fingerprint != sealed.proposal_fingerprint
            || again.argv_digest != sealed.argv_digest
        {
            return Err(GovernedProcessError::DriftDetected);
        }
        Ok(())
    }

    fn complete_from_receipt(
        &self,
        proposal_id: &str,
        owner: &str,
        receipt: ProcessReceipt,
    ) -> Result<GovernedProcessExecution, GovernedProcessError> {
        if !receipt.cleanup_proven {
            let _ = self.store.complete_run(
                proposal_id,
                owner,
                ExecProposalState::Failed,
                &empty_failure_receipt(receipt.duration.as_millis() as u64),
                Utc::now(),
            )?;
            return Err(GovernedProcessError::CleanupUnproven);
        }

        let live_output = live_output_from_receipt(&receipt);
        let terminal = receipt_to_terminal(&receipt);
        let next = if receipt.cancelled {
            ExecProposalState::Cancelled
        } else if receipt.timed_out || receipt.exit_code != Some(0) {
            ExecProposalState::Failed
        } else {
            ExecProposalState::Succeeded
        };

        let row = self
            .store
            .complete_run(proposal_id, owner, next, &terminal, Utc::now())?;
        let outcome = match next {
            ExecProposalState::Succeeded => GovernedProcessOutcome::Succeeded(row),
            ExecProposalState::Cancelled => GovernedProcessOutcome::Cancelled(row),
            _ => GovernedProcessOutcome::Failed(row),
        };
        Ok(GovernedProcessExecution::with_output(outcome, live_output))
    }

    fn fail_claimed(
        &self,
        proposal_id: &str,
        owner: &str,
        receipt: ExecTerminalReceipt,
        source: GovernedProcessError,
    ) -> Result<GovernedProcessOutcome, GovernedProcessError> {
        let row = self.store.complete_run(
            proposal_id,
            owner,
            ExecProposalState::Failed,
            &receipt,
            Utc::now(),
        )?;
        match source {
            GovernedProcessError::CleanupUnproven => Err(GovernedProcessError::CleanupUnproven),
            _ => Ok(GovernedProcessOutcome::Failed(row)),
        }
    }
}

/// Validate the exact `process.run@1` program/argv/cwd argument contract.
///
/// Enforced at the production seal boundary before catalog resolution:
/// - program label `^[a-z][a-z0-9_-]{0,31}$`
/// - at most [`PROCESS_ARGV_COUNT_CAP`] argv entries
/// - at most [`PROCESS_ARGV_BYTE_CAP`] combined program + argv UTF-8 bytes
/// - no NUL or other control characters in program or argv
/// - portable cwd grammar when present
pub fn validate_process_run_arguments(
    program: &str,
    argv: &[String],
    portable_cwd: Option<&str>,
) -> Result<(), GovernedProcessError> {
    validate_process_program_label(program)?;
    if argv.len() > PROCESS_ARGV_COUNT_CAP {
        return Err(GovernedProcessError::InvalidArguments);
    }
    let mut combined = program.len();
    for entry in argv {
        if entry.chars().any(char::is_control) {
            return Err(GovernedProcessError::InvalidArguments);
        }
        combined = combined.saturating_add(entry.len());
        if combined > PROCESS_ARGV_BYTE_CAP {
            return Err(GovernedProcessError::InvalidArguments);
        }
    }
    if let Some(cwd) = portable_cwd {
        validate_portable_cwd_grammar(cwd)?;
    }
    Ok(())
}

/// Program label grammar: `^[a-z][a-z0-9_-]{0,31}$` (1..=32 ASCII chars).
fn validate_process_program_label(program: &str) -> Result<(), GovernedProcessError> {
    let bytes = program.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return Err(GovernedProcessError::InvalidArguments);
    }
    if program.chars().any(char::is_control) {
        return Err(GovernedProcessError::InvalidArguments);
    }
    let first = bytes[0];
    if !first.is_ascii_lowercase() {
        return Err(GovernedProcessError::InvalidArguments);
    }
    if !bytes[1..].iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
    }) {
        return Err(GovernedProcessError::InvalidArguments);
    }
    Ok(())
}

/// Portable cwd grammar shared with process.run@1 / project-relative paths.
fn validate_portable_cwd_grammar(path: &str) -> Result<(), GovernedProcessError> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > 4_096 {
        return Err(GovernedProcessError::CwdRejected);
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(GovernedProcessError::CwdRejected);
    }
    if path.chars().any(char::is_control) {
        return Err(GovernedProcessError::CwdRejected);
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(GovernedProcessError::CwdRejected);
        }
        if component.len() > 255 {
            return Err(GovernedProcessError::CwdRejected);
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(GovernedProcessError::CwdRejected);
        }
        if component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        }) {
            return Err(GovernedProcessError::CwdRejected);
        }
    }
    Ok(())
}

/// Catalog check used by seal and tests.
///
/// Callers that only need catalog membership still go through program-label
/// grammar first when entering via [`GovernedProcessToolGateway::seal`].
pub fn validate_program_catalog(program: &str) -> Result<(), GovernedProcessError> {
    // Path-shaped / absolute labels are denied before unknown/allow branches.
    if program.is_empty()
        || program.len() > 32
        || program.contains('/')
        || program.contains('\\')
        || program.contains(':')
        || program.contains('\0')
        || Path::new(program).components().count() != 1
    {
        return Err(GovernedProcessError::DeniedProgram);
    }

    // Catalog comparison is exact on the fixed lowercase allowlist; mixed-case
    // labels fail the protocol grammar at seal and never reach resolution.
    if DENIED_LABELS.contains(&program)
        || DENIED_EXTENSIONS
            .iter()
            .any(|ext| program.ends_with(ext) || program.contains(ext))
    {
        return Err(GovernedProcessError::DeniedProgram);
    }

    // Script-shim basenames such as npm.cmd / run.ps1 (extension stripped form).
    if program.contains('.') {
        return Err(GovernedProcessError::DeniedProgram);
    }

    if PROCESS_CATALOG.contains(&program) {
        Ok(())
    } else {
        Err(GovernedProcessError::UnknownProgram)
    }
}

/// Build the C4.0 clean child environment: allowlisted base names only,
/// filtered PATH, isolated TEMP/TMP, no sensitive names.
pub fn build_clean_environment(
    resolved_executable: &Path,
) -> Result<BTreeMap<OsString, OsString>, GovernedProcessError> {
    let mut env = BTreeMap::new();

    for key in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"] {
        if sensitive_name(OsStr::new(key)) {
            return Err(GovernedProcessError::EnvironmentRejected);
        }
        if let Some(value) = std::env::var_os(key) {
            env.insert(OsString::from(key), value);
        }
    }

    let temp = std::env::temp_dir();
    env.insert(OsString::from("TEMP"), temp.as_os_str().to_os_string());
    env.insert(OsString::from("TMP"), temp.as_os_str().to_os_string());

    let path = filtered_path(resolved_executable);
    env.insert(OsString::from("PATH"), path);

    // Defense in depth: never ship a sensitive key even if base list expands.
    if env.keys().any(|name| sensitive_name(name)) {
        return Err(GovernedProcessError::EnvironmentRejected);
    }
    Ok(env)
}

fn filtered_path(resolved_executable: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SYSTEMROOT") {
            let root = PathBuf::from(root);
            dirs.push(root.join("System32"));
            dirs.push(root);
        }
        dirs.push(PathBuf::from(r"C:\Windows\System32"));
        dirs.push(PathBuf::from(r"C:\Windows"));
    }

    if let Some(parent) = resolved_executable.parent() {
        dirs.push(parent.to_path_buf());
    }

    // Include parent PATH entries that look like toolchain roots for the catalog.
    if let Some(parent_path) = std::env::var_os("PATH") {
        for entry in std::env::split_paths(&parent_path) {
            if is_toolchain_path_entry(&entry) {
                dirs.push(entry);
            }
        }
    }

    dirs.sort();
    dirs.dedup();
    std::env::join_paths(dirs.iter().filter(|p| p.is_dir())).unwrap_or_default()
}

fn is_toolchain_path_entry(path: &Path) -> bool {
    let text = path.to_string_lossy().to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        r"\.cargo\bin",
        "/.cargo/bin",
        r"\nodejs",
        "/nodejs",
        r"\python",
        "/python",
        r"\rustup",
        "/rustup",
        r"\programs\python",
        "/programs/python",
    ];
    MARKERS.iter().any(|marker| text.contains(marker))
}

fn resolve_program_executable(program: &str) -> Result<PathBuf, GovernedProcessError> {
    validate_program_catalog(program)?;
    // Catalog labels are fixed lowercase; resolution uses the accepted label.
    let label = program;

    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent_path) = std::env::var_os("PATH") {
        for entry in std::env::split_paths(&parent_path) {
            if entry.is_dir() {
                search_dirs.push(entry);
            }
        }
    }
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SYSTEMROOT") {
            search_dirs.push(PathBuf::from(root).join("System32"));
        }
    }

    for dir in search_dirs {
        for candidate in candidate_names(label) {
            let path = dir.join(&candidate);
            if path.is_file() {
                return path
                    .canonicalize()
                    .map_err(|_| GovernedProcessError::ProgramUnavailable);
            }
        }
    }

    // where.exe / PATHEXT fallback via std::process is forbidden here; fail closed.
    Err(GovernedProcessError::ProgramUnavailable)
}

fn candidate_names(label: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            format!("{label}.exe"),
            label.to_owned(),
            format!("{label}.EXE"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![label.to_owned()]
    }
}

fn resolve_portable_cwd(
    root: &Path,
    portable_cwd: Option<&str>,
) -> Result<PathBuf, GovernedProcessError> {
    let relative = portable_cwd.unwrap_or(".");
    validate_portable_cwd_grammar(relative)?;

    let mut current = root.to_owned();
    if relative != "." {
        for component in relative.split('/') {
            current.push(component);
            let metadata = current
                .symlink_metadata()
                .map_err(|_| GovernedProcessError::CwdRejected)?;
            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                return Err(GovernedProcessError::CwdRejected);
            }
            let canonical = current
                .canonicalize()
                .map_err(|_| GovernedProcessError::CwdRejected)?;
            if !canonical.starts_with(root) {
                return Err(GovernedProcessError::CwdRejected);
            }
            current = canonical;
        }
    } else {
        current = root
            .canonicalize()
            .map_err(|_| GovernedProcessError::CwdRejected)?;
    }

    if !current.is_dir() {
        return Err(GovernedProcessError::CwdRejected);
    }
    Ok(current)
}

fn revalidate_root(capability: &ProjectCapability) -> Result<PathBuf, GovernedProcessError> {
    let root = capability
        .canonical_root()
        .canonicalize()
        .map_err(|_| GovernedProcessError::InvalidProjectRoot)?;
    if root != capability.canonical_root() || !root.is_dir() {
        return Err(GovernedProcessError::InvalidProjectRoot);
    }
    Ok(root)
}

fn argv_digest(argv: &[String]) -> String {
    let encoded = serde_json::to_string(argv).unwrap_or_else(|_| "[]".into());
    sha256_hex(encoded.as_bytes())
}

fn process_fingerprint(
    program: &str,
    argv: &[String],
    portable_cwd: Option<&str>,
    executable: &Path,
    native_cwd: &Path,
) -> String {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"process.run@1\0");
    payload.extend_from_slice(program.as_bytes());
    payload.push(0);
    payload.extend_from_slice(
        serde_json::to_string(argv)
            .unwrap_or_else(|_| "[]".into())
            .as_bytes(),
    );
    payload.push(0);
    payload.extend_from_slice(portable_cwd.unwrap_or(".").as_bytes());
    payload.push(0);
    payload.extend_from_slice(executable.to_string_lossy().as_bytes());
    payload.push(0);
    payload.extend_from_slice(native_cwd.to_string_lossy().as_bytes());
    sha256_hex(&payload)
}

fn receipt_to_terminal(receipt: &ProcessReceipt) -> ExecTerminalReceipt {
    let stdout_sha256 = sha256_hex(receipt.stdout.text.as_bytes());
    let stderr_sha256 = sha256_hex(receipt.stderr.text.as_bytes());
    let captured_bytes =
        (receipt.stdout.bytes_retained as u64) + (receipt.stderr.bytes_retained as u64);
    ExecTerminalReceipt {
        exit_code: receipt.exit_code,
        duration_ms: u64::try_from(receipt.duration.as_millis()).unwrap_or(u64::MAX),
        stdout_sha256,
        stderr_sha256,
        captured_bytes,
        truncated: receipt.stdout.truncated || receipt.stderr.truncated,
        encoding_lossy: receipt.stdout.lossy_utf8 || receipt.stderr.lossy_utf8,
        cancelled: receipt.cancelled,
        timed_out: receipt.timed_out,
    }
}

fn live_output_from_receipt(receipt: &ProcessReceipt) -> LiveProcessOutput {
    // Prefer runner-produced observation chunks; fall back to stream texts.
    let mut combined: Vec<RunnerOutputChunk> = receipt.chunks.clone();
    if combined.is_empty() {
        combined = build_output_chunks(&receipt.stdout.text, &receipt.stderr.text);
    }
    combined.sort_by_key(|c| c.sequence);
    let mut chunks = Vec::with_capacity(combined.len());
    for (i, chunk) in combined.into_iter().enumerate() {
        let stream = match chunk.stream {
            OutputStreamTag::Stdout => ProcessOutputStream::Stdout,
            OutputStreamTag::Stderr => ProcessOutputStream::Stderr,
        };
        chunks.push(ProcessOutputChunk {
            stream,
            sequence: i as u64,
            lossy_utf8: chunk.lossy_utf8,
            text: neutralize_control_sequences(&chunk.text),
        });
    }
    LiveProcessOutput {
        chunks,
        truncated: receipt.stdout.truncated || receipt.stderr.truncated,
        encoding_lossy: receipt.stdout.lossy_utf8 || receipt.stderr.lossy_utf8,
        captured_bytes: (receipt.stdout.bytes_retained as u64)
            + (receipt.stderr.bytes_retained as u64),
    }
}

fn empty_failure_receipt(duration_ms: u64) -> ExecTerminalReceipt {
    ExecTerminalReceipt {
        exit_code: None,
        duration_ms,
        stdout_sha256: empty_digest(),
        stderr_sha256: empty_digest(),
        captured_bytes: 0,
        truncated: false,
        encoding_lossy: false,
        cancelled: false,
        timed_out: false,
    }
}

fn empty_digest() -> String {
    sha256_hex(b"")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn run_lease_ms(timeout: Duration) -> u64 {
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    timeout_ms
        .saturating_add(RUN_LEASE_MARGIN_MS)
        .clamp(1, MAX_RUN_LEASE_MS)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use keeper_session::ConversationRepository;

    #[test]
    fn catalog_allows_only_fixed_labels() {
        for label in PROCESS_CATALOG {
            assert!(validate_program_catalog(label).is_ok());
        }
        assert!(matches!(
            validate_program_catalog("go"),
            Err(GovernedProcessError::UnknownProgram)
        ));
    }

    #[test]
    fn catalog_denies_git_shells_and_shims() {
        for label in [
            "git",
            "cmd",
            "powershell",
            "pwsh",
            "bash",
            "wscript",
            "cscript",
            "mshta",
            "npm",
            "run.ps1",
            "tool.cmd",
            "x.bat",
        ] {
            assert!(
                matches!(
                    validate_program_catalog(label),
                    Err(GovernedProcessError::DeniedProgram)
                ),
                "expected deny for {label}"
            );
        }
    }

    #[test]
    fn argv_count_boundary_is_enforced() {
        let at_cap: Vec<String> = (0..PROCESS_ARGV_COUNT_CAP)
            .map(|i| format!("a{i}"))
            .collect();
        assert!(validate_process_run_arguments("node", &at_cap, Some(".")).is_ok());
        let over: Vec<String> = (0..PROCESS_ARGV_COUNT_CAP + 1)
            .map(|i| format!("a{i}"))
            .collect();
        assert!(matches!(
            validate_process_run_arguments("node", &over, Some(".")),
            Err(GovernedProcessError::InvalidArguments)
        ));
    }

    #[test]
    fn combined_program_argv_byte_boundary_is_enforced() {
        let program = "node";
        let remaining = PROCESS_ARGV_BYTE_CAP - program.len();
        let at_cap = vec!["x".repeat(remaining)];
        assert!(validate_process_run_arguments(program, &at_cap, None).is_ok());
        let over = vec!["x".repeat(remaining + 1)];
        assert!(matches!(
            validate_process_run_arguments(program, &over, None),
            Err(GovernedProcessError::InvalidArguments)
        ));
    }

    #[test]
    fn nul_and_control_characters_are_denied() {
        for argv in [
            vec!["ok\0".into()],
            vec!["line\nbreak".into()],
            vec!["tab\there".into()],
            vec!["\u{1}".into()],
            vec!["del\u{7f}".into()],
        ] {
            assert!(
                matches!(
                    validate_process_run_arguments("node", &argv, None),
                    Err(GovernedProcessError::InvalidArguments)
                ),
                "expected control deny for {argv:?}"
            );
        }
        assert!(matches!(
            validate_process_run_arguments("no\0de", &[], None),
            Err(GovernedProcessError::InvalidArguments)
        ));
    }

    #[test]
    fn clean_environment_excludes_sensitive_names() {
        // SAFETY: test process only; restores after assertion.
        let previous = std::env::var_os("API_TOKEN");
        unsafe {
            std::env::set_var("API_TOKEN", "must-not-copy");
            std::env::set_var("OPENAI_API_KEY", "must-not-copy");
        }
        let exe = std::env::current_exe().unwrap();
        let env = build_clean_environment(&exe).unwrap();
        assert!(!env.keys().any(|k| sensitive_name(k)));
        assert!(!env.contains_key(OsStr::new("API_TOKEN")));
        assert!(!env.contains_key(OsStr::new("OPENAI_API_KEY")));
        assert!(!env.contains_key(OsStr::new("HOME")));
        assert!(!env.contains_key(OsStr::new("USERPROFILE")));
        assert!(!env.contains_key(OsStr::new("APPDATA")));
        assert!(env.contains_key(OsStr::new("PATH")));
        assert!(env.contains_key(OsStr::new("TEMP")));
        match previous {
            Some(value) => unsafe { std::env::set_var("API_TOKEN", value) },
            None => unsafe { std::env::remove_var("API_TOKEN") },
        }
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn production_open_always_installs_supervised_runner() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("open.db");
        let gateway = GovernedProcessToolGateway::open(&db).unwrap();
        // Type-level proof: open has no runner parameter. Runtime: DB opens.
        assert!(gateway.get_proposal("missing").unwrap().is_none());
    }

    #[test]
    fn seal_always_binds_fixed_process_timeout() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir(&root).unwrap();
        let cap = ProjectCapability::new("proj-timeout", &root).unwrap();
        let sealed =
            GovernedProcessToolGateway::seal(&cap, "node", &["--version".into()], Some("."))
                .unwrap();
        assert_eq!(sealed.timeout(), PROCESS_TIMEOUT);
        assert_eq!(PROCESS_TIMEOUT, Duration::from_millis(60_000));
        assert_eq!(sealed.output_byte_cap(), PROCESS_OUTPUT_BYTE_CAP);
        // cfg(test) seam only: private mutator exists for accelerated tests.
        let accelerated = sealed.clone().with_timeout(Duration::from_millis(50));
        assert_eq!(accelerated.timeout(), Duration::from_millis(50));
        // Production seal path is independent of any prior mutator usage.
        let resealed =
            GovernedProcessToolGateway::seal(&cap, "node", &["--version".into()], Some("."))
                .unwrap();
        assert_eq!(resealed.timeout(), PROCESS_TIMEOUT);
    }

    #[test]
    fn accelerated_timeout_unit_seam_marks_timed_out_without_production_mutator() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir(&root).unwrap();
        let db = temp.path().join("timeout.db");
        let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
        let project = projects
            .create(keeper_project::CreateProject {
                name: "P".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        let sessions = keeper_storage_sqlite::SqliteConversationRepository::open(&db).unwrap();
        let conversation = sessions
            .create_conversation(keeper_session::NewConversation {
                project_id: project.id,
                title: Some("c".into()),
            })
            .unwrap();
        let run = sessions
            .create_run(keeper_session::NewRun {
                conversation_id: conversation.id,
                user_message: "r".into(),
            })
            .unwrap();
        let cap = ProjectCapability::new(project.id.0.to_string(), &root).unwrap();
        let gateway = GovernedProcessToolGateway::open(&db).unwrap();
        let sealed = GovernedProcessToolGateway::seal(
            &cap,
            "node",
            &["-e".into(), "setTimeout(() => {}, 30000)".into()],
            Some("."),
        )
        .unwrap()
        .with_timeout(Duration::from_millis(80));
        assert_ne!(sealed.timeout(), PROCESS_TIMEOUT);
        let prepared = gateway
            .reserve(
                &cap,
                &conversation.id.0.to_string(),
                RunId(run.id.0),
                "call-to-unit",
                &sealed,
            )
            .unwrap();
        let grant = {
            let hex = Sha256::digest(b"grant-to-unit");
            format!(
                "sha256:{}",
                hex.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        };
        gateway
            .decide(
                &prepared.metadata.proposal_id,
                &conversation.id.0.to_string(),
                ExecDecision::ApproveOnce,
                &grant,
                sealed.proposal_fingerprint(),
            )
            .unwrap();
        let timed = gateway
            .execute(
                &cap,
                &prepared.metadata.proposal_id,
                &sealed,
                ProcessCancellationSignal::new(),
            )
            .unwrap();
        let GovernedProcessOutcome::Failed(row) = timed.outcome else {
            panic!("expected timeout failure, got {timed:?}");
        };
        assert_eq!(row.timed_out, Some(true));
        assert_eq!(row.cancelled, Some(false));
    }

    #[test]
    fn claim_matrix_busy_without_spawn_via_private_store_seam() {
        use chrono::Duration as ChronoDuration;
        use keeper_storage_sqlite::ExecRunClaimOutcome;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir(&root).unwrap();
        let db = temp.path().join("busy.db");
        let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
        let project = projects
            .create(keeper_project::CreateProject {
                name: "P".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        let sessions = keeper_storage_sqlite::SqliteConversationRepository::open(&db).unwrap();
        let conversation = sessions
            .create_conversation(keeper_session::NewConversation {
                project_id: project.id,
                title: Some("c".into()),
            })
            .unwrap();
        let run = sessions
            .create_run(keeper_session::NewRun {
                conversation_id: conversation.id,
                user_message: "r".into(),
            })
            .unwrap();
        let cap = ProjectCapability::new(project.id.0.to_string(), &root).unwrap();
        let gateway = GovernedProcessToolGateway::open(&db).unwrap();
        // Fixed production timeout; Busy is proven via one-shot claim, not timeout mutator.
        let sealed =
            GovernedProcessToolGateway::seal(&cap, "node", &["--version".into()], Some("."))
                .unwrap();
        assert_eq!(sealed.timeout(), PROCESS_TIMEOUT);
        let prepared = gateway
            .reserve(
                &cap,
                &conversation.id.0.to_string(),
                RunId(run.id.0),
                "call-busy-unit",
                &sealed,
            )
            .unwrap();
        let grant = {
            let hex = Sha256::digest(b"grant-busy-unit");
            format!(
                "sha256:{}",
                hex.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        };
        gateway
            .decide(
                &prepared.metadata.proposal_id,
                &conversation.id.0.to_string(),
                ExecDecision::ApproveOnce,
                &grant,
                sealed.proposal_fingerprint(),
            )
            .unwrap();
        let id = prepared.metadata.proposal_id.clone();
        let store = gateway.store();
        let first = store.claim_run(&id, "owner-a", 60_000, Utc::now()).unwrap();
        assert!(matches!(first, ExecRunClaimOutcome::Claimed(_)));
        assert!(matches!(
            store.claim_run(&id, "owner-a", 60_000, Utc::now()).unwrap(),
            ExecRunClaimOutcome::Busy(_)
        ));
        assert!(matches!(
            store.claim_run(&id, "owner-b", 60_000, Utc::now()).unwrap(),
            ExecRunClaimOutcome::Busy(_)
        ));
        assert!(matches!(
            store
                .claim_run(
                    &id,
                    "owner-c",
                    60_000,
                    Utc::now() + ChronoDuration::seconds(120),
                )
                .unwrap(),
            ExecRunClaimOutcome::Busy(_)
        ));
        let outcome = gateway
            .execute(&cap, &id, &sealed, ProcessCancellationSignal::new())
            .unwrap();
        assert!(matches!(outcome.outcome, GovernedProcessOutcome::Busy(_)));
        store.interrupt_run(&id, "owner-a", Utc::now()).unwrap();
    }

    #[test]
    fn counting_runner_proves_single_spawn_and_busy_zero() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir(&root).unwrap();
        let db = temp.path().join("count.db");
        let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
        let project = projects
            .create(keeper_project::CreateProject {
                name: "P".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        let sessions = keeper_storage_sqlite::SqliteConversationRepository::open(&db).unwrap();
        let conversation = sessions
            .create_conversation(keeper_session::NewConversation {
                project_id: project.id,
                title: Some("c".into()),
            })
            .unwrap();
        let run = sessions
            .create_run(keeper_session::NewRun {
                conversation_id: conversation.id,
                user_message: "r".into(),
            })
            .unwrap();
        let cap = ProjectCapability::new(project.id.0.to_string(), &root).unwrap();
        let counter = Arc::new(CountingProcessRunner::new());
        let gateway = GovernedProcessToolGateway::open_with_runner(
            &db,
            counter.clone() as Arc<dyn ProcessRunner>,
        )
        .unwrap();
        let sealed =
            GovernedProcessToolGateway::seal(&cap, "node", &["--version".into()], Some("."))
                .unwrap();
        let prepared = gateway
            .reserve(
                &cap,
                &conversation.id.0.to_string(),
                RunId(run.id.0),
                "call-count",
                &sealed,
            )
            .unwrap();
        let grant = {
            let hex = Sha256::digest(b"grant-count");
            format!(
                "sha256:{}",
                hex.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        };
        gateway
            .decide(
                &prepared.metadata.proposal_id,
                &conversation.id.0.to_string(),
                ExecDecision::ApproveOnce,
                &grant,
                sealed.proposal_fingerprint(),
            )
            .unwrap();
        let ok = gateway
            .execute(
                &cap,
                &prepared.metadata.proposal_id,
                &sealed,
                ProcessCancellationSignal::new(),
            )
            .unwrap();
        assert!(matches!(ok.outcome, GovernedProcessOutcome::Succeeded(_)));
        assert_eq!(counter.spawn_count(), 1);
        let replay = gateway
            .execute(
                &cap,
                &prepared.metadata.proposal_id,
                &sealed,
                ProcessCancellationSignal::new(),
            )
            .unwrap();
        assert!(matches!(
            replay.outcome,
            GovernedProcessOutcome::Terminal(_)
        ));
        assert_eq!(counter.spawn_count(), 1);
    }

    #[test]
    fn cleanup_unproven_runner_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("proj");
        std::fs::create_dir(&root).unwrap();
        let db = temp.path().join("cleanup.db");
        // Minimal fixture via public production + test runner only.
        let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
        let project = projects
            .create(keeper_project::CreateProject {
                name: "P".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        let sessions = keeper_storage_sqlite::SqliteConversationRepository::open(&db).unwrap();
        let conversation = sessions
            .create_conversation(keeper_session::NewConversation {
                project_id: project.id,
                title: Some("c".into()),
            })
            .unwrap();
        let run = sessions
            .create_run(keeper_session::NewRun {
                conversation_id: conversation.id,
                user_message: "r".into(),
            })
            .unwrap();
        let cap = ProjectCapability::new(project.id.0.to_string(), &root).unwrap();
        let runner = Arc::new(CleanupUnprovenRunner::new());
        let gateway = GovernedProcessToolGateway::open_with_runner(
            &db,
            runner.clone() as Arc<dyn ProcessRunner>,
        )
        .unwrap();
        let sealed =
            GovernedProcessToolGateway::seal(&cap, "node", &["--version".into()], Some("."))
                .unwrap();
        let prepared = gateway
            .reserve(
                &cap,
                &conversation.id.0.to_string(),
                RunId(run.id.0),
                "call-cleanup-unit",
                &sealed,
            )
            .unwrap();
        let grant = {
            let hex = Sha256::digest(b"grant-cleanup");
            format!(
                "sha256:{}",
                hex.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )
        };
        gateway
            .decide(
                &prepared.metadata.proposal_id,
                &conversation.id.0.to_string(),
                ExecDecision::ApproveOnce,
                &grant,
                sealed.proposal_fingerprint(),
            )
            .unwrap();
        let err = gateway
            .execute(
                &cap,
                &prepared.metadata.proposal_id,
                &sealed,
                ProcessCancellationSignal::new(),
            )
            .unwrap_err();
        assert!(matches!(err, GovernedProcessError::CleanupUnproven));
        assert_eq!(runner.spawn_count(), 1);
        let row = gateway
            .get_proposal(&prepared.metadata.proposal_id)
            .unwrap()
            .unwrap();
        assert_eq!(row.state, ExecProposalState::Failed);
        // Private test store remains mutation-capable; public get_proposal is not.
        let _ = gateway.store();
    }
}
