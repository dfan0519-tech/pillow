//! ConversationService: single public owner of repository, host, emitter,
//! active registry, and shutdown lifecycle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, RunId as ProtocolRunId, RuntimeProbe, SendTurn, StartSession,
};
use keeper_project::ProjectId;
use keeper_runtime_host::{
    DispatchError, GovernedAdapterFactory, GovernedProcessLifecycle, HostConfig,
    PreparedRuntimeHost, RuntimeHost, SpawnedTurn, attribute_dispatch_receipt,
    complete_dispatch_on_receipt, execute_prepared_dispatch, list_dispatch_records,
    prepare_dispatch, register_dispatch_on_requested,
};
use keeper_session::{
    ConversationId, ConversationRepository, NewConversation, NewRun, RunId, RunStatus,
};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteProjectRepository, SqliteToolCallRepository,
};
use pillow_role_dispatch::BuildAssignmentInput;

use crate::conversation_events::{
    ConversationDto, RuntimeEventEmitter, SendTurnResult, TimelineDto, timeline_dto,
};

use super::dispatch_service::{
    RoleDispatchProposalDto, RoleDispatchRecordDto, dispatch_proposal_dto,
};
use super::ids::{map_session_error, parse_conversation_id, parse_project_id, parse_run_id};
use super::pipeline::{PumpFailSeams, PumpOutcome, pump_turn};
use super::project_cwd::resolve_working_directory;
use super::reconciliation::reconcile_terminal_tool_receipts;
#[cfg(test)]
use super::types::PumpRegistrationHooks;
use super::types::{
    ActiveMap, ActiveRun, ConversationCommandError, PumpHandleRegistration, ServiceAdmission,
    remove_active_run,
};

pub struct ConversationService {
    repository: Arc<SqliteConversationRepository>,
    tool_calls: Arc<SqliteToolCallRepository>,
    projects: Arc<SqliteProjectRepository>,
    host: Arc<RuntimeHost>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    active: ActiveMap,
    /// Linearizes `send_turn` durable admission with `shutdown`.
    admission: ServiceAdmission,
    shut_down: AtomicBool,
    #[cfg(test)]
    after_spawn_before_register_pump: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    after_pump_before_registration_wait: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    force_process_error: Option<Arc<dyn Fn() -> Result<(), String> + Send + Sync>>,
    #[cfg(test)]
    force_terminalization_failure: bool,
}

impl ConversationService {
    /// Production-safe construction: inject a runtime-neutral adapter.
    ///
    /// Builds a runtime host with the accepted governed-read gateway.
    /// Provider wire types must stay inside the adapter crate and never appear
    /// on this boundary.
    pub fn open_with_adapter(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        adapter: Arc<dyn AgentRuntimeAdapter>,
        config: HostConfig,
    ) -> Result<Self, ConversationCommandError> {
        let path = path.as_ref();
        let repository = SqliteConversationRepository::open(path).map_err(map_session_error)?;
        let tool_calls = open_tool_calls(path)?;
        let projects = open_projects(path)?;
        let host =
            RuntimeHost::new_with_governed_reads(adapter, config, path).map_err(|error| {
                ConversationCommandError::new("governed_read_unavailable", error.to_string())
            })?;
        Ok(Self::from_parts(
            repository, tool_calls, projects, host, emitter,
        ))
    }

    /// Production governed composition: prepare the host-owned tool service,
    /// give its erased execution `Arc` to the adapter factory, then finish the
    /// host with the same concrete service allocation.
    pub fn open_with_adapter_factory<F>(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
        factory: F,
    ) -> Result<Self, ConversationCommandError>
    where
        F: GovernedAdapterFactory<Error = ConversationCommandError>,
    {
        let path = path.as_ref();
        let repository = SqliteConversationRepository::open(path).map_err(map_session_error)?;
        let tool_calls = open_tool_calls(path)?;
        let projects = open_projects(path)?;
        let prepared = PreparedRuntimeHost::open(path, config).map_err(|error| {
            ConversationCommandError::new("governed_read_unavailable", error.to_string())
        })?;
        let host = prepared.finish_with_adapter_factory(factory)?;
        Ok(Self::from_parts(
            repository, tool_calls, projects, host, emitter,
        ))
    }

