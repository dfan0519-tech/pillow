//! Prove ConversationService routes through an injected AgentRuntimeAdapter
//! (probe / session / turn / cancel) without hard-coding FakeRuntimeAdapter.
//! Also covers project-root `working_directory` propagation and one-prompt
//! host admission for the Grok production config shape.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeProbe, RuntimeSession, RuntimeSessionId, SendTurn, StartSession,
};
use keeper_project::CreateProject;
use keeper_runtime_host::HostConfig;
use keeper_storage_sqlite::SqliteProjectRepository;

use crate::conversation_events::RecordingEmitter;
use crate::conversation_service::ConversationService;
use crate::runtime_composition::production_grok_host_config;

use super::support::{seed_project, temp_db};

/// Deterministic in-process adapter used only to prove composition routing.
struct InjectedRoutingAdapter {
    probe_calls: AtomicUsize,
    session_calls: AtomicUsize,
    arm_calls: AtomicUsize,
    send_calls: AtomicUsize,
    cancel_calls: AtomicUsize,
    shutdown_calls: AtomicUsize,
    sessions: Mutex<HashMap<RuntimeSessionId, String>>,
    armed: Mutex<HashMap<RunId, Arc<AtomicBool>>>,
    /// Captured `StartSession.working_directory` values (project-root proof).
    working_directories: Mutex<Vec<Option<String>>>,
    /// When true, `send_turn` waits until cancel is observed (cancel path).
    hold_for_cancel: AtomicBool,
    /// When true, `send_turn` holds until `release_send` is set (admission).
    hold_until_release: AtomicBool,
    release_send: AtomicBool,
}

impl InjectedRoutingAdapter {
    fn new() -> Self {
        Self {
            probe_calls: AtomicUsize::new(0),
            session_calls: AtomicUsize::new(0),
            arm_calls: AtomicUsize::new(0),
            send_calls: AtomicUsize::new(0),
            cancel_calls: AtomicUsize::new(0),
            shutdown_calls: AtomicUsize::new(0),
            sessions: Mutex::new(HashMap::new()),
            armed: Mutex::new(HashMap::new()),
            working_directories: Mutex::new(Vec::new()),
            hold_for_cancel: AtomicBool::new(false),
            hold_until_release: AtomicBool::new(false),
            release_send: AtomicBool::new(false),
        }
    }

    fn set_hold_for_cancel(&self, hold: bool) {
        self.hold_for_cancel.store(hold, Ordering::SeqCst);
    }

    fn set_hold_until_release(&self, hold: bool) {
        self.hold_until_release.store(hold, Ordering::SeqCst);
        if hold {
            self.release_send.store(false, Ordering::SeqCst);
        }
    }

    fn release_held_send(&self) {
        self.release_send.store(true, Ordering::SeqCst);
    }

    fn last_working_directory(&self) -> Option<String> {
        self.working_directories
            .lock()
            .expect("cwd")
            .last()
            .cloned()
            .flatten()
    }
}

