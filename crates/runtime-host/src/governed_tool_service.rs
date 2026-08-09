//! Owner-scoped governed tool service.
//!
//! Step 6 moves owner/gateway lifetime into one shared allocation while the
//! read/write/process coordinators still live behind the compatibility free
//! functions. Steps 7-9 migrate that state; Step 10 removes the wrappers and
//! introduces the atomic all-tools lease.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_agent_protocol::RunId;
use keeper_policy::{ExecDecision, WriteDecision};
use keeper_storage_sqlite::{StoredExecProposal, StoredWriteProposal};
use keeper_tool_executor::{
    GovernedProcessToolGateway, GovernedReadToolGateway, GovernedWriteToolGateway,
};
use uuid::Uuid;

use crate::governed_read::{
    GovernedReadBridgeError, bind_run, open_gateway, release_owner, release_run, run_is_owned_by,
};
use crate::governed_write::{
    self as governed_write, GovernedWriteBridgeError, LiveWriteProposalView, WriteDiffView,
    bind_write_run, open_write_gateway, release_write_owner, release_write_run,
    write_run_is_owned_by,
};
use crate::host::{
    GovernedProcessBridgeError, LiveProcessProposalView, ProcessOutputView, bind_process_run,
    open_process_gateway, process_run_is_owned_by, release_process_owner, release_process_run,
};

/// Host-owned governance allocation shared with an adapter through an erased
/// [`keeper_governance_port::GovernedExecutionPort`] `Arc`.
///
/// The service never owns an adapter, so `RuntimeHost -> adapter -> service`
/// plus `RuntimeHost -> service` is acyclic. Its owner identity and gateways
/// are instance-scoped even while coordinator state remains in the transitional
/// global compatibility layer.
pub struct GovernedToolService {
    owner_id: Uuid,
    read_gateway: Option<Arc<GovernedReadToolGateway>>,
    write_gateway: Option<Arc<GovernedWriteToolGateway>>,
    process_gateway: Option<Arc<GovernedProcessToolGateway>>,
    /// Retained construction identity; recovery uses gateway handles only.
    #[allow(dead_code)]
    database_path: Option<PathBuf>,
    shut_down: AtomicBool,
    /// Linearizes split transitional bind/release with owner-wide shutdown.
    lifecycle: Mutex<()>,
}

impl std::fmt::Debug for GovernedToolService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GovernedToolService")
            .field("owner_id", &self.owner_id)
            .field("has_read_gateway", &self.read_gateway.is_some())
            .field("has_write_gateway", &self.write_gateway.is_some())
            .field("has_process_gateway", &self.process_gateway.is_some())
            .field("shut_down", &self.is_shut_down())
            .finish()
    }
}

impl GovernedToolService {
    /// Open all governed tool gateways for one owner-scoped service.
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, GovernedReadBridgeError> {
        let path = database_path.as_ref();
        let read = open_gateway(path)?;
        let write = open_write_gateway(path).map_err(|_| GovernedReadBridgeError::Gateway)?;
        let process = open_process_gateway(path).map_err(|_| GovernedReadBridgeError::Gateway)?;
        Ok(Self::build(
            Some(read),
            Some(write),
            Some(process),
            Some(path.to_path_buf()),
        ))
    }

    pub(crate) fn without_gateways() -> Self {
        Self::build(None, None, None, None)
    }

    fn build(
        read_gateway: Option<Arc<GovernedReadToolGateway>>,
        write_gateway: Option<Arc<GovernedWriteToolGateway>>,
        process_gateway: Option<Arc<GovernedProcessToolGateway>>,
        database_path: Option<PathBuf>,
    ) -> Self {
        Self {
            owner_id: Uuid::new_v4(),
            read_gateway,
            write_gateway,
            process_gateway,
            database_path,
            shut_down: AtomicBool::new(false),
            lifecycle: Mutex::new(()),
        }
    }

    /// Whether this service has been closed by prepared-host failure, explicit
    /// host shutdown, or host drop.
    pub fn is_shut_down(&self) -> bool {
        self.shut_down.load(Ordering::SeqCst)
    }

    pub(crate) fn owns_read_run(&self, run_id: RunId) -> bool {
        run_is_owned_by(self.owner_id, run_id)
    }

    pub(crate) fn owns_write_run(&self, run_id: RunId) -> bool {
        write_run_is_owned_by(self.owner_id, run_id)
    }

    pub(crate) fn owns_process_run(&self, run_id: RunId) -> bool {
        process_run_is_owned_by(self.owner_id, run_id)
    }

    pub(crate) fn register_governed_read_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedReadBridgeError> {
        let _lifecycle = self.lifecycle_guard();
        if self.is_shut_down() {
            return Err(GovernedReadBridgeError::Gateway);
        }
        let Some(gateway) = self.read_gateway.as_ref().cloned() else {
            return Ok(());
        };
        let project_id = project_id.into();
        let conversation_id = conversation_id.into();
        bind_run(
            self.owner_id,
            gateway,
            run_id,
            project_id.clone(),
            conversation_id.clone(),
            project_root.as_ref(),
        )?;
        if let Some(write_gateway) = self.write_gateway.as_ref().cloned() {
            bind_write_run(
                self.owner_id,
                write_gateway,
                run_id,
                project_id,
                conversation_id,
                project_root,
            )
            .map_err(|_| GovernedReadBridgeError::InvalidContext)?;
        }
        Ok(())
    }