    /// Deterministic fake runtime for tests and **explicit** development startup.
    ///
    /// Uses [`RuntimeHost::with_fake`]. This is **not** the production Grok
    /// runtime. Production composition must use [`open_with_adapter`] via the
    /// desktop runtime composition layer; fake must never be a silent fallback.
    pub fn open(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        honor_fixture_delays: bool,
    ) -> Result<Self, ConversationCommandError> {
        Self::open_with_config(path, emitter, HostConfig::new(4, honor_fixture_delays, 64))
    }

    /// Fake-runtime construction with a custom host config (tests / cancel barriers).
    ///
    /// Prefer [`open_with_adapter`] when composing a real or injected adapter.
    pub fn open_with_config(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
    ) -> Result<Self, ConversationCommandError> {
        let path = path.as_ref();
        let repository = SqliteConversationRepository::open(path).map_err(map_session_error)?;
        let tool_calls = open_tool_calls(path)?;
        let projects = open_projects(path)?;
        let host = RuntimeHost::with_fake(config).map_err(|error| {
            ConversationCommandError::new("runtime_host_unavailable", error.to_string())
        })?;
        Ok(Self::from_parts(
            repository, tool_calls, projects, host, emitter,
        ))
    }

    /// Like [`open_with_config`], plus test seams for pump registration / fail-closed.
    ///
    /// Test-only: not part of the production conversation service API.
    #[cfg(test)]
    pub fn open_with_config_and_pump_hook(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
        hooks: PumpRegistrationHooks,
    ) -> Result<Self, ConversationCommandError> {
        let mut service = Self::open_with_config(path, emitter, config)?;
        service.after_spawn_before_register_pump = hooks.after_spawn_before_register;
        service.after_pump_before_registration_wait = hooks.after_pump_before_registration_wait;
        service.force_process_error = hooks.force_process_error;
        service.force_terminalization_failure = hooks.force_terminalization_failure;
        Ok(service)
    }

    fn from_parts(
        repository: SqliteConversationRepository,
        tool_calls: SqliteToolCallRepository,
        projects: SqliteProjectRepository,
        host: RuntimeHost,
        emitter: Arc<dyn RuntimeEventEmitter>,
    ) -> Self {
        Self {
            repository: Arc::new(repository),
            tool_calls: Arc::new(tool_calls),
            projects: Arc::new(projects),
            host: Arc::new(host),
            emitter,
            active: Arc::new(Mutex::new(HashMap::new())),
            admission: ServiceAdmission::new(),
            shut_down: AtomicBool::new(false),
            #[cfg(test)]
            after_spawn_before_register_pump: None,
            #[cfg(test)]
            after_pump_before_registration_wait: None,
            #[cfg(test)]
            force_process_error: None,
            #[cfg(test)]
            force_terminalization_failure: false,
        }
    }

    /// Probe the injected runtime adapter through the host (no provider wire).
    pub fn probe(&self) -> Result<RuntimeProbe, ConversationCommandError> {
        self.host.probe().map_err(|error| {
            ConversationCommandError::new("runtime_probe_failed", error.to_string())
        })
    }

    /// Idempotent recovery: non-terminal runs become `Interrupted`.
    pub fn recover_inflight(&self) -> Result<usize, ConversationCommandError> {
        let recovered = self
            .repository
            .mark_inflight_interrupted()
            .map_err(map_session_error)?;
        reconcile_terminal_tool_receipts(&self.repository, &self.tool_calls).map_err(|error| {
            ConversationCommandError::new("tool_receipt_reconciliation_failed", error)
        })?;
        // Write proposals: pending/approved → interrupted; no live grants/diffs.
        let _ = self.recover_write_proposals_on_startup()?;
        Ok(recovered)
    }

    /// Observable size of the production active-run registry (tests / diagnostics).
    pub fn active_run_count(&self) -> usize {
        self.active.lock().map(|guard| guard.len()).unwrap_or(0)
    }

    /// Host worker registry size (tests / diagnostics).
    pub fn worker_registry_len(&self) -> usize {
        self.host.worker_registry_len()
    }

    /// The host instance that owns this service's governed-write façade.
    pub(super) fn runtime_host(&self) -> &RuntimeHost {
        &self.host
    }

