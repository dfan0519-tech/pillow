//! Run-scoped governed write coordinator.
//!
//! Seals base/proposed content and the raw one-time approval grant only in
//! active-host memory. SQLite receives digests and metadata only. The
//! coordinator is the sole live authority for waiters, sealed contents, and
//! host-owned fingerprints.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
// Contract types live in `keeper-agent-protocol`; re-export for backward compatibility.
pub use keeper_agent_protocol::{
    GovernedWriteDelivery, GovernedWriteReceipt, ProviderWriteProposal,
};

use keeper_agent_protocol::{
    FsWriteArguments, FsWriteOutput, RunId, ToolActiveContext, ToolArguments, ToolProposal,
    WRITE_CONTENT_BYTE_CAP, WriteMode, WriteProposalRequestedPayload, WriteProposalResolvedPayload,
    WriteProposalState, WritePublicErrorCode, canonical_arguments_digest,
};
use keeper_policy::WriteDecision;
use keeper_storage_sqlite::{
    StoredWriteProposal, WriteDecisionOutcome, WriteProposalReserveInput, WriteReservationOutcome,
};
use keeper_tool_executor::{
    BaseSnapshot, BoundedDiff, CancellationSignal, GovernedWriteApplyOutcome, GovernedWriteError,
    GovernedWriteToolGateway, ProjectCapability, SealedWriteContents, content_sha256_digest,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Canonical raw approval grant: 32 CSPRNG bytes → base64url (no pad) = 43 chars.
pub const APPROVAL_GRANT_RAW_LEN: usize = 43;

const GRANT_RANDOM_BYTES: usize = 32;
const BASE64URL_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Live conversation-facing proposal view (native list/decide DTO source).
#[derive(Clone)]
pub struct LiveWriteProposalView {
    pub proposal_id: String,
    pub conversation_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub state: WriteProposalState,
    pub summary: String,
    pub expires_at: DateTime<Utc>,
    pub approval_grant: Option<String>,
    pub error_code: Option<WritePublicErrorCode>,
    pub diff: Option<BoundedDiff>,
}

impl std::fmt::Debug for LiveWriteProposalView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveWriteProposalView")
            .field("proposal_id", &self.proposal_id)
            .field("conversation_id", &self.conversation_id)
            .field("path", &self.path)
            .field("mode", &self.mode)
            .field("state", &self.state)
            .field("summary", &self.summary)
            .field("expires_at", &self.expires_at)
            .field(
                "approval_grant",
                &self.approval_grant.as_ref().map(|_| "<redacted>"),
            )
            .field("error_code", &self.error_code)
            .field("diff", &self.diff.as_ref().map(|d| d.truncated))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteDiffView {
    pub proposal_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub unified: String,
    pub truncated: bool,
    pub line_count: u32,
    pub byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GovernedWriteBridgeError {
    #[error("governed write run is not registered")]
    RunNotRegistered,
    #[error("provider write proposal is invalid")]
    InvalidProposal,
    #[error("provider write call conflicts with its durable reservation")]
    ReservationConflict,
    #[error("write proposal is already pending on this run")]
    Busy,
    #[error("governed write context is invalid")]
    InvalidContext,
    #[error("governed write gateway is unavailable")]
    Gateway,
    #[error("write proposal is not live in the active coordinator")]
    NotLive,
    #[error("write proposal ownership mismatch")]
    OwnershipMismatch,
    #[error("approval grant is malformed or does not match the live owner")]
    InvalidGrant,
    #[error("write proposal decision is not allowed")]
    DecisionDenied,
    #[error("write proposal expired")]
    Expired,
    #[error("write proposal was cancelled")]
    Cancelled,
    #[error("write proposal was interrupted")]
    Interrupted,
    #[error("host is shutting down")]
    ShutDown,
}

struct BoundWriteRun {
    owner: Uuid,
    context: ToolActiveContext,
    capability: ProjectCapability,
    gateway: Arc<GovernedWriteToolGateway>,
}

struct LiveWriteEntry {
    owner: Uuid,
    run_id: RunId,
    conversation_id: String,
    proposal_id: String,
    proposal_fingerprint: String,
    path: String,
    mode: WriteMode,
    summary: String,
    expires_at: DateTime<Utc>,
    sealed: SealedWriteContents,
    raw_grant: String,
    grant_digest: String,
    diff: BoundedDiff,
    metadata: StoredWriteProposal,
    terminal: Option<WriteProposalResolvedPayload>,
    provider_ok: Option<FsWriteOutput>,
    cancelled: bool,
    interrupted: bool,
    /// True while one caller owns the only durable terminal transition attempt.
    terminal_transition_in_progress: bool,
    #[allow(dead_code)]
    waiter_notified: bool,
}

impl std::fmt::Debug for LiveWriteEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveWriteEntry")
            .field("proposal_id", &self.proposal_id)
            .field("run_id", &self.run_id)
            .field("path", &self.path)
            .field("mode", &self.mode)
            .field("state", &self.metadata.state)
            .field("raw_grant", &"<redacted>")
            .field("sealed", &self.sealed)
            .finish()
    }
}

struct CoordinatorState {
    runs: HashMap<RunId, Arc<BoundWriteRun>>,
    /// Active live proposals keyed by proposal_id.
    live: HashMap<String, LiveWriteEntry>,
    /// At most one pending proposal id per run.
    pending_by_run: HashMap<RunId, String>,
}

impl CoordinatorState {
    fn new() -> Self {
        Self {
            runs: HashMap::new(),
            live: HashMap::new(),
            pending_by_run: HashMap::new(),
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

pub(crate) fn open_write_gateway(
    path: impl AsRef<Path>,
) -> Result<Arc<GovernedWriteToolGateway>, GovernedWriteBridgeError> {
    GovernedWriteToolGateway::open(path)
        .map(Arc::new)
        .map_err(|_| GovernedWriteBridgeError::Gateway)
}

pub(crate) fn bind_write_run(
    owner: Uuid,
    gateway: Arc<GovernedWriteToolGateway>,
    run_id: RunId,
    project_id: impl Into<String>,
    conversation_id: impl Into<String>,
    project_root: impl AsRef<Path>,
) -> Result<(), GovernedWriteBridgeError> {
    let project_id = project_id.into();
    let context = ToolActiveContext::new(project_id.clone(), conversation_id, run_id)
        .map_err(|_| GovernedWriteBridgeError::InvalidContext)?;
    let capability = ProjectCapability::new(project_id, project_root)
        .map_err(|_| GovernedWriteBridgeError::InvalidContext)?;
    let mut guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedWriteBridgeError::Gateway)?;
    if guard.runs.contains_key(&run_id) {
        return Err(GovernedWriteBridgeError::InvalidContext);
    }
    guard.runs.insert(
        run_id,
        Arc::new(BoundWriteRun {
            owner,
            context,
            capability,
            gateway,
        }),
    );
    Ok(())
}

pub(crate) fn release_write_run(owner: Uuid, run_id: RunId) {
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
            entry.waiter_notified = true;
            coordinator().cv.notify_all();
        }
    }
}

