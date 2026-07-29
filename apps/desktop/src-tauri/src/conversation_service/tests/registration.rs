use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use keeper_runtime_host::HostConfig;
use keeper_session::{
    ConversationRepository, NewConversation, NewRun, NewRunEvent, RunStatus, RuntimeEventKind,
};
use keeper_storage_sqlite::SqliteConversationRepository;

use crate::conversation_events::RecordingEmitter;
use crate::conversation_service::{ConversationService, PumpRegistrationHooks};

use super::support::{open_service, seed_project, temp_db};

/// Deterministic regression for the active-pump registration race:
/// force the pump to finish work while `send_turn` still holds the
/// `JoinHandle` (before map registration). Without the registration gate,
/// the pump would remove the entry and the fallback re-insert would leak.
///
/// Strengthened signal: an explicit test-only hook fires on the pump thread
/// after `pump_turn` returns and before registration wait/removal. The test
/// rendezvous on that signal rather than inferring readiness from host worker
/// idleness alone.
#[test]
fn pump_finish_before_handle_register_clears_active_entry() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "reg-race");
    let enter = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let enter_hook = Arc::clone(&enter);
    let release_hook = Arc::clone(&release);

    // Explicit post-pump_turn / pre-registration-wait rendezvous.
    let pump_post_turn = Arc::new(Barrier::new(2));
    let pump_post_turn_hook = Arc::clone(&pump_post_turn);
    let pump_reached_post_turn = Arc::new(AtomicBool::new(false));
    let pump_reached_flag = Arc::clone(&pump_reached_post_turn);

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
        PumpRegistrationHooks {
            after_spawn_before_register: Some(Arc::new(move || {
                // send_turn is blocked here with the JoinHandle not yet stored.
                enter_hook.wait();
                release_hook.wait();
            })),
            after_pump_before_registration_wait: Some(Arc::new(move || {
                pump_reached_flag.store(true, Ordering::SeqCst);
                // Rendezvous: prove we are past pump_turn and before
                // wait_until_registered / remove_active_run.
                pump_post_turn_hook.wait();
            })),
        },
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

    // Blocked in after_spawn_before_register: handle not in map yet.
    enter.wait();

    // Wait for the explicit post-pump_turn signal (not merely host worker idle).
    pump_post_turn.wait();
    assert!(
        pump_reached_post_turn.load(Ordering::SeqCst),
        "pump must fire the post-pump_turn signal before registration wait"
    );
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

#[test]
fn shutdown_is_idempotent_and_leaves_zero_residuals() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "shutdown");
    let service = open_service(&path);
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

    let service = open_service(&path);
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
