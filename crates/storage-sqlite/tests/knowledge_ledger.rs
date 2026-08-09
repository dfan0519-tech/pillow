//! K0d dispatch / acceptance ledger: migration, full lifecycle, isolation,
//! privacy red lines, restart recovery.

use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    DispatchTask, DocumentRole, DocumentStatus, KnowledgeError, RecordVerdict, RegisterDocument,
    RegisteredBy, SetTaskStatus, SqliteKnowledgeRepository, SqliteProjectRepository, TaskStatus,
    VerdictOutcome,
};
use rusqlite::Connection;

fn digest(seed: u8) -> String {
    format!("sha256:{seed:02x}{}", "0".repeat(62))
}

fn open_knowledge(temp: &tempfile::TempDir) -> SqliteKnowledgeRepository {
    SqliteKnowledgeRepository::open(temp.path().join("keeper.db")).expect("open knowledge")
}

fn seed_project(temp: &tempfile::TempDir, name: &str) -> String {
    let projects = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let root = temp.path().join(name);
    std::fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    project.id.0.to_string()
}

// ---------------------------------------------------------------------------
// 1. Migration 0011 idempotent
// ---------------------------------------------------------------------------

#[test]
fn migration_0011_is_idempotent_and_creates_ledger_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");

    let first = SqliteKnowledgeRepository::open(&db).expect("first open");
    drop(first);
    let second = SqliteKnowledgeRepository::open(&db).expect("second open");
    drop(second);

    let connection = Connection::open(&db).unwrap();
    for table in [
        "knowledge_tasks",
        "knowledge_verdicts",
        "knowledge_task_documents",
    ] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "table {table} must exist exactly once");
    }

    let mut statement = connection
        .prepare("PRAGMA table_info(knowledge_tasks)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    for required in [
        "task_id",
        "project_id",
        "title",
        "brief_path",
        "worker_label",
        "dispatched_at",
        "status",
        "completion_signal",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "missing column {required}"
        );
    }

    let mut statement = connection
        .prepare("PRAGMA table_info(knowledge_verdicts)")
        .unwrap();
    let vcols: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    for required in [
        "verdict_id",
        "task_id",
        "project_id",
        "outcome",
        "gate_result",
        "e2e_result",
        "summary",
        "residuals",
        "decided_at",
    ] {
        assert!(
            vcols.iter().any(|c| c == required),
            "missing verdict column {required}"
        );
    }

    // Illegal task status rejected by CHECK.
    let err = connection.execute(
        "INSERT INTO knowledge_tasks (
            task_id, project_id, title, brief_path, worker_label,
            dispatched_at, status, completion_signal
         ) VALUES ('t1', 'p1', 'x', NULL, 'grok3', 't', 'running', NULL)",
        [],
    );
    assert!(err.is_err(), "illegal task status must fail CHECK");

    // Illegal verdict outcome rejected by CHECK.
    let err = connection.execute(
        "INSERT INTO knowledge_verdicts (
            verdict_id, task_id, project_id, outcome, gate_result,
            e2e_result, summary, residuals, decided_at
         ) VALUES ('v1', NULL, 'p1', 'ok', NULL, NULL, 's', NULL, 't')",
        [],
    );
    assert!(err.is_err(), "illegal verdict outcome must fail CHECK");
}

