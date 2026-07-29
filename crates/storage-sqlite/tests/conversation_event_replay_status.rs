//! Same-kind replay must not reapply event-derived status after later states.

mod common;

use common::{open_pair, seed_project};
use keeper_session::{
    ConversationRepository, NewConversation, NewRun, NewRunEvent, RunStatus, RuntimeEventKind,
};

/// Early same-kind `run_started` replay must not reapply Running after the run
/// has advanced (Cancelling, Completed, …). Status stays put; first payload wins.
#[test]
fn same_kind_run_started_replay_ignores_status_after_later_states() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "replay-no-status");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();

    // Path A: Running -> Cancelling, then replay early run_started.
    let run_a = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "to cancelling".into(),
        })
        .unwrap();
    let first_a = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run_a.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"original"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    sessions
        .transition_run(run_a.id, RunStatus::Cancelling)
        .unwrap();
    let replay_a = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run_a.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"replacement"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    assert_eq!(replay_a, first_a);
    assert_eq!(replay_a.payload_json, r#"{"prompt_preview":"original"}"#);
    assert_eq!(
        sessions
            .load_timeline(conversation.id)
            .unwrap()
            .runs
            .iter()
            .find(|r| r.id == run_a.id)
            .unwrap()
            .status,
        RunStatus::Cancelling,
        "run_started replay must not reapply Running over Cancelling"
    );

    // Path B: advance to Completed, then replay early run_started.
    let run_b = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "to completed".into(),
        })
        .unwrap();
    let first_b = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run_b.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"orig-b"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run_b.id,
                sequence: 1,
                kind: RuntimeEventKind::TurnCompleted,
                payload_json: r#"{"summary":"done"}"#.into(),
            },
            Some(RunStatus::Completed),
        )
        .unwrap();
    let replay_b = sessions
        .append_event_with_status(
            NewRunEvent {
                run_id: run_b.id,
                sequence: 0,
                kind: RuntimeEventKind::RunStarted,
                payload_json: r#"{"prompt_preview":"replacement-b"}"#.into(),
            },
            Some(RunStatus::Running),
        )
        .unwrap();
    assert_eq!(replay_b, first_b);
    assert_eq!(
        sessions
            .load_timeline(conversation.id)
            .unwrap()
            .runs
            .iter()
            .find(|r| r.id == run_b.id)
            .unwrap()
            .status,
        RunStatus::Completed,
        "run_started replay must not touch Completed"
    );
}