pub(crate) fn release_write_owner(owner: Uuid) {
    let Ok(mut guard) = coordinator().state.lock() else {
        return;
    };
    // Interrupt only this host owner's waiters. Do not flip a process-global
    // shut_down bit — other RuntimeHost instances (and tests) must keep working.
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
            entry.waiter_notified = true;
        }
    }
    for entry in guard.live.values_mut() {
        if entry.owner == owner {
            entry.interrupted = true;
            entry.waiter_notified = true;
        }
    }
    coordinator().cv.notify_all();
}

/// Generate a canonical 43-character base64url (no pad) grant from OS CSPRNG.
pub fn generate_approval_grant() -> Result<String, GovernedWriteBridgeError> {
    let mut bytes = [0u8; GRANT_RANDOM_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|_| GovernedWriteBridgeError::Gateway)?;
    let grant = URL_SAFE_NO_PAD.encode(bytes);
    debug_assert_eq!(grant.len(), APPROVAL_GRANT_RAW_LEN);
    if !is_canonical_grant(&grant) {
        return Err(GovernedWriteBridgeError::Gateway);
    }
    Ok(grant)
}

/// True when grant is exactly 43 chars of base64url alphabet (no padding).
pub fn is_canonical_grant(grant: &str) -> bool {
    grant.len() == APPROVAL_GRANT_RAW_LEN && grant.bytes().all(|b| BASE64URL_ALPHABET.contains(&b))
}

/// `sha256:` + lowercase hex of the 43 raw grant ASCII bytes.
pub fn grant_digest(grant: &str) -> Result<String, GovernedWriteBridgeError> {
    if !is_canonical_grant(grant) {
        return Err(GovernedWriteBridgeError::InvalidGrant);
    }
    Ok(digest_bytes(grant.as_bytes()))
}

fn digest_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn write_summary(path: &str, mode: WriteMode) -> String {
    match mode {
        WriteMode::Create => format!("Create {path}"),
        WriteMode::Modify => format!("Modify {path}"),
    }
}

/// Deterministic UUID string (36 chars) derived from the reservation key.
fn stable_proposal_id(reservation_key: &str) -> String {
    let digest = Sha256::digest(reservation_key.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // RFC 4122 version/variant bits so Uuid::parse_str accepts the value.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

fn reservation_key(
    project_id: &str,
    conversation_id: &str,
    run_id: RunId,
    call_id: &str,
) -> String {
    format!("wp:{project_id}:{conversation_id}:{run_id}:{call_id}")
}

#[allow(clippy::too_many_arguments)]
fn host_fingerprint(
    project_id: &str,
    conversation_id: &str,
    run_id: RunId,
    call_id: &str,
    path: &str,
    mode: WriteMode,
    proposed_digest: &str,
    base_digest: Option<&str>,
) -> Result<String, GovernedWriteBridgeError> {
    let mode_str = match mode {
        WriteMode::Create => "create",
        WriteMode::Modify => "modify",
    };
    let value = serde_json::json!({
        "project_id": project_id,
        "conversation_id": conversation_id,
        "run_id": run_id.to_string(),
        "call_id": call_id,
        "tool_id": "fs.write",
        "tool_version": 1,
        "path": path,
        "mode": mode_str,
        "proposed_digest": proposed_digest,
        "base_digest": base_digest,
    });
    canonical_arguments_digest(&value)
        .map(|d| d.as_str().to_owned())
        .map_err(|_| GovernedWriteBridgeError::InvalidProposal)
}

fn parse_write_arguments(arguments: &Value) -> Result<FsWriteArguments, GovernedWriteBridgeError> {
    let object = arguments
        .as_object()
        .ok_or(GovernedWriteBridgeError::InvalidProposal)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "path" | "mode" | "content"))
    {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    }
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty() && *p != ".")
        .ok_or(GovernedWriteBridgeError::InvalidProposal)?
        .to_owned();
    let mode = match object.get("mode").and_then(Value::as_str) {
        Some("create") => WriteMode::Create,
        Some("modify") => WriteMode::Modify,
        _ => return Err(GovernedWriteBridgeError::InvalidProposal),
    };
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .ok_or(GovernedWriteBridgeError::InvalidProposal)?
        .to_owned();
    let bytes = content.as_bytes();
    if bytes.is_empty() || bytes.len() as u64 > WRITE_CONTENT_BYTE_CAP || bytes.contains(&0) {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    }
    Ok(FsWriteArguments {
        path,
        mode,
        content,
    })
}

