mod common;

use common::{open_pair, seed_project, table_exists};
use keeper_session::{ConversationRepository, NewConversation, NewRun, RunStatus, SessionError};
use keeper_storage_sqlite::{
    ProjectStoreError, SqliteConversationRepository, SqliteProjectRepository,
};
use rusqlite::Connection;

/// True frozen Slice A database: only migration 0001, no conversation tables.
#[test]
fn migration_0001_project_data_remains_readable_after_0002() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project_id = uuid::Uuid::new_v4().to_string();
    let root = temp.path().join("legacy-project");
    std::fs::create_dir(&root).unwrap();
    let root_path = root.to_string_lossy().into_owned();
    let canonical = root.canonicalize().unwrap().to_string_lossy().into_owned();
    let now = "2026-07-30T00:00:00+00:00";

    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_projects.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO projects (
                    id, name, root_path, canonical_root_path, trust_state, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 'untrusted', ?5, ?5)",
                rusqlite::params![project_id, "Legacy Project", root_path, canonical, now],
            )
            .unwrap();

        assert!(table_exists(&connection, "projects"));
        assert!(
            !table_exists(&connection, "conversations"),
            "0002 tables must be absent before upgrade"
        );
        assert!(!table_exists(&connection, "messages"));
        assert!(!table_exists(&connection, "runs"));
        assert!(!table_exists(&connection, "run_events"));
    }

    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let listed = projects.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Legacy Project");
    assert_eq!(listed[0].id.0.to_string(), project_id);

    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: listed[0].id,
            title: Some("after migration".into()),
        })
        .unwrap();
    assert_eq!(conversation.project_id, listed[0].id);
    assert_eq!(projects.list().unwrap().len(), 1);
}

#[test]
fn conversations_are_isolated_by_project() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project_a = seed_project(&projects, &temp, "project-a");
    let project_b = seed_project(&projects, &temp, "project-b");

    let a1 = sessions
        .create_conversation(NewConversation {
            project_id: project_a.id,
            title: Some("A1".into()),
        })
        .unwrap();
    let a2 = sessions
        .create_conversation(NewConversation {
            project_id: project_a.id,
            title: Some("A2".into()),
        })
        .unwrap();
    let b1 = sessions
        .create_conversation(NewConversation {
            project_id: project_b.id,
            title: Some("B1".into()),
        })
        .unwrap();

    let listed_a = sessions.list_conversations(project_a.id).unwrap();
    let listed_b = sessions.list_conversations(project_b.id).unwrap();

    assert_eq!(listed_a.len(), 2);
    assert!(listed_a.iter().all(|c| c.project_id == project_a.id));
    assert_eq!(
        listed_a.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![a1.id, a2.id]
    );
    assert_eq!(listed_b, vec![b1]);
}

#[test]
fn create_conversation_requires_existing_project() {
    let temp = tempfile::tempdir().unwrap();
    let (_, sessions) = open_pair(&temp);
    let missing = keeper_project::ProjectId::new();

    let error = sessions
        .create_conversation(NewConversation {
            project_id: missing,
            title: None,
        })
        .unwrap_err();

    assert_eq!(error, SessionError::ProjectNotFound(missing));
}

#[test]
fn create_run_persists_user_message_and_pending_run() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "run-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("run".into()),
        })
        .unwrap();

    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "implement Task 1".into(),
        })
        .unwrap();

    assert_eq!(run.conversation_id, conversation.id);
    assert_eq!(run.status, RunStatus::Pending);

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.conversation.id, conversation.id);
    assert_eq!(timeline.messages.len(), 1);
    assert_eq!(timeline.messages[0].content, "implement Task 1");
    assert_eq!(timeline.messages[0].role.as_str(), "user");
    assert_eq!(timeline.runs, vec![run]);
    assert!(timeline.events.is_empty());
}

#[test]
fn illegal_terminal_transition_fails_without_partial_writes() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "terminal-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "finish me".into(),
        })
        .unwrap();

    let completed = sessions
        .transition_run(run.id, RunStatus::Running)
        .and_then(|r| sessions.transition_run(r.id, RunStatus::Completed))
        .unwrap();
    assert_eq!(completed.status, RunStatus::Completed);

    let error = sessions
        .transition_run(run.id, RunStatus::Failed)
        .unwrap_err();
    assert!(matches!(
        error,
        SessionError::IllegalTransition {
            from: RunStatus::Completed,
            to: RunStatus::Failed
        }
    ));

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    assert_eq!(timeline.runs.len(), 1);
    assert_eq!(timeline.runs[0].status, RunStatus::Completed);
    assert_eq!(timeline.runs[0].updated_at, completed.updated_at);
}

#[test]
fn mark_inflight_interrupted_restores_non_terminal_runs_on_restart() {
    let temp = tempfile::tempdir().unwrap();
    let (projects, sessions) = open_pair(&temp);
    let project = seed_project(&projects, &temp, "restart-project");
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: None,
        })
        .unwrap();

    let pending = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "pending".into(),
        })
        .unwrap();
    let running = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "running".into(),
        })
        .unwrap();
    sessions
        .transition_run(running.id, RunStatus::Running)
        .unwrap();
    let cancelling = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "cancelling".into(),
        })
        .unwrap();
    sessions
        .transition_run(cancelling.id, RunStatus::Running)
        .unwrap();
    sessions
        .transition_run(cancelling.id, RunStatus::Cancelling)
        .unwrap();
    let completed = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "done".into(),
        })
        .unwrap();
    sessions
        .transition_run(completed.id, RunStatus::Running)
        .unwrap();
    sessions
        .transition_run(completed.id, RunStatus::Completed)
        .unwrap();

    let sessions = SqliteConversationRepository::open(temp.path().join("keeper.db")).unwrap();
    let changed = sessions.mark_inflight_interrupted().unwrap();
    assert_eq!(changed, 3);

    let timeline = sessions.load_timeline(conversation.id).unwrap();
    let status_by_id: std::collections::HashMap<_, _> = timeline
        .runs
        .iter()
        .map(|run| (run.id, run.status))
        .collect();

    assert_eq!(status_by_id[&pending.id], RunStatus::Interrupted);
    assert_eq!(status_by_id[&running.id], RunStatus::Interrupted);
    assert_eq!(status_by_id[&cancelling.id], RunStatus::Interrupted);
    assert_eq!(status_by_id[&completed.id], RunStatus::Completed);

    assert_eq!(sessions.mark_inflight_interrupted().unwrap(), 0);
}

#[test]
fn transition_run_rejects_unknown_run() {
    let temp = tempfile::tempdir().unwrap();
    let (_, sessions) = open_pair(&temp);
    let missing = keeper_session::RunId::new();
    let error = sessions
        .transition_run(missing, RunStatus::Running)
        .unwrap_err();
    assert_eq!(error, SessionError::RunNotFound(missing));
}

#[test]
fn existing_project_repository_errors_remain_available() {
    let _ = ProjectStoreError::LockPoisoned;
}
