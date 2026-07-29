//! Single owner of conversation repository + deterministic runtime host.
//!
//! Authoritative order for every runtime event:
//! 1. validate
//! 2. persist event + resulting run status in SQLite
//! 3. commit
//! 4. emit to the renderer
//!
//! Never emit an event that has not been durably committed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use keeper_agent_protocol::{
    RunId as ProtocolRunId, RuntimeEvent, RuntimeEventKind as ProtocolEventKind, SendTurn,
    StartSession, validate_payload,
};
use keeper_project::ProjectId;
use keeper_runtime_host::{HostConfig, RuntimeHost, SpawnedTurn};
use keeper_session::{
    ConversationId, ConversationRepository, NewConversation, NewRun, NewRunEvent, RunId, RunStatus,
    RuntimeEventKind, SessionError,
};
use keeper_storage_sqlite::SqliteConversationRepository;
use uuid::Uuid;

use crate::conversation_events::{
    ConversationDto, RuntimeEventEmitter, SendTurnResult, TimelineDto, live_runtime_event_dto,
    status_for_kind, timeline_dto,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCommandError {
    pub code: &'static str,
    pub message: String,
}

impl ConversationCommandError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConversationCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ConversationCommandError {}

struct ActiveRun {
    conversation_id: ConversationId,
    /// Join handle for the persistence/emit pump (not the host worker).
    pump: Option<thread::JoinHandle<()>>,
}

type ActiveMap = Arc<Mutex<HashMap<RunId, ActiveRun>>>;

/// Handshake so a pump never removes its active entry before the `JoinHandle`
/// is authoritatively stored.
///
/// Without this gate, a fast pump can finish and `remove` the pre-insert entry
/// while `send_turn` still holds the handle. Re-inserting that completed handle
/// then leaves a zombie entry (`active_run_count() == 1`) with no pump left to
/// clean it up, and `drain`/shutdown can time out.
struct PumpHandleRegistration {
    ready: Mutex<bool>,
    cv: Condvar,
}

impl PumpHandleRegistration {
    fn new() -> Self {
        Self {
            ready: Mutex::new(false),
            cv: Condvar::new(),
        }
    }

    fn wait_until_registered(&self) {
        let mut ready = self
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*ready {
            ready = self
                .cv
                .wait(ready)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn mark_registered(&self) {
        let mut ready = self
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *ready = true;
        self.cv.notify_one();
    }
}

pub struct ConversationService {
    repository: Arc<SqliteConversationRepository>,
    host: Arc<RuntimeHost>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    active: ActiveMap,
    shut_down: AtomicBool,
    /// Test seam: runs after the pump thread is spawned and before its
    /// `JoinHandle` is stored (and before the registration gate is released).
    after_spawn_before_register_pump: Option<Arc<dyn Fn() + Send + Sync>>,
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
            shut_down: AtomicBool::new(false),
            after_spawn_before_register_pump: None,
        })
    }

    /// Like [`open_with_config`], plus a test seam that fires after the pump
    /// thread is spawned and before its `JoinHandle` is registered.
    pub fn open_with_config_and_pump_hook(
        path: impl AsRef<Path>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        config: HostConfig,
        after_spawn_before_register_pump: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, ConversationCommandError> {
        let mut service = Self::open_with_config(path, emitter, config)?;
        service.after_spawn_before_register_pump = after_spawn_before_register_pump;
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
        if self.shut_down.load(Ordering::SeqCst) {
            return Err(ConversationCommandError::new(
                "conversation_shut_down",
                "Conversation service is shut down.",
            ));
        }

        let conversation_id = parse_conversation_id(&conversation_id)?;
        let trimmed = content.trim().to_owned();
        if trimmed.is_empty() {
            return Err(ConversationCommandError::new(
                "invalid_turn_content",
                "Turn content must not be empty.",
            ));
        }

        let run = self
            .repository
            .create_run(NewRun {
                conversation_id,
                user_message: trimmed.clone(),
            })
            .map_err(map_session_error)?;

        let session = self
            .host
            .start_session(StartSession {
                conversation_key: conversation_id.0.to_string(),
                working_directory: None,
            })
            .map_err(|error| {
                ConversationCommandError::new("runtime_session_failed", error.to_string())
            })?;

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
                // Spawn failure: remove only this exact run entry.
                remove_active_run(&self.active, run.id);
                let _ = self.repository.transition_run(run.id, RunStatus::Failed);
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

        Ok(SendTurnResult {
            run_id: run.id.0.to_string(),
        })
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
    /// 1. reject new work through the host (and local gate);
    /// 2. join/drain all active pumps;
    /// 3. leave zero active entries, workers and slots;
    /// 4. surface a bounded actionable error rather than hanging forever.
    pub fn shutdown(&self) -> Result<(), ConversationCommandError> {
        let already = self.shut_down.swap(true, Ordering::SeqCst);

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

        thread::Builder::new()
            .name(format!("keeper-conv-pump-{}", run_id.0))
            .spawn(move || {
                pump_turn(repository, emitter, conversation_id, run_id, spawned);
                // Do not remove the active entry until send_turn has stored this
                // thread's JoinHandle. Otherwise a completed handle can be
                // re-inserted with nobody left to clean it up.
                registration.wait_until_registered();
                // Production cleanup: remove this exact run only (never a foreign key).
                remove_active_run(&active, run_id);
            })
            .expect("conversation pump thread")
    }
}

fn remove_active_run(active: &ActiveMap, run_id: RunId) {
    if let Ok(mut guard) = active.lock() {
        guard.remove(&run_id);
    }
}

fn pump_turn(
    repository: Arc<SqliteConversationRepository>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    conversation_id: ConversationId,
    _run_id: RunId,
    spawned: SpawnedTurn,
) {
    let delivery = Arc::clone(spawned.delivery());
    loop {
        match delivery.pop_timeout(Duration::from_millis(50)) {
            Ok(Some(event)) => {
                if let Err(error) =
                    process_runtime_event(&repository, emitter.as_ref(), conversation_id, &event)
                {
                    eprintln!("Keeper IDE conversation pump error: {error}");
                    break;
                }
                if event.is_terminal() {
                    break;
                }
            }
            Ok(None) => {
                if delivery.is_closed() && delivery.is_empty() {
                    // Drain any late events.
                    for event in delivery.drain() {
                        let _ = process_runtime_event(
                            &repository,
                            emitter.as_ref(),
                            conversation_id,
                            &event,
                        );
                    }
                    break;
                }
                if spawned.is_finished() && delivery.is_empty() {
                    break;
                }
            }
            Err(error) => {
                eprintln!("Keeper IDE conversation delivery error: {error}");
                break;
            }
        }
    }

    // Join host worker; Drop of SpawnedTurn also joins if needed.
    let _ = spawned.join();
}

fn process_runtime_event(
    repository: &SqliteConversationRepository,
    emitter: &dyn RuntimeEventEmitter,
    conversation_id: ConversationId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    // 1. Validate
    let validated = event
        .validate()
        .map_err(|error| format!("validate: {error}"))?;
    validate_payload(validated.kind, &validated.payload)
        .map_err(|error| format!("payload: {error}"))?;

    let kind = map_kind(validated.kind)?;
    let domain_sequence = protocol_to_domain_sequence(validated.sequence)?;
    let payload_json = serde_json::to_string(&validated.payload)
        .map_err(|error| format!("payload serialize: {error}"))?;
    let domain_run_id = RunId(validated.run_id.0);
    let target_status = status_for_kind(kind);

    // 2–3. Persist event + resulting run status in one SQLite transaction, then commit.
    let recorded = repository
        .append_event_with_status(
            NewRunEvent {
                run_id: domain_run_id,
                sequence: domain_sequence,
                kind,
                payload_json,
            },
            target_status,
        )
        .map_err(|error| format!("append_event_with_status: {error}"))?;

    // 4. Emit only after durable commit of both event and resulting status.
    let dto = live_runtime_event_dto(
        recorded.event_id.0.to_string(),
        domain_run_id,
        conversation_id,
        validated.sequence,
        kind,
        validated.payload.clone(),
        recorded.created_at,
    );
    emitter
        .emit_runtime_event(&dto)
        .map_err(|error| format!("emit: {error}"))?;
    Ok(())
}

fn protocol_to_domain_sequence(protocol_sequence: u64) -> Result<u64, String> {
    protocol_sequence
        .checked_sub(1)
        .ok_or_else(|| format!("protocol sequence {protocol_sequence} is not 1-based"))
}

fn map_kind(kind: ProtocolEventKind) -> Result<RuntimeEventKind, String> {
    RuntimeEventKind::parse(kind_str(kind))
        .ok_or_else(|| format!("unknown runtime event kind: {kind:?}"))
}

fn kind_str(kind: ProtocolEventKind) -> &'static str {
    match kind {
        ProtocolEventKind::RunStarted => "run_started",
        ProtocolEventKind::AssistantDelta => "assistant_delta",
        ProtocolEventKind::ReasoningSummaryDelta => "reasoning_summary_delta",
        ProtocolEventKind::PlanUpdated => "plan_updated",
        ProtocolEventKind::ToolCallStarted => "tool_call_started",
        ProtocolEventKind::ToolCallOutput => "tool_call_output",
        ProtocolEventKind::ToolCallCompleted => "tool_call_completed",
        ProtocolEventKind::SubagentStarted => "subagent_started",
        ProtocolEventKind::SubagentProgress => "subagent_progress",
        ProtocolEventKind::SubagentCompleted => "subagent_completed",
        ProtocolEventKind::ArtifactProduced => "artifact_produced",
        ProtocolEventKind::TurnCompleted => "turn_completed",
        ProtocolEventKind::TurnFailed => "turn_failed",
        ProtocolEventKind::RunCancelled => "run_cancelled",
        ProtocolEventKind::Heartbeat => "heartbeat",
    }
}

fn parse_project_id(value: &str) -> Result<ProjectId, ConversationCommandError> {
    Uuid::parse_str(value)
        .map(ProjectId)
        .map_err(|_| ConversationCommandError::new("project_not_found", "Project not found."))
}

fn parse_conversation_id(value: &str) -> Result<ConversationId, ConversationCommandError> {
    Uuid::parse_str(value).map(ConversationId).map_err(|_| {
        ConversationCommandError::new("conversation_not_found", "Conversation not found.")
    })
}

fn parse_run_id(value: &str) -> Result<RunId, ConversationCommandError> {
    Uuid::parse_str(value)
        .map(RunId)
        .map_err(|_| ConversationCommandError::new("run_not_found", "Run not found."))
}

fn map_session_error(error: SessionError) -> ConversationCommandError {
    eprintln!("Keeper IDE conversation store error: {error}");
    match error {
        SessionError::ProjectNotFound(_) => {
            ConversationCommandError::new("project_not_found", "Project not found.")
        }
        SessionError::ConversationNotFound(_) => {
            ConversationCommandError::new("conversation_not_found", "Conversation not found.")
        }
        SessionError::RunNotFound(_) => {
            ConversationCommandError::new("run_not_found", "Run not found.")
        }
        SessionError::IllegalTransition { .. } => ConversationCommandError::new(
            "illegal_run_transition",
            "Illegal run status transition.",
        ),
        SessionError::InvalidSequence { .. } => ConversationCommandError::new(
            "invalid_event_sequence",
            "Invalid runtime event sequence.",
        ),
        SessionError::SequenceOutOfRange(_) => ConversationCommandError::new(
            "invalid_event_sequence",
            "Runtime event sequence out of range.",
        ),
        SessionError::Store(_) => ConversationCommandError::new(
            "conversation_store_unavailable",
            "Conversation storage is temporarily unavailable.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_events::RecordingEmitter;
    use keeper_project::CreateProject;
    use keeper_session::ConversationRepository;
    use keeper_storage_sqlite::{SqliteConversationRepository, SqliteProjectRepository};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("keeper.db");
        (temp, path)
    }

    fn seed_project(path: &Path, name: &str) -> ProjectId {
        let projects = SqliteProjectRepository::open(path).unwrap();
        let root = path.parent().unwrap().join(name);
        std::fs::create_dir_all(&root).unwrap();
        projects
            .create(CreateProject {
                name: name.into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap()
            .id
    }

    #[test]
    fn projects_retain_isolated_conversation_lists() {
        let (_temp, path) = temp_db();
        let a = seed_project(&path, "proj-a");
        let b = seed_project(&path, "proj-b");
        let service =
            ConversationService::open(&path, Arc::new(RecordingEmitter::new()), false).unwrap();

        let ca = service
            .create_conversation(a.0.to_string(), Some("A".into()))
            .unwrap();
        let _cb = service
            .create_conversation(b.0.to_string(), Some("B".into()))
            .unwrap();

        let listed_a = service.list_conversations(a.0.to_string()).unwrap();
        let listed_b = service.list_conversations(b.0.to_string()).unwrap();
        assert_eq!(listed_a.len(), 1);
        assert_eq!(listed_a[0].id, ca.id);
        assert_eq!(listed_b.len(), 1);
        assert_ne!(listed_a[0].id, listed_b[0].id);
    }

    #[test]
    fn persistence_commits_before_renderer_emit() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "persist");
        let repo_for_hook = SqliteConversationRepository::open(&path).unwrap();
        let seen = Arc::new(AtomicUsize::new(0));
        let seen_hook = Arc::clone(&seen);
        let path_hook = path.clone();

        let emitter = RecordingEmitter::with_hook(move |dto| {
            // Second SQLite connection must see both event and resulting status.
            let repo = SqliteConversationRepository::open(&path_hook).unwrap();
            let conversation_id = ConversationId(Uuid::parse_str(&dto.conversation_id).unwrap());
            let run_id = RunId(Uuid::parse_str(&dto.run_id).unwrap());
            let timeline = repo.load_timeline(conversation_id).unwrap();
            assert!(
                timeline.events.iter().any(|event| {
                    event.event_id.0.to_string() == dto.event_id
                        || (event.run_id.0.to_string() == dto.run_id
                            && event.sequence + 1 == dto.sequence)
                }),
                "event must be durable in SQLite before emit: {:?}",
                dto.kind
            );
            let run = timeline
                .runs
                .iter()
                .find(|run| run.id == run_id)
                .expect("run must exist at emit time");
            match dto.kind.as_str() {
                "run_started" => assert_eq!(
                    run.status,
                    RunStatus::Running,
                    "run_started must commit Running with the event"
                ),
                "turn_completed" => assert_eq!(
                    run.status,
                    RunStatus::Completed,
                    "turn_completed must commit Completed with the event"
                ),
                "turn_failed" => assert_eq!(
                    run.status,
                    RunStatus::Failed,
                    "turn_failed must commit Failed with the event"
                ),
                "run_cancelled" => assert_eq!(
                    run.status,
                    RunStatus::Cancelled,
                    "run_cancelled must commit Cancelled with the event"
                ),
                _ => {}
            }
            seen_hook.fetch_add(1, Ordering::SeqCst);
            let _ = &repo_for_hook;
        });

        let service = ConversationService::open(&path, Arc::new(emitter), false).unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), Some("p".into()))
            .unwrap();
        let result = service
            .send_turn(conversation.id.clone(), "summarize the workspace".into())
            .unwrap();
        service.drain(Duration::from_secs(10)).unwrap();

        assert!(seen.load(Ordering::SeqCst) > 0);
        assert_eq!(service.active_run_count(), 0);
        let timeline = service.load_timeline(conversation.id).unwrap();
        assert!(!timeline.events.is_empty());
        assert_eq!(timeline.runs[0].run_id, result.run_id);
        assert_eq!(timeline.runs[0].status, "completed");
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.kind == "assistant_delta")
        );
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.kind == "plan_updated")
        );
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.kind == "tool_call_completed")
        );
        assert!(
            timeline
                .events
                .iter()
                .any(|event| event.kind == "subagent_completed")
        );
        assert_eq!(
            timeline.events.last().map(|event| event.kind.as_str()),
            Some("turn_completed")
        );
        // Sequences are contiguous 1-based for the renderer.
        for (index, event) in timeline.events.iter().enumerate() {
            assert_eq!(event.sequence, (index as u64) + 1);
        }
    }

    #[test]
    fn cancellation_yields_one_terminal_and_no_residual_slots() {
        use std::sync::Barrier;

        let (_temp, path) = temp_db();
        let project = seed_project(&path, "cancel");
        let emitter = Arc::new(RecordingEmitter::new());
        let enter = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let enter_hook = Arc::clone(&enter);
        let release_hook = Arc::clone(&release);

        let service = ConversationService::open_with_config(
            &path,
            Arc::clone(&emitter) as Arc<_>,
            HostConfig {
                max_concurrent_turns: 4,
                honor_fixture_delays: true,
                event_queue_capacity: 64,
                after_worker_registered: Some(Arc::new(move || {
                    enter_hook.wait();
                    release_hook.wait();
                })),
            },
        )
        .unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), None)
            .unwrap();

        let service = Arc::new(service);
        let service_send = Arc::clone(&service);
        let conversation_id = conversation.id.clone();
        let send =
            thread::spawn(move || service_send.send_turn(conversation_id, "cancel me".into()));

        // Worker is registered and blocked before the fixture body runs.
        enter.wait();
        let pending = service.load_timeline(conversation.id.clone()).unwrap();
        let run_id = pending.runs[0].run_id.clone();
        service.cancel_run(run_id.clone()).unwrap();
        release.wait();

        let result = send.join().expect("send thread").unwrap();
        assert_eq!(result.run_id, run_id);
        service.drain(Duration::from_secs(10)).unwrap();

        let timeline = service.load_timeline(conversation.id).unwrap();
        let terminals: Vec<_> = timeline
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.kind.as_str(),
                    "turn_completed" | "turn_failed" | "run_cancelled"
                )
            })
            .collect();
        assert_eq!(
            terminals.len(),
            1,
            "exactly one terminal event: {terminals:?}"
        );
        assert_eq!(terminals[0].kind, "run_cancelled");
        assert_eq!(timeline.runs[0].status, "cancelled");
        assert_eq!(service.in_flight_turns(), 0);
        assert!(service.host_is_idle());
        assert_eq!(service.active_run_count(), 0);
    }

    #[test]
    fn sequential_turns_return_active_run_count_to_zero() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "seq");
        let service =
            ConversationService::open(&path, Arc::new(RecordingEmitter::new()), false).unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), Some("seq".into()))
            .unwrap();

        for i in 0..8 {
            let _ = service
                .send_turn(conversation.id.clone(), format!("turn {i}"))
                .unwrap();
            service.drain(Duration::from_secs(10)).unwrap();
            assert_eq!(
                service.active_run_count(),
                0,
                "active registry must empty after turn {i}"
            );
            assert_eq!(service.in_flight_turns(), 0);
            assert!(service.host_is_idle());
        }
    }

    /// Deterministic regression for the active-pump registration race:
    /// force the pump to finish work while `send_turn` still holds the
    /// `JoinHandle` (before map registration). Without the registration gate,
    /// the pump would remove the entry and the fallback re-insert would leak.
    #[test]
    fn pump_finish_before_handle_register_clears_active_entry() {
        use std::sync::Barrier;

        let (_temp, path) = temp_db();
        let project = seed_project(&path, "reg-race");
        let enter = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let enter_hook = Arc::clone(&enter);
        let release_hook = Arc::clone(&release);

        let service = ConversationService::open_with_config_and_pump_hook(
            &path,
            Arc::new(RecordingEmitter::new()),
            HostConfig {
                max_concurrent_turns: 4,
                // No fixture delays — pump can finish immediately after spawn.
                honor_fixture_delays: false,
                event_queue_capacity: 64,
                after_worker_registered: None,
            },
            Some(Arc::new(move || {
                // send_turn is blocked here with the JoinHandle not yet stored.
                enter_hook.wait();
                release_hook.wait();
            })),
        )
        .unwrap();

        let service = Arc::new(service);
        let service_send = Arc::clone(&service);
        let conversation = service
            .create_conversation(project.0.to_string(), Some("reg-race".into()))
            .unwrap();
        let conversation_id = conversation.id.clone();
        let send = thread::spawn(move || {
            service_send
                .send_turn(conversation_id, "finish before register".into())
                .unwrap()
        });

        // Blocked in after_spawn_before_register_pump: handle not in map yet.
        enter.wait();

        // Wait until the host worker is fully done. The pump thread has finished
        // pump_turn (or is about to) and is parked on the registration gate —
        // the vulnerable point where the old code removed the map entry.
        let settle_deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < settle_deadline {
            if service.worker_registry_len() == 0 && service.in_flight_turns() == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            service.worker_registry_len(),
            0,
            "host worker must finish while handle registration is held"
        );
        assert_eq!(service.in_flight_turns(), 0);
        // Entry must still be present (pump cannot remove before registration).
        assert_eq!(
            service.active_run_count(),
            1,
            "active entry must survive until JoinHandle is stored"
        );

        release.wait();
        let _ = send.join().expect("send thread");
        service.drain(Duration::from_secs(10)).unwrap();

        assert_eq!(service.active_run_count(), 0);
        assert_eq!(service.worker_registry_len(), 0);
        assert_eq!(service.in_flight_turns(), 0);
        assert!(service.host_is_idle());

        let timeline = service.load_timeline(conversation.id).unwrap();
        assert_eq!(timeline.runs[0].status, "completed");
    }

    /// No-delay sequential stress: many turns back-to-back with fixture delays
    /// off, each leaving zero residuals (active / workers / in-flight).
    #[test]
    fn no_delay_sequential_stress_clears_all_residuals() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "stress-seq");
        let service =
            ConversationService::open(&path, Arc::new(RecordingEmitter::new()), false).unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), Some("stress".into()))
            .unwrap();

        for i in 0..32 {
            let _ = service
                .send_turn(conversation.id.clone(), format!("stress turn {i}"))
                .unwrap();
            service.drain(Duration::from_secs(10)).unwrap();
            assert_eq!(
                service.active_run_count(),
                0,
                "active registry residual after turn {i}"
            );
            assert_eq!(
                service.worker_registry_len(),
                0,
                "worker registry residual after turn {i}"
            );
            assert_eq!(
                service.in_flight_turns(),
                0,
                "in-flight residual after turn {i}"
            );
            assert!(service.host_is_idle(), "host not fully idle after turn {i}");
        }
    }

    struct FailingEmitter;

    impl RuntimeEventEmitter for FailingEmitter {
        fn emit_runtime_event(
            &self,
            _event: &crate::conversation_events::RuntimeEventDto,
        ) -> Result<(), String> {
            Err("forced pump emit failure".into())
        }
    }

    #[test]
    fn pump_error_removes_active_run_entry() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "pump-err");
        let service = ConversationService::open(&path, Arc::new(FailingEmitter), false).unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), None)
            .unwrap();
        let _ = service
            .send_turn(conversation.id, "will fail at emit".into())
            .unwrap();
        service.drain(Duration::from_secs(10)).unwrap();
        assert_eq!(service.active_run_count(), 0);
        assert_eq!(service.in_flight_turns(), 0);
        assert!(service.host_is_idle());
    }

    #[test]
    fn spawn_failure_removes_active_run_entry() {
        use std::sync::Barrier;

        let (_temp, path) = temp_db();
        let project = seed_project(&path, "spawn-fail");
        let enter = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let enter_hook = Arc::clone(&enter);
        let release_hook = Arc::clone(&release);
        // Hold the only concurrency slot so a second spawn is rejected.
        let service = ConversationService::open_with_config(
            &path,
            Arc::new(RecordingEmitter::new()),
            HostConfig {
                max_concurrent_turns: 1,
                honor_fixture_delays: true,
                event_queue_capacity: 64,
                after_worker_registered: Some(Arc::new(move || {
                    enter_hook.wait();
                    release_hook.wait();
                })),
            },
        )
        .unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), None)
            .unwrap();
        let service = Arc::new(service);
        let service_hold = Arc::clone(&service);
        let hold_id = conversation.id.clone();
        let holder = thread::spawn(move || {
            service_hold
                .send_turn(hold_id, "hold the slot".into())
                .unwrap()
        });

        enter.wait();
        // Second turn must fail admission while the first holds the only slot.
        let err = service
            .send_turn(conversation.id.clone(), "overflow".into())
            .unwrap_err();
        assert_eq!(err.code, "runtime_spawn_failed");
        // Failed spawn must not leave a residual active entry (holder still active).
        assert_eq!(service.active_run_count(), 1);

        release.wait();
        let _ = holder.join().expect("holder");
        service.drain(Duration::from_secs(10)).unwrap();
        assert_eq!(service.active_run_count(), 0);
        assert_eq!(service.in_flight_turns(), 0);
        assert!(service.host_is_idle());
    }

    #[test]
    fn shutdown_is_idempotent_and_leaves_zero_residuals() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "shutdown");
        let service =
            ConversationService::open(&path, Arc::new(RecordingEmitter::new()), false).unwrap();
        let conversation = service
            .create_conversation(project.0.to_string(), None)
            .unwrap();
        let _ = service
            .send_turn(conversation.id.clone(), "finish me".into())
            .unwrap();

        service.shutdown().unwrap();
        assert_eq!(service.active_run_count(), 0);
        assert_eq!(service.in_flight_turns(), 0);
        assert!(service.host_is_idle());

        // Idempotent second call.
        service.shutdown().unwrap();

        let err = service
            .send_turn(conversation.id, "after shutdown".into())
            .unwrap_err();
        assert_eq!(err.code, "conversation_shut_down");
    }

    #[test]
    fn selecting_another_conversation_does_not_stop_active_run() {
        let (_temp, path) = temp_db();
        let project = seed_project(&path, "bg");
        let emitter = Arc::new(RecordingEmitter::new());
        let service =
            ConversationService::open(&path, Arc::clone(&emitter) as Arc<_>, false).unwrap();
        let first = service
            .create_conversation(project.0.to_string(), Some("one".into()))
            .unwrap();
        let second = service
            .create_conversation(project.0.to_string(), Some("two".into()))
            .unwrap();

        let run = service
            .send_turn(first.id.clone(), "background turn".into())
            .unwrap();
        // "Select" second conversation by loading its empty timeline while first streams.
        let other = service.load_timeline(second.id).unwrap();
        assert!(other.events.is_empty());

        service.drain(Duration::from_secs(10)).unwrap();
        assert_eq!(service.active_run_count(), 0);
        let timeline = service.load_timeline(first.id).unwrap();
        assert_eq!(timeline.runs[0].run_id, run.run_id);
        assert_eq!(timeline.runs[0].status, "completed");
        assert!(!timeline.events.is_empty());
    }

    #[test]
    fn restart_marks_inflight_interrupted_idempotently() {
        let (temp, path) = temp_db();
        let project = seed_project(&path, "recover");
        let sessions = SqliteConversationRepository::open(&path).unwrap();
        let conversation = sessions
            .create_conversation(NewConversation {
                project_id: project,
                title: Some("recover".into()),
            })
            .unwrap();
        let run = sessions
            .create_run(NewRun {
                conversation_id: conversation.id,
                user_message: "left running".into(),
            })
            .unwrap();
        sessions.transition_run(run.id, RunStatus::Running).unwrap();
        sessions
            .append_event(NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"x"}"#.into(),
            })
            .unwrap();
        drop(sessions);

        let service =
            ConversationService::open(&path, Arc::new(RecordingEmitter::new()), false).unwrap();
        let first = service.recover_inflight().unwrap();
        assert_eq!(first, 1);
        let second = service.recover_inflight().unwrap();
        assert_eq!(second, 0);

        let timeline = service
            .load_timeline(conversation.id.0.to_string())
            .unwrap();
        assert_eq!(timeline.messages.len(), 1);
        assert_eq!(timeline.messages[0].content, "left running");
        assert_eq!(timeline.runs[0].status, "interrupted");
        assert_eq!(timeline.events.len(), 1);
        assert_eq!(timeline.events[0].kind, "run_started");
        // temp keep alive
        let _ = temp;
    }
}