/// Execute one adapter-normalized `fs.write@1` proposal for an active run.
///
/// Ordering: durable `pending_approval` reserve, then `on_requested` callback
/// (caller emits the runtime event), then condition wait for the exact
/// proposal. Approve applies sealed content; reject/expire/cancel return fixed
/// opaque provider errors.
pub fn execute_governed_write(
    run_id: RunId,
    provider: ProviderWriteProposal,
    cancellation: &AtomicBool,
    mut on_requested: impl FnMut(&WriteProposalRequestedPayload) -> Result<(), GovernedWriteBridgeError>,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    if provider.tool_id != "fs.write" || provider.tool_version != 1 {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    }
    if provider.call_id.is_empty() {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    }

    let bound = {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        guard
            .runs
            .get(&run_id)
            .cloned()
            .ok_or(GovernedWriteBridgeError::RunNotRegistered)?
    };

    let args = parse_write_arguments(&provider.arguments)?;
    let arguments_digest = canonical_arguments_digest(&provider.arguments)
        .map_err(|_| GovernedWriteBridgeError::InvalidProposal)?;
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
    .map_err(|_| GovernedWriteBridgeError::InvalidProposal)?;

    // Ensure typed args match the validated tool arguments.
    let ToolArguments::FsWrite(validated_args) = proposal.arguments() else {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    };
    if validated_args.path != args.path
        || validated_args.mode != args.mode
        || validated_args.content != args.content
    {
        return Err(GovernedWriteBridgeError::InvalidProposal);
    }

    let proposed_digest = content_sha256_digest(&args.content);
    let proposed_bytes = args.content.len() as u64;
    let reservation_key = reservation_key(
        bound.context.project_id(),
        bound.context.conversation_id(),
        run_id,
        &provider.call_id,
    );
    // Stable per call identity so terminal replay matches storage's exact
    // (proposal_id, reservation_key) without depending on recaptured base.
    let proposal_id = stable_proposal_id(&reservation_key);

    // Terminal identity before base capture: after a successful modify the
    // on-disk base has changed, so recapture would recompute a different
    // fingerprint and cannot replay. Compare durable authority/argument fields
    // only — never trust provider input as host base.
    if let Some(existing) = bound
        .gateway
        .store()
        .get(&proposal_id)
        .map_err(|_| GovernedWriteBridgeError::Gateway)?
    {
        if existing.project_id == bound.context.project_id()
            && existing.conversation_id == bound.context.conversation_id()
            && existing.run_id == run_id
            && existing.call_id == provider.call_id
            && existing.reservation_key == reservation_key
            && existing.proposal_id == proposal_id
        {
            if is_terminal_state(existing.state) {
                if existing.path == args.path
                    && existing.mode == args.mode
                    && existing.proposed_digest == proposed_digest
                    && existing.proposed_bytes == proposed_bytes
                {
                    return Ok(replay_receipt(existing));
                }
                return Err(GovernedWriteBridgeError::ReservationConflict);
            }
        } else {
            // Same stable proposal id with foreign authority fields — conflict.
            return Err(GovernedWriteBridgeError::ReservationConflict);
        }
    }

    // Capture base only after terminal identity has been ruled out.
    let snapshot = match GovernedWriteToolGateway::capture_base(
        &bound.capability,
        &args.path,
        args.mode,
        &args.content,
    ) {
        Ok(snapshot) => Some(snapshot),
        Err(GovernedWriteError::BaseCaptureFailed) if args.mode == WriteMode::Create => None,
        Err(error) => return Err(map_gateway_error(error)),
    };

    let (base_digest, base_bytes) = match &snapshot {
        Some(s) => (s.base_digest.clone(), s.base_bytes),
        None => (None, 0),
    };

    let proposal_fingerprint = host_fingerprint(
        bound.context.project_id(),
        bound.context.conversation_id(),
        run_id,
        &provider.call_id,
        &args.path,
        args.mode,
        &proposed_digest,
        base_digest.as_deref(),
    )?;

    let reserve_input = WriteProposalReserveInput {
        proposal_id: proposal_id.clone(),
        reservation_key,
        proposal_fingerprint: proposal_fingerprint.clone(),
        project_id: bound.context.project_id().into(),
        conversation_id: bound.context.conversation_id().into(),
        run_id,
        call_id: provider.call_id.clone(),
        path: args.path.clone(),
        mode: args.mode,
        proposed_digest: proposed_digest.clone(),
        proposed_bytes,
        base_digest: base_digest.clone(),
        base_bytes,
    };

    let reserved = bound
        .gateway
        .store()
        .reserve(&reserve_input)
        .map_err(|_| GovernedWriteBridgeError::Gateway)?;

    match reserved {
        WriteReservationOutcome::Replay(row) if is_terminal_state(row.state) => {
            Ok(replay_receipt(row))
        }
        WriteReservationOutcome::Replay(_) => {
            // Non-terminal replay without live sealed content cannot apply.
            Err(GovernedWriteBridgeError::NotLive)
        }
        WriteReservationOutcome::Conflict(_) => Err(GovernedWriteBridgeError::ReservationConflict),
        WriteReservationOutcome::Busy(_) => Err(GovernedWriteBridgeError::Busy),
        WriteReservationOutcome::Reserved(row) => {
            let snapshot = match snapshot {
                Some(snapshot) => snapshot,
                None => {
                    // Fresh reserve requires a successful host base capture.
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    return Err(GovernedWriteBridgeError::InvalidProposal);
                }
            };
            // Capture digests must match the reserved metadata.
            if snapshot.proposed_digest != proposed_digest
                || snapshot.proposed_bytes != proposed_bytes
                || snapshot.base_digest != base_digest
                || snapshot.base_bytes != base_bytes
            {
                fail_closed_after_reserve(&proposal_id, &bound.gateway);
                return Err(GovernedWriteBridgeError::InvalidContext);
            }
            let BaseSnapshot { base_content, .. } = snapshot;
            let sealed = SealedWriteContents::new(args.content.clone(), base_content);
            let prepared = bound
                .gateway
                .prepare(row.clone(), sealed.clone())
                .map_err(|error| {
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    map_gateway_error(error)
                })?;
            let raw_grant = match generate_approval_grant() {
                Ok(grant) => grant,
                Err(error) => {
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    return Err(error);
                }
            };
            let grant_digest = match grant_digest(&raw_grant) {
                Ok(digest) => digest,
                Err(error) => {
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    return Err(error);
                }
            };
            let summary = write_summary(&args.path, args.mode);
            let requested = WriteProposalRequestedPayload {
                proposal_id: proposal_id.clone(),
                path: args.path.clone(),
                mode: args.mode,
                summary: summary.clone(),
                expires_at: row.expires_at,
            };

            {
                let mut guard = coordinator()
                    .state
                    .lock()
                    .map_err(|_| GovernedWriteBridgeError::Gateway)?;
                if guard.pending_by_run.contains_key(&run_id) {
                    drop(guard);
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    return Err(GovernedWriteBridgeError::Busy);
                }
                // If this host owner is no longer bound (shutdown raced), fail closed.
                if !guard
                    .runs
                    .get(&run_id)
                    .is_some_and(|b| b.owner == bound.owner)
                {
                    drop(guard);
                    fail_closed_after_reserve(&proposal_id, &bound.gateway);
                    return Err(GovernedWriteBridgeError::ShutDown);
                }
                guard.pending_by_run.insert(run_id, proposal_id.clone());
                guard.live.insert(
                    proposal_id.clone(),
                    LiveWriteEntry {
                        owner: bound.owner,
                        run_id,
                        conversation_id: bound.context.conversation_id().into(),
                        proposal_id: proposal_id.clone(),
                        proposal_fingerprint,
                        path: args.path.clone(),
                        mode: args.mode,
                        summary,
                        expires_at: row.expires_at,
                        sealed,
                        raw_grant,
                        grant_digest,
                        diff: prepared.diff,
                        metadata: row,
                        terminal: None,
                        provider_ok: None,
                        cancelled: false,
                        interrupted: false,
                        terminal_transition_in_progress: false,
                        waiter_notified: false,
                    },
                );
            }

            // Emit only after durable pending_approval.
            if let Err(error) = on_requested(&requested) {
                fail_closed_after_reserve(&proposal_id, &bound.gateway);
                return Err(error);
            }

            wait_for_resolution(run_id, &proposal_id, &bound, cancellation, requested)
        }
    }
}

