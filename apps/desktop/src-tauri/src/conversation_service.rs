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
use std::sync::{Arc, Mutex};
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

pub struct ConversationService {
    repository: Arc<SqliteConversationRepository>,
    host: Arc<RuntimeHost>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    active: Mutex<HashMap<RunId, ActiveRun>>,
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
            active: Mutex::new(HashMap::new()),
        })
    }

    /// Idempotent recovery: non-terminal runs become `Interrupted`.
    pub fn recover_inflight(&self) -> Result<usize, ConversationCommandError> {
        self.repository
            .mark_inflight_interrupted()
            .map_err(map_session_error)
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
                if let Ok(mut active) = self.active.lock() {
                    active.remove(&run.id);
                }
                let _ = self.repository.transition_run(run.id, RunStatus::Failed);
                return Err(ConversationCommandError::new(
                    "runtime_spawn_failed",
                    error.to_string(),
                ));
            }
        };

        let pump = self.spawn_pump(run.id, conversation_id, spawned);
        {
            let mut active = self.active.lock().map_err(|_| {
                ConversationCommandError::new(
                    "conversation_state_unavailable",
                    "Conversation state is temporarily unavailable.",
                )
            })?;
            if let Some(entry) = active.get_mut(&run.id) {
                entry.pump = Some(pump);
            }
        }

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
                    if self.host.worker_registry_len() == 0 {
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
                if active.values().all(|entry| entry.pump.is_none())
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
    ) -> thread::JoinHandle<()> {
        let repository = Arc::clone(&self.repository);
        let emitter = Arc::clone(&self.emitter);
        let active = Arc::new(());
        let _keep = active;

        thread::Builder::new()
            .name(format!("keeper-conv-pump-{}", run_id.0))
            .spawn(move || {
                pump_turn(repository, emitter, conversation_id, run_id, spawned);
            })
            .expect("conversation pump thread")
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

    // 2–3. Persist event then resulting run status (each API commits its own txn).
    let recorded = repository
        .append_event(NewRunEvent {
            run_id: domain_run_id,
            sequence: domain_sequence,
            kind,
            payload_json,
        })
        .map_err(|error| format!("append_event: {error}"))?;

    if let Some(target) = status_for_kind(kind) {
        apply_status_transition(repository, conversation_id, domain_run_id, target)?;
    }

    // 4. Emit only after durable commit.
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

fn apply_status_transition(
    repository: &SqliteConversationRepository,
    conversation_id: ConversationId,
    run_id: RunId,
    target: RunStatus,
) -> Result<(), String> {
    let timeline = repository
        .load_timeline(conversation_id)
        .map_err(|error| format!("load_timeline: {error}"))?;
    let current = timeline
        .runs
        .iter()
        .find(|run| run.id == run_id)
        .map(|run| run.status)
        .ok_or_else(|| format!("run missing after append: {}", run_id.0))?;

    if current == target || current.is_terminal() {
        return Ok(());
    }

    // Cancel terminal may arrive while still Running if cancel raced past
    // the explicit Cancelling transition.
    if target == RunStatus::Cancelled && current == RunStatus::Running {
        repository
            .transition_run(run_id, RunStatus::Cancelling)
            .map_err(|error| format!("transition cancelling: {error}"))?;
    }

    let timeline = repository
        .load_timeline(conversation_id)
        .map_err(|error| format!("load_timeline: {error}"))?;
    let current = timeline
        .runs
        .iter()
        .find(|run| run.id == run_id)
        .map(|run| run.status)
        .ok_or_else(|| format!("run missing: {}", run_id.0))?;

    if current == target || current.is_terminal() {
        return Ok(());
    }

    match repository.transition_run(run_id, target) {
        Ok(_) => Ok(()),
        Err(SessionError::IllegalTransition { .. }) => Ok(()),
        Err(error) => Err(format!("transition_run: {error}")),
    }
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
            let repo = SqliteConversationRepository::open(&path_hook).unwrap();
            let conversation_id = ConversationId(Uuid::parse_str(&dto.conversation_id).unwrap());
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
            seen_hook.fetch_add(1, Ordering::SeqCst);
            // Silence unused warning for first-open repo (ensures migrations applied).
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
