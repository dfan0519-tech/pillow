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

/// Real service pump: force authoritative process failure at the production
/// boundary → durable Interrupted, no invented terminal event, zero residuals.
#[test]
fn real_pump_authoritative_process_failure_interrupts_cleanly() {
    use crate::conversation_service::PumpRegistrationHooks;

    let (_temp, path) = temp_db();
    let project = seed_project(&path, "pump-auth-fail");
    let service = ConversationService::open_with_config_and_pump_hook(
        &path,
        Arc::new(RecordingEmitter::new()),
        HostConfig {
            max_concurrent_turns: 4,
            honor_fixture_delays: false,
            event_queue_capacity: 64,
            after_worker_registered: None,
        },
        PumpRegistrationHooks {
            force_process_error: Some(Arc::new(|| {
                Err("forced authoritative process failure".into())
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let conversation = service
        .create_conversation(project.0.to_string(), None)
        .unwrap();
    let result = service
        .send_turn(conversation.id.clone(), "force process fail".into())
        .unwrap();
    service.drain(Duration::from_secs(5)).unwrap();

    let timeline = service.load_timeline(conversation.id).unwrap();
    let run = timeline
        .runs
        .iter()
        .find(|r| r.run_id == result.run_id)
        .expect("run");
    assert_eq!(run.status, "interrupted");
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
    assert!(
        terminals.is_empty(),
        "must not invent a terminal runtime event: {terminals:?}"
    );
    assert_eq!(service.active_run_count(), 0);
    assert_eq!(service.worker_registry_len(), 0);
    assert_eq!(service.in_flight_turns(), 0);
    assert!(service.host_is_idle());
}

/// Real pump: process failure then forced terminalization failure → residual
/// active entry; short drain and short shutdown both return actionable errors.
#[test]
fn real_pump_terminalization_failure_fail_closed_on_drain_and_shutdown() {
    use crate::conversation_service::PumpRegistrationHooks;

    let (_temp, path) = temp_db();
    let project = seed_project(&path, "pump-term-fail");
    let service = ConversationService::open_with_config_and_pump_hook(
        &path,
        Arc::new(RecordingEmitter::new()),
        HostConfig {
            max_concurrent_turns: 4,
            honor_fixture_delays: false,
            event_queue_capacity: 64,
            after_worker_registered: None,
        },
        PumpRegistrationHooks {
            force_process_error: Some(Arc::new(|| {
                Err("forced authoritative process failure".into())
            })),
            force_terminalization_failure: true,
            ..Default::default()
        },
    )
    .unwrap();
    let conversation = service
        .create_conversation(project.0.to_string(), None)
        .unwrap();
    let _ = service
        .send_turn(conversation.id.clone(), "force term fail".into())
        .unwrap();

    // Pump finishes with residual active entry (handle still joinable via drain).
    let drain_err = service
        .drain(Duration::from_millis(200))
        .expect_err("residual active must fail drain");
    assert_eq!(drain_err.code, "drain_timeout");
    assert!(
        service.active_run_count() >= 1,
        "active residual must be retained"
    );

    let shutdown_err = service
        .shutdown_with_timeout(Duration::from_millis(200))
        .expect_err("residual active must fail shutdown");
    assert!(
        matches!(shutdown_err.code, "drain_timeout" | "shutdown_incomplete"),
        "actionable fail-closed code, got {}",
        shutdown_err.code
    );
    assert!(service.active_run_count() >= 1);
}

/// Emitted DTO comes from durable recorded event; same-kind replay with a
/// replacement payload re-emits the first persisted payload.
#[test]
fn process_runtime_event_emits_persisted_payload_on_same_kind_replay() {
    use chrono::Utc;
    use keeper_agent_protocol::{
        RunId as ProtocolRunId, RuntimeEvent, RuntimeEventKind as ProtocolEventKind,
    };
    use keeper_session::{ConversationRepository, NewConversation, NewRun, RuntimeEventKind};
    use keeper_storage_sqlite::SqliteConversationRepository;
    use serde_json::json;

    use crate::conversation_service::pipeline::process_runtime_event;

    let (_temp, path) = temp_db();
    let project = seed_project(&path, "emit-recorded");
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
            user_message: "payload truth".into(),
        })
        .unwrap();
    let emitter = RecordingEmitter::new();
    let protocol_run = ProtocolRunId(run.id.0);
    let first = RuntimeEvent {
        run_id: protocol_run,
        sequence: 1,
        kind: ProtocolEventKind::RunStarted,
        payload: json!({"prompt_preview": "original"}),
        created_at: Utc::now(),
    };
    let replacement = RuntimeEvent {
        run_id: protocol_run,
        sequence: 1,
        kind: ProtocolEventKind::RunStarted,
        payload: json!({"prompt_preview": "replacement"}),
        created_at: Utc::now(),
    };
    process_runtime_event(&repo, &emitter, conversation.id, &first).unwrap();
    process_runtime_event(&repo, &emitter, conversation.id, &replacement).unwrap();

    let emitted = emitter.snapshot();
    assert_eq!(emitted.len(), 2);
    for dto in &emitted {
        assert_eq!(dto.kind, "run_started");
        assert_eq!(dto.sequence, 1);
        assert_eq!(dto.payload["prompt_preview"], "original");
    }
    let timeline = repo.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.events.len(), 1);
    assert_eq!(timeline.events[0].kind, RuntimeEventKind::RunStarted);
    assert_eq!(
        timeline.events[0].payload_json,
        r#"{"prompt_preview":"original"}"#
    );
    assert_eq!(timeline.runs[0].status, keeper_session::RunStatus::Running);
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