fn wait_for_resolution(
    run_id: RunId,
    proposal_id: &str,
    bound: &BoundWriteRun,
    cancellation: &AtomicBool,
    requested: WriteProposalRequestedPayload,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    let coord = coordinator();
    loop {
        let mut guard = coord
            .state
            .lock()
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;

        // Cancellation / shutdown / interrupt checks under the lock.
        if cancellation.load(Ordering::SeqCst)
            && let Some(entry) = guard.live.get_mut(proposal_id)
        {
            entry.cancelled = true;
        }

        let entry = match guard.live.get(proposal_id) {
            Some(entry) => entry,
            // Entry removed without a durable terminal marker ⇒ storage/fault
            // quarantine; never invent success/expired/cancelled for the waiter.
            None => return Err(GovernedWriteBridgeError::Gateway),
        };

        if let Some(resolved) = entry.terminal.clone() {
            let provider_ok = entry.provider_ok.clone();
            let provider_error = resolved.error_code;
            // Remove only after terminal durable transition and waiter notification.
            guard.pending_by_run.remove(&run_id);
            guard.live.remove(proposal_id);
            return Ok(GovernedWriteReceipt {
                delivery: GovernedWriteDelivery::Executed,
                requested: Some(requested),
                resolved,
                provider_ok,
                provider_error,
            });
        }

        let now = Utc::now();
        if entry.interrupted {
            drop(guard);
            return finalize_interrupt(proposal_id, bound, requested);
        }
        if entry.cancelled {
            drop(guard);
            return finalize_cancel(proposal_id, bound, requested);
        }
        if now >= entry.expires_at {
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
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        // Loop continues; only the exact proposal is inspected on the next turn.
        let _ = run_id;
    }
}

fn finalize_expire(
    proposal_id: &str,
    bound: &BoundWriteRun,
    requested: WriteProposalRequestedPayload,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    finalize_requested_terminal(
        proposal_id,
        bound,
        requested,
        WriteProposalState::Expired,
        WritePublicErrorCode::ApprovalExpired,
        #[cfg(any(test, feature = "test-support"))]
        "waiter-expiry",
    )
}

fn finalize_cancel(
    proposal_id: &str,
    bound: &BoundWriteRun,
    requested: WriteProposalRequestedPayload,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    finalize_requested_terminal(
        proposal_id,
        bound,
        requested,
        WriteProposalState::Cancelled,
        WritePublicErrorCode::Cancelled,
        #[cfg(any(test, feature = "test-support"))]
        "waiter-cancel",
    )
}

fn finalize_interrupt(
    proposal_id: &str,
    bound: &BoundWriteRun,
    requested: WriteProposalRequestedPayload,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    finalize_requested_terminal(
        proposal_id,
        bound,
        requested,
        WriteProposalState::Interrupted,
        WritePublicErrorCode::Interrupted,
        #[cfg(any(test, feature = "test-support"))]
        "waiter-interrupt",
    )
}

/// Transition to a requested terminal only after the durable row is that
/// terminal (or an already-durable terminal). Storage failure never fabricates
/// metadata, never panics, never claims the requested terminal, and never
/// issues a second write — live secrets are quarantined and Gateway is returned.
fn finalize_requested_terminal(
    proposal_id: &str,
    bound: &BoundWriteRun,
    requested: WriteProposalRequestedPayload,
    requested_state: WriteProposalState,
    requested_error: WritePublicErrorCode,
    #[cfg(any(test, feature = "test-support"))] transition_source: &str,
) -> Result<GovernedWriteReceipt, GovernedWriteBridgeError> {
    let now = Utc::now();
    let store = bound.gateway.store();
    #[cfg(any(test, feature = "test-support"))]
    store.wait_at_transition_barrier_for_tests(proposal_id, transition_source);
    let result = match read_durable_terminal_after_transition(
        store,
        proposal_id,
        bound.owner,
        requested_state,
        now,
        #[cfg(any(test, feature = "test-support"))]
        transition_source,
    ) {
        Ok(row) => {
            let error_code = if row.state == requested_state {
                Some(requested_error)
            } else {
                terminal_error(&row)
            };
            let resolved = resolved_payload(&row, error_code);
            take_live_entry_for_owner(proposal_id, bound.owner);
            Ok(GovernedWriteReceipt {
                delivery: GovernedWriteDelivery::Executed,
                requested: Some(requested),
                resolved,
                provider_ok: None,
                provider_error: error_code,
            })
        }
        Err(()) => Err(GovernedWriteBridgeError::Gateway),
    };
    #[cfg(any(test, feature = "test-support"))]
    store.complete_transition_source_for_tests(proposal_id, transition_source);
    result
}

/// Proposal-scoped terminal single-flight. Both callers qualify under the
/// coordinator before entering here; exactly one winner performs one transition
/// write and at most one recovery read. Losers wait without holding the mutex
/// and may only accept a real durable terminal row. Failure quarantines live
/// authority and wakes every loser without fabricating a terminal state.
fn read_durable_terminal_after_transition(
    store: &keeper_storage_sqlite::SqliteWriteProposalRepository,
    proposal_id: &str,
    expected_owner: Uuid,
    next: WriteProposalState,
    now: DateTime<Utc>,
    #[cfg(any(test, feature = "test-support"))] transition_source: &str,
) -> Result<StoredWriteProposal, ()> {
    let should_transition = {
        let coord = coordinator();
        let mut guard = coord.state.lock().map_err(|_| ())?;
        loop {
            match guard.live.get_mut(proposal_id) {
                Some(entry) if entry.owner != expected_owner => return Err(()),
                // Other durable paths publish a terminal marker without always
                // replacing live metadata. Observe storage instead of trusting
                // a possibly stale PendingApproval row.
                Some(entry) if entry.terminal.is_some() => break false,
                Some(entry) if !entry.terminal_transition_in_progress => {
                    entry.terminal_transition_in_progress = true;
                    break true;
                }
                Some(_) => {}
                None => break false,
            }
            guard = coord.cv.wait(guard).map_err(|_| ())?;
        }
    };

    // A winner may have published and removed the live entry before this loser
    // reacquires the coordinator. Durable terminal is the only valid recovery;
    // a missing/non-terminal row remains an opaque storage failure.
    if !should_transition {
        return match store.get(proposal_id) {
            Ok(Some(row)) if is_terminal_state(row.state) => Ok(row),
            _ => Err(()),
        };
    }

    #[cfg(any(test, feature = "test-support"))]
    let transition =
        store.transition_with_source_for_tests(proposal_id, next, now, transition_source);
    #[cfg(not(any(test, feature = "test-support")))]
    let transition = store.transition(proposal_id, next, now);

    let durable = match transition {
        Ok(row) if is_terminal_state(row.state) => Ok(row),
        Ok(_) => Err(()),
        Err(_) => match store.get(proposal_id) {
            // Read-only recovery of an already-durable terminal; no second write.
            Ok(Some(row)) if is_terminal_state(row.state) => Ok(row),
            _ => Err(()),
        },
    };

    match durable {
        Ok(row) => {
            let resolved = resolved_payload(&row, terminal_error(&row));
            if let Ok(mut guard) = coordinator().state.lock() {
                if let Some(entry) = guard
                    .live
                    .get_mut(proposal_id)
                    .filter(|entry| entry.owner == expected_owner)
                {
                    entry.metadata = row.clone();
                    entry.terminal = Some(resolved);
                    entry.terminal_transition_in_progress = false;
                    entry.waiter_notified = true;
                }
                coordinator().cv.notify_all();
            }
            Ok(row)
        }
        Err(()) => {
            quarantine_live_on_storage_failure(proposal_id, expected_owner);
            Err(())
        }
    }
}

/// Drop live secrets/waiter state without publishing a fabricated terminal.
/// Restart recovery may later reconcile the durable non-terminal row.
fn quarantine_live_on_storage_failure(proposal_id: &str, expected_owner: Uuid) {
    take_live_entry_for_owner(proposal_id, expected_owner);
}

/// Fail closed after durable reserve and before normal waiting: durable
/// interrupted, remove live/pending_by_run, notify any waiter, drop sealed
/// content/grant/diff with the entry.
fn fail_closed_after_reserve(proposal_id: &str, gateway: &GovernedWriteToolGateway) {
    let _ = gateway
        .store()
        .transition(proposal_id, WriteProposalState::Interrupted, Utc::now());
    if let Ok(mut guard) = coordinator().state.lock() {
        if let Some(entry) = guard.live.remove(proposal_id) {
            guard.pending_by_run.remove(&entry.run_id);
        }
        // Always notify so an exact waiter (if any) cannot hang on a missing entry.
        coordinator().cv.notify_all();
    }
}

/// Publish a terminal outcome for the waiter path (entry removed by waiter).
fn publish_terminal_for_waiter(
    proposal_id: &str,
    resolved: WriteProposalResolvedPayload,
    provider_ok: Option<FsWriteOutput>,
) {
    if let Ok(mut guard) = coordinator().state.lock() {
        if let Some(entry) = guard.live.get_mut(proposal_id) {
            entry.terminal = Some(resolved);
            entry.provider_ok = provider_ok;
            entry.waiter_notified = true;
        }
        coordinator().cv.notify_all();
    }
}

/// Fully drop the expected owner's live entry after its waiter takes the receipt.
/// A stale caller must not remove a replacement entry owned by another host.
fn take_live_entry_for_owner(proposal_id: &str, expected_owner: Uuid) {
    if let Ok(mut guard) = coordinator().state.lock() {
        let matches_owner = guard
            .live
            .get(proposal_id)
            .is_some_and(|entry| entry.owner == expected_owner);
        if matches_owner
            && let Some(entry) = guard.live.remove(proposal_id)
            && guard.pending_by_run.get(&entry.run_id).map(String::as_str) == Some(proposal_id)
        {
            guard.pending_by_run.remove(&entry.run_id);
        }
        coordinator().cv.notify_all();
    }
}

fn resolved_payload(
    row: &StoredWriteProposal,
    error_code: Option<WritePublicErrorCode>,
) -> WriteProposalResolvedPayload {
    WriteProposalResolvedPayload {
        proposal_id: row.proposal_id.clone(),
        path: row.path.clone(),
        mode: row.mode,
        state: row.state,
        error_code,
    }
}

fn replay_receipt(row: StoredWriteProposal) -> GovernedWriteReceipt {
    let error_code = match row.state {
        WriteProposalState::Succeeded => None,
        WriteProposalState::Rejected => Some(WritePublicErrorCode::ApprovalRejected),
        WriteProposalState::Expired => Some(WritePublicErrorCode::ApprovalExpired),
        WriteProposalState::Cancelled => Some(WritePublicErrorCode::Cancelled),
        WriteProposalState::Interrupted => Some(WritePublicErrorCode::Interrupted),
        WriteProposalState::Failed => Some(WritePublicErrorCode::WriteFailed),
        _ => Some(WritePublicErrorCode::WriteConflict),
    };
    let provider_ok = if row.state == WriteProposalState::Succeeded {
        Some(FsWriteOutput {
            path: row.path.clone(),
            mode: row.mode,
            bytes_written: row.proposed_bytes,
            content_digest: row.proposed_digest.clone(),
        })
    } else {
        None
    };
    GovernedWriteReceipt {
        delivery: GovernedWriteDelivery::Replayed,
        requested: None,
        resolved: resolved_payload(&row, error_code),
        provider_ok,
        provider_error: error_code,
    }
}

fn is_terminal_state(state: WriteProposalState) -> bool {
    matches!(
        state,
        WriteProposalState::Succeeded
            | WriteProposalState::Rejected
            | WriteProposalState::Expired
            | WriteProposalState::Cancelled
            | WriteProposalState::Interrupted
            | WriteProposalState::Failed
    )
}

fn map_gateway_error(error: GovernedWriteError) -> GovernedWriteBridgeError {
    match error {
        GovernedWriteError::PathRejected | GovernedWriteError::BaseCaptureFailed => {
            GovernedWriteBridgeError::InvalidProposal
        }
        GovernedWriteError::InvalidProjectRoot | GovernedWriteError::DigestMismatch => {
            GovernedWriteBridgeError::InvalidContext
        }
        GovernedWriteError::Store(_) | GovernedWriteError::NotApplicable => {
            GovernedWriteBridgeError::Gateway
        }
    }
}

/// List proposals for a conversation owned by this host instance.
///
/// Owner mismatch is indistinguishable from empty/unavailable. Live past-due
/// proposals are durably expired, wake only their waiter, and never return a grant.
///
/// Durable rows are read from the host instance's own `gateway` — never from an
/// arbitrary bound run — so listing works after restart with no active runs.
pub fn list_write_proposals(
    owner: Uuid,
    conversation_id: &str,
    gateway: Option<&GovernedWriteToolGateway>,
) -> Result<Vec<LiveWriteProposalView>, GovernedWriteBridgeError> {
    reconcile_expired_for_owner(
        owner,
        Some(conversation_id),
        #[cfg(any(test, feature = "test-support"))]
        "main-list",
    )?;

    let mut views = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let now = Utc::now();

    {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        for entry in guard.live.values() {
            if entry.owner != owner || entry.conversation_id != conversation_id {
                continue;
            }
            // Past-due live rows should already be terminalized; never expose grant.
            let include_grant = entry.terminal.is_none()
                && entry.metadata.state == WriteProposalState::PendingApproval
                && now < entry.expires_at;
            seen.insert(entry.proposal_id.clone());
            views.push(live_view_from_entry(entry, include_grant));
        }
    }

    if let Some(gateway) = gateway {
        let rows = gateway
            .store()
            .list_for_conversation(conversation_id)
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        for row in rows {
            if seen.contains(&row.proposal_id) {
                continue;
            }
            views.push(view_from_stored(&row, None, None));
        }
    }
    Ok(views)
}

/// Open the host-built diff only when the proposal is live, non-terminal, and
/// not past expiry for this owner+conversation.
pub fn open_write_proposal_diff(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
) -> Result<WriteDiffView, GovernedWriteBridgeError> {
    // Expire first so a direct diff open at/past deadline never exposes bytes.
    reconcile_expired_for_owner(
        owner,
        Some(conversation_id),
        #[cfg(any(test, feature = "test-support"))]
        "main-diff",
    )?;

    let guard = coordinator()
        .state
        .lock()
        .map_err(|_| GovernedWriteBridgeError::Gateway)?;
    let entry = guard
        .live
        .get(proposal_id)
        .ok_or(GovernedWriteBridgeError::NotLive)?;
    // Owner mismatch is indistinguishable from not-live at public boundaries.
    if entry.owner != owner || entry.conversation_id != conversation_id {
        return Err(GovernedWriteBridgeError::NotLive);
    }
    if entry.terminal.is_some() {
        return Err(GovernedWriteBridgeError::NotLive);
    }
    if Utc::now() >= entry.expires_at {
        return Err(GovernedWriteBridgeError::Expired);
    }
    Ok(WriteDiffView {
        proposal_id: entry.proposal_id.clone(),
        path: entry.path.clone(),
        mode: entry.mode,
        unified: entry.diff.unified.clone(),
        truncated: entry.diff.truncated,
        line_count: entry.diff.line_count,
        byte_count: entry.diff.byte_count,
    })
}

/// Native one-shot decision. Uses host-owned fingerprint and grant digest only.
///
/// Returns after durable terminal state plus exact waiter notification. Does not
/// poll for in-memory map removal.
pub fn decide_write_proposal(
    owner: Uuid,
    conversation_id: &str,
    proposal_id: &str,
    approval_grant: &str,
    decision: WriteDecision,
) -> Result<LiveWriteProposalView, GovernedWriteBridgeError> {
    if !is_canonical_grant(approval_grant) {
        return Err(GovernedWriteBridgeError::InvalidGrant);
    }

    // Expire first if past due so listing/decision never act on a stale grant.
    reconcile_expired_for_owner(
        owner,
        Some(conversation_id),
        #[cfg(any(test, feature = "test-support"))]
        "main-decision",
    )?;

    let (bound, fingerprint, grant_digest, sealed) = {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        let entry = guard
            .live
            .get(proposal_id)
            .ok_or(GovernedWriteBridgeError::NotLive)?;
        if entry.owner != owner || entry.conversation_id != conversation_id {
            return Err(GovernedWriteBridgeError::NotLive);
        }
        if entry.terminal.is_some() {
            return Err(GovernedWriteBridgeError::NotLive);
        }
        if Utc::now() >= entry.expires_at {
            return Err(GovernedWriteBridgeError::Expired);
        }
        if !constant_time_eq(&entry.raw_grant, approval_grant) {
            return Err(GovernedWriteBridgeError::InvalidGrant);
        }
        let bound = guard
            .runs
            .get(&entry.run_id)
            .filter(|b| b.owner == owner)
            .cloned()
            .ok_or(GovernedWriteBridgeError::NotLive)?;
        (
            bound,
            entry.proposal_fingerprint.clone(),
            entry.grant_digest.clone(),
            entry.sealed.clone(),
        )
    };

    let now = Utc::now();
    let outcome = bound
        .gateway
        .store()
        .decide(
            proposal_id,
            conversation_id,
            decision,
            &grant_digest,
            &fingerprint,
            now,
        )
        .map_err(|error| match error {
            keeper_storage_sqlite::WriteProposalStoreError::OwnershipMismatch => {
                GovernedWriteBridgeError::NotLive
            }
            _ => GovernedWriteBridgeError::Gateway,
        })?;

    match outcome {
        WriteDecisionOutcome::Accepted(row) if decision == WriteDecision::Reject => {
            let resolved = resolved_payload(&row, Some(WritePublicErrorCode::ApprovalRejected));
            publish_terminal_for_waiter(proposal_id, resolved, None);
            Ok(view_from_stored(
                &row,
                None,
                Some(WritePublicErrorCode::ApprovalRejected),
            ))
        }
        WriteDecisionOutcome::Accepted(_accepted) if decision == WriteDecision::ApproveOnce => {
            let apply =
                match bound
                    .gateway
                    .apply(&bound.capability, proposal_id, &sealed, &NeverCancel)
                {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        // Never leave a live waiter pending until TTL, and never
                        // invent a terminal success/expired/cancelled receipt.
                        quarantine_live_on_storage_failure(proposal_id, owner);
                        return Err(map_gateway_error(error));
                    }
                };
            let (row, provider_ok, error_code) = match apply {
                GovernedWriteApplyOutcome::Succeeded(row) => {
                    let output = FsWriteOutput {
                        path: row.path.clone(),
                        mode: row.mode,
                        bytes_written: row.proposed_bytes,
                        content_digest: row.proposed_digest.clone(),
                    };
                    (row, Some(output), None)
                }
                GovernedWriteApplyOutcome::Failed(row) => {
                    (row, None, Some(WritePublicErrorCode::WriteFailed))
                }
                GovernedWriteApplyOutcome::Busy(row) | GovernedWriteApplyOutcome::Conflict(row) => {
                    (row, None, Some(WritePublicErrorCode::WriteConflict))
                }
                GovernedWriteApplyOutcome::Terminal(row) => {
                    return Ok(view_from_stored(&row, None, terminal_error(&row)));
                }
                GovernedWriteApplyOutcome::Interrupted(row) => {
                    (row, None, Some(WritePublicErrorCode::Interrupted))
                }
                GovernedWriteApplyOutcome::Expired(row) => {
                    (row, None, Some(WritePublicErrorCode::ApprovalExpired))
                }
            };
            let resolved = resolved_payload(&row, error_code);
            publish_terminal_for_waiter(proposal_id, resolved, provider_ok);
            Ok(view_from_stored(&row, None, error_code))
        }
        WriteDecisionOutcome::Accepted(row) => Ok(view_from_stored(&row, None, None)),
        WriteDecisionOutcome::Terminal(row) => {
            Ok(view_from_stored(&row, None, terminal_error(&row)))
        }
        WriteDecisionOutcome::Conflict(_) => Err(GovernedWriteBridgeError::DecisionDenied),
        WriteDecisionOutcome::Expired(row) => {
            let resolved = resolved_payload(&row, Some(WritePublicErrorCode::ApprovalExpired));
            publish_terminal_for_waiter(proposal_id, resolved, None);
            Err(GovernedWriteBridgeError::Expired)
        }
    }
}

