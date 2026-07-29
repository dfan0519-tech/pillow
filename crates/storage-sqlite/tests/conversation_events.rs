//! Event sequence, concurrency and multi-run timeline regression tests.

mod common;

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use common::{open_pair, seed_project};
use keeper_session::{
    ConversationRepository, NewConversation, NewRun, NewRunEvent, RunStatus, RuntimeEventKind,
    SessionError,
};
use keeper_storage_sqlite::{SqliteConversationRepository, SqliteProjectRepository};
use rusqlite::Connection;

#[test]
fn append_event_orders_by_sequence_and_duplicate_replay_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "events-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "stream please".into(),
        })
        .unwrap();

    let first = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"ok":true}"#.into(),
        })
        .unwrap();
    let second = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 1,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: r#"{"text":"hello"}"#.into(),
        })
        .unwrap();
    let third = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 2,
            kind: RuntimeEventKind::Heartbeat,
            payload_json: "{}".into(),
        })
        .unwrap();

    let replay = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 1,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: r#"{"text":"should not replace"}"#.into(),
        })
        .unwrap();

    assert_eq!(replay, second);
    assert_eq!(replay.kind, RuntimeEventKind::AssistantDelta);
    assert_eq!(replay.payload_json, r#"{"text":"hello"}"#);

    // Non-latest exact replay remains idempotent and does not replace payload.
    let replay_first = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: RuntimeEventKind::Heartbeat,
            payload_json: r#"{"ignored":true}"#.into(),
        })
        .unwrap();
    assert_eq!(replay_first, first);

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(
        timeline
            .events
            .iter()
            .map(|e| e.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(timeline.events[0], first);
    assert_eq!(timeline.events[1], second);
    assert_eq!(timeline.events[2], third);
    assert_eq!(timeline.events.len(), 3);
}

#[test]
fn append_event_rejects_gaps_backwards_and_out_of_order() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "mono-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "monotonic".into(),
        })
        .unwrap();

    // First event must be sequence 0 (gap at the start).
    let skip_zero = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 1,
            kind: RuntimeEventKind::RunStarted,
            payload_json: "{}".into(),
        })
        .unwrap_err();
    assert_eq!(
        skip_zero,
        SessionError::InvalidSequence {
            run_id: run.id,
            expected: 0,
            got: 1,
        }
    );

    sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: "{}".into(),
        })
        .unwrap();

    // Gap after established head.
    let gap = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 3,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: "{}".into(),
        })
        .unwrap_err();
    assert_eq!(
        gap,
        SessionError::InvalidSequence {
            run_id: run.id,
            expected: 1,
            got: 3,
        }
    );

    sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 1,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: r#"{"text":"a"}"#.into(),
        })
        .unwrap();

    // Existing earlier sequence: exact duplicate replay (not a backwards write).
    let replay = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 0,
            kind: RuntimeEventKind::Heartbeat,
            payload_json: "{}".into(),
        })
        .unwrap();
    assert_eq!(replay.sequence, 0);
    assert_eq!(replay.kind, RuntimeEventKind::RunStarted);

    // Out-of-order new insert beyond next expected.
    let out_of_order = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: 5,
            kind: RuntimeEventKind::Heartbeat,
            payload_json: "{}".into(),
        })
        .unwrap_err();
    assert_eq!(
        out_of_order,
        SessionError::InvalidSequence {
            run_id: run.id,
            expected: 2,
            got: 5,
        }
    );

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(
        timeline
            .events
            .iter()
            .map(|e| e.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn append_event_rejects_sequence_above_i64_max() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "range-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "range".into(),
        })
        .unwrap();

    let overflow = (i64::MAX as u64).saturating_add(1);
    assert!(overflow > i64::MAX as u64);

    let error = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: overflow,
            kind: RuntimeEventKind::RunStarted,
            payload_json: "{}".into(),
        })
        .unwrap_err();
    assert_eq!(error, SessionError::SequenceOutOfRange(overflow));
}

#[test]
fn append_event_accepts_sequence_at_i64_max_boundary_when_next() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "boundary-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "boundary".into(),
        })
        .unwrap();
    let db = temp.path().join("keeper.db");

    // Seed max-1 via SQL so repository next-expected is i64::MAX without
    // materializing a full 0..max chain.
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute(
                "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
                 VALUES (?1, ?2, ?3, 'heartbeat', '{}', ?4)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    run.id.0.to_string(),
                    i64::MAX - 1,
                    "2026-07-30T00:00:00+00:00",
                ],
            )
            .unwrap();
    }

    let at_max = i64::MAX as u64;
    let recorded = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: at_max,
            kind: RuntimeEventKind::TurnCompleted,
            payload_json: r#"{"boundary":true}"#.into(),
        })
        .unwrap();
    assert_eq!(recorded.sequence, at_max);
    assert_eq!(recorded.kind, RuntimeEventKind::TurnCompleted);

    // Next after i64::MAX cannot be stored as SQLite INTEGER.
    let overflow = at_max + 1;
    let error = sessions
        .append_event(NewRunEvent {
            run_id: run.id,
            sequence: overflow,
            kind: RuntimeEventKind::Heartbeat,
            payload_json: "{}".into(),
        })
        .unwrap_err();
    assert_eq!(error, SessionError::SequenceOutOfRange(overflow));
}

