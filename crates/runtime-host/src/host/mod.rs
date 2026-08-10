//! Bounded runtime host: concurrency limits, protocol guard, worker lifecycle.
//!
//! Spawn registration, cancel, and join are linearized through the host worker
//! map and each [`WorkerRecord`]'s phase machine (`Starting -> Running ->
//! Joining -> Joined`). Async turn slots are RAII [`TurnSlotPermit`]s owned by
//! the worker record — no unregistered in-flight window and no manual
//! double/missed release on the async path.
//!
//! Synchronous `send_turn` shares the same concurrency counter but is not an
//! OS worker record. Admission is linearized with shutdown via
//! [`SyncTurnLifecycle`]: the gate is closed before workers are cancelled, and
//! shutdown waits on a condition variable until every admitted sync turn
//! releases its permit.

mod config;
mod prepared;
mod process;
mod spawned;

#[cfg(test)]
mod registration_race_tests;

pub use config::{
    DEFAULT_MAX_CONCURRENT_TURNS, HostConfig, normalize_max_concurrent_turns,
    workspace_fixture_path,
};
pub use prepared::{GovernedAdapterFactory, PreparedRuntimeHost};
pub use process::{
    GovernedProcessBridgeError, GovernedProcessDelivery, GovernedProcessLifecycle,
    GovernedProcessReceipt, GovernedShellOutput, GovernedShellReceipt, LiveProcessProposalView,
    ProcessOrigin, ProcessOutputChunk, ProcessOutputStream, ProcessOutputView,
    ProcessProposalRequestedView, ProcessRiskClass, ProviderProcessProposal, ProviderShellProposal,
    decide_process_proposal, execute_governed_process, execute_governed_shell,
    list_process_proposals, live_process_authority_count, open_process_output,
    pending_process_run_count, recover_process_proposals,
};
pub(crate) use process::{
    bind_process_run, open_process_gateway, process_run_is_owned_by, release_process_owner,
    release_process_run,
};
#[cfg(any(test, feature = "test-support"))]
pub use process::{
    clear_pre_claim_hook_for_tests, force_live_process_expiry_for_tests,
    set_pre_claim_hook_for_tests,
};
pub use spawned::SpawnedTurn;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::GovernedToolService;
use crate::fake::FakeRuntimeAdapter;
use crate::governed_read::GovernedReadBridgeError;
use crate::governed_write::{GovernedWriteBridgeError, LiveWriteProposalView, WriteDiffView};
use crate::sink::ProtocolGuardSink;
use crate::worker::{SyncTurnLifecycle, TurnSlotPermit, WorkerRecord, run_turn_worker};
use chrono::{DateTime, Utc};
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeProbe, RuntimeSession, SendTurn,
    StartSession,
};
use keeper_policy::{ExecDecision, WriteDecision};
use keeper_storage_sqlite::{StoredExecProposal, StoredWriteProposal};
use keeper_tool_executor::{GovernedProcessToolGateway, GovernedWriteToolGateway};

/// Host that bounds concurrency and enforces the provider-neutral protocol.
pub struct RuntimeHost {
    adapter: Arc<dyn AgentRuntimeAdapter>,
    config: HostConfig,
    /// Shared turn-slot counter (async permits + sync `send_turn` guards).
    slots: Arc<AtomicU32>,
    shut_down: AtomicBool,
    workers: Mutex<HashMap<RunId, Arc<WorkerRecord>>>,
    /// Sync `send_turn` admission linearized with shutdown (mutex + condvar).
    sync_turns: SyncTurnLifecycle,
    governed_tools: Arc<GovernedToolService>,
}

impl RuntimeHost {
    pub fn new(adapter: Arc<dyn AgentRuntimeAdapter>, config: HostConfig) -> Self {
        Self::build(
            adapter,
            config,
            Arc::new(GovernedToolService::without_gateways()),
        )
    }

    pub fn new_with_governed_reads(
        adapter: Arc<dyn AgentRuntimeAdapter>,
        config: HostConfig,
        database_path: impl AsRef<Path>,
    ) -> Result<Self, GovernedReadBridgeError> {
        PreparedRuntimeHost::open(database_path, config).map(|prepared| prepared.finish(adapter))
    }

    pub fn new_with_governed_tools(
        adapter: Arc<dyn AgentRuntimeAdapter>,
        config: HostConfig,
        database_path: impl AsRef<Path>,
    ) -> Result<Self, GovernedWriteBridgeError> {
        PreparedRuntimeHost::open(database_path, config)
            .map_err(|_| GovernedWriteBridgeError::Gateway)
            .map(|prepared| prepared.finish(adapter))
    }