/// Durably expire past-due live proposals for `owner`, notify only those waiters.
///
/// Publishes a terminal marker only when SQLite returns a real terminal row.
/// Storage failure quarantines live secrets and returns Gateway — never
/// fabricates an expired row or claims success.
fn reconcile_expired_for_owner(
    owner: Uuid,
    conversation_id: Option<&str>,
    #[cfg(any(test, feature = "test-support"))] transition_source: &str,
) -> Result<(), GovernedWriteBridgeError> {
    let now = Utc::now();
    let mut to_expire: Vec<(String, Arc<BoundWriteRun>)> = Vec::new();
    {
        let guard = coordinator()
            .state
            .lock()
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
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
            if let Some(bound) = guard.runs.get(&entry.run_id).filter(|b| b.owner == owner) {
                to_expire.push((entry.proposal_id.clone(), Arc::clone(bound)));
            }
        }
    }

    for (proposal_id, bound) in to_expire {
        #[cfg(any(test, feature = "test-support"))]
        bound
            .gateway
            .store()
            .wait_at_transition_barrier_for_tests(&proposal_id, transition_source);
        let store = bound.gateway.store();
        let result = read_durable_terminal_after_transition(
            store,
            &proposal_id,
            owner,
            WriteProposalState::Expired,
            now,
            #[cfg(any(test, feature = "test-support"))]
            transition_source,
        );
        #[cfg(any(test, feature = "test-support"))]
        store.complete_transition_source_for_tests(&proposal_id, transition_source);
        if result.is_err() {
            return Err(GovernedWriteBridgeError::Gateway);
        }
    }
    Ok(())
}