    /// Resolve a durable project root path for restart write recovery.
    pub(super) fn project_root_path(&self, project_id: &str) -> Option<std::path::PathBuf> {
        let id = parse_project_id(project_id).ok()?;
        self.projects
            .get(id)
            .ok()
            .flatten()
            .map(|project| std::path::PathBuf::from(project.root_path))
    }

    pub fn list_conversations(
        &self,
        project_id: String,
    ) -> Result<Vec<ConversationDto>, ConversationCommandError> {
        let project_id = parse_project_id(&project_id)?;
        let conversations = self
            .repository
            .list_conversations(project_id)
            .map_err(map_session_error)?;
        Ok(conversations.iter().map(ConversationDto::from).collect())
    }

    pub fn create_conversation(
        &self,
        project_id: String,
        title: Option<String>,
    ) -> Result<ConversationDto, ConversationCommandError> {
        self.create_conversation_with_id(project_id, title)
            .map(|(conversation, _)| conversation)
    }

    pub(crate) fn create_conversation_with_id(
        &self,
        project_id: String,
        title: Option<String>,
    ) -> Result<(ConversationDto, ConversationId), ConversationCommandError> {
        let project_id = parse_project_id(&project_id)?;
        let title = title
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let conversation = self
            .repository
            .create_conversation(NewConversation { project_id, title })
            .map_err(map_session_error)?;
        let conversation_id = conversation.id;
        Ok((ConversationDto::from(&conversation), conversation_id))
    }

    pub fn load_timeline(
        &self,
        conversation_id: String,
    ) -> Result<TimelineDto, ConversationCommandError> {
        let conversation_id = parse_conversation_id(&conversation_id)?;
        let timeline = self
            .repository
            .load_timeline(conversation_id)
            .map_err(map_session_error)?;
        timeline_dto(&timeline).map_err(|error| {
            ConversationCommandError::new("timeline_payload_invalid", error.to_string())
        })
    }

    pub(crate) fn conversation_project_context(
        &self,
        conversation_id: &str,
    ) -> Result<(ConversationId, ProjectId, PathBuf), ConversationCommandError> {
        let conversation_id = parse_conversation_id(conversation_id)?;
        let timeline = self
            .repository
            .load_timeline(conversation_id)
            .map_err(map_session_error)?;
        let root = resolve_working_directory(&self.repository, &self.projects, conversation_id)?;
        Ok((
            conversation_id,
            timeline.conversation.project_id,
            PathBuf::from(root),
        ))
    }