#[test]
fn append_event_duplicate_replay_is_safe_across_two_connections() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = seed_project(&projects, &temp, "dual-conn");
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
            user_message: "race".into(),
        })
        .unwrap();
    drop(sessions);
    drop(projects);

    let barrier = Arc::new(Barrier::new(2));
    let run_id = run.id;
    let db_a = db.clone();
    let db_b = db.clone();
    let barrier_a = Arc::clone(&barrier);
    let barrier_b = Arc::clone(&barrier);

    let handle_a = thread::spawn(move || {
        let repo = SqliteConversationRepository::open(&db_a).unwrap();
        barrier_a.wait();
        repo.append_event(NewRunEvent {
            run_id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"source":"a"}"#.into(),
        })
    });
    let handle_b = thread::spawn(move || {
        let repo = SqliteConversationRepository::open(&db_b).unwrap();
        barrier_b.wait();
        repo.append_event(NewRunEvent {
            run_id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"source":"b"}"#.into(),
        })
    });

    let result_a = handle_a.join().expect("thread a panicked");
    let result_b = handle_b.join().expect("thread b panicked");

    let event_a = result_a.expect("connection A must not fail with SQLITE_BUSY");
    let event_b = result_b.expect("connection B must not fail with SQLITE_BUSY");

    assert_eq!(event_a, event_b);
    assert_eq!(event_a.sequence, 0);
    assert_eq!(event_a.kind, RuntimeEventKind::RunStarted);

    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.events.len(), 1);
    assert_eq!(timeline.events[0], event_a);
}

/// Old two-transaction crash window: `append_event(turn_completed)` alone can
/// leave a durable terminal event while the run is still `Running`. The atomic
/// API must never expose that intermediate state to a second connection.
#[test]
fn append_event_with_status_is_atomic_against_crash_window() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let projects = SqliteProjectRepository::open(&db).unwrap();
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

#[test]
fn load_timeline_orders_events_by_owning_run_then_sequence() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "timeline-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();

    let run_a = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "first run".into(),
        })
        .unwrap();
    // Ensure distinct created_at when clocks are coarse.
    thread::sleep(Duration::from_millis(5));
    let run_b = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "second run".into(),
        })
        .unwrap();

    // Later run gets lower sequences so a global sequence ORDER BY would
    // interleave incorrectly if run conversation order is ignored.
    sessions
        .append_event(NewRunEvent {
            run_id: run_b.id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"run":"b","seq":0}"#.into(),
        })
        .unwrap();
    sessions
        .append_event(NewRunEvent {
            run_id: run_b.id,
            sequence: 1,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: r#"{"run":"b","seq":1}"#.into(),
        })
        .unwrap();
    sessions
        .append_event(NewRunEvent {
            run_id: run_a.id,
            sequence: 0,
            kind: RuntimeEventKind::RunStarted,
            payload_json: r#"{"run":"a","seq":0}"#.into(),
        })
        .unwrap();
    sessions
        .append_event(NewRunEvent {
            run_id: run_a.id,
            sequence: 1,
            kind: RuntimeEventKind::AssistantDelta,
            payload_json: r#"{"run":"a","seq":1}"#.into(),
        })
        .unwrap();
    sessions
        .append_event(NewRunEvent {
            run_id: run_a.id,
            sequence: 2,
            kind: RuntimeEventKind::TurnCompleted,
            payload_json: r#"{"run":"a","seq":2}"#.into(),
        })
        .unwrap();

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(
        timeline.runs.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![run_a.id, run_b.id]
    );

    let ordered: Vec<(keeper_session::RunId, u64)> = timeline
        .events
        .iter()
        .map(|e| (e.run_id, e.sequence))
        .collect();
    assert_eq!(
        ordered,
        vec![
            (run_a.id, 0),
            (run_a.id, 1),
            (run_a.id, 2),
            (run_b.id, 0),
            (run_b.id, 1),
        ],
        "events must be grouped by run conversation order, then sequence"
    );
}