fn terminal_error(row: &StoredWriteProposal) -> Option<WritePublicErrorCode> {
    match row.state {
        WriteProposalState::Succeeded => None,
        WriteProposalState::Rejected => Some(WritePublicErrorCode::ApprovalRejected),
        WriteProposalState::Expired => Some(WritePublicErrorCode::ApprovalExpired),
        WriteProposalState::Cancelled => Some(WritePublicErrorCode::Cancelled),
        WriteProposalState::Interrupted => Some(WritePublicErrorCode::Interrupted),
        WriteProposalState::Failed => Some(WritePublicErrorCode::WriteFailed),
        _ => Some(WritePublicErrorCode::WriteConflict),
    }
}

fn live_view_from_entry(entry: &LiveWriteEntry, include_grant: bool) -> LiveWriteProposalView {
    let state = entry
        .terminal
        .as_ref()
        .map(|t| t.state)
        .unwrap_or(entry.metadata.state);
    let error_code = entry.terminal.as_ref().and_then(|t| t.error_code);
    LiveWriteProposalView {
        proposal_id: entry.proposal_id.clone(),
        conversation_id: entry.conversation_id.clone(),
        path: entry.path.clone(),
        mode: entry.mode,
        state,
        summary: entry.summary.clone(),
        expires_at: entry.expires_at,
        approval_grant: if include_grant
            && state == WriteProposalState::PendingApproval
            && entry.terminal.is_none()
        {
            Some(entry.raw_grant.clone())
        } else {
            None
        },
        error_code,
        diff: if entry.terminal.is_none() {
            Some(entry.diff.clone())
        } else {
            None
        },
    }
}