    pub fn send_turn(
        &self,
        conversation_id: String,
        content: String,
    ) -> Result<SendTurnResult, ConversationCommandError> {
        let conversation_id = parse_conversation_id(&conversation_id)?;
        let trimmed = content.trim().to_owned();
        if trimmed.is_empty() {
            return Err(ConversationCommandError::new(
                "invalid_turn_content",
                "Turn content must not be empty.",
            ));
        }

        // Linearize durable run creation with shutdown: once shutdown owns the
        // gate, no new Pending run/session/pump can appear.
        let _admission = self.admission.try_admit()?;
        // Mirror gate into AtomicBool for diagnostics / fast reject after close.
        if self.shut_down.load(Ordering::SeqCst) {
            return Err(ConversationCommandError::new(
                "conversation_shut_down",
                "Conversation service is shut down.",
            ));
        }

        // Resolve project root before durable run / adapter session so invalid
        // roots never reach the provider process.
        let working_directory =
            resolve_working_directory(&self.repository, &self.projects, conversation_id)?;
        let project_id = self
            .repository
            .load_timeline(conversation_id)
            .map_err(map_session_error)?
            .conversation
            .project_id;

        let run = self
            .repository
            .create_run(NewRun {
                conversation_id,
                user_message: trimmed.clone(),
            })
            .map_err(map_session_error)?;
        let protocol_run_id = ProtocolRunId(run.id.0);
        if let Err(error) = self.host.register_governed_read_run(
            protocol_run_id,
            project_id.0.to_string(),
            conversation_id.0.to_string(),
            &working_directory,
        ) {
            self.fail_admitted_run(run.id, RunStatus::Failed);
            return Err(ConversationCommandError::new(
                "governed_read_unavailable",
                error.to_string(),
            ));
        }
        if let Err(error) = self.host.register_governed_process_run(
            protocol_run_id,
            project_id.0.to_string(),
            conversation_id.0.to_string(),
            &working_directory,
        ) {
            // The established combined release path owns read/write/process
            // bindings. Roll back before the provider session can start.
            self.host.release_governed_read_run(protocol_run_id);
            self.fail_admitted_run(run.id, RunStatus::Failed);
            return Err(ConversationCommandError::new(
                "governed_process_unavailable",
                error.to_string(),
            ));
        }

        let session = match self.host.start_session(StartSession {
            conversation_key: conversation_id.0.to_string(),
            working_directory: Some(working_directory),
        }) {
            Ok(session) => session,
            Err(error) => {
                // Admitted but session failed: durable run must not stay Pending.
                self.host.release_governed_read_run(protocol_run_id);
                self.fail_admitted_run(run.id, RunStatus::Failed);
                return Err(ConversationCommandError::new(
                    "runtime_session_failed",
                    error.to_string(),
                ));
            }
        };

        // Publish the run before spawn so cancel can target it as soon as the
        // host worker is registered (including during after_worker_registered).
        {
            let mut active = self.active.lock().map_err(|_| {
                ConversationCommandError::new(
                    "conversation_state_unavailable",
                    "Conversation state is temporarily unavailable.",
                )
            })?;
            active.insert(
                run.id,
                ActiveRun {
                    conversation_id,
                    pump: None,
                },
            );
        }

        let spawned = match self.host.spawn_turn(SendTurn {
            session_id: session.session_id,
            run_id: protocol_run_id,
            content: trimmed,
        }) {
            Ok(spawned) => spawned,
            Err(error) => {
                // Spawn failure: remove only this exact run entry; terminal truth.
                self.host.release_governed_read_run(protocol_run_id);
                remove_active_run(&self.active, run.id);
                self.fail_admitted_run(run.id, RunStatus::Failed);
                return Err(ConversationCommandError::new(
                    "runtime_spawn_failed",
                    error.to_string(),
                ));
            }
        };

        let registration = Arc::new(PumpHandleRegistration::new());
        let pump = self.spawn_pump(run.id, conversation_id, spawned, Arc::clone(&registration));

        // Test seam: hold here so a fast pump can finish work before the handle
        // is stored. The registration gate keeps the map entry alive until we
        // mark_registered below — pumps must not remove early.
        #[cfg(test)]
        if let Some(hook) = self.after_spawn_before_register_pump.as_ref() {
            hook();
        }

        {
            let mut active = self.active.lock().map_err(|_| {
                ConversationCommandError::new(
                    "conversation_state_unavailable",
                    "Conversation state is temporarily unavailable.",
                )
            })?;
            // Entry was inserted before spawn and the pump waits for registration
            // before remove, so it must still be present. Never drop the handle.
            if let Some(entry) = active.get_mut(&run.id) {
                entry.pump = Some(pump);
            } else {
                // Defensive: never detach a live JoinHandle if the map was cleared
                // by an unexpected path (e.g. poisoned recovery). Keep joinable.
                active.insert(
                    run.id,
                    ActiveRun {
                        conversation_id,
                        pump: Some(pump),
                    },
                );
            }
        }
        // Release the pump only after the JoinHandle is authoritatively stored.
        registration.mark_registered();
        // Admission drops here: run is fully owned by active registry + pump;
        // shutdown's subsequent drain joins it.

        Ok(SendTurnResult {
            run_id: run.id.0.to_string(),
        })
    }