// ---------------------------------------------------------------------------
// 2. Dispatch → complete → accept full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn dispatch_complete_accept_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let task = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "K0d ledger implementation".into(),
            brief_path: Some("docs/execution/k0d-brief.md".into()),
            worker_label: "grok3".into(),
        })
        .expect("dispatch");
    assert_eq!(task.status, TaskStatus::Dispatched);
    assert_eq!(task.worker_label, "grok3");
    assert_eq!(
        task.brief_path.as_deref(),
        Some("docs/execution/k0d-brief.md")
    );

    // Attach a produced document.
    let doc = repo
        .register(RegisterDocument {
            project_id: project_id.clone(),
            path: "docs/execution/2026-08-09-k0d-ledger-report.md".into(),
            role: DocumentRole::Report,
            status: Some(DocumentStatus::Evidence),
            content_digest: digest(1),
            registered_by: RegisteredBy::Worker,
            source_run_id: None,
        })
        .unwrap();
    repo.attach_task_document(&task.task_id, &doc.document_id)
        .unwrap();
    let docs = repo.list_task_documents(&task.task_id).unwrap();
    assert_eq!(docs, vec![doc.document_id.clone()]);

    let completed = repo
        .set_task_status(SetTaskStatus {
            task_id: task.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("K0D-LEDGER-COMPLETE".into()),
        })
        .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(
        completed.completion_signal.as_deref(),
        Some("K0D-LEDGER-COMPLETE")
    );

    let verdict = repo
        .record_verdict(RecordVerdict {
            project_id: project_id.clone(),
            task_id: Some(task.task_id.clone()),
            outcome: VerdictOutcome::Accepted,
            gate_result: Some("exit 0".into()),
            e2e_result: Some("not-run".into()),
            summary: "Ledger tables + renderer cover dispatch/acceptance byproduct.".into(),
            residuals: Some("Wire controller write path next.".into()),
        })
        .unwrap();
    assert_eq!(verdict.outcome, VerdictOutcome::Accepted);
    assert_eq!(verdict.task_id.as_deref(), Some(task.task_id.as_str()));
    assert!(verdict.summary.contains("Ledger tables"));

    // After acceptance, completed-without-verdict is empty.
    let missing = repo.list_completed_without_verdict(&project_id).unwrap();
    assert!(missing.is_empty());

    let loaded = repo.get_task(&task.task_id).unwrap().unwrap();
    assert_eq!(loaded.status, TaskStatus::Completed);
    let v = repo.get_verdict(&verdict.verdict_id).unwrap().unwrap();
    assert_eq!(v.summary, verdict.summary);
}

// ---------------------------------------------------------------------------
// 3. Completed without verdict listed (core detection)
// ---------------------------------------------------------------------------

#[test]
fn completed_without_verdict_is_detectable() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let orphan = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "Forgotten acceptance".into(),
            brief_path: None,
            worker_label: "grok3".into(),
        })
        .unwrap();
    repo.set_task_status(SetTaskStatus {
        task_id: orphan.task_id.clone(),
        status: TaskStatus::Completed,
        completion_signal: Some("WORKER-DONE".into()),
    })
    .unwrap();

    let accepted = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "Properly accepted".into(),
            brief_path: None,
            worker_label: "codex1".into(),
        })
        .unwrap();
    repo.set_task_status(SetTaskStatus {
        task_id: accepted.task_id.clone(),
        status: TaskStatus::Completed,
        completion_signal: None,
    })
    .unwrap();
    repo.record_verdict(RecordVerdict {
        project_id: project_id.clone(),
        task_id: Some(accepted.task_id.clone()),
        outcome: VerdictOutcome::Accepted,
        gate_result: None,
        e2e_result: None,
        summary: "Fine.".into(),
        residuals: None,
    })
    .unwrap();

    // Still dispatched — not completed, so not in the gap list.
    repo.dispatch_task(DispatchTask {
        project_id: project_id.clone(),
        title: "In flight".into(),
        brief_path: None,
        worker_label: "grok2".into(),
    })
    .unwrap();

    let missing = repo.list_completed_without_verdict(&project_id).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].task_id, orphan.task_id);
    assert_eq!(missing[0].title, "Forgotten acceptance");
}

// ---------------------------------------------------------------------------
// 4. Verdict without task_id (controller-self work)
// ---------------------------------------------------------------------------

#[test]
fn verdict_may_have_no_task_id() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let verdict = repo
        .record_verdict(RecordVerdict {
            project_id: project_id.clone(),
            task_id: None,
            outcome: VerdictOutcome::Partial,
            gate_result: Some("exit 0".into()),
            e2e_result: Some("10/10".into()),
            summary: "Controller fixed numbering without a worker.".into(),
            residuals: Some("still need dispatcher hook".into()),
        })
        .unwrap();
    assert!(verdict.task_id.is_none());
    assert_eq!(verdict.outcome, VerdictOutcome::Partial);

    let listed = repo.list_verdicts_by_project(&project_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].task_id.is_none());
}

// ---------------------------------------------------------------------------
// 5. Illegal status / outcome rejected
// ---------------------------------------------------------------------------

