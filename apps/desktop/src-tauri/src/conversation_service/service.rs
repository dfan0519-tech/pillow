//! ConversationService: single public owner of repository, host, emitter,
//! active registry, and shutdown lifecycle.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{RunId as ProtocolRunId, SendTurn, StartSession};
use keeper_runtime_host::{HostConfig, RuntimeHost, SpawnedTurn};
use keeper_session::{
    ConversationId, ConversationRepository, NewConversation, NewRun, RunId, RunStatus,
};
use keeper_storage_sqlite::SqliteConversationRepository;

use crate::conversation_events::{
    ConversationDto, RuntimeEventEmitter, SendTurnResult, TimelineDto, timeline_dto,
};

use super::ids::{map_session_error, parse_conversation_id, parse_project_id, parse_run_id};
use super::pipeline::{PumpOutcome, pump_turn};
use super::types::{
    ActiveMap, ActiveRun, ConversationCommandError, PumpHandleRegistration, ServiceAdmission,
    remove_active_run,
};

/// Optional test seams for pump registration ordering.
///
/// Both hooks are production-no-ops (`None`). Tests use them to force the
/// post-`pump_turn`, pre-remove gate without relying on timing alone.
#[derive(Default)]
pub struct PumpRegistrationHooks {
    /// Fires on the `send_turn` thread after the pump is spawned and before its
    /// `JoinHandle` is stored (and before the registration gate is released).
    pub after_spawn_before_register: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Fires on the pump thread after `pump_turn` returns and before
    /// `wait_until_registered` / active-entry removal.
    pub after_pump_before_registration_wait: Option<Arc<dyn Fn() + Send + Sync>>,
}

pub struct ConversationService {
    repository: Arc<SqliteConversationRepository>,
    host: Arc<RuntimeHost>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    active: ActiveMap,
    /// Linearizes `send_turn` durable admission with `shutdown`.
    admission: ServiceAdmission,
    shut_down: AtomicBool,
    after_spawn_before_register_pump: Option<Arc<dyn Fn() + Send + Sync>>,
    after_pump_before_registration_wait: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ConversationService {
    pub fn open(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        honor_fixture_delays: bool,
    ) -> Result<Self, ConversationCommandError> {
        Self::open_with_config(
            path,
            emitter,
            HostConfig {
                max_concurrent_turns: 4,
                honor_fixture_delays,
                event_queue_capacity: 64,
                after_worker_registered: None,
            },
        )
    }

    /// Test / advanced construction with a custom host config (e.g. cancel barriers).
    pub fn open_with_config(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
    ) -> Result<Self, ConversationCommandError> {
        let repository = SqliteConversationRepository::open(path).map_err(map_session_error)?;
        let host = RuntimeHost::with_fake(config).map_err(|error| {
            ConversationCommandError::new("runtime_host_unavailable", error.to_string())
        })?;

        Ok(Self {
            repository: Arc::new(repository),
            host: Arc::new(host),
            emitter,
            active: Arc::new(Mutex::new(HashMap::new())),
            admission: ServiceAdmission::new(),
            shut_down: AtomicBool::new(false),
            after_spawn_before_register_pump: None,
            after_pump_before_registration_wait: None,
        })
    }

    /// Like [`open_with_config`], plus test seams for pump registration ordering.
    pub fn open_with_config_and_pump_hook(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
        hooks: PumpRegistrationHooks,
    ) -> Result<Self, ConversationCommandError> {
        let mut service = Self::open_with_config(path, emitter, config)?;
        service.after_spawn_before_register_pump = hooks.after_spawn_before_register;
        service.after_pump_before_registration_wait = hooks.after_pump_before_registration_wait;
        Ok(service)
    }

    /// Idempotent recovery: non-terminal runs become `Interrupted`.
    pub fn recover_inflight(&self) -> Result<usize, ConversationCommandError> {
        self.repository
            .mark_inflight_interrupted()
            .map_err(map_session_error)
    }

    /// Observable size of the production active-run registry (tests / diagnostics).
    pub fn active_run_count(&self) -> usize {
        self.active.lock().map(|guard| guard.len()).unwrap_or(0)
    }

    /// Host worker registry size (tests / diagnostics).
    pub fn worker_registry_len(&self) -> usize {
        self.host.worker_registry_len()
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
        let project_id = parse_project_id(&project_id)?;
        let title = title
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let conversation = self
            .repository
            .create_conversation(NewConversation { project_id, title })
            .map_err(map_session_error)?;
        Ok(ConversationDto::from(&conversation))
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

        let run = self
            .repository
            .create_run(NewRun {
                conversation_id,
                user_message: trimmed.clone(),
            })
            .map_err(map_session_error)?;

        let session = match self.host.start_session(StartSession {
            conversation_key: conversation_id.0.to_string(),
            working_directory: None,
        }) {
            Ok(session) => session,
            Err(error) => {
                // Admitted but session failed: durable run must not stay Pending.
                self.fail_admitted_run(run.id, RunStatus::Failed);
                return Err(ConversationCommandError::new(
                    "runtime_session_failed",
                    error.to_string(),
                ));
            }
        };

        let protocol_run_id = ProtocolRunId(run.id.0);

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

    /// Idempotent graceful shutdown:
    /// 1. close the local admission gate (no new durable runs);
    /// 2. wait for in-flight `send_turn` admissions to finish ownership handoff;
    /// 3. reject new host work, cancel inflight workers, join them;
    /// 4. join/drain all active pumps;
    /// 5. leave zero active entries, workers and slots;
    /// 6. surface a bounded actionable error rather than hanging forever.
    pub fn shutdown(&self) -> Result<(), ConversationCommandError> {
        let already = self.shut_down.swap(true, Ordering::SeqCst);
        // Own the admission gate before any drain so racing send_turn cannot
        // create a Pending run after we begin shutdown.
        self.admission.close();
        // Bounded wait: fail closed rather than hang forever.
        self.admission.wait_until_idle(Duration::from_secs(15))?;

        // Host rejects new turns, cancels inflight workers, joins them.
        self.host.shutdown().map_err(|error| {
            ConversationCommandError::new("runtime_shutdown_failed", error.to_string())
        })?;

        // Drain pumps that may still be finishing persist/emit.
        self.drain(Duration::from_secs(15))?;

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
        let after_pump = self.after_pump_before_registration_wait.clone();

        thread::Builder::new()
            .name(format!("keeper-conv-pump-{}", run_id.0))
            .spawn(move || {
                let outcome = pump_turn(repository, emitter, conversation_id, run_id, spawned);
                // Test seam: post-pump_turn, pre-registration-wait.
                if let Some(hook) = after_pump.as_ref() {
                    hook();
                }
                // Wait until JoinHandle is stored, then remove only if durable terminal.
                registration.wait_until_registered();
                if matches!(outcome, PumpOutcome::DurableTerminal) {
                    remove_active_run(&active, run_id);
                } else {
                    eprintln!(
                        "Keeper IDE conversation pump left residual active run {} \
                         (non-terminal after pump failure)",
                        run_id.0
                    );
                }
            })
            .expect("conversation pump thread")
    }
}