    /// Propose a controller→role dispatch as an ordinary governed process proposal.
    ///
    /// Returns after durable `pending_approval` reserve (Requested). Does **not**
    /// approve or spawn. Human approval stays on `decide_exec_proposal`.
    pub fn propose_role_dispatch(
        &self,
        conversation_id: String,
        input: BuildAssignmentInput,
    ) -> Result<RoleDispatchProposalDto, ConversationCommandError> {
        let prepared = prepare_dispatch(input).map_err(map_dispatch_error)?;

        if self.shut_down.load(Ordering::SeqCst) {
            return Err(ConversationCommandError::new(
                "conversation_shut_down",
                "Conversation service is shut down.",
            ));
        }

        let (conversation_id, project_id, root) =
            self.conversation_project_context(&conversation_id)?;
        let _admission = self.admission.try_admit()?;

        let run = self
            .repository
            .create_run(NewRun {
                conversation_id,
                user_message: format!("[role-dispatch] {}", prepared.assignment.task_text),
            })
            .map_err(map_session_error)?;
        let protocol_run_id = ProtocolRunId(run.id.0);

        if let Err(error) = self.host.register_governed_process_run(
            protocol_run_id,
            project_id.0.to_string(),
            conversation_id.0.to_string(),
            &root,
        ) {
            self.fail_admitted_run(run.id, RunStatus::Failed);
            return Err(ConversationCommandError::new(
                "governed_process_unavailable",
                error.to_string(),
            ));
        }

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_worker = Arc::clone(&cancel);
        let prepared_for_worker = prepared.clone();
        let conversation_for_worker = conversation_id.0.to_string();
        let assignment_for_terminal = Arc::new(Mutex::new(prepared.assignment.clone()));

        let worker = thread::spawn(move || {
            let assignment_lock = Arc::clone(&assignment_for_terminal);
            let result = execute_prepared_dispatch(
                protocol_run_id,
                &prepared_for_worker,
                cancel_for_worker.as_ref(),
                |event| match event {
                    GovernedProcessLifecycle::Requested(requested) => {
                        if let Err(error) = register_dispatch_on_requested(
                            &prepared_for_worker,
                            conversation_for_worker.clone(),
                            protocol_run_id,
                            requested.proposal_id.clone(),
                        ) {
                            let _ = tx.send(Err(error.code().into()));
                            return Err(keeper_runtime_host::GovernedProcessBridgeError::Gateway);
                        }
                        let _ = tx.send(Ok(requested.proposal_id.clone()));
                        Ok(())
                    }
                    GovernedProcessLifecycle::Decided(_) | GovernedProcessLifecycle::Started(_) => {
                        Ok(())
                    }
                },
            );

            match result {
                Ok(receipt) => {
                    if let Ok(mut assignment) = assignment_lock.lock() {
                        let _ = complete_dispatch_on_receipt(
                            &receipt.proposal_id,
                            &mut assignment,
                            &receipt,
                        );
                    } else {
                        let _ =
                            attribute_dispatch_receipt(&prepared_for_worker.assignment, &receipt);
                    }
                    let _ = receipt.state;
                }
                Err(error) => {
                    let _ = tx.send(Err(error.to_string()));
                }
            }
        });

        let proposal_id = match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(id)) => id,
            Ok(Err(message)) => {
                cancel.store(true, Ordering::SeqCst);
                self.host.release_governed_process_run(protocol_run_id);
                self.fail_admitted_run(run.id, RunStatus::Failed);
                let _ = worker.join();
                return Err(ConversationCommandError::new(
                    "role_dispatch_process_unavailable",
                    message,
                ));
            }
            Err(_) => {
                cancel.store(true, Ordering::SeqCst);
                self.host.release_governed_process_run(protocol_run_id);
                self.fail_admitted_run(run.id, RunStatus::Failed);
                let _ = worker.join();
                return Err(ConversationCommandError::new(
                    "role_dispatch_process_unavailable",
                    "Timed out waiting for process proposal reserve.",
                ));
            }
        };

        // Detach the waiter so human approval is not required to return the DTO.
        // Terminal attribution continues on the worker thread.
        std::mem::forget(worker);

        let views = self
            .host
            .list_process_proposals(&conversation_id.0.to_string())
            .map_err(|error| {
                ConversationCommandError::new("process_unavailable", error.to_string())
            })?;
        let view = views
            .into_iter()
            .find(|v| v.proposal_id == proposal_id)
            .ok_or_else(|| {
                ConversationCommandError::new(
                    "process_not_live",
                    "Dispatch process proposal is not live.",
                )
            })?;

        Ok(dispatch_proposal_dto(&prepared, run.id.0.to_string(), view))
    }

    /// In-memory dispatch records for a conversation (this host instance).
    pub fn list_role_dispatches(
        &self,
        conversation_id: String,
    ) -> Result<Vec<RoleDispatchRecordDto>, ConversationCommandError> {
        let _ = self.conversation_project_context(&conversation_id)?;
        let records = list_dispatch_records(&conversation_id).map_err(map_dispatch_error)?;
        Ok(records
            .into_iter()
            .map(|record| RoleDispatchRecordDto {
                assignment_id: record.assignment_id.0.to_string(),
                initiator_slot_id: record.initiator_slot_id.0.to_string(),
                target_slot_id: record.target_slot_id.0.to_string(),
                governance: record.governance.as_str().into(),
                proposal_id: record.proposal_id,
                call_id: record.call_id,
                claimed_governed_execution: record
                    .outcome
                    .as_ref()
                    .map(|o| o.claimed_governed_execution),
                exit_code: record.outcome.as_ref().and_then(|o| o.output.exit_code),
                duration_ms: record.outcome.as_ref().and_then(|o| o.output.duration_ms),
            })
            .collect())
    }

    /// Transition an admitted run that never became a live pump to a terminal
    /// status so shutdown cannot leave a truthful Pending orphan.
    fn fail_admitted_run(&self, run_id: RunId, status: RunStatus) {
        let _ = self.repository.transition_run(run_id, status);
    }

    pub fn cancel_run(&self, run_id: String) -> Result<(), ConversationCommandError> {
        let run_id = parse_run_id(&run_id)?;
        let conversation_id = {
            let active = self.active.lock().map_err(|_| {
                ConversationCommandError::new(
                    "conversation_state_unavailable",
                    "Conversation state is temporarily unavailable.",
                )
            })?;
            active.get(&run_id).map(|entry| entry.conversation_id)
        };

        // Best-effort status: Running -> Cancelling for UI snapshot accuracy.
        if let Some(conversation_id) = conversation_id {
            let timeline = self
                .repository
                .load_timeline(conversation_id)
                .map_err(map_session_error)?;
            if let Some(run) = timeline.runs.iter().find(|run| run.id == run_id)
                && run.status == RunStatus::Running
            {
                let _ = self
                    .repository
                    .transition_run(run_id, RunStatus::Cancelling);
            }
        }

        match self.host.cancel_turn(ProtocolRunId(run_id.0)) {
            Ok(()) => Ok(()),
            Err(keeper_agent_protocol::RuntimeError::RunNotFound { .. }) => Ok(()),
            Err(keeper_agent_protocol::RuntimeError::ShutDown) => Ok(()),
            Err(error) => Err(ConversationCommandError::new(
                "runtime_cancel_failed",
                error.to_string(),
            )),
        }
    }

    /// Wait until every pump has finished (test helper).
    ///
    /// Safe when pumps self-remove their active entries concurrently: handles
    /// are taken under the lock, then joined outside it.
    pub fn drain(&self, timeout: Duration) -> Result<(), ConversationCommandError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let handles: Vec<thread::JoinHandle<()>> = {
                let mut active = self.active.lock().map_err(|_| {
                    ConversationCommandError::new(
                        "conversation_state_unavailable",
                        "Conversation state is temporarily unavailable.",
                    )
                })?;
                active
                    .values_mut()
                    .filter_map(|entry| entry.pump.take())
                    .collect()
            };
            if handles.is_empty() {
                // Also wait for host workers that finished pumps already removed.
                if self.host.is_fully_idle() || self.host.in_flight_turns() == 0 {
                    // Brief settle for map cleanup after Drop.
                    if self.host.worker_registry_len() == 0 && self.active_run_count() == 0 {
                        return Ok(());
                    }
                }
            }
            for handle in handles {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Err(ConversationCommandError::new(
                        "drain_timeout",
                        "Timed out waiting for conversation pumps.",
                    ));
                }
                let _ = handle.join();
            }
            {
                let active = self.active.lock().map_err(|_| {
                    ConversationCommandError::new(
                        "conversation_state_unavailable",
                        "Conversation state is temporarily unavailable.",
                    )
                })?;
                if active.is_empty()
                    && self.host.worker_registry_len() == 0
                    && self.host.in_flight_turns() == 0
                {
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(ConversationCommandError::new(
                    "drain_timeout",
                    "Timed out waiting for conversation pumps.",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Idempotent graceful shutdown (production bound: 15s).
    pub fn shutdown(&self) -> Result<(), ConversationCommandError> {
        self.shutdown_with_timeout(Duration::from_secs(15))
    }

    /// Shared shutdown implementation. Production uses 15s; tests may pass a
    /// short bound so residual fail-closed paths return quickly.
    pub fn shutdown_with_timeout(&self, bound: Duration) -> Result<(), ConversationCommandError> {
        let already = self.shut_down.swap(true, Ordering::SeqCst);
        // Own the admission gate before any drain so racing send_turn cannot
        // create a Pending run after we begin shutdown.
        self.admission.close();
        // Bounded wait: fail closed rather than hang forever.
        self.admission.wait_until_idle(bound)?;

        // Host rejects new turns, cancels inflight workers, joins them.
        self.host.shutdown().map_err(|error| {
            ConversationCommandError::new("runtime_shutdown_failed", error.to_string())
        })?;

        // Drain pumps that may still be finishing persist/emit.
        self.drain(bound)?;

        if self.active_run_count() != 0
            || self.host.worker_registry_len() != 0
            || self.host.in_flight_turns() != 0
            || !self.host.is_fully_idle()
        {
            return Err(ConversationCommandError::new(
                "shutdown_incomplete",
                format!(
                    "Conversation shutdown left residual state: active={}, workers={}, in_flight={}, idle={}",
                    self.active_run_count(),
                    self.host.worker_registry_len(),
                    self.host.in_flight_turns(),
                    self.host.is_fully_idle(),
                ),
            ));
        }

        // Second call must remain successful once idle.
        if already {
            return Ok(());
        }
        Ok(())
    }

    pub fn host_is_idle(&self) -> bool {
        self.host.is_fully_idle()
    }

    pub fn in_flight_turns(&self) -> u32 {
        self.host.in_flight_turns()
    }

    pub fn repository(&self) -> &SqliteConversationRepository {
        &self.repository
    }

    fn spawn_pump(
        &self,
        run_id: RunId,
        conversation_id: ConversationId,
        spawned: SpawnedTurn,
        registration: Arc<PumpHandleRegistration>,
    ) -> thread::JoinHandle<()> {
        let repository = Arc::clone(&self.repository);
        let emitter = Arc::clone(&self.emitter);
        let active = Arc::clone(&self.active);
        let host = Arc::clone(&self.host);
        #[cfg(test)]
        let after_pump = self.after_pump_before_registration_wait.clone();
        #[cfg(test)]
        let seams = PumpFailSeams {
            force_process_error: self.force_process_error.clone(),
            force_terminalization_failure: self.force_terminalization_failure,
        };
        #[cfg(not(test))]
        let seams = PumpFailSeams::default();

        thread::Builder::new()
            .name(format!("keeper-conv-pump-{}", run_id.0))
            .spawn(move || {
                let outcome =
                    pump_turn(repository, emitter, conversation_id, run_id, spawned, seams);
                host.release_governed_read_run(ProtocolRunId(run_id.0));
                // Test seam: post-pump_turn, pre-registration-wait.
                #[cfg(test)]
                if let Some(hook) = after_pump.as_ref() {
                    hook();
                }
                // Wait until JoinHandle is stored, then remove only if durable terminal.
                registration.wait_until_registered();
                if matches!(outcome, PumpOutcome::DurableTerminal) {
                    remove_active_run(&active, run_id);
                } else {
                    eprintln!(
                        "Pillow conversation pump left residual active run {} \
                         (non-terminal after pump failure)",
                        run_id.0
                    );
                }
            })
            .expect("conversation pump thread")
    }
}

fn open_projects(path: &Path) -> Result<SqliteProjectRepository, ConversationCommandError> {
    SqliteProjectRepository::open(path).map_err(|error| {
        eprintln!("Pillow project store open error: {error}");
        ConversationCommandError::new(
            "project_store_unavailable",
            "Project storage is temporarily unavailable.",
        )
    })
}

fn map_dispatch_error(error: DispatchError) -> ConversationCommandError {
    ConversationCommandError::new(error.code(), error.to_string())
}

fn open_tool_calls(path: &Path) -> Result<SqliteToolCallRepository, ConversationCommandError> {
    SqliteToolCallRepository::open(path).map_err(|error| {
        eprintln!("Pillow tool call store open error: {error}");
        ConversationCommandError::new(
            "tool_call_store_unavailable",
            "Tool call storage is temporarily unavailable.",
        )
    })
}