#[test]
fn illegal_task_status_and_verdict_outcome_rejected() {
    assert!(TaskStatus::parse("running").is_err());
    assert!(TaskStatus::parse("Dispatched").is_err());
    assert!(VerdictOutcome::parse("ok").is_err());
    assert!(VerdictOutcome::parse("Accepted").is_err());

    let err = TaskStatus::parse("live").unwrap_err();
    assert_eq!(err.code(), "knowledge_invalid_task_status");
    let err = VerdictOutcome::parse("pass").unwrap_err();
    assert_eq!(err.code(), "knowledge_invalid_verdict_outcome");
}

// ---------------------------------------------------------------------------
// 6. Project isolation
// ---------------------------------------------------------------------------

#[test]
fn tasks_and_verdicts_isolated_by_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_a = seed_project(&temp, "proj-a");
    let project_b = seed_project(&temp, "proj-b");
    let repo = open_knowledge(&temp);

    let ta = repo
        .dispatch_task(DispatchTask {
            project_id: project_a.clone(),
            title: "A task".into(),
            brief_path: None,
            worker_label: "grok3".into(),
        })
        .unwrap();
    let tb = repo
        .dispatch_task(DispatchTask {
            project_id: project_b.clone(),
            title: "B task".into(),
            brief_path: None,
            worker_label: "grok2".into(),
        })
        .unwrap();

    repo.record_verdict(RecordVerdict {
        project_id: project_a.clone(),
        task_id: Some(ta.task_id.clone()),
        outcome: VerdictOutcome::Accepted,
        gate_result: None,
        e2e_result: None,
        summary: "A only".into(),
        residuals: None,
    })
    .unwrap();
    repo.record_verdict(RecordVerdict {
        project_id: project_b.clone(),
        task_id: Some(tb.task_id.clone()),
        outcome: VerdictOutcome::Rejected,
        gate_result: None,
        e2e_result: None,
        summary: "B only".into(),
        residuals: None,
    })
    .unwrap();

    let tasks_a = repo.list_tasks_by_project(&project_a).unwrap();
    assert_eq!(tasks_a.len(), 1);
    assert_eq!(tasks_a[0].title, "A task");
    assert!(!tasks_a.iter().any(|t| t.task_id == tb.task_id));

    let tasks_b = repo.list_tasks_by_project(&project_b).unwrap();
    assert_eq!(tasks_b.len(), 1);
    assert_eq!(tasks_b[0].title, "B task");

    let va = repo.list_verdicts_by_project(&project_a).unwrap();
    assert_eq!(va.len(), 1);
    assert_eq!(va[0].summary, "A only");
    let vb = repo.list_verdicts_by_project(&project_b).unwrap();
    assert_eq!(vb.len(), 1);
    assert_eq!(vb[0].summary, "B only");
}

// ---------------------------------------------------------------------------
// 7. Privacy red lines on write
// ---------------------------------------------------------------------------

/// Forbidden tokens assembled at runtime so the source never holds contiguous
/// secret-gate samples (lease / session / host user path).
fn privacy_samples() -> (String, String, String, String) {
    let lease = format!("{}-{}-{}-{}", "grok", "lease", "msij5g0v", "fc6b73");
    let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
    let win = format!("built at {}:\\{}\\someone\\.cargo", "C", "Users");
    let msys = format!("path /{}/{}/someone/.grok/bin", "c", "Users");
    (lease, session, win, msys)
}

