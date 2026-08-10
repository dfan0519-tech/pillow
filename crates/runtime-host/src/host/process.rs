//! Run-scoped governed process coordinator for `process.run@1`.
//!
//! Provider proposals carry only program/argv/cwd. Host-owned project,
//! conversation, run and call identity are injected from the bound run.
//! Process maps are independent of governed read/write maps.
//!
//! Authority: one non-terminal pending process action per owning run;
//! foreign conversation/project cannot list, decide or open-output;
//! busy/replay never creates new spawn authority; restart interrupts
//! non-terminal rows; a cancelled turn delivers at most one validated
//! terminal receipt before the host-forced run terminal.
//!
//! Ledger/execution authority is exclusively [`GovernedProcessToolGateway`].
//! This module never opens a raw exec-proposal repository or registers a
//! database-path / gateway-pointer side channel.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
// Contract types live in `keeper-agent-protocol`; re-export for backward compatibility.
pub use keeper_agent_protocol::{
    GovernedProcessDelivery, GovernedProcessReceipt, GovernedShellOutput, GovernedShellReceipt,
    ProviderProcessProposal, ProviderShellProposal,
};

use keeper_agent_protocol::{
    ProcessProposalDecidedPayload, ProcessProposalRequestedPayload,
    ProcessRiskClass as ProtocolRiskClass, ProcessRunArguments, ProcessRunOutput,
    ProcessStartedPayload, ProcessTerminalOutcomePayload, RunId, ToolActiveContext, ToolArguments,
    ToolProposal, canonical_arguments_digest,
};
use keeper_governance_port::CancellationProbe;
use keeper_policy::{
    ExecDecision, ExecProposalState, canonical_process_subject,
    conservative_shell_command_segments, is_exec_terminal,
};
use keeper_process_executor::{
    CancellationSignal as ProcessCancellationSignal, OutputChunk as RunnerOutputChunk,
    OutputObserver,
};
use keeper_storage_sqlite::{
    ExecDecisionOutcome, PermissionRuleAction, PermissionRulePrefix,
    PermissionRuleSegmentMatchSnapshot, SqlitePermissionRuleRepository, StoredExecProposal,
    StoredPermissionRuleReceipt,
};
use keeper_tool_executor::{
    ClaimedObserver, GovernedProcessError, GovernedProcessExecution, GovernedProcessOutcome,
    GovernedProcessToolGateway, LiveProcessOutput, ProcessOutputChunk as GatewayOutputChunk,
    ProcessOutputStream as GatewayOutputStream, ProjectCapability, SealedProcessSpec,
    SealedShellSpec,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::governed_write::{generate_approval_grant, grant_digest};
use crate::permission_rules::{
    HostPermissionRuleConjunctionEvaluation, PermissionRuleAttributionView,
    attribution_from_receipt, evaluate_host_permission_rule_conjunction,
};

// ── Public types ────────────────────────────────────────────────────────────

/// Host-emitted request summary (also mirrors protocol requested payload fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessProposalRequestedView {
    pub proposal_id: String,
    pub program: String,
    pub argv: Vec<String>,
    pub portable_cwd: Option<String>,
    pub summary: String,
    pub risk_class: ProcessRiskClass,
    pub expires_at: DateTime<Utc>,
}

