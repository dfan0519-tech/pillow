use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use keeper_session::{ConversationId, ConversationRepository, RunId, RunStatus};
use keeper_storage_sqlite::SqliteConversationRepository;
use uuid::Uuid;

use crate::conversation_events::RecordingEmitter;
use crate::conversation_service::ConversationService;

use super::support::{open_service, seed_project, temp_db};

#[test]
fn projects_retain_isolated_conversation_lists() {
    let (_temp, path) = temp_db();
    let a = seed_project(&path, "proj-a");
    let b = seed_project(&path, "proj-b");
    let service = open_service(&path);

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
fn sequential_turns_return_active_run_count_to_zero() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "seq");
    let service = open_service(&path);
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

/// No-delay sequential stress: many turns back-to-back with fixture delays
/// off, each leaving zero residuals (active / workers / in-flight).
#[test]
fn no_delay_sequential_stress_clears_all_residuals() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "stress-seq");
    let service = open_service(&path);
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

#[test]
fn selecting_another_conversation_does_not_stop_active_run() {
    let (_temp, path) = temp_db();
    let project = seed_project(&path, "bg");
    let emitter = Arc::new(RecordingEmitter::new());
    let service = ConversationService::open(&path, Arc::clone(&emitter) as Arc<_>, false).unwrap();
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
