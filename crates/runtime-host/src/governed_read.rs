//! Run-scoped composition bridge from normalized provider proposals to B3.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

// Contract types live in `keeper-agent-protocol`; re-export for backward compatibility.
pub use keeper_agent_protocol::{GovernedReadDelivery, GovernedReadReceipt, ProviderToolProposal};

use keeper_agent_protocol::{RunId, ToolActiveContext, ToolProposal, canonical_arguments_digest};
use keeper_governance_port::CancellationProbe;
use keeper_tool_executor::{
    CancellationSignal, GovernedReadToolGateway, GovernedToolError, GovernedToolOutcome,
    ProjectCapability,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum GovernedReadBridgeError {
    #[error("governed read run is not registered")]
    RunNotRegistered,
    #[error("provider tool proposal is invalid")]
    InvalidProposal,
    #[error("provider tool call conflicts with its durable reservation")]
    ReservationConflict,
    #[error("provider tool call is already executing")]
    InProgress,
    #[error("governed read context is invalid")]
    InvalidContext,
    #[error("governed read gateway is unavailable")]
    Gateway,
}

struct BoundRun {
    owner: Uuid,
    context: ToolActiveContext,
    capability: ProjectCapability,
    gateway: Arc<GovernedReadToolGateway>,
}

fn active_runs() -> &'static Mutex<HashMap<RunId, Arc<BoundRun>>> {
    static ACTIVE_RUNS: OnceLock<Mutex<HashMap<RunId, Arc<BoundRun>>>> = OnceLock::new();
    ACTIVE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn open_gateway(
    path: impl AsRef<Path>,
) -> Result<Arc<GovernedReadToolGateway>, GovernedReadBridgeError> {
    GovernedReadToolGateway::open(path)
        .map(Arc::new)
        .map_err(|_| GovernedReadBridgeError::Gateway)
}

pub(crate) fn bind_run(
    owner: Uuid,
    gateway: Arc<GovernedReadToolGateway>,
    run_id: RunId,
    project_id: impl Into<String>,
    conversation_id: impl Into<String>,
    project_root: impl AsRef<Path>,
) -> Result<(), GovernedReadBridgeError> {
    let project_id = project_id.into();
    let context = ToolActiveContext::new(project_id.clone(), conversation_id, run_id)
        .map_err(|_| GovernedReadBridgeError::InvalidContext)?;
    let capability = ProjectCapability::new(project_id, project_root)
        .map_err(|_| GovernedReadBridgeError::InvalidContext)?;
    let mut runs = active_runs()
        .lock()
        .map_err(|_| GovernedReadBridgeError::Gateway)?;
    if runs.contains_key(&run_id) {
        return Err(GovernedReadBridgeError::InvalidContext);
    }
    runs.insert(
        run_id,
        Arc::new(BoundRun {
            owner,
            context,
            capability,
            gateway,
        }),
    );
    Ok(())
}

pub(crate) fn release_run(owner: Uuid, run_id: RunId) {
    let Ok(mut runs) = active_runs().lock() else {
        return;
    };
    if runs.get(&run_id).is_some_and(|bound| bound.owner == owner) {
        runs.remove(&run_id);
    }
}

pub(crate) fn release_owner(owner: Uuid) {
    let Ok(mut runs) = active_runs().lock() else {
        return;
    };
    runs.retain(|_, bound| bound.owner != owner);
}

pub(crate) fn run_is_owned_by(owner: Uuid, run_id: RunId) -> bool {
    active_runs()
        .lock()
        .map(|runs| runs.get(&run_id).is_some_and(|bound| bound.owner == owner))
        .unwrap_or(false)
}

/// Execute one adapter-normalized fixed-manifest proposal for an active run.
///
/// The adapter supplies no project identity or filesystem authority. Both come
/// from the host registration created by the owning conversation service.
///
/// `cancellation` is level-triggered: the host re-probes on every cancel check
/// (including tool-executor checkpoints). Snapshotting once at entry is
/// fail-open and forbidden.
pub fn execute_governed_read(
    run_id: RunId,
    provider: ProviderToolProposal,
    cancellation: &dyn CancellationProbe,
) -> Result<GovernedReadReceipt, GovernedReadBridgeError> {
    let bound = active_runs()
        .lock()
        .map_err(|_| GovernedReadBridgeError::Gateway)?
        .get(&run_id)
        .cloned()
        .ok_or(GovernedReadBridgeError::RunNotRegistered)?;
    let arguments_digest = canonical_arguments_digest(&provider.arguments)
        .map_err(|_| GovernedReadBridgeError::InvalidProposal)?;
    let proposal = ToolProposal {
        project_id: bound.context.project_id().into(),
        conversation_id: bound.context.conversation_id().into(),
        run_id,
        call_id: provider.call_id,
        tool_id: provider.tool_id,
        tool_version: provider.tool_version,
        arguments: provider.arguments,
        arguments_digest,
    }
    .validate_for_context(&bound.context)
    .map_err(|_| GovernedReadBridgeError::InvalidProposal)?;
    // Live probe adapter: each CancellationSignal check re-reads the probe.
    let cancellation = LiveProbeCancellation(cancellation);

    match bound
        .gateway
        .execute(&proposal, &bound.capability, &cancellation)
    {
        Ok(GovernedToolOutcome::Executed(result)) => Ok(GovernedReadReceipt {
            result,
            delivery: GovernedReadDelivery::Executed,
        }),
        Ok(GovernedToolOutcome::Replayed(result)) => Ok(GovernedReadReceipt {
            result,
            delivery: GovernedReadDelivery::Replayed,
        }),
        Ok(GovernedToolOutcome::InProgress) => Err(GovernedReadBridgeError::InProgress),
        Err(GovernedToolError::ReservationConflict) => {
            Err(GovernedReadBridgeError::ReservationConflict)
        }
        Err(
            GovernedToolError::Executor(_)
            | GovernedToolError::Store(_)
            | GovernedToolError::InvalidReceipt,
        ) => Err(GovernedReadBridgeError::Gateway),
    }
}

/// Forwards tool-executor cancel checks to a live [`CancellationProbe`].
///
/// Must not cache the first reading — every `is_cancelled` re-probes.
struct LiveProbeCancellation<'a>(&'a dyn CancellationProbe);

impl CancellationSignal for LiveProbeCancellation<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}
