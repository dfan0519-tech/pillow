//! Atomic append_event_with_status crash-window and recovery regression tests.

mod common;

use common::{open_pair, seed_project};
use keeper_session::{
    ConversationRepository, NewConversation, NewRun, NewRunEvent, RunStatus, RuntimeEventKind,
    SessionError,
};
use keeper_storage_sqlite::SqliteConversationRepository;

/// Old two-transaction crash window: `append_event(turn_completed)` alone can
/// leave a durable terminal event while the run is still `Running`. The atomic
/// API must never expose that intermediate state to a second connection.
#[test]
fn append_event_with_status_is_atomic_against_crash_window() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
    let project = seed_project(&projects, &temp, "atomic-status");
    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "crash window".into(),
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

    // Demonstrate the old crash window on the non-atomic path.
    sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 1,
            kind: RuntimeEventKind::TurnCompleted,
            payload_json: r#"{"summary":"done"}"#.into(),
        })
        .unwrap();
    {
        let peer = SqliteConversationRepository::open(&db).unwrap();
        let timeline = peer.load_timeline(conversation.id).unwrap();
        assert!(
            timeline
                .events
                .iter()
                .any(|e| e.kind == RuntimeEventKind::TurnCompleted)
        );
        assert_eq!(
            timeline.runs[0].status,
            RunStatus::Running,
            "two-transaction path can leave turn_completed while still Running"
        );
    }

    // Reset with a fresh run and prove the atomic API closes the window.
    let run2 = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "atomic".into(),
        })
        .unwrap();
    sessions
        .transition_run(run2.id, RunStatus::Running)
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run2.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"y"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    // Idempotent same-status after Running is a no-op for transition.
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run2.id,
                sequence: 1,
                kind: RuntimeEventKind::AssistantDelta,
                payload_json: r#"{"text":"hi"}"#.into(),
            },
            None,
        )
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run2.id,
                sequence: 2,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"done"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap();

    let peer = SqliteConversationRepository::open(&db).unwrap();
    let timeline = peer.load_timeline(conversation.id).unwrap();
    let run2_row = timeline
        .runs
        .iter()
        .find(|r| r.id == run2.id)
        .expect("run2");
    assert!(
        timeline
            .events
            .iter()
            .any(|e| e.run_id == run2.id && e.kind == RuntimeEventKind::TurnCompleted)
    );
    assert_eq!(
        run2_row.status,
        RunStatus::Completed,
        "atomic API must commit event and Completed together"
    );

    // Idempotent replay of terminal event keeps Completed.
    let replay = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run2.id,
                sequence: 2,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"ignored"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap();
    assert_eq!(replay.sequence, 2);
    assert_eq!(
        sessions.load_timeline(conversation.id).unwrap().runs
            [timeline.runs.iter().position(|r| r.id == run2.id).unwrap()]
        .status,
        RunStatus::Completed
    );
}

#[test]
fn append_event_with_status_cancelled_from_running_walks_cancelling() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "cancel-atomic");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "cancel".into(),
        })
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"c"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 1,
                kind: RuntimeEventKind::RunCancelled,
                payload_json: r#"{"reason":"user"}"#.into(),
            },
            Some(RunStatus::Cancelled),
        )
        .unwrap();

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs[0].status, RunStatus::Cancelled);
}

#[test]
fn append_event_with_status_preserves_sequence_and_i64_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "bounds-atomic");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "bounds".into(),
        })
        .unwrap();

    let gap = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 1,
                kind: RuntimeEventKind::RunStarted,
                payload_json: "{}".into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap_err();
    assert!(matches!(
        gap,
        SessionError::InvalidSequence {
            expected: 0,
            got: 1,
            ..
        }
    ));

    let overflow = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: (i64::MAX as u64) + 1,
                kind: RuntimeEventKind::RunStarted,
                payload_json: "{}".into(),
            },
            None,
        )
        .unwrap_err();
    assert!(matches!(overflow, SessionError::SequenceOutOfRange(_)));
}