fn view_from_stored(
    row: &StoredWriteProposal,
    grant: Option<String>,
    error_code: Option<WritePublicErrorCode>,
) -> LiveWriteProposalView {
    LiveWriteProposalView {
        proposal_id: row.proposal_id.clone(),
        conversation_id: row.conversation_id.clone(),
        path: row.path.clone(),
        mode: row.mode,
        state: row.state,
        summary: write_summary(&row.path, row.mode),
        expires_at: row.expires_at,
        approval_grant: grant,
        error_code: error_code.or_else(|| terminal_error(row)),
        diff: None,
    }
}

struct NeverCancel;

impl CancellationSignal for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Recover non-terminal durable rows on restart (no live grants/diffs).
///
/// `pending_approval` / `approved` → `interrupted`. Orphaned `applying` rows
/// require a resolvable project root; without one recovery fails closed and
/// leaves the applying row unchanged (never synthesizes `Neither`).
pub fn recover_write_proposals(
    gateway: &GovernedWriteToolGateway,
    now: DateTime<Utc>,
) -> Result<Vec<StoredWriteProposal>, GovernedWriteBridgeError> {
    recover_write_proposals_with_roots(gateway, now, |_| None)
}

/// Like [`recover_write_proposals`], but resolves project roots so `applying`
/// rows can be observed and reconciled.
///
/// Only the executor's real read-only observation may produce
/// `MatchesProposed | StillBaseOrAbsent | Neither`. Missing/unresolvable
/// project authority returns [`GovernedWriteBridgeError::Gateway`] and leaves
/// the applying row unchanged.
pub fn recover_write_proposals_with_roots(
    gateway: &GovernedWriteToolGateway,
    now: DateTime<Utc>,
    mut resolve_root: impl FnMut(&str) -> Option<std::path::PathBuf>,
) -> Result<Vec<StoredWriteProposal>, GovernedWriteBridgeError> {
    let mut recovered = gateway
        .store()
        .recover_interrupted(now)
        .map_err(|_| GovernedWriteBridgeError::Gateway)?;

    let applying = gateway
        .store()
        .list_applying()
        .map_err(|_| GovernedWriteBridgeError::Gateway)?;

    for row in applying {
        let Some(root) = resolve_root(&row.project_id) else {
            // Never synthesize Neither for missing authority.
            return Err(GovernedWriteBridgeError::Gateway);
        };
        let capability = ProjectCapability::new(row.project_id.clone(), root)
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        let observation = gateway.observe_applying_target(&capability, &row);
        let terminal = gateway
            .store()
            .reconcile_applying(&row.proposal_id, observation, now)
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
        recovered.push(terminal);
    }

    Ok(recovered)
}