    pub(crate) fn register_governed_write_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedWriteBridgeError> {
        let _lifecycle = self.lifecycle_guard();
        if self.is_shut_down() {
            return Err(GovernedWriteBridgeError::ShutDown);
        }
        let Some(gateway) = self.write_gateway.as_ref().cloned() else {
            return Ok(());
        };
        bind_write_run(
            self.owner_id,
            gateway,
            run_id,
            project_id,
            conversation_id,
            project_root,
        )
    }

    pub(crate) fn register_governed_process_run(
        &self,
        run_id: RunId,
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        project_root: impl AsRef<Path>,
    ) -> Result<(), GovernedProcessBridgeError> {
        let _lifecycle = self.lifecycle_guard();
        if self.is_shut_down() {
            return Err(GovernedProcessBridgeError::ShutDown);
        }
        let Some(gateway) = self.process_gateway.as_ref().cloned() else {
            return Ok(());
        };
        bind_process_run(
            self.owner_id,
            gateway,
            run_id,
            project_id,
            conversation_id,
            project_root,
        )
    }

    pub(crate) fn release_governed_run(&self, run_id: RunId) {
        let _lifecycle = self.lifecycle_guard();
        release_run(self.owner_id, run_id);
        release_write_run(self.owner_id, run_id);
        release_process_run(self.owner_id, run_id);
    }

    pub(crate) fn release_governed_write_run(&self, run_id: RunId) {
        let _lifecycle = self.lifecycle_guard();
        release_write_run(self.owner_id, run_id);
    }

    pub(crate) fn release_governed_process_run(&self, run_id: RunId) {
        let _lifecycle = self.lifecycle_guard();
        release_process_run(self.owner_id, run_id);
    }

    pub(crate) fn write_gateway(&self) -> Option<&GovernedWriteToolGateway> {
        self.write_gateway.as_deref()
    }

    pub(crate) fn process_gateway(&self) -> Option<&GovernedProcessToolGateway> {
        self.process_gateway.as_deref()
    }

    pub(crate) fn list_process_proposals(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<LiveProcessProposalView>, GovernedProcessBridgeError> {
        crate::host::list_process_proposals(
            self.owner_id,
            conversation_id,
            self.process_gateway.as_deref(),
        )
    }

    pub(crate) fn decide_process_proposal(
        &self,
        conversation_id: &str,
        proposal_id: &str,
        decision: ExecDecision,
    ) -> Result<LiveProcessProposalView, GovernedProcessBridgeError> {
        crate::host::decide_process_proposal(self.owner_id, conversation_id, proposal_id, decision)
    }

    pub(crate) fn open_process_output(
        &self,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<ProcessOutputView, GovernedProcessBridgeError> {
        crate::host::open_process_output(
            self.owner_id,
            conversation_id,
            proposal_id,
            self.process_gateway.as_deref(),
        )
    }

    pub(crate) fn recover_process_proposals(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<StoredExecProposal>, GovernedProcessBridgeError> {
        let Some(gateway) = self.process_gateway.as_ref() else {
            return Ok(Vec::new());
        };
        crate::host::recover_process_proposals(gateway.as_ref(), now)
    }

    pub(crate) fn live_process_authority_count(&self) -> usize {
        crate::host::live_process_authority_count(self.owner_id)
    }

    pub(crate) fn pending_process_run_count(&self) -> usize {
        crate::host::pending_process_run_count(self.owner_id)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn force_live_process_expiry_for_tests(&self, proposal_id: &str) -> bool {
        crate::host::force_live_process_expiry_for_tests(self.owner_id, proposal_id)
    }

    pub(crate) fn list_write_proposals(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<LiveWriteProposalView>, GovernedWriteBridgeError> {
        governed_write::list_write_proposals(
            self.owner_id,
            conversation_id,
            self.write_gateway.as_deref(),
        )
    }

    pub(crate) fn decide_write_proposal(
        &self,
        conversation_id: &str,
        proposal_id: &str,
        approval_grant: &str,
        decision: WriteDecision,
    ) -> Result<LiveWriteProposalView, GovernedWriteBridgeError> {
        governed_write::decide_write_proposal(
            self.owner_id,
            conversation_id,
            proposal_id,
            approval_grant,
            decision,
        )
    }

    pub(crate) fn open_write_proposal_diff(
        &self,
        conversation_id: &str,
        proposal_id: &str,
    ) -> Result<WriteDiffView, GovernedWriteBridgeError> {
        governed_write::open_write_proposal_diff(self.owner_id, conversation_id, proposal_id)
    }

    pub(crate) fn recover_write_proposals(
        &self,
        now: DateTime<Utc>,
        resolve_root: impl FnMut(&str) -> Option<PathBuf>,
    ) -> Result<Vec<StoredWriteProposal>, GovernedWriteBridgeError> {
        let Some(gateway) = self.write_gateway.as_ref() else {
            return Ok(Vec::new());
        };
        governed_write::recover_write_proposals_with_roots(gateway, now, resolve_root)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn force_live_expiry_for_tests(&self, proposal_id: &str) -> bool {
        governed_write::force_live_expiry_for_tests(self.owner_id, proposal_id)
    }

    /// Close the execution view and idempotently release every transitional
    /// binding owned by this service. RuntimeHost invokes this only after
    /// adapter shutdown, worker join, and residual cleanup checks.
    pub fn shutdown(&self) {
        let _lifecycle = self.lifecycle_guard();
        self.shut_down.store(true, Ordering::SeqCst);
        release_owner(self.owner_id);
        release_write_owner(self.owner_id);
        release_process_owner(self.owner_id);
    }

    fn lifecycle_guard(&self) -> MutexGuard<'_, ()> {
        self.lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for GovernedToolService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