#[test]
fn privacy_red_lines_reject_lease_session_and_host_paths() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);
    let (lease, session, win, msys) = privacy_samples();

    // Lease id as worker_label.
    let err = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "bad slot".into(),
            brief_path: None,
            worker_label: lease,
        })
        .unwrap_err();
    assert!(
        matches!(err, KnowledgeError::InvalidWorkerLabel { .. })
            || matches!(err, KnowledgeError::PrivacyViolation),
        "lease label must be rejected: {err:?}"
    );

    // Session id in title.
    let err = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: format!("run {session}"),
            brief_path: None,
            worker_label: "grok3".into(),
        })
        .unwrap_err();
    assert_eq!(err, KnowledgeError::PrivacyViolation);

    // Host user path in summary.
    let err = repo
        .record_verdict(RecordVerdict {
            project_id: project_id.clone(),
            task_id: None,
            outcome: VerdictOutcome::Accepted,
            gate_result: None,
            e2e_result: None,
            summary: win,
            residuals: None,
        })
        .unwrap_err();
    assert_eq!(err, KnowledgeError::PrivacyViolation);

    let err = repo
        .record_verdict(RecordVerdict {
            project_id: project_id.clone(),
            task_id: None,
            outcome: VerdictOutcome::Accepted,
            gate_result: None,
            e2e_result: None,
            summary: msys,
            residuals: None,
        })
        .unwrap_err();
    assert_eq!(err, KnowledgeError::PrivacyViolation);

    // Clean slot label accepted.
    let ok = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "clean dispatch".into(),
            brief_path: Some("docs/execution/ok-brief.md".into()),
            worker_label: "grok3".into(),
        })
        .unwrap();
    assert_eq!(ok.worker_label, "grok3");
}

// ---------------------------------------------------------------------------
// 8. Restart recovery
// ---------------------------------------------------------------------------

#[test]
fn restart_recovers_tasks_and_verdicts() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project_id = {
        let projects = SqliteProjectRepository::open(&db).unwrap();
        let root = temp.path().join("proj-restart");
        std::fs::create_dir_all(&root).unwrap();
        projects
            .create(CreateProject {
                name: "proj-restart".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap()
            .id
            .0
            .to_string()
    };

    let (task_id, verdict_id) = {
        let repo = SqliteKnowledgeRepository::open(&db).unwrap();
        let task = repo
            .dispatch_task(DispatchTask {
                project_id: project_id.clone(),
                title: "Survive restart".into(),
                brief_path: None,
                worker_label: "grok3".into(),
            })
            .unwrap();
        repo.set_task_status(SetTaskStatus {
            task_id: task.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("DONE".into()),
        })
        .unwrap();
        let verdict = repo
            .record_verdict(RecordVerdict {
                project_id: project_id.clone(),
                task_id: Some(task.task_id.clone()),
                outcome: VerdictOutcome::Accepted,
                gate_result: Some("exit 0".into()),
                e2e_result: None,
                summary: "Persists across reopen.".into(),
                residuals: None,
            })
            .unwrap();
        (task.task_id, verdict.verdict_id)
    };
    // Handle dropped: simulate process exit.

    let repo = SqliteKnowledgeRepository::open(&db).expect("reopen");
    let task = repo
        .get_task(&task_id)
        .unwrap()
        .expect("task after restart");
    assert_eq!(task.status, TaskStatus::Completed);
    assert_eq!(task.worker_label, "grok3");
    assert_eq!(task.completion_signal.as_deref(), Some("DONE"));

    let verdict = repo
        .get_verdict(&verdict_id)
        .unwrap()
        .expect("verdict after restart");
    assert_eq!(verdict.summary, "Persists across reopen.");
    assert_eq!(verdict.task_id.as_deref(), Some(task_id.as_str()));

    let missing = repo.list_completed_without_verdict(&project_id).unwrap();
    assert!(missing.is_empty());
}

// ---------------------------------------------------------------------------
// Bonus: status is never auto-migrated on verdict write
// ---------------------------------------------------------------------------

#[test]
fn recording_verdict_does_not_auto_change_task_status() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let task = repo
        .dispatch_task(DispatchTask {
            project_id: project_id.clone(),
            title: "Stay dispatched".into(),
            brief_path: None,
            worker_label: "grok3".into(),
        })
        .unwrap();
    // Controller records a verdict without explicitly completing the task.
    repo.record_verdict(RecordVerdict {
        project_id: project_id.clone(),
        task_id: Some(task.task_id.clone()),
        outcome: VerdictOutcome::Rejected,
        gate_result: None,
        e2e_result: None,
        summary: "Rejected while still dispatched.".into(),
        residuals: None,
    })
    .unwrap();

    let after = repo.get_task(&task.task_id).unwrap().unwrap();
    assert_eq!(
        after.status,
        TaskStatus::Dispatched,
        "verdict write must not auto-migrate task status"
    );
}