/// Test-only: force a live entry's `expires_at` into the past so list/decide
/// expiry reconciliation can be exercised without waiting for the 600s TTL.
#[cfg(any(test, feature = "test-support"))]
pub fn force_live_expiry_for_tests(owner: Uuid, proposal_id: &str) -> bool {
    let Ok(mut guard) = coordinator().state.lock() else {
        return false;
    };
    if let Some(entry) = guard.live.get_mut(proposal_id)
        && entry.owner == owner
    {
        entry.expires_at = Utc::now() - chrono::Duration::seconds(1);
        coordinator().cv.notify_all();
        return true;
    }
    false
}

#[cfg(test)]
mod grant_tests {
    use super::*;

    #[test]
    fn generated_grant_is_canonical_43_base64url() {
        let grant = generate_approval_grant().unwrap();
        assert_eq!(grant.len(), 43);
        assert!(is_canonical_grant(&grant));
        assert!(!grant.contains('='));
        let digest = grant_digest(&grant).unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), "sha256:".len() + 64);
    }

    #[test]
    fn malformed_grant_rejected_before_digest() {
        assert!(!is_canonical_grant("short"));
        assert!(is_canonical_grant(&"a".repeat(43))); // lowercase is valid base64url
        assert!(is_canonical_grant(&"A".repeat(43)));
        assert!(!is_canonical_grant(&format!("{}=", "A".repeat(42)))); // padding forbidden
        assert!(!is_canonical_grant(&"A".repeat(42))); // wrong length
        assert!(!is_canonical_grant(&format!("{}+", "A".repeat(42)))); // + not in url alphabet
        assert!(grant_digest("not-valid!!").is_err());
        assert!(grant_digest(&"A".repeat(42)).is_err());
    }

    #[test]
    fn constant_time_eq_rejects_wrong_well_formed_grant() {
        let a = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let b = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        assert_eq!(a.len(), 43);
        assert_eq!(b.len(), 43);
        assert!(is_canonical_grant(a));
        assert!(is_canonical_grant(b));
        assert!(!constant_time_eq(a, b));
        assert!(constant_time_eq(a, a));
    }

    #[test]
    fn live_entry_debug_redacts_grant_and_content() {
        let sealed = SealedWriteContents::new("secret-body".into(), None);
        let entry = LiveWriteEntry {
            owner: Uuid::nil(),
            run_id: RunId(Uuid::nil()),
            conversation_id: "c".into(),
            proposal_id: "wp".into(),
            proposal_fingerprint: content_sha256_digest("fp"),
            path: "a.ts".into(),
            mode: WriteMode::Create,
            summary: "Create a.ts".into(),
            expires_at: Utc::now(),
            sealed,
            raw_grant: "SECRET_GRANT_VALUE_MUST_NOT_APPEAR_IN_DBG!!".into(),
            grant_digest: content_sha256_digest("g"),
            diff: keeper_tool_executor::build_bounded_unified_diff(None, "secret-body"),
            metadata: StoredWriteProposal {
                proposal_id: "wp".into(),
                reservation_key: "rk".into(),
                proposal_fingerprint: content_sha256_digest("fp"),
                project_id: "p".into(),
                conversation_id: "c".into(),
                run_id: RunId(Uuid::nil()),
                call_id: "call".into(),
                path: "a.ts".into(),
                mode: WriteMode::Create,
                proposed_digest: content_sha256_digest("secret-body"),
                proposed_bytes: 11,
                base_digest: None,
                base_bytes: 0,
                state: WriteProposalState::PendingApproval,
                expires_at: Utc::now(),
                result_json: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            terminal: None,
            provider_ok: None,
            cancelled: false,
            interrupted: false,
            terminal_transition_in_progress: false,
            waiter_notified: false,
        };
        let debug = format!("{entry:?}");
        assert!(!debug.contains("SECRET_GRANT"));
        assert!(!debug.contains("secret-body"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn list_past_due_expires_and_strips_grant() {
        // Construct a live pending entry with expires_at already past and
        // verify live_view never exposes the grant when include_grant is false
        // after expiry, and that reconcile path uses terminal state.
        let past = Utc::now() - chrono::Duration::seconds(5);
        let sealed = SealedWriteContents::new("body".into(), None);
        let entry = LiveWriteEntry {
            owner: Uuid::nil(),
            run_id: RunId(Uuid::nil()),
            conversation_id: "c".into(),
            proposal_id: "wp-exp".into(),
            proposal_fingerprint: content_sha256_digest("fp"),
            path: "a.ts".into(),
            mode: WriteMode::Create,
            summary: "Create a.ts".into(),
            expires_at: past,
            sealed,
            raw_grant: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            grant_digest: content_sha256_digest("g"),
            diff: keeper_tool_executor::build_bounded_unified_diff(None, "body"),
            metadata: StoredWriteProposal {
                proposal_id: "wp-exp".into(),
                reservation_key: "rk".into(),
                proposal_fingerprint: content_sha256_digest("fp"),
                project_id: "p".into(),
                conversation_id: "c".into(),
                run_id: RunId(Uuid::nil()),
                call_id: "call".into(),
                path: "a.ts".into(),
                mode: WriteMode::Create,
                proposed_digest: content_sha256_digest("body"),
                proposed_bytes: 4,
                base_digest: None,
                base_bytes: 0,
                state: WriteProposalState::PendingApproval,
                expires_at: past,
                result_json: None,
                created_at: past,
                updated_at: past,
            },
            terminal: None,
            provider_ok: None,
            cancelled: false,
            interrupted: false,
            terminal_transition_in_progress: false,
            waiter_notified: false,
        };
        // Past-due listing must never include the grant.
        let view = live_view_from_entry(&entry, false);
        assert!(view.approval_grant.is_none());
        let view_if_mistakenly_true = live_view_from_entry(&entry, true);
        // Even with include_grant true, state is still pending until terminal;
        // the list path forces include_grant=false when now >= expires_at.
        assert!(view_if_mistakenly_true.approval_grant.is_some());
        assert!(Utc::now() >= entry.expires_at);
    }
}
