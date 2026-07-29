use std::sync::Arc;
use std::thread;
use std::time::Duration;

use keeper_runtime_host::HostConfig;

use crate::conversation_events::{RecordingEmitter, RuntimeEventDto, RuntimeEventEmitter};
use crate::conversation_service::ConversationService;

use super::support::{seed_project, temp_db};

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
    let send = thread::spawn(move || service_send.send_turn(conversation_id, "cancel me".into()));

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

struct FailingEmitter;

impl RuntimeEventEmitter for FailingEmitter {
    fn emit_runtime_event(&self, _event: &RuntimeEventDto) -> Result<(), String> {
        Err("forced pump emit failure".into())
    }
}

/// Emit failure is non-authoritative: durable run still reaches Completed with
/// exactly one terminal event and zero residual active/workers/slots.
#[test]
fn failing_emitter_still_reaches_completed_with_zero_residuals() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "emit-fail");
    let service = ConversationService::open(&path, Arc::new(FailingEmitter), false).unwrap();
    let conversation = service
        .create_conversation(project.0.to_string(), None)
        .unwrap();
    let result = service
        .send_turn(conversation.id.clone(), "will fail at emit".into())
        .unwrap();
    service.drain(Duration::from_secs(10)).unwrap();
    let timeline = service.load_timeline(conversation.id).unwrap();
    let run = timeline
        .runs
        .iter()
        .find(|run| run.run_id == result.run_id)
        .expect("run");
    assert_eq!(run.status, "completed");
    let terminals: Vec<_> = timeline
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.kind.as_str(),
                "turn_completed" | "turn_failed" | "run_cancelled"
            )
        })
        .collect();
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    assert_eq!(terminals[0].kind, "turn_completed");
    assert_eq!(service.active_run_count(), 0);
    assert_eq!(service.worker_registry_len(), 0);
    assert_eq!(service.in_flight_turns(), 0);
    assert!(service.host_is_idle());
}

/// Processing failure → Interrupted without inventing a terminal event; missing
/// run terminalization is fail-closed residual.
#[test]
fn pump_processing_failure_interrupts_or_fail_closed() {
    use keeper_session::{
        ConversationRepository, NewConversation, NewRun, NewRunEvent, RunId, RunStatus,
        RuntimeEventKind,
    };
    use keeper_storage_sqlite::SqliteConversationRepository;
    use uuid::Uuid;

    use crate::conversation_service::pipeline::{PumpOutcome, ensure_interrupted};

    let (_temp, path) = temp_db();
    let project = seed_project(&path, "pump-interrupt");
    let repo = SqliteConversationRepository::open(&path).unwrap();
    let conversation = repo
        .create_conversation(NewConversation {
            project_id: project,
            title: None,
        })
        .unwrap();
    let run = repo
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "interrupt me".into(),
        })
        .unwrap();
    repo.append_event_with_status(
        NewRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"prompt_preview":"x"}"#.into(),
        },
        Some(RunStatus::Running),
    )
    .unwrap();
    assert_eq!(
        ensure_interrupted(&repo, run.id),
        PumpOutcome::DurableTerminal
    );
    let timeline = repo.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs[0].status, RunStatus::Interrupted);
    assert_eq!(timeline.events.len(), 1);
    assert_eq!(timeline.events[0].kind, RuntimeEventKind::RunStarted);
    assert_eq!(
        ensure_interrupted(&repo, run.id),
        PumpOutcome::DurableTerminal
    );
    assert_eq!(
        ensure_interrupted(&repo, RunId(Uuid::new_v4())),
        PumpOutcome::ResidualNonTerminal
    );
}

/// Emit-path failures still clear the active entry once the durable run is terminal.
#[test]
fn pump_error_removes_active_run_entry() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "pump-err");
    let service = ConversationService::open(&path, Arc::new(FailingEmitter), false).unwrap();
    let conversation = service
        .create_conversation(project.0.to_string(), None)
        .unwrap();
    let _ = service
        .send_turn(conversation.id.clone(), "will fail at emit".into())
        .unwrap();
    service.drain(Duration::from_secs(10)).unwrap();
    assert_eq!(
        service.load_timeline(conversation.id).unwrap().runs[0].status,
        "completed"
    );
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
