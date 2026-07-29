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

/// Illegal resulting status must roll back the event insert: a second
/// connection must see neither the event nor a status change.
#[test]
fn illegal_status_transition_rolls_back_event_on_second_connection() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
    let project = seed_project(&projects, &temp, "illegal-status");
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
            user_message: "pending only".into(),
        })
        .unwrap();
    assert_eq!(run.status, RunStatus::Pending);

    let err = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"skip running"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap_err();
    assert!(
        matches!(err, SessionError::IllegalTransition { .. }),
        "Pending -> Completed must fail: {err:?}"
    );

    // Second connection must observe neither the event nor a status change.
    let peer = SqliteConversationRepository::open(&db).unwrap();
    let timeline = peer.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs[0].status, RunStatus::Pending);
    assert!(
        timeline.events.is_empty(),
        "illegal transition must not leave a durable turn_completed event: {:?}",
        timeline.events
    );

    // Legal path still works after the failed attempt (sequence 0 free).
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"ok"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs[0].status, RunStatus::Running);
    assert_eq!(timeline.events.len(), 1);
}

/// Exact idempotent replay against an already-matching terminal status
/// remains successful and does not error.
#[test]
fn idempotent_terminal_replay_against_matching_status_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "idempotent-terminal");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "replay".into(),
        })
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"r"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 1,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"done"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap();

    let replay = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 1,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"ignored"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap();
    assert_eq!(replay.sequence, 1);
    assert_eq!(
        sessions.load_timeline(conversation.id).unwrap().runs[0].status,
        RunStatus::Completed
    );
}

/// Seq-0 run_started then seq-0 turn_completed is rejected; both connections
/// still see the original event/status (no wrong terminal advancement).
#[test]
fn conflicting_kind_replay_rejected_leaves_status_on_second_connection() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let projects = keeper_storage_sqlite::SqliteProjectRepository::open(&db).unwrap();
    let project = seed_project(&projects, &temp, "conflict-kind");
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
            user_message: "conflict".into(),
        })
        .unwrap();
    let original = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"first"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    let conflict = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"should not win"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap_err();
    assert_eq!(
        conflict,
        SessionError::ConflictingEventKind {
            run_id: run.id,
            sequence: 0,
            persisted: RuntimeEventKind::RunStarted,
            got: RuntimeEventKind::TurnCompleted,
        }
    );
    for repo in [&sessions, &SqliteConversationRepository::open(&db).unwrap()] {
        let timeline = repo.load_timeline(conversation.id).unwrap();
        assert_eq!(timeline.runs[0].status, RunStatus::Running);
        assert_eq!(timeline.events, vec![original.clone()]);
    }
}

/// Same-kind replay with a different payload: first durable payload wins.
#[test]
fn same_kind_replay_first_payload_wins() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "first-write-wins");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "payload".into(),
        })
        .unwrap();
    let first = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"original"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    let replay = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"replacement"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    assert_eq!(replay, first);
    assert_eq!(replay.payload_json, r#"{"prompt_preview":"original"}"#);
    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs[0].status, RunStatus::Running);
    assert_eq!(timeline.events.len(), 1);
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