impl AgentRuntimeAdapter for InjectedRoutingAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.probe_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "injected-routing".into(),
            version: "test".into(),
            detail: Some("composition boundary fixture".into()),
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.session_calls.fetch_add(1, Ordering::SeqCst);
        self.working_directories
            .lock()
            .expect("cwd")
            .push(request.working_directory.clone());
        let session_id = RuntimeSessionId::new();
        self.sessions
            .lock()
            .expect("sessions")
            .insert(session_id, request.conversation_key.clone());
        Ok(RuntimeSession {
            session_id,
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.arm_calls.fetch_add(1, Ordering::SeqCst);
        let mut armed = self.armed.lock().expect("armed");
        if armed.contains_key(&run_id) {
            return Err(RuntimeError::RunAlreadyActive { run_id });
        }
        armed.insert(run_id, Arc::new(AtomicBool::new(false)));
        Ok(())
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.armed.lock().expect("armed").remove(&run_id);
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        {
            let sessions = self.sessions.lock().expect("sessions");
            if !sessions.contains_key(&request.session_id) {
                return Err(RuntimeError::SessionNotFound {
                    session_id: request.session_id,
                });
            }
        }
        let cancel = {
            let armed = self.armed.lock().expect("armed");
            armed
                .get(&request.run_id)
                .cloned()
                .ok_or(RuntimeError::RunNotFound {
                    run_id: request.run_id,
                })?
        };

        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 1,
            kind: RuntimeEventKind::RunStarted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;

        if self.hold_until_release.load(Ordering::SeqCst) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !self.release_send.load(Ordering::SeqCst) && !cancel.load(Ordering::SeqCst) {
                if std::time::Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }

        if self.hold_for_cancel.load(Ordering::SeqCst) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !cancel.load(Ordering::SeqCst) {
                if std::time::Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            if cancel.load(Ordering::SeqCst) {
                sink.emit(RuntimeEvent {
                    run_id: request.run_id,
                    sequence: 2,
                    kind: RuntimeEventKind::RunCancelled,
                    payload: serde_json::json!({ "reason": "user" }),
                    created_at: Utc::now(),
                })?;
                self.armed.lock().expect("armed").remove(&request.run_id);
                return Ok(());
            }
        }

        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 2,
            kind: RuntimeEventKind::AssistantDelta,
            payload: serde_json::json!({ "text": "injected-ok" }),
            created_at: Utc::now(),
        })?;
        sink.emit(RuntimeEvent {
            run_id: request.run_id,
            sequence: 3,
            kind: RuntimeEventKind::TurnCompleted,
            payload: serde_json::json!({}),
            created_at: Utc::now(),
        })?;
        self.armed.lock().expect("armed").remove(&request.run_id);
        Ok(())
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        let armed = self.armed.lock().expect("armed");
        if let Some(flag) = armed.get(&run_id) {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(RuntimeError::RunNotFound { run_id })
        }
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        self.shutdown_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn default_host_config() -> HostConfig {
    HostConfig::new(4, false, 64)
}

#[test]
fn adapter_factory_and_host_share_the_same_governed_service_allocation() {
    let (_temp, path) = temp_db();
    let adapter = Arc::new(InjectedRoutingAdapter::new());
    let emitter = Arc::new(RecordingEmitter::new());
    let factory_port: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
    let captured = Arc::clone(&factory_port);
    let adapter_for_factory = Arc::clone(&adapter);

    let service = ConversationService::open_with_adapter_factory(
        &path,
        Arc::clone(&emitter) as Arc<_>,
        default_host_config(),
        move |execution| {
            *captured.lock().expect("factory port") =
                Some(Arc::as_ptr(&execution) as *const () as usize);
            Ok(adapter_for_factory as Arc<dyn AgentRuntimeAdapter>)
        },
    )
    .expect("factory composition");

    let execution_ptr = factory_port
        .lock()
        .expect("factory port")
        .take()
        .expect("factory received governance");
    assert_eq!(
        execution_ptr,
        service.runtime_host().governed_tool_service() as *const _ as usize,
        "desktop factory must receive the exact service allocation owned by the host"
    );
    service.shutdown().expect("shutdown");
}