/// Waiter-thread lifecycle notifications for the caller-emits pattern.
///
/// Fired only after the corresponding durable step: requested after reserve,
/// decided after decide commit, started only after claim `Claimed` (`Running`).
/// Terminal remains on the post-return receipt (not this enum).
#[derive(Debug)]
pub enum GovernedProcessLifecycle<'a> {
    Requested(&'a ProcessProposalRequestedView),
    Decided(&'a ProcessProposalDecidedPayload),
    Started(&'a ProcessStartedPayload),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRiskClass {
    Standard,
    High,
}

impl From<ProcessRiskClass> for ProtocolRiskClass {
    fn from(value: ProcessRiskClass) -> Self {
        match value {
            ProcessRiskClass::Standard => ProtocolRiskClass::Standard,
            ProcessRiskClass::High => ProtocolRiskClass::High,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOrigin {
    Agent,
}

/// Live conversation-facing proposal view (native list/decide DTO source).
///
/// Raw approval grants never appear here — only whether a decision is available.
#[derive(Clone)]
pub struct LiveProcessProposalView {
    pub proposal_id: String,
    pub conversation_id: String,
    pub origin: ProcessOrigin,
    pub state: ExecProposalState,
    pub program: String,
    pub argv: Vec<String>,
    pub portable_cwd: Option<String>,
    pub risk_class: ProcessRiskClass,
    pub summary: String,
    pub expires_at: DateTime<Utc>,
    /// True while this host holds a live pending grant for approve-once/reject.
    pub decision_available: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub cancelled: Option<bool>,
    pub timed_out: Option<bool>,
    pub truncated: Option<bool>,
    pub encoding_lossy: Option<bool>,
    pub captured_bytes: Option<u64>,
    pub rule_subject: String,
    pub auto_approved_by_rule: Option<PermissionRuleAttributionView>,
}

impl std::fmt::Debug for LiveProcessProposalView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveProcessProposalView")
            .field("proposal_id", &self.proposal_id)
            .field("conversation_id", &self.conversation_id)
            .field("state", &self.state)
            .field("program", &self.program)
            .field("argv", &self.argv)
            .field("summary", &self.summary)
            .field("decision_available", &self.decision_available)
            .finish()
    }
}

/// Bounded host-owned output projection (metadata + optional retained chunks).
#[derive(Clone, Debug)]
pub struct ProcessOutputView {
    pub proposal_id: String,
    pub program: String,
    pub argv: Vec<String>,
    pub state: ExecProposalState,
    pub chunks: Vec<ProcessOutputChunk>,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub captured_bytes: u64,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    /// True only while this host instance retained live sealed material/output.
    pub live_retained: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GovernedProcessBridgeError {
    #[error("governed process run is not registered")]
    RunNotRegistered,
    #[error("provider process proposal is invalid")]
    InvalidProposal,
    #[error("provider process call conflicts with its durable reservation")]
    ReservationConflict,
    #[error("process proposal is already pending on this run")]
    Busy,
    #[error("governed process context is invalid")]
    InvalidContext,
    #[error("governed process gateway is unavailable")]
    Gateway,
    #[error("process proposal is not live in the active coordinator")]
    NotLive,
    #[error("process proposal ownership mismatch")]
    OwnershipMismatch,
    #[error("process proposal decision is not allowed")]
    DecisionDenied,
    #[error("process proposal expired")]
    Expired,
    #[error("process proposal was cancelled")]
    Cancelled,
    #[error("process proposal was interrupted")]
    Interrupted,
    #[error("host is shutting down")]
    ShutDown,
    #[error("process cleanup could not be proven")]
    CleanupUnproven,
}

// ── Coordinator state ───────────────────────────────────────────────────────

struct BoundProcessRun {
    owner: Uuid,
    context: ToolActiveContext,
    capability: ProjectCapability,
    gateway: Arc<GovernedProcessToolGateway>,
    permission_rules: Option<Arc<SqlitePermissionRuleRepository>>,
}

#[cfg(any(test, feature = "test-support"))]
type PreClaimHook = Box<dyn FnMut(&str) + Send>;

struct LiveProcessEntry {
    owner: Uuid,
    run_id: RunId,
    conversation_id: String,
    proposal_id: String,
    proposal_fingerprint: String,
    program: String,
    argv: Vec<String>,
    portable_cwd: Option<String>,
    summary: String,
    risk_class: ProcessRiskClass,
    expires_at: DateTime<Utc>,
    sealed: SealedCommandAuthority,
    /// Memory-only raw grant; never serialized, never on public views.
    raw_grant: String,
    grant_digest: String,
    metadata: StoredExecProposal,
    terminal: Option<StoredExecProposal>,
    provider_ok: Option<ProcessRunOutput>,
    cancelled: bool,
    interrupted: bool,
    process_cancel: ProcessCancellationSignal,
    output_chunks: Vec<ProcessOutputChunk>,
    decided_payload: Option<ProcessProposalDecidedPayload>,
    /// Claim-confirmed only. Pre-claim drafts must never set this.
    started_payload: Option<ProcessStartedPayload>,
    /// True only after durable `claim_run` → `Claimed` (`Running`).
    started_confirmed: bool,
    /// Once-only waiter delivery flags (in-flight emit path).
    decided_delivered: bool,
    started_delivered: bool,
    waiter_notified: bool,
    observer_delivery_in_progress: bool,
    observer_failure_cleanup_in_progress: bool,
    observer_quarantine_deferred: bool,
    auto_rule_pending: bool,
    /// Structured commands expose one canonical subject. Shell audit subjects
    /// live in ordered durable segment receipts, so this compatibility string
    /// stays empty rather than flattening away segment boundaries.
    rule_subject: String,
    auto_approved_by_rule: Option<PermissionRuleAttributionView>,
    #[cfg(any(test, feature = "test-support"))]
    pre_claim_hook: Option<PreClaimHook>,
}

#[derive(Clone)]
enum SealedCommandAuthority {
    Structured(SealedProcessSpec),
    Shell(SealedShellSpec),
}

impl SealedCommandAuthority {
    fn program(&self) -> &str {
        match self {
            Self::Structured(sealed) => sealed.program(),
            Self::Shell(sealed) => sealed.program(),
        }
    }

    fn argv(&self) -> &[String] {
        match self {
            Self::Structured(sealed) => sealed.argv(),
            Self::Shell(sealed) => sealed.argv(),
        }
    }

    fn shell_command(&self) -> Option<&str> {
        match self {
            Self::Structured(_) => None,
            Self::Shell(sealed) => Some(sealed.command()),
        }
    }

    fn portable_cwd(&self) -> Option<&str> {
        match self {
            Self::Structured(sealed) => sealed.portable_cwd(),
            Self::Shell(sealed) => sealed.portable_cwd(),
        }
    }

    fn argv_digest(&self) -> &str {
        match self {
            Self::Structured(sealed) => sealed.argv_digest(),
            Self::Shell(sealed) => sealed.argv_digest(),
        }
    }

    fn proposal_fingerprint(&self) -> &str {
        match self {
            Self::Structured(sealed) => sealed.proposal_fingerprint(),
            Self::Shell(sealed) => sealed.proposal_fingerprint(),
        }
    }

    fn reserve(
        &self,
        bound: &BoundProcessRun,
        run_id: RunId,
        call_id: &str,
    ) -> Result<StoredExecProposal, GovernedProcessError> {
        match self {
            Self::Structured(sealed) => bound
                .gateway
                .reserve(
                    &bound.capability,
                    bound.context.conversation_id(),
                    run_id,
                    call_id,
                    sealed,
                )
                .map(|prepared| prepared.metadata),
            Self::Shell(sealed) => bound
                .gateway
                .reserve_shell(
                    &bound.capability,
                    bound.context.conversation_id(),
                    run_id,
                    call_id,
                    sealed,
                )
                .map(|prepared| prepared.metadata),
        }
    }

    fn execute_with_observer(
        &self,
        bound: &BoundProcessRun,
        proposal_id: &str,
        process_cancel: ProcessCancellationSignal,
        observer: OutputObserver,
        on_claimed: ClaimedObserver,
    ) -> Result<GovernedProcessExecution, GovernedProcessError> {
        match self {
            Self::Structured(sealed) => bound.gateway.execute_with_observer(
                &bound.capability,
                proposal_id,
                sealed,
                process_cancel,
                Some(observer),
                Some(on_claimed),
            ),
            Self::Shell(sealed) => bound.gateway.execute_shell_with_observer(
                &bound.capability,
                proposal_id,
                sealed,
                process_cancel,
                Some(observer),
                Some(on_claimed),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalPolicy {
    StructuredRules,
    ShellRules,
}

impl std::fmt::Debug for LiveProcessEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveProcessEntry")
            .field("proposal_id", &self.proposal_id)
            .field("run_id", &self.run_id)
            .field("program", &self.program)
            .field("state", &self.metadata.state)
            .field("raw_grant", &"<redacted>")
            .finish()
    }
}

struct CoordinatorState {
    runs: HashMap<RunId, Arc<BoundProcessRun>>,
    live: HashMap<String, LiveProcessEntry>,
    pending_by_run: HashMap<RunId, String>,
    retained_terminal: VecDeque<RetainedTerminalOutput>,
    capabilities: HashMap<(Uuid, String), ProjectCapability>,
}

/// Transcript cache is host-owner scoped, bounded, and never durable.
struct RetainedTerminalOutput {
    owner: Uuid,
    conversation_id: String,
    proposal_id: String,
    program: String,
    argv: Vec<String>,
    metadata: StoredExecProposal,
    chunks: Vec<ProcessOutputChunk>,
}

const TERMINAL_OUTPUTS_PER_OWNER: usize = 32;

impl CoordinatorState {
    fn new() -> Self {
        Self {
            runs: HashMap::new(),
            live: HashMap::new(),
            pending_by_run: HashMap::new(),
            retained_terminal: VecDeque::new(),
            capabilities: HashMap::new(),
        }
    }
}

struct Coordinator {
    state: Mutex<CoordinatorState>,
    cv: Condvar,
}

fn coordinator() -> &'static Coordinator {
    static COORDINATOR: OnceLock<Coordinator> = OnceLock::new();
    COORDINATOR.get_or_init(|| Coordinator {
        state: Mutex::new(CoordinatorState::new()),
        cv: Condvar::new(),
    })
}

// ── Bind / release ──────────────────────────────────────────────────────────

pub(crate) fn open_process_gateway(
    path: impl AsRef<Path>,
) -> Result<Arc<GovernedProcessToolGateway>, GovernedProcessBridgeError> {
    GovernedProcessToolGateway::open(path)
        .map(Arc::new)
        .map_err(|_| GovernedProcessBridgeError::Gateway)
}

pub(crate) fn bind_process_run(
    owner: Uuid,
    gateway: Arc<GovernedProcessToolGateway>,
    permission_rules: Option<Arc<SqlitePermissionRuleRepository>>,
    run_id: RunId,
    project_id: impl Into<String>,
    conversation_id: impl Into<String>,
    project_root: impl AsRef<Path>,
) -> Result<(), GovernedProcessBridgeError> {
    let project_id = project_id.into();
    let context = ToolActiveContext::new(project_id.clone(), conversation_id, run_id)
        .map_err(|_| GovernedProcessBridgeError::InvalidContext)?;
    let capability = ProjectCapability::new(project_id, project_root)
        .map_err(|_| GovernedProcessBridgeError::InvalidContext)?;
    let mut guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedProcessBridgeError::Gateway)?;
    if guard.runs.contains_key(&run_id) {
        return Err(GovernedProcessBridgeError::InvalidContext);
    }
    guard.runs.insert(
        run_id,
        Arc::new(BoundProcessRun {
            owner,
            context,
            capability,
            gateway,
            permission_rules,
        }),
    );
    let (conversation, capability) = {
        let bound = guard.runs.get(&run_id).expect("bound process run");
        (
            bound.context.conversation_id().to_owned(),
            bound.capability.clone(),
        )
    };
    guard.capabilities.insert((owner, conversation), capability);
    Ok(())
}

pub(crate) fn release_process_run(owner: Uuid, run_id: RunId) {
    let Ok(mut guard) = coordinator().state.lock() else {
        return;
    };
    if guard
        .runs
        .get(&run_id)
        .is_some_and(|bound| bound.owner == owner)
    {
        guard.runs.remove(&run_id);
        if let Some(proposal_id) = guard.pending_by_run.remove(&run_id)
            && let Some(entry) = guard.live.get_mut(&proposal_id)
        {
            entry.interrupted = true;
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
            entry.waiter_notified = true;
            coordinator().cv.notify_all();
        }
    }
}

pub(crate) fn release_process_owner(owner: Uuid) {
    let Ok(mut guard) = coordinator().state.lock() else {
        return;
    };
    let run_ids: Vec<RunId> = guard
        .runs
        .iter()
        .filter(|(_, bound)| bound.owner == owner)
        .map(|(id, _)| *id)
        .collect();
    for run_id in run_ids {
        guard.runs.remove(&run_id);
        if let Some(proposal_id) = guard.pending_by_run.remove(&run_id)
            && let Some(entry) = guard.live.get_mut(&proposal_id)
        {
            entry.interrupted = true;
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
            entry.waiter_notified = true;
        }
    }
    for entry in guard.live.values_mut() {
        if entry.owner == owner {
            entry.interrupted = true;
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
            entry.waiter_notified = true;
        }
    }
    guard.retained_terminal.retain(|entry| entry.owner != owner);
    guard
        .capabilities
        .retain(|(entry_owner, _), _| *entry_owner != owner);
    coordinator().cv.notify_all();
}

pub(crate) fn process_run_is_owned_by(owner: Uuid, run_id: RunId) -> bool {
    coordinator()
        .state
        .lock()
        .map(|guard| {
            guard
                .runs
                .get(&run_id)
                .is_some_and(|bound| bound.owner == owner)
        })
        .unwrap_or(false)
}

/// Residual live process authority count for this host owner (tests / invariants).
pub fn live_process_authority_count(owner: Uuid) -> usize {
    let Ok(guard) = coordinator().state.lock() else {
        return 0;
    };
    guard
        .live
        .values()
        .filter(|e| e.owner == owner && e.terminal.is_none())
        .count()
}

/// Residual pending-by-run slots for this host owner.
pub fn pending_process_run_count(owner: Uuid) -> usize {
    let Ok(guard) = coordinator().state.lock() else {
        return 0;
    };
    guard
        .pending_by_run
        .keys()
        .filter(|run_id| {
            guard.runs.get(run_id).is_some_and(|b| b.owner == owner)
                || guard
                    .live
                    .values()
                    .any(|e| e.run_id == **run_id && e.owner == owner)
        })
        .count()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn process_summary(program: &str, argv: &[String]) -> String {
    let mut parts = Vec::with_capacity(1 + argv.len());
    parts.push(serde_json::to_string(program).unwrap_or_else(|_| format!("\"{program}\"")));
    for arg in argv {
        parts.push(serde_json::to_string(arg).unwrap_or_else(|_| format!("\"{arg}\"")));
    }
    parts.join(" ")
}

fn classify_risk(program: &str, argv: &[String]) -> ProcessRiskClass {
    let high = match program {
        "node" | "nodejs" => argv.iter().any(|a| a == "-e" || a == "--eval"),
        "python" | "python3" => argv.iter().any(|a| a == "-c" || a == "-m"),
        _ => false,
    };
    if high {
        ProcessRiskClass::High
    } else {
        ProcessRiskClass::Standard
    }
}

fn parse_process_arguments(
    arguments: &Value,
) -> Result<ProcessRunArguments, GovernedProcessBridgeError> {
    let object = arguments
        .as_object()
        .ok_or(GovernedProcessBridgeError::InvalidProposal)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "program" | "argv" | "cwd"))
    {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    }
    let program = object
        .get("program")
        .and_then(Value::as_str)
        .ok_or(GovernedProcessBridgeError::InvalidProposal)?
        .to_owned();
    let argv = object
        .get("argv")
        .and_then(Value::as_array)
        .ok_or(GovernedProcessBridgeError::InvalidProposal)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or(GovernedProcessBridgeError::InvalidProposal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cwd = match object.get("cwd") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(GovernedProcessBridgeError::InvalidProposal),
    };
    Ok(ProcessRunArguments { program, argv, cwd })
}

fn provider_output_from_row(row: &StoredExecProposal) -> Option<ProcessRunOutput> {
    if !is_exec_terminal(row.state) {
        return None;
    }
    if matches!(
        row.state,
        ExecProposalState::Rejected | ExecProposalState::Expired | ExecProposalState::Interrupted
    ) {
        row.stdout_digest.as_ref()?;
    }
    let stdout = row.stdout_digest.clone().unwrap_or_else(empty_digest);
    let stderr = row.stderr_digest.clone().unwrap_or_else(empty_digest);
    Some(ProcessRunOutput {
        exit_code: row.exit_code,
        duration_ms: row.duration_ms.unwrap_or(0),
        stdout_sha256: stdout,
        stderr_sha256: stderr,
        captured_bytes: row.captured_bytes.unwrap_or(0),
        truncated: row.truncated.unwrap_or(false),
        encoding_lossy: row.encoding_lossy.unwrap_or(false),
        cancelled: row.cancelled.unwrap_or(false),
        timed_out: row.timed_out.unwrap_or(false),
    })
}

fn empty_digest() -> String {
    use sha2::{Digest, Sha256};
    let hex = Sha256::digest([])
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn requested_payload_from_view(
    view: &ProcessProposalRequestedView,
) -> ProcessProposalRequestedPayload {
    ProcessProposalRequestedPayload {
        proposal_id: view.proposal_id.clone(),
        program: view.program.clone(),
        argv: view.argv.clone(),
        portable_cwd: view.portable_cwd.clone(),
        summary: view.summary.clone(),
        risk_class: view.risk_class.into(),
        expires_at: view.expires_at,
    }
}

fn terminal_payload(row: &StoredExecProposal) -> ProcessTerminalOutcomePayload {
    ProcessTerminalOutcomePayload {
        proposal_id: row.proposal_id.clone(),
        program: row.program.clone(),
        state: row.state,
        exit_code: row.exit_code,
        duration_ms: row.duration_ms,
        truncated: row.truncated,
        encoding_lossy: row.encoding_lossy,
        captured_bytes: row.captured_bytes,
        cancelled: row.cancelled,
        timed_out: row.timed_out,
    }
}

fn map_gateway_chunks(live: &LiveProcessOutput) -> Vec<ProcessOutputChunk> {
    live.chunks
        .iter()
        .map(|c: &GatewayOutputChunk| ProcessOutputChunk {
            stream: match c.stream {
                GatewayOutputStream::Stdout => ProcessOutputStream::Stdout,
                GatewayOutputStream::Stderr => ProcessOutputStream::Stderr,
            },
            sequence: c.sequence,
            lossy_utf8: c.lossy_utf8,
            text: c.text.clone(),
        })
        .collect()
}

fn replay_receipt(row: StoredExecProposal) -> GovernedProcessReceipt {
    let provider_ok = provider_output_from_row(&row);
    let cancelled = row.cancelled.unwrap_or(false) || row.state == ExecProposalState::Cancelled;
    let timed_out = row.timed_out.unwrap_or(false);
    let terminal = Some(terminal_payload(&row));
    GovernedProcessReceipt {
        delivery: GovernedProcessDelivery::Replayed,
        proposal_id: row.proposal_id.clone(),
        state: row.state,
        requested: None,
        decided: None,
        started: None,
        terminal,
        provider_ok,
        cancelled,
        timed_out,
    }
}

fn map_seal_error(error: GovernedProcessError) -> GovernedProcessBridgeError {
    match error {
        GovernedProcessError::InvalidArguments
        | GovernedProcessError::UnknownProgram
        | GovernedProcessError::DeniedProgram
        | GovernedProcessError::ProgramUnavailable
        | GovernedProcessError::CwdRejected => GovernedProcessBridgeError::InvalidProposal,
        GovernedProcessError::InvalidProjectRoot | GovernedProcessError::CapabilityMismatch => {
            GovernedProcessBridgeError::InvalidContext
        }
        GovernedProcessError::NotApplicable => GovernedProcessBridgeError::Busy,
        GovernedProcessError::CleanupUnproven => GovernedProcessBridgeError::CleanupUnproven,
        _ => GovernedProcessBridgeError::Gateway,
    }
}

// ── Execute path ────────────────────────────────────────────────────────────

/// Execute one adapter-normalized `process.run@1` proposal for an active run.
///
/// Ordering: durable `pending_approval` reserve, then lifecycle callback for
/// requested, then condition wait for approve/reject/expire/cancel/interrupt.
/// Approve claims once and spawns through the gateway; busy/replay never spawn
/// again.
///
/// Lifecycle callbacks fire on the waiter thread after each durable step
/// becomes eligible (decided after decide commit; started only after claim
/// `Claimed`; terminal remains on the post-return receipt). Callers that emit
/// protocol events must de-duplicate against the receipt's post-return path.
///
/// Typed process event payloads on the receipt follow
/// reserve → requested → decided → started → terminal (reject/expire omit start).
///
/// `cancellation` is level-triggered: the waiter re-probes on every wait cycle.
/// Snapshotting once at execute entry is fail-open and forbidden.
pub fn execute_governed_process(
    run_id: RunId,
    provider: ProviderProcessProposal,
    cancellation: &dyn CancellationProbe,
    mut on_lifecycle: impl FnMut(GovernedProcessLifecycle<'_>) -> Result<(), GovernedProcessBridgeError>,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    if provider.tool_id != "process.run" || provider.tool_version != 1 {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    }
    if provider.call_id.is_empty() {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    }

    let bound = bound_process_run(run_id)?;

    let args = parse_process_arguments(&provider.arguments)?;
    let arguments_digest = canonical_arguments_digest(&provider.arguments)
        .map_err(|_| GovernedProcessBridgeError::InvalidProposal)?;
    let proposal = ToolProposal {
        project_id: bound.context.project_id().into(),
        conversation_id: bound.context.conversation_id().into(),
        run_id,
        call_id: provider.call_id.clone(),
        tool_id: provider.tool_id.clone(),
        tool_version: provider.tool_version,
        arguments: provider.arguments.clone(),
        arguments_digest,
    }
    .validate_for_context(&bound.context)
    .map_err(|_| GovernedProcessBridgeError::InvalidProposal)?;

    let ToolArguments::ProcessRun(validated_args) = proposal.arguments() else {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    };
    if validated_args.program != args.program
        || validated_args.argv != args.argv
        || validated_args.cwd != args.cwd
    {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    }

    // One pending process action per owning run — before seal/reserve so busy
    // never creates new spawn authority.
    ensure_process_run_available(run_id)?;
    let sealed = SealedCommandAuthority::Structured(
        GovernedProcessToolGateway::seal(
            &bound.capability,
            &args.program,
            &args.argv,
            args.cwd.as_deref(),
        )
        .map_err(map_seal_error)?,
    );

    execute_governed_sealed(
        run_id,
        provider.call_id,
        bound,
        sealed,
        ApprovalPolicy::StructuredRules,
        cancellation,
        &mut on_lifecycle,
    )
}

/// Execute one ACP shell-string proposal through the same reserve, approval,
/// claim, supervised process, cancellation, timeout and durable receipt path
/// as [`execute_governed_process`]. Conservative S2 rules may auto-decide only
/// when every reliably segmented command has an allow (or any segment denies).
pub fn execute_governed_shell(
    run_id: RunId,
    provider: ProviderShellProposal,
    cancellation: &dyn CancellationProbe,
    mut on_lifecycle: impl FnMut(GovernedProcessLifecycle<'_>) -> Result<(), GovernedProcessBridgeError>,
) -> Result<GovernedShellReceipt, GovernedProcessBridgeError> {
    if provider.call_id.is_empty() || provider.command.is_empty() {
        return Err(GovernedProcessBridgeError::InvalidProposal);
    }
    let bound = bound_process_run(run_id)?;
    ensure_process_run_available(run_id)?;
    let sealed = SealedCommandAuthority::Shell(
        GovernedProcessToolGateway::seal_shell(
            &bound.capability,
            &provider.command,
            provider.portable_cwd.as_deref(),
        )
        .map_err(map_seal_error)?,
    );
    let approved_command = provider.command;
    let process = execute_governed_sealed(
        run_id,
        provider.call_id,
        Arc::clone(&bound),
        sealed,
        ApprovalPolicy::ShellRules,
        cancellation,
        &mut on_lifecycle,
    )?;
    let output = retained_shell_output(&bound, &process);
    Ok(GovernedShellReceipt {
        process,
        approved_command,
        output,
    })
}

fn bound_process_run(run_id: RunId) -> Result<Arc<BoundProcessRun>, GovernedProcessBridgeError> {
    let guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedProcessBridgeError::Gateway)?;
    guard
        .runs
        .get(&run_id)
        .cloned()
        .ok_or(GovernedProcessBridgeError::RunNotRegistered)
}

fn ensure_process_run_available(run_id: RunId) -> Result<(), GovernedProcessBridgeError> {
    let guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedProcessBridgeError::Gateway)?;
    if guard.pending_by_run.contains_key(&run_id) {
        return Err(GovernedProcessBridgeError::Busy);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_governed_sealed(
    run_id: RunId,
    call_id: String,
    bound: Arc<BoundProcessRun>,
    sealed: SealedCommandAuthority,
    approval_policy: ApprovalPolicy,
    cancellation: &dyn CancellationProbe,
    on_lifecycle: &mut impl FnMut(
        GovernedProcessLifecycle<'_>,
    ) -> Result<(), GovernedProcessBridgeError>,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    // Terminal identity replay via deterministic reservation key + fingerprint
    // from reserve — never pre-list storage rows.
    let row = sealed
        .reserve(&bound, run_id, &call_id)
        .map_err(|error| match error {
            GovernedProcessError::NotApplicable => GovernedProcessBridgeError::Busy,
            other => map_seal_error(other),
        })?;

    if is_exec_terminal(row.state) {
        if row.proposal_fingerprint == sealed.proposal_fingerprint()
            && row.project_id == bound.context.project_id()
            && row.conversation_id == bound.context.conversation_id()
            && row.run_id == run_id
            && row.call_id == call_id
        {
            return Ok(replay_receipt(row));
        }
        return Err(GovernedProcessBridgeError::ReservationConflict);
    }

    if row.state != ExecProposalState::PendingApproval {
        return Err(GovernedProcessBridgeError::NotLive);
    }

    if row.proposal_fingerprint != sealed.proposal_fingerprint()
        || row.argv_digest != sealed.argv_digest()
        || row.program != sealed.program()
        || row.portable_cwd.as_deref() != sealed.portable_cwd()
    {
        return Err(GovernedProcessBridgeError::ReservationConflict);
    }

    let raw_grant = generate_approval_grant().map_err(|_| GovernedProcessBridgeError::Gateway)?;
    let grant_digest_value =
        grant_digest(&raw_grant).map_err(|_| GovernedProcessBridgeError::Gateway)?;
    let summary = process_summary(sealed.program(), sealed.argv());
    let risk_class = match approval_policy {
        ApprovalPolicy::StructuredRules => classify_risk(sealed.program(), sealed.argv()),
        ApprovalPolicy::ShellRules => ProcessRiskClass::High,
    };
    let process_cancel = ProcessCancellationSignal::new();
    let proposal_id = row.proposal_id.clone();
    let (rule_subject, rule_subjects) = match approval_policy {
        ApprovalPolicy::StructuredRules => {
            let subject = canonical_process_subject(sealed.program(), sealed.argv());
            (subject.clone(), Some(vec![subject]))
        }
        ApprovalPolicy::ShellRules => (
            String::new(),
            sealed
                .shell_command()
                .and_then(conservative_shell_command_segments),
        ),
    };
    let rule_evaluation = if let Some(subjects) = rule_subjects {
        match evaluate_host_permission_rule_conjunction(
            bound.permission_rules.as_deref(),
            bound.context.project_id(),
            PermissionRulePrefix::Process,
            &subjects,
        ) {
            Ok(evaluation) => evaluation,
            Err(()) => {
                fail_closed_after_reserve(&proposal_id, &bound);
                return Err(GovernedProcessBridgeError::Gateway);
            }
        }
    } else {
        HostPermissionRuleConjunctionEvaluation::RequireConfirmation
    };
    let auto_rule_pending = !matches!(
        &rule_evaluation,
        HostPermissionRuleConjunctionEvaluation::RequireConfirmation
    );

    let requested = ProcessProposalRequestedView {
        proposal_id: proposal_id.clone(),
        program: sealed.program().to_owned(),
        argv: sealed.argv().to_vec(),
        portable_cwd: sealed.portable_cwd().map(str::to_owned),
        summary: summary.clone(),
        risk_class,
        expires_at: row.expires_at,
    };

    {
        let mut guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        if guard.pending_by_run.contains_key(&run_id) {
            drop(guard);
            fail_closed_after_reserve(&proposal_id, &bound);
            return Err(GovernedProcessBridgeError::Busy);
        }
        if !guard
            .runs
            .get(&run_id)
            .is_some_and(|b| b.owner == bound.owner)
        {
            drop(guard);
            fail_closed_after_reserve(&proposal_id, &bound);
            return Err(GovernedProcessBridgeError::ShutDown);
        }
        guard.pending_by_run.insert(run_id, proposal_id.clone());
        guard.live.insert(
            proposal_id.clone(),
            LiveProcessEntry {
                owner: bound.owner,
                run_id,
                conversation_id: bound.context.conversation_id().into(),
                proposal_id: proposal_id.clone(),
                proposal_fingerprint: sealed.proposal_fingerprint().to_owned(),
                program: sealed.program().to_owned(),
                argv: sealed.argv().to_vec(),
                portable_cwd: sealed.portable_cwd().map(str::to_owned),
                summary,
                risk_class,
                expires_at: row.expires_at,
                sealed: sealed.clone(),
                raw_grant,
                grant_digest: grant_digest_value,
                metadata: row,
                terminal: None,
                provider_ok: None,
                cancelled: false,
                interrupted: false,
                process_cancel,
                output_chunks: Vec::new(),
                decided_payload: None,
                started_payload: None,
                started_confirmed: false,
                decided_delivered: false,
                started_delivered: false,
                waiter_notified: false,
                observer_delivery_in_progress: false,
                observer_failure_cleanup_in_progress: false,
                observer_quarantine_deferred: false,
                auto_rule_pending,
                rule_subject,
                auto_approved_by_rule: None,
                #[cfg(any(test, feature = "test-support"))]
                pre_claim_hook: None,
            },
        );
    }

    if let Err(error) = on_lifecycle(GovernedProcessLifecycle::Requested(&requested)) {
        fail_closed_after_reserve(&proposal_id, &bound);
        return Err(error);
    }

    let rule_matches = match rule_evaluation {
        HostPermissionRuleConjunctionEvaluation::Allow(matched) => Some(matched),
        HostPermissionRuleConjunctionEvaluation::Deny(matched) => Some(vec![matched]),
        HostPermissionRuleConjunctionEvaluation::RequireConfirmation => None,
    };
    if let Some(matched) = rule_matches
        && let Err(error) = decide_process_proposal_by_rules(
            bound.owner,
            bound.context.conversation_id(),
            &proposal_id,
            &matched,
        )
    {
        fail_closed_after_reserve(&proposal_id, &bound);
        return Err(error);
    }

    wait_for_resolution(
        run_id,
        &proposal_id,
        &bound,
        cancellation,
        requested,
        on_lifecycle,
    )
}

fn wait_for_resolution(
    run_id: RunId,
    proposal_id: &str,
    bound: &BoundProcessRun,
    cancellation: &dyn CancellationProbe,
    requested: ProcessProposalRequestedView,
    on_lifecycle: &mut impl FnMut(
        GovernedProcessLifecycle<'_>,
    ) -> Result<(), GovernedProcessBridgeError>,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    let coord = coordinator();
    loop {
        let mut guard = coord
            .state
            .lock()
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;

        // Level-triggered: re-probe on every wait cycle (never entry snapshot).
        if cancellation.is_cancelled()
            && let Some(entry) = guard.live.get_mut(proposal_id)
        {
            entry.cancelled = true;
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
        }

        // In-flight decided: after durable decide, before (or with) terminal.
        let pending_decided = guard.live.get(proposal_id).and_then(|entry| {
            if !entry.decided_delivered {
                entry.decided_payload.clone()
            } else {
                None
            }
        });
        if let Some(decided) = pending_decided {
            if let Some(entry) = guard.live.get_mut(proposal_id) {
                entry.decided_delivered = true;
                entry.observer_delivery_in_progress = true;
            }
            drop(guard);
            if let Err(error) = on_lifecycle(GovernedProcessLifecycle::Decided(&decided)) {
                fail_closed_after_observer_failure(proposal_id, bound);
                return Err(error);
            }
            if finish_observer_delivery(proposal_id, bound) {
                return Err(GovernedProcessBridgeError::Gateway);
            }
            continue;
        }

        // In-flight started: only after claim_run Claimed (started_confirmed).
        let pending_started = guard.live.get(proposal_id).and_then(|entry| {
            if !entry.started_delivered && entry.started_confirmed {
                entry.started_payload.clone()
            } else {
                None
            }
        });
        if let Some(started) = pending_started {
            if let Some(entry) = guard.live.get_mut(proposal_id) {
                entry.started_delivered = true;
                entry.observer_delivery_in_progress = true;
            }
            drop(guard);
            if let Err(error) = on_lifecycle(GovernedProcessLifecycle::Started(&started)) {
                fail_closed_after_observer_failure(proposal_id, bound);
                return Err(error);
            }
            if finish_observer_delivery(proposal_id, bound) {
                return Err(GovernedProcessBridgeError::Gateway);
            }
            continue;
        }

        let entry = match guard.live.get(proposal_id) {
            Some(entry) => entry,
            None => return Err(GovernedProcessBridgeError::Gateway),
        };

        if let Some(terminal_row) = entry.terminal.clone() {
            let provider_ok = entry.provider_ok.clone();
            let decided = entry.decided_payload.clone();
            // Receipt.started only when claim was confirmed (never pre-claim draft).
            let started = if entry.started_confirmed {
                entry.started_payload.clone()
            } else {
                None
            };
            let cancelled = terminal_row.cancelled.unwrap_or(false)
                || terminal_row.state == ExecProposalState::Cancelled;
            let timed_out = terminal_row.timed_out.unwrap_or(false);
            let proposal_id_out = terminal_row.proposal_id.clone();
            let state = terminal_row.state;
            let terminal = Some(terminal_payload(&terminal_row));
            let requested_payload = Some(requested_payload_from_view(&requested));
            guard.pending_by_run.remove(&run_id);
            if let Some(entry) = guard.live.remove(proposal_id) {
                retain_terminal_output(&mut guard, &entry);
            }
            return Ok(GovernedProcessReceipt {
                delivery: GovernedProcessDelivery::Executed,
                proposal_id: proposal_id_out,
                state,
                requested: requested_payload,
                decided,
                started,
                terminal,
                provider_ok,
                cancelled,
                timed_out,
            });
        }

        let now = Utc::now();
        if entry.interrupted {
            drop(guard);
            return finalize_interrupt(proposal_id, bound, requested);
        }
        if entry.cancelled {
            let state = entry.metadata.state;
            drop(guard);
            if state == ExecProposalState::PendingApproval || state == ExecProposalState::Approved {
                return finalize_cancel(proposal_id, bound, requested);
            }
            let guard = coord
                .state
                .lock()
                .map_err(|_| GovernedProcessBridgeError::Gateway)?;
            let (_g, _) = coord
                .cv
                .wait_timeout(guard, Duration::from_millis(50))
                .map_err(|_| GovernedProcessBridgeError::Gateway)?;
            continue;
        }
        if now >= entry.expires_at && entry.metadata.state == ExecProposalState::PendingApproval {
            drop(guard);
            return finalize_expire(proposal_id, bound, requested);
        }

        let wait_ms = entry
            .expires_at
            .signed_duration_since(now)
            .num_milliseconds()
            .clamp(1, 250) as u64;
        let (_guard, _timeout) = coord
            .cv
            .wait_timeout(guard, Duration::from_millis(wait_ms))
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        let _ = run_id;
    }
}

fn finalize_expire(
    proposal_id: &str,
    bound: &BoundProcessRun,
    requested: ProcessProposalRequestedView,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    finalize_via_gateway(
        proposal_id,
        bound,
        requested,
        ExecProposalState::Expired,
        |gw, cap, cid, pid| gw.expire_proposal(cap, cid, pid),
    )
}

fn finalize_cancel(
    proposal_id: &str,
    bound: &BoundProcessRun,
    requested: ProcessProposalRequestedView,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    finalize_via_gateway(
        proposal_id,
        bound,
        requested,
        ExecProposalState::Cancelled,
        |gw, cap, cid, pid| gw.cancel_proposal(cap, cid, pid),
    )
}

fn finalize_interrupt(
    proposal_id: &str,
    bound: &BoundProcessRun,
    requested: ProcessProposalRequestedView,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    finalize_via_gateway(
        proposal_id,
        bound,
        requested,
        ExecProposalState::Interrupted,
        |gw, cap, cid, pid| gw.interrupt_proposal(cap, cid, pid),
    )
}

fn finalize_via_gateway(
    proposal_id: &str,
    bound: &BoundProcessRun,
    requested: ProcessProposalRequestedView,
    _expected: ExecProposalState,
    op: impl FnOnce(
        &GovernedProcessToolGateway,
        &ProjectCapability,
        &str,
        &str,
    ) -> Result<StoredExecProposal, GovernedProcessError>,
) -> Result<GovernedProcessReceipt, GovernedProcessBridgeError> {
    // Clear grant before durable transition. Take the rule attribution in the
    // same pass: the live entry is gone by the time the decided payload is
    // built, and an auto-decision the timeline cannot show is invisible.
    let mut rule_attribution = None;
    if let Ok(mut guard) = coordinator().state.lock()
        && let Some(entry) = guard.live.get_mut(proposal_id)
    {
        entry.raw_grant.clear();
        rule_attribution = entry.auto_approved_by_rule.clone();
    }

    let result = op(
        &bound.gateway,
        &bound.capability,
        bound.context.conversation_id(),
        proposal_id,
    );

    let row = match result {
        Ok(row) if is_exec_terminal(row.state) => row,
        Ok(_) | Err(_) => {
            // Race or already-terminal: owner-bound get for durable truth.
            match bound.gateway.get_for_owner(
                &bound.capability,
                bound.context.conversation_id(),
                proposal_id,
            ) {
                Ok(Some(existing)) if is_exec_terminal(existing.state) => existing,
                _ => {
                    quarantine_live(proposal_id);
                    return Err(GovernedProcessBridgeError::Gateway);
                }
            }
        }
    };

    let cancelled = row.state == ExecProposalState::Cancelled;
    let decided = if row.state == ExecProposalState::Rejected {
        Some(ProcessProposalDecidedPayload {
            proposal_id: row.proposal_id.clone(),
            program: row.program.clone(),
            decision: ExecDecision::Reject,
            state: row.state,
            auto_approved_by_rule: rule_attribution.clone(),
        })
    } else {
        None
    };
    take_live_entry(proposal_id);
    Ok(GovernedProcessReceipt {
        delivery: GovernedProcessDelivery::Executed,
        proposal_id: row.proposal_id.clone(),
        state: row.state,
        requested: Some(requested_payload_from_view(&requested)),
        decided,
        started: None,
        terminal: Some(terminal_payload(&row)),
        provider_ok: provider_output_from_row(&row),
        cancelled,
        timed_out: false,
    })
}

fn fail_closed_after_reserve(proposal_id: &str, bound: &BoundProcessRun) {
    let _ = bound.gateway.interrupt_proposal(
        &bound.capability,
        bound.context.conversation_id(),
        proposal_id,
    );
    if let Ok(mut guard) = coordinator().state.lock() {
        remove_live_entry_and_pending(&mut guard, proposal_id);
        coordinator().cv.notify_all();
    }
}

/// Fail closed after a post-commit lifecycle observer rejects delivery.
///
/// Pending/approved proposals can be interrupted directly. A started proposal
/// is already durably `Running`, so first cancel its supervised execution and
/// wait for that owner path to persist a real terminal receipt. Live/pending
/// authority is released only after durable terminal truth is observable.
fn fail_closed_after_observer_failure(proposal_id: &str, bound: &BoundProcessRun) {
    let process_cancel = coordinator().state.lock().ok().and_then(|mut guard| {
        let entry = guard.live.get_mut(proposal_id)?;
        if entry.owner != bound.owner || entry.run_id != bound.context.run_id() {
            return None;
        }
        entry.raw_grant.clear();
        entry.observer_delivery_in_progress = false;
        entry.observer_failure_cleanup_in_progress = true;
        Some(entry.process_cancel.clone())
    });
    if let Some(process_cancel) = process_cancel {
        process_cancel.cancel();
    }

    let interrupted = bound
        .gateway
        .interrupt_proposal(
            &bound.capability,
            bound.context.conversation_id(),
            proposal_id,
        )
        .is_ok_and(|row| is_exec_terminal(row.state));

    if !interrupted {
        wait_for_durable_observer_failure_terminal(proposal_id, bound);
    }

    if let Ok(mut guard) = coordinator().state.lock() {
        let owned = guard.live.get(proposal_id).is_some_and(|entry| {
            entry.owner == bound.owner && entry.run_id == bound.context.run_id()
        });
        if owned {
            remove_live_entry_and_pending(&mut guard, proposal_id);
        }
        coordinator().cv.notify_all();
    }
}

/// Complete one successful lock-free observer delivery.
///
/// A concurrent decision/execution failure may have requested quarantine while
/// the callback was running. Preserve the pre-fix public `Gateway` result, but
/// defer removal until the callback has returned so an error can instead enter
/// the durable observer-failure cleanup path.
fn finish_observer_delivery(proposal_id: &str, bound: &BoundProcessRun) -> bool {
    let Ok(mut guard) = coordinator().state.lock() else {
        return false;
    };
    let Some(entry) = guard.live.get_mut(proposal_id) else {
        return false;
    };
    if entry.owner != bound.owner || entry.run_id != bound.context.run_id() {
        return false;
    }
    entry.observer_delivery_in_progress = false;
    if !entry.observer_quarantine_deferred {
        return false;
    }
    remove_live_entry_and_pending(&mut guard, proposal_id);
    coordinator().cv.notify_all();
    true
}

fn wait_for_durable_observer_failure_terminal(proposal_id: &str, bound: &BoundProcessRun) {
    let coord = coordinator();
    // Deliberately no wall-clock deadline: releasing the single-flight slot
    // before a real terminal is proven would permit overlapping authority.
    loop {
        let durable = bound.gateway.get_for_owner(
            &bound.capability,
            bound.context.conversation_id(),
            proposal_id,
        );
        if let Ok(Some(row)) = durable {
            if is_exec_terminal(row.state) {
                return;
            }
            if matches!(
                row.state,
                ExecProposalState::PendingApproval | ExecProposalState::Approved
            ) && bound
                .gateway
                .interrupt_proposal(
                    &bound.capability,
                    bound.context.conversation_id(),
                    proposal_id,
                )
                .is_ok_and(|row| is_exec_terminal(row.state))
            {
                return;
            }
        }

        let Ok(mut guard) = coord.state.lock() else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        if let Some(entry) = guard.live.get_mut(proposal_id)
            && entry.owner == bound.owner
            && entry.run_id == bound.context.run_id()
        {
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
            if entry
                .terminal
                .as_ref()
                .is_some_and(|row| is_exec_terminal(row.state))
            {
                return;
            }
        }
        let _ = coord.cv.wait_timeout(guard, Duration::from_millis(50));
    }
}

fn remove_live_entry_and_pending(guard: &mut CoordinatorState, proposal_id: &str) {
    if let Some(entry) = guard.live.remove(proposal_id)
        && guard
            .pending_by_run
            .get(&entry.run_id)
            .is_some_and(|pending| pending == proposal_id)
    {
        guard.pending_by_run.remove(&entry.run_id);
    }
}

fn quarantine_live(proposal_id: &str) {
    if let Ok(mut guard) = coordinator().state.lock() {
        if let Some(entry) = guard.live.get_mut(proposal_id)
            && (entry.observer_delivery_in_progress || entry.observer_failure_cleanup_in_progress)
        {
            entry.raw_grant.clear();
            entry.process_cancel.cancel();
            entry.observer_quarantine_deferred = true;
            coordinator().cv.notify_all();
            return;
        }
        remove_live_entry_and_pending(&mut guard, proposal_id);
        coordinator().cv.notify_all();
    }
}

fn take_live_entry(proposal_id: &str) {
    if let Ok(mut guard) = coordinator().state.lock() {
        remove_live_entry_and_pending(&mut guard, proposal_id);
        coordinator().cv.notify_all();
    }
}

fn retain_terminal_output(guard: &mut CoordinatorState, entry: &LiveProcessEntry) {
    let Some(metadata) = entry.terminal.clone() else {
        return;
    };
    guard
        .retained_terminal
        .retain(|cached| !(cached.owner == entry.owner && cached.proposal_id == entry.proposal_id));
    while guard
        .retained_terminal
        .iter()
        .filter(|cached| cached.owner == entry.owner)
        .count()
        >= TERMINAL_OUTPUTS_PER_OWNER
    {
        let Some(index) = guard
            .retained_terminal
            .iter()
            .position(|cached| cached.owner == entry.owner)
        else {
            break;
        };
        guard.retained_terminal.remove(index);
    }
    guard.retained_terminal.push_back(RetainedTerminalOutput {
        owner: entry.owner,
        conversation_id: entry.conversation_id.clone(),
        proposal_id: entry.proposal_id.clone(),
        program: entry.program.clone(),
        argv: entry.argv.clone(),
        metadata,
        chunks: entry.output_chunks.clone(),
    });
}

fn retained_shell_output(
    bound: &BoundProcessRun,
    receipt: &GovernedProcessReceipt,
) -> Option<GovernedShellOutput> {
    receipt.provider_ok.as_ref()?;
    let guard = coordinator().state.lock().ok()?;
    let retained = guard.retained_terminal.iter().find(|entry| {
        entry.owner == bound.owner
            && entry.conversation_id == bound.context.conversation_id()
            && entry.proposal_id == receipt.proposal_id
    })?;
    let mut chunks = retained.chunks.clone();
    chunks.sort_by_key(|chunk| chunk.sequence);
    let output = chunks
        .into_iter()
        .map(|chunk| chunk.text)
        .collect::<String>();
    Some(GovernedShellOutput {
        output,
        truncated: retained.metadata.truncated.unwrap_or(false),
    })
}

fn append_live_output_chunk(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    chunk: RunnerOutputChunk,
) {
    if let Ok(mut guard) = coordinator().state.lock()
        && let Some(entry) = guard.live.get_mut(proposal_id)
        && entry.owner == owner
        && entry.conversation_id == conversation_id
        && entry.terminal.is_none()
    {
        entry.output_chunks.push(ProcessOutputChunk {
            stream: match chunk.stream {
                keeper_process_executor::OutputStreamTag::Stdout => ProcessOutputStream::Stdout,
                keeper_process_executor::OutputStreamTag::Stderr => ProcessOutputStream::Stderr,
            },
            sequence: chunk.sequence,
            lossy_utf8: chunk.lossy_utf8,
            text: chunk.text,
        });
        coordinator().cv.notify_all();
    }
}

fn publish_terminal_for_waiter(
    proposal_id: &str,
    row: StoredExecProposal,
    provider_ok: Option<ProcessRunOutput>,
    live_output: Option<LiveProcessOutput>,
    decided: Option<ProcessProposalDecidedPayload>,
    started: Option<ProcessStartedPayload>,
) {
    if let Ok(mut guard) = coordinator().state.lock() {
        if let Some(entry) = guard.live.get_mut(proposal_id) {
            entry.metadata = row.clone();
            entry.terminal = Some(row);
            entry.provider_ok = provider_ok;
            entry.raw_grant.clear();
            entry.decided_payload = decided;
            if let Some(s) = started {
                entry.started_payload = Some(s);
                entry.started_confirmed = true;
            } else if !entry.started_confirmed {
                // Never invent a start on non-claim paths; keep claim-confirmed
                // payload if the post-claim hook already recorded one.
                entry.started_payload = None;
            }
            if let Some(live) = live_output {
                entry.output_chunks = map_gateway_chunks(&live);
            }
            entry.waiter_notified = true;
        }
        coordinator().cv.notify_all();
    }
}

// ── List / decide / open-output ─────────────────────────────────────────────

/// List process proposals for a conversation owned by this host instance.
pub fn list_process_proposals(
    owner: Uuid,
    conversation_id: &str,
    gateway: Option<&GovernedProcessToolGateway>,
    permission_rules: Option<&SqlitePermissionRuleRepository>,
) -> Result<Vec<LiveProcessProposalView>, GovernedProcessBridgeError> {
    reconcile_expired_for_owner(owner, Some(conversation_id))?;

    let mut views = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let now = Utc::now();
    let mut capability: Option<ProjectCapability> = None;

    {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        for entry in guard.live.values() {
            if entry.owner != owner || entry.conversation_id != conversation_id {
                continue;
            }
            let decision_available = entry.terminal.is_none()
                && entry.metadata.state == ExecProposalState::PendingApproval
                && now < entry.expires_at
                && !entry.raw_grant.is_empty()
                && !entry.auto_rule_pending;
            seen.insert(entry.proposal_id.clone());
            views.push(live_view_from_entry(entry, decision_available));
        }
        // Prefer capability from a bound run for this owner+conversation.
        for bound in guard.runs.values() {
            if bound.owner == owner && bound.context.conversation_id() == conversation_id {
                capability = Some(bound.capability.clone());
                break;
            }
        }
    }

    if let (Some(gateway), Some(cap)) = (gateway, capability) {
        let rows = gateway
            .list_for_conversation(&cap, conversation_id)
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        for row in rows {
            if seen.contains(&row.proposal_id) {
                continue;
            }
            let rule_receipt = permission_rules
                .map(|rules| rules.get_exec_rule_receipt(&row.proposal_id))
                .transpose()
                .map_err(|_| GovernedProcessBridgeError::Gateway)?
                .flatten();
            views.push(view_from_stored(
                &row,
                &[],
                row.portable_cwd.clone(),
                false,
                rule_receipt.as_ref(),
            ));
        }
    }
    Ok(views)
}

/// Open host-retained process output for a live proposal owned by this conversation.
pub fn open_process_output(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    gateway: Option<&GovernedProcessToolGateway>,
) -> Result<ProcessOutputView, GovernedProcessBridgeError> {
    reconcile_expired_for_owner(owner, Some(conversation_id))?;

    let guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedProcessBridgeError::Gateway)?;
    if let Some(entry) = guard.live.get(proposal_id) {
        if entry.owner != owner || entry.conversation_id != conversation_id {
            return Err(GovernedProcessBridgeError::NotLive);
        }
        let meta = entry.terminal.as_ref().unwrap_or(&entry.metadata);
        return Ok(output_view(
            entry.proposal_id.clone(),
            entry.program.clone(),
            entry.argv.clone(),
            meta,
            entry.output_chunks.clone(),
            true,
        ));
    }
    if let Some(entry) = guard.retained_terminal.iter().find(|entry| {
        entry.owner == owner
            && entry.conversation_id == conversation_id
            && entry.proposal_id == proposal_id
    }) {
        return Ok(output_view(
            entry.proposal_id.clone(),
            entry.program.clone(),
            entry.argv.clone(),
            &entry.metadata,
            entry.chunks.clone(),
            true,
        ));
    }
    let capability = guard
        .capabilities
        .get(&(owner, conversation_id.to_owned()))
        .cloned();
    drop(guard);
    let (Some(gateway), Some(capability)) = (gateway, capability) else {
        return Err(GovernedProcessBridgeError::NotLive);
    };
    let Some(row) = gateway
        .get_for_owner(&capability, conversation_id, proposal_id)
        .map_err(|_| GovernedProcessBridgeError::NotLive)?
    else {
        return Err(GovernedProcessBridgeError::NotLive);
    };
    if !is_exec_terminal(row.state) {
        return Err(GovernedProcessBridgeError::NotLive);
    }
    Ok(output_view(
        row.proposal_id.clone(),
        row.program.clone(),
        Vec::new(),
        &row,
        Vec::new(),
        false,
    ))
}

fn output_view(
    proposal_id: String,
    program: String,
    argv: Vec<String>,
    metadata: &StoredExecProposal,
    chunks: Vec<ProcessOutputChunk>,
    live_retained: bool,
) -> ProcessOutputView {
    ProcessOutputView {
        proposal_id,
        program,
        argv,
        state: metadata.state,
        chunks,
        truncated: metadata.truncated.unwrap_or(false),
        encoding_lossy: metadata.encoding_lossy.unwrap_or(false),
        captured_bytes: metadata.captured_bytes.unwrap_or(0),
        exit_code: metadata.exit_code,
        duration_ms: metadata.duration_ms,
        live_retained,
    }
}

/// Ordinary public decision API: owner-scoped identity + approve-once/reject only.
///
/// Does not accept or return a raw grant. The host supplies its retained
/// grant digest internally to the gateway.
pub fn decide_process_proposal(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    decision: ExecDecision,
) -> Result<LiveProcessProposalView, GovernedProcessBridgeError> {
    decide_process_proposal_inner(owner, conversation_id, proposal_id, decision, None)
}

fn decide_process_proposal_by_rules(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    matched: &[PermissionRuleSegmentMatchSnapshot],
) -> Result<LiveProcessProposalView, GovernedProcessBridgeError> {
    let first = matched
        .first()
        .ok_or(GovernedProcessBridgeError::DecisionDenied)?;
    let decision = match first.matched.rule.action {
        PermissionRuleAction::Allow => ExecDecision::ApproveOnce,
        PermissionRuleAction::Deny => ExecDecision::Reject,
    };
    decide_process_proposal_inner(owner, conversation_id, proposal_id, decision, Some(matched))
}

fn decide_process_proposal_inner(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    decision: ExecDecision,
    matched: Option<&[PermissionRuleSegmentMatchSnapshot]>,
) -> Result<LiveProcessProposalView, GovernedProcessBridgeError> {
    reconcile_expired_for_owner(owner, Some(conversation_id))?;

    let (bound, fingerprint, grant_digest_value, sealed, process_cancel, program, argv, cwd) = {
        let mut guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        let entry = guard
            .live
            .get_mut(proposal_id)
            .ok_or(GovernedProcessBridgeError::NotLive)?;
        if entry.owner != owner || entry.conversation_id != conversation_id {
            return Err(GovernedProcessBridgeError::NotLive);
        }
        if entry.terminal.is_some() {
            return Err(GovernedProcessBridgeError::NotLive);
        }
        if Utc::now() >= entry.expires_at {
            return Err(GovernedProcessBridgeError::Expired);
        }
        if matched.is_none() && entry.raw_grant.is_empty() {
            return Err(GovernedProcessBridgeError::DecisionDenied);
        }
        if matched.is_some() && !entry.auto_rule_pending {
            return Err(GovernedProcessBridgeError::DecisionDenied);
        }
        // Single-use: clear raw grant material at decision time.
        entry.raw_grant.clear();
        let run_id = entry.run_id;
        let fingerprint = entry.proposal_fingerprint.clone();
        let grant_digest_value = entry.grant_digest.clone();
        let sealed = entry.sealed.clone();
        let process_cancel = entry.process_cancel.clone();
        let program = entry.program.clone();
        let argv = entry.argv.clone();
        let cwd = entry.portable_cwd.clone();
        let bound = guard
            .runs
            .get(&run_id)
            .filter(|b| b.owner == owner)
            .cloned()
            .ok_or(GovernedProcessBridgeError::NotLive)?;
        (
            bound,
            fingerprint,
            grant_digest_value,
            sealed,
            process_cancel,
            program,
            argv,
            cwd,
        )
    };

    let outcome = if let Some(matched) = matched {
        bound
            .permission_rules
            .as_deref()
            .ok_or(GovernedProcessBridgeError::Gateway)?
            .decide_exec_by_rule_segments(
                proposal_id,
                conversation_id,
                matched,
                &grant_digest_value,
                &fingerprint,
                Utc::now(),
            )
            .map_err(|error| match error {
                keeper_storage_sqlite::ExecProposalStoreError::OwnershipMismatch => {
                    GovernedProcessBridgeError::NotLive
                }
                _ => GovernedProcessBridgeError::Gateway,
            })?
    } else {
        bound
            .gateway
            .decide_for_owner(
                &bound.capability,
                conversation_id,
                proposal_id,
                decision,
                &grant_digest_value,
                &fingerprint,
            )
            .map_err(|error| match error {
                GovernedProcessError::CapabilityMismatch | GovernedProcessError::NotApplicable => {
                    GovernedProcessBridgeError::NotLive
                }
                _ => GovernedProcessBridgeError::Gateway,
            })?
    };

    let rule_receipt = if matched.is_some() {
        bound
            .permission_rules
            .as_deref()
            .ok_or(GovernedProcessBridgeError::Gateway)?
            .get_exec_rule_receipt(proposal_id)
            .map_err(|_| GovernedProcessBridgeError::Gateway)?
    } else {
        None
    };
    if matched.is_some()
        && let Ok(mut guard) = coordinator().state.lock()
        && let Some(entry) = guard.live.get_mut(proposal_id)
    {
        entry.auto_rule_pending = false;
        entry.auto_approved_by_rule = rule_receipt.as_ref().map(attribution_from_receipt);
    }

    match outcome {
        ExecDecisionOutcome::Accepted(row) if decision == ExecDecision::Reject => {
            let decided = ProcessProposalDecidedPayload {
                proposal_id: row.proposal_id.clone(),
                program: row.program.clone(),
                decision: ExecDecision::Reject,
                state: row.state,
                auto_approved_by_rule: rule_receipt.as_ref().map(attribution_from_receipt),
            };
            // Durable reject first; mark decided eligibility + terminal, notify
            // waiter (no EventSink here — waiter owns emit callbacks).
            publish_terminal_for_waiter(proposal_id, row.clone(), None, None, Some(decided), None);
            Ok(view_from_stored(
                &row,
                &argv,
                cwd,
                false,
                rule_receipt.as_ref(),
            ))
        }
        ExecDecisionOutcome::Accepted(_approved) if decision == ExecDecision::ApproveOnce => {
            let decided = ProcessProposalDecidedPayload {
                proposal_id: proposal_id.to_owned(),
                program: program.clone(),
                decision: ExecDecision::ApproveOnce,
                state: ExecProposalState::Approved,
                auto_approved_by_rule: rule_receipt.as_ref().map(attribution_from_receipt),
            };
            let started = ProcessStartedPayload {
                proposal_id: proposal_id.to_owned(),
                program: program.clone(),
                argv: argv.clone(),
                portable_cwd: cwd.clone(),
            };

            // After durable decide only: record decided eligibility and wake
            // waiter. Do NOT mark started yet — claim may still return Busy /
            // Expired / Conflict without a legitimate start.
            if let Ok(mut guard) = coordinator().state.lock()
                && let Some(entry) = guard.live.get_mut(proposal_id)
            {
                entry.metadata.state = ExecProposalState::Approved;
                entry.decided_payload = Some(decided.clone());
                entry.started_payload = None;
                entry.started_confirmed = false;
            }
            coordinator().cv.notify_all();

            // Test-only seam: run after durable approve, before claim/execute.
            #[cfg(any(test, feature = "test-support"))]
            take_pre_claim_hook_for_tests(proposal_id);

            // One-shot claim → spawn. Busy/terminal never re-spawn.
            let observer_owner = owner;
            let observer_conversation = conversation_id.to_owned();
            let observer_proposal = proposal_id.to_owned();
            let observer: OutputObserver = Arc::new(move |chunk| {
                append_live_output_chunk(
                    observer_owner,
                    &observer_conversation,
                    &observer_proposal,
                    chunk,
                );
            });
            // Post-claim only: confirm started eligibility after durable Running.
            let claim_owner = owner;
            let claim_conversation = conversation_id.to_owned();
            let claim_proposal = proposal_id.to_owned();
            let claim_started = started.clone();
            let on_claimed: ClaimedObserver = Box::new(move || {
                if let Ok(mut guard) = coordinator().state.lock()
                    && let Some(entry) = guard.live.get_mut(&claim_proposal)
                    && entry.owner == claim_owner
                    && entry.conversation_id == claim_conversation
                    && entry.terminal.is_none()
                {
                    entry.started_payload = Some(claim_started);
                    entry.started_confirmed = true;
                }
                coordinator().cv.notify_all();
            });
            let exec: Result<GovernedProcessExecution, GovernedProcessError> = sealed
                .execute_with_observer(&bound, proposal_id, process_cancel, observer, on_claimed);

            match exec {
                Ok(execution) => {
                    let row = execution.metadata().clone();
                    let live = execution.live_output;
                    if row.state == ExecProposalState::Running {
                        quarantine_live(proposal_id);
                        return Err(GovernedProcessBridgeError::Busy);
                    }
                    let provider_ok = provider_output_from_row(&row);
                    // Non-Claimed / non-spawn outcomes must not advertise start.
                    let started_out = match &execution.outcome {
                        GovernedProcessOutcome::Busy(_)
                        | GovernedProcessOutcome::Terminal(_)
                        | GovernedProcessOutcome::Rejected(_)
                        | GovernedProcessOutcome::Expired(_)
                        | GovernedProcessOutcome::Conflict(_) => None,
                        _ => {
                            // Prefer claim-confirmed payload already on the live
                            // entry; fall back to local only when claim succeeded.
                            Some(started)
                        }
                    };
                    publish_terminal_for_waiter(
                        proposal_id,
                        row.clone(),
                        provider_ok,
                        live,
                        Some(decided),
                        started_out,
                    );
                    Ok(view_from_stored(
                        &row,
                        &argv,
                        cwd,
                        false,
                        rule_receipt.as_ref(),
                    ))
                }
                Err(GovernedProcessError::CleanupUnproven) => {
                    quarantine_live(proposal_id);
                    Err(GovernedProcessBridgeError::CleanupUnproven)
                }
                Err(error) => {
                    quarantine_live(proposal_id);
                    Err(map_seal_error(error))
                }
            }
        }
        ExecDecisionOutcome::Accepted(row) => Ok(view_from_stored(
            &row,
            &argv,
            cwd,
            false,
            rule_receipt.as_ref(),
        )),
        ExecDecisionOutcome::Terminal(row) => Ok(view_from_stored(
            &row,
            &argv,
            cwd,
            false,
            rule_receipt.as_ref(),
        )),
        ExecDecisionOutcome::Conflict(_) => Err(GovernedProcessBridgeError::DecisionDenied),
        ExecDecisionOutcome::Expired(row) => {
            publish_terminal_for_waiter(proposal_id, row, None, None, None, None);
            Err(GovernedProcessBridgeError::Expired)
        }
    }
}

fn reconcile_expired_for_owner(
    owner: Uuid,
    conversation_id: Option<&str>,
) -> Result<(), GovernedProcessBridgeError> {
    let now = Utc::now();
    let mut to_expire: Vec<(String, Arc<BoundProcessRun>)> = Vec::new();
    {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedProcessBridgeError::Gateway)?;
        for entry in guard.live.values() {
            if entry.owner != owner {
                continue;
            }
            if let Some(cid) = conversation_id
                && entry.conversation_id != cid
            {
                continue;
            }
            if entry.terminal.is_some() {
                continue;
            }
            if now < entry.expires_at {
                continue;
            }
            if entry.metadata.state != ExecProposalState::PendingApproval {
                continue;
            }
            if let Some(bound) = guard.runs.get(&entry.run_id).filter(|b| b.owner == owner) {
                to_expire.push((entry.proposal_id.clone(), Arc::clone(bound)));
            }
        }
    }

    for (proposal_id, bound) in to_expire {
        match bound.gateway.expire_proposal(
            &bound.capability,
            bound.context.conversation_id(),
            &proposal_id,
        ) {
            Ok(row) => {
                publish_terminal_for_waiter(&proposal_id, row, None, None, None, None);
            }
            Err(_) => {
                quarantine_live(&proposal_id);
                return Err(GovernedProcessBridgeError::Gateway);
            }
        }
    }
    Ok(())
}

fn live_view_from_entry(
    entry: &LiveProcessEntry,
    decision_available: bool,
) -> LiveProcessProposalView {
    let state = entry
        .terminal
        .as_ref()
        .map(|t| t.state)
        .unwrap_or(entry.metadata.state);
    let meta = entry.terminal.as_ref().unwrap_or(&entry.metadata);
    LiveProcessProposalView {
        proposal_id: entry.proposal_id.clone(),
        conversation_id: entry.conversation_id.clone(),
        origin: ProcessOrigin::Agent,
        state,
        program: entry.program.clone(),
        argv: entry.argv.clone(),
        portable_cwd: entry.portable_cwd.clone(),
        risk_class: entry.risk_class,
        summary: entry.summary.clone(),
        expires_at: entry.expires_at,
        decision_available,
        exit_code: meta.exit_code,
        duration_ms: meta.duration_ms,
        cancelled: meta.cancelled,
        timed_out: meta.timed_out,
        truncated: meta.truncated,
        encoding_lossy: meta.encoding_lossy,
        captured_bytes: meta.captured_bytes,
        rule_subject: entry.rule_subject.clone(),
        auto_approved_by_rule: entry.auto_approved_by_rule.clone(),
    }
}

fn view_from_stored(
    row: &StoredExecProposal,
    argv: &[String],
    portable_cwd: Option<String>,
    decision_available: bool,
    rule_receipt: Option<&StoredPermissionRuleReceipt>,
) -> LiveProcessProposalView {
    let cwd = portable_cwd.or_else(|| row.portable_cwd.clone());
    let summary = if argv.is_empty() {
        process_summary(&row.program, &[])
    } else {
        process_summary(&row.program, argv)
    };
    LiveProcessProposalView {
        proposal_id: row.proposal_id.clone(),
        conversation_id: row.conversation_id.clone(),
        origin: ProcessOrigin::Agent,
        state: row.state,
        program: row.program.clone(),
        argv: argv.to_vec(),
        portable_cwd: cwd,
        risk_class: if row.program == "shell" {
            ProcessRiskClass::High
        } else {
            classify_risk(&row.program, argv)
        },
        summary,
        expires_at: row.expires_at,
        decision_available,
        exit_code: row.exit_code,
        duration_ms: row.duration_ms,
        cancelled: row.cancelled,
        timed_out: row.timed_out,
        truncated: row.truncated,
        encoding_lossy: row.encoding_lossy,
        captured_bytes: row.captured_bytes,
        rule_subject: if row.program == "shell" {
            String::new()
        } else {
            rule_receipt
                .map(|receipt| receipt.match_subject.clone())
                .unwrap_or_else(|| canonical_process_subject(&row.program, argv))
        },
        auto_approved_by_rule: rule_receipt.map(attribution_from_receipt),
    }
}

/// Restart recovery through the gateway only. No database path / repository.
pub fn recover_process_proposals(
    gateway: &GovernedProcessToolGateway,
    now: DateTime<Utc>,
) -> Result<Vec<StoredExecProposal>, GovernedProcessBridgeError> {
    gateway
        .recover_interrupted(now)
        .map_err(|_| GovernedProcessBridgeError::Gateway)
}

/// Test seam: force a live entry's `expires_at` into the past.
///
/// Absent from ordinary product builds (`cfg(test)` only).
#[cfg(any(test, feature = "test-support"))]
pub fn force_live_process_expiry_for_tests(owner: Uuid, proposal_id: &str) -> bool {
    let Ok(mut guard) = coordinator().state.lock() else {
        return false;
    };
    if let Some(entry) = guard.live.get_mut(proposal_id)
        && entry.owner == owner
    {
        entry.expires_at = Utc::now() - chrono::Duration::seconds(1);
        return true;
    }
    false
}

/// Install a proposal-scoped, one-shot pre-claim hook (test builds only).
#[cfg(any(test, feature = "test-support"))]
pub fn set_pre_claim_hook_for_tests(proposal_id: &str, hook: Box<dyn FnMut(&str) + Send>) -> bool {
    let Ok(mut guard) = coordinator().state.lock() else {
        return false;
    };
    let Some(entry) = guard.live.get_mut(proposal_id) else {
        return false;
    };
    entry.pre_claim_hook = Some(hook);
    true
}

/// Clear a proposal-scoped pre-claim hook (test builds only).
#[cfg(any(test, feature = "test-support"))]
pub fn clear_pre_claim_hook_for_tests(proposal_id: &str) -> bool {
    let Ok(mut guard) = coordinator().state.lock() else {
        return false;
    };
    let Some(entry) = guard.live.get_mut(proposal_id) else {
        return false;
    };
    entry.pre_claim_hook = None;
    true
}

/// Test seam: run after durable approve, before one-shot claim/execute.
///
/// The hook lives on the target proposal, so parallel approve tests cannot
/// consume it. It is taken under the coordinator lock and invoked lock-free.
#[cfg(any(test, feature = "test-support"))]
fn take_pre_claim_hook_for_tests(proposal_id: &str) {
    let mut hook = match coordinator().state.lock() {
        Ok(mut guard) => guard
            .live
            .get_mut(proposal_id)
            .and_then(|entry| entry.pre_claim_hook.take()),
        Err(_) => None,
    };
    if let Some(ref mut hook) = hook {
        hook(proposal_id);
    }
}