    pub(super) fn build(
        adapter: Arc<dyn AgentRuntimeAdapter>,
        config: HostConfig,
        governed_tools: Arc<GovernedToolService>,
    ) -> Self {
        #[cfg_attr(not(any(test, feature = "test-support")), allow(unused_mut))]
        let mut normalized = HostConfig::new(
            normalize_max_concurrent_turns(config.max_concurrent_turns),
            config.honor_fixture_delays,
            config.event_queue_capacity.max(1),
        );
        #[cfg(any(test, feature = "test-support"))]
        {
            normalized.after_worker_registered = config.after_worker_registered;
        }
        Self {
            adapter,
            config: normalized,
            slots: Arc::new(AtomicU32::new(0)),
            shut_down: AtomicBool::new(false),
            workers: Mutex::new(HashMap::new()),
            sync_turns: SyncTurnLifecycle::new(),
            governed_tools,
        }
    }

    pub fn with_fake(config: HostConfig) -> Result<Self, RuntimeError> {
        let adapter = FakeRuntimeAdapter::from_committed_fixture(config.honor_fixture_delays)?;
        Ok(Self::new(Arc::new(adapter), config))
    }

    pub fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.ensure_alive()?;
        self.adapter.probe()
    }

    pub fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.ensure_alive()?;
        self.adapter.start_session(request)
    }

    pub fn register_governed_read_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedReadBridgeError> {
        self.governed_tools.register_governed_read_run(
            run_id,
            project_id,
            conversation_id,
            project_root,
        )
    }

    pub fn register_governed_write_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedWriteBridgeError> {
        self.governed_tools.register_governed_write_run(
            run_id,
            project_id,
            conversation_id,
            project_root,
        )
    }

    pub fn register_governed_process_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedProcessBridgeError> {
        self.governed_tools.register_governed_process_run(
            run_id,
            project_id,
            conversation_id,
            project_root,
        )
    }

    pub fn release_governed_read_run(&self, run_id: RunId) {
        self.governed_tools.release_governed_run(run_id);
    }

    pub fn release_governed_write_run(&self, run_id: RunId) {
        self.governed_tools.release_governed_write_run(run_id);
    }

    pub fn release_governed_process_run(&self, run_id: RunId) {
        self.governed_tools.release_governed_process_run(run_id);
    }

    pub fn write_gateway(&self) -> Option<&GovernedWriteToolGateway> {
        self.governed_tools.write_gateway()
    }

    pub fn process_gateway(&self) -> Option<&GovernedProcessToolGateway> {
        self.governed_tools.process_gateway()
    }

    /// Concrete control-side view of the owner-scoped service shared with the
    /// adapter's erased execution port.
    pub fn governed_tool_service(&self) -> &GovernedToolService {
        &self.governed_tools
    }

    /// Owner-scoped process proposal list for this host instance.
    pub fn list_process_proposals(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<LiveProcessProposalView>, GovernedProcessBridgeError> {
        self.governed_tools.list_process_proposals(conversation_id)
    }

    /// Owner-scoped one-shot process decision for a live proposal on this host.
    ///
    /// Accepts approve-once / reject only. Does not accept or return a raw grant;
    /// the host supplies its retained digest to the gateway internally.
    pub fn decide_process_proposal(
        &self,
        conversation_id: &str,
        proposal_id: &str,
        decision: ExecDecision,
    ) -> Result<LiveProcessProposalView, GovernedProcessBridgeError> {
        self.governed_tools
            .decide_process_proposal(conversation_id, proposal_id, decision)
    }

    /// Owner-scoped live process output open.
    pub fn open_process_output(
        &self,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<ProcessOutputView, GovernedProcessBridgeError> {
        self.governed_tools
            .open_process_output(conversation_id, proposal_id)
    }

    /// Restart recovery for process proposals (non-terminal → interrupted).
    ///
    /// Goes only through [`GovernedProcessToolGateway`]; never opens a raw
    /// repository or database-path side channel.
    pub fn recover_process_proposals(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<StoredExecProposal>, GovernedProcessBridgeError> {
        self.governed_tools.recover_process_proposals(now)
    }

    /// Residual non-terminal process authority for this host (tests / invariants).
    pub fn live_process_authority_count(&self) -> usize {
        self.governed_tools.live_process_authority_count()
    }

    /// Residual pending-by-run process slots for this host (tests / invariants).
    pub fn pending_process_run_count(&self) -> usize {
        self.governed_tools.pending_process_run_count()
    }

    /// Test seam: force live process expiry for integration tests.
    /// Absent from ordinary product builds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn force_live_process_expiry_for_tests(&self, proposal_id: &str) -> bool {
        self.governed_tools
            .force_live_process_expiry_for_tests(proposal_id)
    }

    /// Owner-scoped list: only proposals for this host instance.
    ///
    /// Durable rows come from this host's own write gateway (works with no
    /// bound runs after restart). Live grants remain owner-scoped.
    pub fn list_write_proposals(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<LiveWriteProposalView>, GovernedWriteBridgeError> {
        self.governed_tools.list_write_proposals(conversation_id)
    }

    /// Owner-scoped one-shot decision for a live proposal on this host.
    pub fn decide_write_proposal(
        &self,
        conversation_id: &str,
        proposal_id: &str,
        approval_grant: &str,
        decision: WriteDecision,
    ) -> Result<LiveWriteProposalView, GovernedWriteBridgeError> {
        self.governed_tools.decide_write_proposal(
            conversation_id,
            proposal_id,
            approval_grant,
            decision,
        )
    }

    /// Owner-scoped live diff open.
    pub fn open_write_proposal_diff(
        &self,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<WriteDiffView, GovernedWriteBridgeError> {
        self.governed_tools
            .open_write_proposal_diff(conversation_id, proposal_id)
    }

    /// Restart recovery for this host's write gateway (interrupted + applying).
    pub fn recover_write_proposals(
        &self,
        now: DateTime<Utc>,
        resolve_root: impl FnMut(&str) -> Option<std::path::PathBuf>,
    ) -> Result<Vec<StoredWriteProposal>, GovernedWriteBridgeError> {
        self.governed_tools
            .recover_write_proposals(now, resolve_root)
    }

    /// Test-only expiry injection (requires `test-support` or unit-test cfg).
    #[cfg(any(test, feature = "test-support"))]
    pub fn force_live_expiry_for_tests(&self, proposal_id: &str) -> bool {
        self.governed_tools.force_live_expiry_for_tests(proposal_id)
    }

    pub fn send_turn(
        &self,
        request: SendTurn,
        sink: &mut dyn EventSink,
    ) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        // Lifecycle admission first: linearizes with shutdown close/wait so an
        // admitted sync turn is never invisible to join. Drop order is reverse
        // of declaration — permit releases before admission, so wait_until_idle
        // implies this turn's concurrency slot is already free.
        let _admission = self.sync_turns.try_admit()?;
        let _permit = TurnSlotPermit::try_acquire(&self.slots, self.config.max_concurrent_turns)?;
        let run_id = request.run_id;
        let mut guarded = ProtocolGuardSink::new(run_id, sink);
        self.adapter.send_turn(request, &mut guarded)?;
        guarded.finish()?;
        Ok(())
    }

    /// Run a turn on a background thread; cancellable as soon as this returns.
    ///
    /// Shutdown check, turn-slot permit acquisition, and insertion of the
    /// `Starting` worker record (with the permit moved into the record) are one
    /// critical section under the workers map. Shutdown either waits for that
    /// record or the spawn is rejected before any slot is held.
    pub fn spawn_turn(self: &Arc<Self>, request: SendTurn) -> Result<SpawnedTurn, RuntimeError> {
        self.ensure_alive()?;

        let run_id = request.run_id;

        // Linearize with shutdown: re-check shut_down, reject duplicate run_id
        // (never replace a live record), acquire the permit, and publish
        // Starting with permit ownership under the same workers lock.
        let record = {
            let mut workers = self.workers.lock().expect("workers");
            if self.shut_down.load(Ordering::SeqCst) {
                return Err(RuntimeError::ShutDown);
            }
            if workers.contains_key(&run_id) {
                return Err(RuntimeError::RunAlreadyActive { run_id });
            }
            let permit =
                TurnSlotPermit::try_acquire(&self.slots, self.config.max_concurrent_turns)?;
            let record = WorkerRecord::new(self.config.event_queue_capacity, permit);
            workers.insert(run_id, Arc::clone(&record));
            record
        };

        #[cfg(any(test, feature = "test-support"))]
        if let Some(hook) = self.config.after_worker_registered_hook() {
            hook();
        }

        // arm_turn may block; shutdown observes Starting and waits.
        if let Err(e) = self.adapter.arm_turn(run_id) {
            self.fail_spawn_cleanup(run_id, &record);
            return Err(e);
        }

        // Shutdown may have begun during arm; reject without residual work.
        if self.shut_down.load(Ordering::SeqCst) {
            let _ = self.adapter.disarm_turn(run_id);
            self.fail_spawn_cleanup(run_id, &record);
            return Err(RuntimeError::ShutDown);
        }

        let host = Arc::clone(self);
        let delivery_for_thread = Arc::clone(&record.delivery);
        let cancel_flag = Arc::clone(&record.cancel);
        let record_for_thread = Arc::clone(&record);

        let join = thread::Builder::new()
            .name(format!("keeper-turn-{run_id}"))
            .spawn(move || {
                let result = run_turn_worker(
                    &host.adapter,
                    request,
                    run_id,
                    &cancel_flag,
                    &delivery_for_thread,
                );
                delivery_for_thread.close();
                // Free concurrency when the body finishes; await_joined also
                // takes the permit (idempotent) before publishing Joined.
                record_for_thread.release_slot_permit();
                record_for_thread.mark_body_finished();
                result
            });

        let join = match join {
            Ok(handle) => handle,
            Err(e) => {
                let _ = self.adapter.disarm_turn(run_id);
                self.fail_spawn_cleanup(run_id, &record);
                return Err(RuntimeError::Internal {
                    message: format!("spawn turn worker: {e}"),
                });
            }
        };

        record.install_running(join);

        // Late shutdown: worker is Running and will be joined by shutdown.
        if self.shut_down.load(Ordering::SeqCst) {
            // Still return the handle so the caller can observe cancel; the
            // worker is fully owned. Callers that race shutdown may also see
            // ShutDown only on the pre-register / pre-thread paths above.
        }

        Ok(SpawnedTurn {
            run_id,
            host: Arc::clone(self),
            record,
        })
    }

    pub fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        let known = {
            let workers = self.workers.lock().expect("workers");
            if let Some(w) = workers.get(&run_id) {
                // Atomic with terminal commit: delivery cancel + host flag.
                w.request_cancel();
                true
            } else {
                false
            }
        };
        match self.adapter.cancel_turn(run_id) {
            Ok(()) => Ok(()),
            // Map RunNotFound → Ok only for host-owned workers, where the host
            // cancel flag already makes cancel real. Never invent success for
            // an unknown run.
            Err(RuntimeError::RunNotFound { .. }) if known => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Shut down the adapter and join all host-owned workers before returning.
    ///
    /// Host workers in `Starting`, `Running`, or `Joining`, and every admitted
    /// synchronous `send_turn`, are waited on even when `adapter.shutdown()`
    /// returns an error. The adapter error is returned only after host cleanup
    /// is complete.
    ///
    /// Deadlock-safe order:
    /// 1. Close the sync-turn gate (no new admissions; existing holds remain).
    /// 2. Publish `shut_down` under the workers lock and cancel async workers.
    /// 3. Wait (condvar) until every admitted sync turn releases — **before**
    ///    adapter resource teardown, so an admitted sync turn cannot observe
    ///    cleared sessions (`SessionNotFound`) as a normal shutdown race.
    /// 4. `adapter.shutdown()` (error retained).
    /// 5. Join every async worker record.
    /// 6. Empty registry must imply zero turn slots (even on adapter error).
    pub fn shutdown(&self) -> Result<(), RuntimeError> {
        // Close first so try_admit cannot race past AtomicBool shut_down alone.
        self.sync_turns.close();

        {
            let workers = self.workers.lock().expect("workers");
            let _already = self.shut_down.swap(true, Ordering::SeqCst);
            for (run_id, worker) in workers.iter() {
                worker.request_cancel();
                let _ = self.adapter.cancel_turn(*run_id);
                worker.delivery.close();
            }
        }

        // Admitted sync turns must finish while adapter sessions are still live.
        self.sync_turns.wait_until_idle();

        let adapter_result = self.adapter.shutdown();
        // Join host workers even when the adapter reports failure.
        self.join_all_workers();

        // Always require zero residuals after join, even if the adapter erred.
        // Prefer the adapter error when both fail; residual is still evaluated
        // so cleanup bookkeeping is verified either way.
        let residual = self.ensure_registry_empty_implies_no_slots();
        self.governed_tools.shutdown();
        adapter_result?;
        residual?;
        Ok(())
    }

    pub fn in_flight_turns(&self) -> u32 {
        self.slots.load(Ordering::SeqCst)
    }

    /// True when the worker registry is empty, no sync admissions remain, and
    /// no turn-slot permits remain.
    ///
    /// After a successful shutdown this must hold. It also documents the
    /// invariant that an empty async worker map cannot leave a leaked permit.
    pub fn is_fully_idle(&self) -> bool {
        let workers = self.workers.lock().expect("workers");
        workers.is_empty()
            && self.sync_turns.in_flight() == 0
            && self.slots.load(Ordering::SeqCst) == 0
    }

    /// Admitted synchronous turns still holding the lifecycle gate (tests).
    pub fn sync_turns_in_flight(&self) -> u32 {
        self.sync_turns.in_flight()
    }

    /// Number of worker records still registered (test / invariant helper).
    pub fn worker_registry_len(&self) -> usize {
        self.workers.lock().expect("workers").len()
    }

    /// Count of registered workers that still own their async slot permit.
    pub fn workers_holding_slot_permits(&self) -> usize {
        let workers = self.workers.lock().expect("workers");
        workers.values().filter(|w| w.has_slot_permit()).count()
    }

    pub fn event_queue_capacity(&self) -> usize {
        self.config.event_queue_capacity
    }

    /// Wait until every registered worker is truly `Joined`.
    fn join_all_workers(&self) {
        loop {
            let batch: Vec<(RunId, Arc<WorkerRecord>)> = {
                let workers = self.workers.lock().expect("workers");
                if workers.is_empty() {
                    return;
                }
                workers.iter().map(|(id, w)| (*id, Arc::clone(w))).collect()
            };

            for (_id, worker) in &batch {
                let mut discarded = Vec::new();
                let _ = worker.await_joined(&mut discarded);
            }

            let mut workers = self.workers.lock().expect("workers");
            for (id, _) in batch {
                workers.remove(&id);
            }
            if workers.is_empty() {
                return;
            }
            // A spawn may have inserted during join; loop until the map drains.
        }
    }

    /// Abandon a Starting record: permit is released inside `abandon_starting`
    /// before `Joined` is published, then the map entry is removed **only if it
    /// still points at this exact record** (never detach another owner).
    fn fail_spawn_cleanup(&self, run_id: RunId, record: &Arc<WorkerRecord>) {
        record.request_cancel();
        record.delivery.close();
        record.abandon_starting();
        let mut workers = self.workers.lock().expect("workers");
        match workers.get(&run_id) {
            Some(current) if Arc::ptr_eq(current, record) => {
                workers.remove(&run_id);
            }
            _ => {
                // Already removed, or a different record owns this run_id —
                // do not remove foreign ownership.
            }
        }
    }

    fn ensure_alive(&self) -> Result<(), RuntimeError> {
        if self.shut_down.load(Ordering::SeqCst) {
            Err(RuntimeError::ShutDown)
        } else {
            Ok(())
        }
    }

    /// Cancel a host-owned worker without requiring the host to still be alive.
    ///
    /// Used by [`SpawnedTurn`] Drop so a blocked adapter is unblocked even when
    /// `shut_down` is already set. Tolerates `RunNotFound` only because the
    /// caller holds the host worker record.
    pub(super) fn cancel_owned_worker_best_effort(&self, run_id: RunId, record: &WorkerRecord) {
        record.request_cancel();
        match self.adapter.cancel_turn(run_id) {
            Ok(()) => {}
            Err(RuntimeError::RunNotFound { .. }) => {}
            Err(_) => {}
        }
    }

    /// Remove `run_id` from the registry only when it still points at `record`.
    pub(super) fn remove_worker_if_owner(&self, run_id: RunId, record: &Arc<WorkerRecord>) {
        let mut workers = self.workers.lock().expect("workers");
        match workers.get(&run_id) {
            Some(current) if Arc::ptr_eq(current, record) => {
                workers.remove(&run_id);
            }
            _ => {}
        }
    }

    /// Post-shutdown invariant: empty worker registry + idle sync gate ⇒ no
    /// turn-slot permits.
    ///
    /// Async permits live on worker records and are dropped before `Joined` is
    /// published. Sync admissions are closed and drained before this check, so
    /// a non-zero counter is always a bookkeeping leak.
    fn ensure_registry_empty_implies_no_slots(&self) -> Result<(), RuntimeError> {
        let workers = self.workers.lock().expect("workers");
        if !workers.is_empty() {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {} worker record(s) still registered",
                    workers.len()
                ),
            });
        }
        let sync = self.sync_turns.in_flight();
        if sync != 0 {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {sync} admitted synchronous turn(s) still in flight"
                ),
            });
        }
        let slots = self.slots.load(Ordering::SeqCst);
        if slots != 0 {
            return Err(RuntimeError::Internal {
                message: format!(
                    "shutdown completed with {slots} in-flight turn slot(s) and empty worker registry"
                ),
            });
        }
        Ok(())
    }
}

impl Drop for RuntimeHost {
    fn drop(&mut self) {
        // Idempotent: close the shared service and release read/write/process
        // authority when explicit shutdown was skipped.
        self.governed_tools.shutdown();
    }
}