#[test]
fn injected_adapter_routes_probe_session_turn_and_cancel() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "composition");
    let adapter = Arc::new(InjectedRoutingAdapter::new());
    let emitter = Arc::new(RecordingEmitter::new());

    let service = ConversationService::open_with_adapter(
        &path,
        Arc::clone(&emitter) as Arc<_>,
        Arc::clone(&adapter) as Arc<dyn AgentRuntimeAdapter>,
        default_host_config(),
    )
    .unwrap();

    // Probe routes to the injected adapter (not fake).
    let probe = service.probe().unwrap();
    assert!(probe.healthy);
    assert_eq!(probe.adapter_name, "injected-routing");
    assert_eq!(adapter.probe_calls.load(Ordering::SeqCst), 1);

    let conversation = service
        .create_conversation(project.0.to_string(), Some("inject".into()))
        .unwrap();

    // Happy-path turn: start_session + arm + send_turn on the injected adapter.
    let result = service
        .send_turn(conversation.id.clone(), "hello injected".into())
        .unwrap();
    service.drain(Duration::from_secs(5)).unwrap();

    assert_eq!(adapter.session_calls.load(Ordering::SeqCst), 1);
    assert_eq!(adapter.arm_calls.load(Ordering::SeqCst), 1);
    assert_eq!(adapter.send_calls.load(Ordering::SeqCst), 1);

    let timeline = service.load_timeline(conversation.id.clone()).unwrap();
    assert_eq!(timeline.runs[0].run_id, result.run_id);
    assert_eq!(timeline.runs[0].status, "completed");
    assert!(
        timeline
            .events
            .iter()
            .any(|event| event.kind == "assistant_delta"
                && event.payload.get("text").and_then(|v| v.as_str()) == Some("injected-ok"))
    );
    assert_eq!(
        timeline.events.last().map(|e| e.kind.as_str()),
        Some("turn_completed")
    );
    assert_eq!(service.active_run_count(), 0);

    // Cancel path: hold send_turn until cancel_turn is observed.
    adapter.set_hold_for_cancel(true);
    let service = Arc::new(service);
    let service_send = Arc::clone(&service);
    let conversation_id = conversation.id.clone();
    let send =
        thread::spawn(move || service_send.send_turn(conversation_id, "please cancel".into()));

    // Wait until the second run is unfinished on the timeline *and* send_turn
    // has entered the adapter. arm_calls increments before either is true, so
    // it is the wrong signal. Never fall back to runs.last(): that selects
    // the first completed run when the second is not visible yet.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let run_id = loop {
        if std::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for second unfinished timeline run \
                 after send_turn (arm_calls={}, send_calls={})",
                adapter.arm_calls.load(Ordering::SeqCst),
                adapter.send_calls.load(Ordering::SeqCst)
            );
        }
        if adapter.send_calls.load(Ordering::SeqCst) >= 2 {
            let pending = service.load_timeline(conversation.id.clone()).unwrap();
            if let Some(run) = pending.runs.iter().find(|run| {
                run.status == "running" || run.status == "pending" || run.status == "cancelling"
            }) {
                break run.run_id.clone();
            }
        }
        thread::sleep(Duration::from_millis(5));
    };

    service.cancel_run(run_id.clone()).unwrap();
    let send_result = send.join().expect("send thread").unwrap();
    assert_eq!(send_result.run_id, run_id);
    service.drain(Duration::from_secs(5)).unwrap();

    assert!(
        adapter.cancel_calls.load(Ordering::SeqCst) >= 1,
        "cancel_run must reach injected adapter.cancel_turn"
    );
    assert_eq!(adapter.send_calls.load(Ordering::SeqCst), 2);

    let after_cancel = service.load_timeline(conversation.id).unwrap();
    let cancelled = after_cancel
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .expect("cancelled run");
    assert_eq!(cancelled.status, "cancelled");
    let terminals: Vec<_> = after_cancel
        .events
        .iter()
        .filter(|event| event.run_id == run_id)
        .filter(|event| {
            matches!(
                event.kind.as_str(),
                "turn_completed" | "turn_failed" | "run_cancelled"
            )
        })
        .collect();
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    assert_eq!(terminals[0].kind, "run_cancelled");

    service.shutdown().unwrap();
    assert_eq!(adapter.shutdown_calls.load(Ordering::SeqCst), 1);
    assert!(service.host_is_idle());
    assert_eq!(service.active_run_count(), 0);
}

#[test]
fn project_root_propagates_as_canonical_working_directory() {
    let (_temp, path) = temp_db();
    let project_id = seed_project(&path, "cwd-prop");
    let projects = SqliteProjectRepository::open(&path).unwrap();
    let project = projects.get(project_id).unwrap().expect("project");
    let expected = PathBuf::from(&project.root_path)
        .canonicalize()
        .expect("canonical root");

    let adapter = Arc::new(InjectedRoutingAdapter::new());
    let service = ConversationService::open_with_adapter(
        &path,
        Arc::new(RecordingEmitter::new()) as Arc<_>,
        Arc::clone(&adapter) as Arc<dyn AgentRuntimeAdapter>,
        default_host_config(),
    )
    .unwrap();
    let conversation = service
        .create_conversation(project_id.0.to_string(), Some("cwd".into()))
        .unwrap();

    service
        .send_turn(conversation.id, "use project root".into())
        .unwrap();
    service.drain(Duration::from_secs(5)).unwrap();

    assert_eq!(adapter.session_calls.load(Ordering::SeqCst), 1);
    let cwd = adapter
        .last_working_directory()
        .expect("working_directory must be set");
    assert_eq!(Path::new(&cwd), expected.as_path());
    service.shutdown().unwrap();
}

#[test]
fn invalid_project_root_rejects_before_adapter_spawn() {
    let (_temp, path) = temp_db();
    let projects = SqliteProjectRepository::open(&path).unwrap();
    let root = path.parent().unwrap().join("gone-root");
    std::fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: "gone".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();

    let adapter = Arc::new(InjectedRoutingAdapter::new());
    let service = ConversationService::open_with_adapter(
        &path,
        Arc::new(RecordingEmitter::new()) as Arc<_>,
        Arc::clone(&adapter) as Arc<dyn AgentRuntimeAdapter>,
        default_host_config(),
    )
    .unwrap();
    let conversation = service
        .create_conversation(project.id.0.to_string(), Some("bad-root".into()))
        .unwrap();

    // Remove the registered root so resolve fails before start_session.
    std::fs::remove_dir_all(&root).unwrap();

    let err = service
        .send_turn(conversation.id.clone(), "should not spawn".into())
        .unwrap_err();
    assert_eq!(err.code, "invalid_project_root");
    assert_eq!(
        adapter.session_calls.load(Ordering::SeqCst),
        0,
        "adapter must not start_session when root is invalid"
    );
    assert_eq!(adapter.send_calls.load(Ordering::SeqCst), 0);

    // No durable run should have been created either (fail before create_run).
    let timeline = service.load_timeline(conversation.id).unwrap();
    assert!(
        timeline.runs.is_empty(),
        "invalid root must not admit a Pending run"
    );
    service.shutdown().unwrap();
}

#[test]
fn non_directory_project_root_rejects_before_adapter_spawn() {
    let (_temp, path) = temp_db();
    let projects = SqliteProjectRepository::open(&path).unwrap();
    let root = path.parent().unwrap().join("file-root");
    std::fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: "file-root".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();

    let adapter = Arc::new(InjectedRoutingAdapter::new());
    let service = ConversationService::open_with_adapter(
        &path,
        Arc::new(RecordingEmitter::new()) as Arc<_>,
        Arc::clone(&adapter) as Arc<dyn AgentRuntimeAdapter>,
        default_host_config(),
    )
    .unwrap();
    let conversation = service
        .create_conversation(project.id.0.to_string(), None)
        .unwrap();

    std::fs::remove_dir_all(&root).unwrap();
    std::fs::write(&root, b"not-a-directory").unwrap();

    let err = service
        .send_turn(conversation.id, "nope".into())
        .unwrap_err();
    assert_eq!(err.code, "invalid_project_root");
    assert_eq!(adapter.session_calls.load(Ordering::SeqCst), 0);
    service.shutdown().unwrap();
}

#[test]
fn one_prompt_admission_matches_production_grok_host_config() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "one-prompt");
    let adapter = Arc::new(InjectedRoutingAdapter::new());
    adapter.set_hold_until_release(true);

    let service = Arc::new(
        ConversationService::open_with_adapter(
            &path,
            Arc::new(RecordingEmitter::new()) as Arc<_>,
            Arc::clone(&adapter) as Arc<dyn AgentRuntimeAdapter>,
            production_grok_host_config(),
        )
        .unwrap(),
    );
    let conversation = service
        .create_conversation(project.0.to_string(), Some("limit".into()))
        .unwrap();

    let service_first = Arc::clone(&service);
    let conversation_id = conversation.id.clone();
    let first =
        thread::spawn(move || service_first.send_turn(conversation_id, "first concurrent".into()));

    // Wait until the first turn is armed (slot held).
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while adapter.arm_calls.load(Ordering::SeqCst) < 1 {
        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for first arm_turn");
        }
        thread::sleep(Duration::from_millis(5));
    }

    let second_err = service
        .send_turn(conversation.id.clone(), "second concurrent".into())
        .unwrap_err();
    assert!(
        second_err.code == "runtime_spawn_failed"
            || second_err.message.contains("concurrency")
            || second_err.message.contains("Concurrency"),
        "second prompt must be rejected under one-prompt admission: {second_err:?}"
    );

    adapter.release_held_send();
    first.join().expect("first thread").unwrap();
    service.drain(Duration::from_secs(5)).unwrap();
    service.shutdown().unwrap();
}
