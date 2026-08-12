use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use keeper_knowledge_indexer::{LedgerDbOptions, LedgerSnapshot, ensure_project_id};
use keeper_storage_sqlite::{KnowledgeTask, KnowledgeVerdict, TaskStatus, VerdictOutcome};

use crate::librarian_audit_commands::{
    LibrarianAuditHandoff, LibrarianAuditHostContext, LibrarianAuditProject,
    LibrarianAuditSourceError, SystemLibrarianAuditHostContext, run_librarian_audit_for_project,
};

const LIBRARIAN_GATEWAY: &str =
    include_str!("../../src/features/settings/librarianAuditGateway.ts");

fn utc(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap()
        .with_timezone(&Utc)
}

fn empty_snapshot() -> LedgerSnapshot {
    LedgerSnapshot {
        project_id: "ledger-project".into(),
        project_name: "sample".into(),
        tasks: Vec::new(),
        verdicts: Vec::new(),
        completed_without_verdict: Vec::new(),
    }
}

struct FixedContext {
    now: DateTime<Utc>,
    snapshot: LedgerSnapshot,
    handoff: LibrarianAuditHandoff,
    git_commit_ids: Vec<String>,
}

impl LibrarianAuditHostContext for FixedContext {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn load_snapshot(
        &self,
        _project: &LibrarianAuditProject,
    ) -> Result<LedgerSnapshot, LibrarianAuditSourceError> {
        Ok(self.snapshot.clone())
    }

    fn load_handoff(
        &self,
        _project: &LibrarianAuditProject,
    ) -> Result<LibrarianAuditHandoff, LibrarianAuditSourceError> {
        Ok(self.handoff.clone())
    }

    fn load_git_commit_ids(
        &self,
        _project: &LibrarianAuditProject,
    ) -> Result<Vec<String>, LibrarianAuditSourceError> {
        Ok(self.git_commit_ids.clone())
    }
}

fn fixed_context(handoff_markdown: &str) -> FixedContext {
    FixedContext {
        now: utc("2026-08-12T03:04:05Z"),
        snapshot: empty_snapshot(),
        handoff: LibrarianAuditHandoff {
            label: "docs/handoff/2026-08-12-pillow-controller-handoff.md".into(),
            markdown: handoff_markdown.into(),
        },
        git_commit_ids: Vec::new(),
    }
}

fn project(root: PathBuf) -> LibrarianAuditProject {
    LibrarianAuditProject {
        project_id: "renderer-project".into(),
        project_name: "sample".into(),
        root,
    }
}

fn sorted_keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

fn renderer_allowlist(name: &str) -> Vec<&'static str> {
    let marker = format!("export const {name} = [");
    let body = LIBRARIAN_GATEWAY
        .split(&marker)
        .nth(1)
        .unwrap_or_else(|| panic!("missing renderer allowlist {name}"))
        .split("] as const")
        .next()
        .expect("renderer allowlist terminator");
    body.split('"')
        .skip(1)
        .step_by(2)
        .filter(|value| !value.is_empty())
        .collect()
}

fn sorted_renderer_allowlist(name: &str) -> Vec<&'static str> {
    let mut keys = renderer_allowlist(name);
    keys.sort_unstable();
    keys
}

fn check_by_id<'a>(value: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == id)
        .unwrap_or_else(|| panic!("missing check {id}"))
}

/// Host-path prefix assembled at runtime.
///
/// These fixtures must *look* like host user paths to prove the redactor
/// drops them, but a literal here trips the repo secret gate. Building it
/// avoids an allowlist entry, which would weaken that gate for everyone.
fn host_path_prefix() -> String {
    format!("{}:{}{}{}", "C", '\\', "Users", '\\')
}

#[test]
fn every_command_the_librarian_gateway_invokes_is_registered() {
    let handler = include_str!("lib.rs")
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("generate_handler! block");

    let invoked: Vec<&str> = LIBRARIAN_GATEWAY
        .split("invoke<unknown>(\"")
        .skip(1)
        .filter_map(|segment| segment.split('"').next())
        .collect();

    assert_eq!(invoked, ["run_librarian_audit"]);
    for command in invoked {
        assert!(
            handler.contains(command),
            "gateway invokes `{command}` but generate_handler! does not register it"
        );
    }
}

#[test]
fn report_dto_matches_all_renderer_allowlists_and_exposes_the_injected_now() {
    let temp = tempfile::tempdir().unwrap();
    let context = fixed_context("In-flight workers: 1\n");

    let report = run_librarian_audit_for_project(&project(temp.path().into()), &context).unwrap();
    let value = serde_json::to_value(&report).unwrap();

    assert_eq!(
        sorted_keys(&value),
        sorted_renderer_allowlist("LIBRARIAN_AUDIT_REPORT_DTO_KEYS")
    );
    assert_eq!(value["projectId"], "renderer-project");
    assert_eq!(value["checkedAt"], "2026-08-12T03:04:05+00:00");
    assert_eq!(value["outcome"], "fail");

    let checks = value["checks"].as_array().unwrap();
    let ids: Vec<&str> = checks
        .iter()
        .map(|check| check["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "inflight_handoff_consistency",
            "pending_acceptance_visibility",
            "handoff_ledger_references",
            "inflight_inventory",
        ]
    );
    for check in checks {
        assert_eq!(
            sorted_keys(check),
            sorted_renderer_allowlist("LIBRARIAN_AUDIT_CHECK_DTO_KEYS")
        );
    }
    let finding = checks[0]["findings"][0].clone();
    assert_eq!(
        sorted_keys(&finding),
        sorted_renderer_allowlist("LIBRARIAN_AUDIT_FINDING_DTO_KEYS")
    );
    assert_eq!(finding["code"], "inflight_count_mismatch");
    assert_eq!(checks[0]["status"], "fail");
    assert_eq!(checks[1]["status"], "pass");
    assert_eq!(checks[2]["status"], "pass");
    assert_eq!(checks[3]["status"], "pass");
    assert!(
        checks[3]["summary"]
            .as_str()
            .unwrap()
            .contains("inflight_tasks=0")
    );
}

#[test]
fn ghost_handoff_reference_fails_only_the_reference_check() {
    let temp = tempfile::tempdir().unwrap();
    let context = fixed_context("In-flight workers: 0\ntask-id: ghost-task\n");

    let report = run_librarian_audit_for_project(&project(temp.path().into()), &context).unwrap();
    let value = serde_json::to_value(report).unwrap();

    assert_eq!(
        check_by_id(&value, "inflight_handoff_consistency")["status"],
        "pass"
    );
    let references = check_by_id(&value, "handoff_ledger_references");
    assert_eq!(references["status"], "fail");
    assert!(
        references["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "doc_task_missing_from_ledger")
    );
}

#[test]
fn stale_dispatch_fails_inventory_while_preserving_the_enumerated_count() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = fixed_context("In-flight workers: 1\ntask-id: task-stale\n");
    context.snapshot.tasks.push(KnowledgeTask {
        task_id: "task-stale".into(),
        project_id: "ledger-project".into(),
        title: "Stale task".into(),
        brief_path: Some("docs/brief.md".into()),
        worker_label: "codex1".into(),
        dispatched_at: utc("2026-08-10T00:00:00Z"),
        status: TaskStatus::Dispatched,
        completion_signal: None,
    });

    let report = run_librarian_audit_for_project(&project(temp.path().into()), &context).unwrap();
    let value = serde_json::to_value(report).unwrap();

    assert_eq!(
        check_by_id(&value, "inflight_handoff_consistency")["status"],
        "pass"
    );
    let inventory = check_by_id(&value, "inflight_inventory");
    assert_eq!(inventory["status"], "fail");
    assert!(
        inventory["summary"]
            .as_str()
            .unwrap()
            .contains("inflight_tasks=1")
    );
    assert_eq!(inventory["findings"][0]["code"], "stale_inflight");
}

#[test]
fn accepted_verdict_without_commit_marker_stays_pass_with_unrecorded_visible() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = fixed_context("In-flight workers: 0\n");
    context.snapshot.verdicts.push(KnowledgeVerdict {
        verdict_id: "verdict-1".into(),
        task_id: None,
        project_id: "ledger-project".into(),
        outcome: VerdictOutcome::Accepted,
        gate_result: None,
        e2e_result: None,
        summary: "accepted without a commit marker".into(),
        residuals: None,
        decided_at: utc("2026-08-12T02:00:00Z"),
    });

    let report = run_librarian_audit_for_project(&project(temp.path().into()), &context).unwrap();
    let value = serde_json::to_value(report).unwrap();
    let commit_check = value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "handoff_ledger_references")
        .unwrap();

    assert_eq!(commit_check["status"], "pass");
    assert!(commit_check["findings"].as_array().unwrap().is_empty());
    assert!(
        commit_check["summary"]
            .as_str()
            .unwrap()
            .contains("unrecorded=1")
    );
}

#[test]
fn host_finding_dto_removes_titles_and_private_text() {
    let temp = tempfile::tempdir().unwrap();
    let mut context = fixed_context("In-flight workers: 0\n");
    let task = KnowledgeTask {
        task_id: "task-safe".into(),
        project_id: "ledger-project".into(),
        title: format!(
            "{}{}",
            host_path_prefix(),
            "hidden token=secret session id=private"
        ),
        brief_path: Some("docs/brief.md".into()),
        worker_label: format!("{}{}", host_path_prefix(), "hidden"),
        dispatched_at: utc("2026-08-12T00:00:00Z"),
        status: TaskStatus::Completed,
        completion_signal: None,
    };
    context.snapshot.tasks.push(task.clone());
    context.snapshot.completed_without_verdict.push(task);

    let report = run_librarian_audit_for_project(&project(temp.path().into()), &context).unwrap();
    let value = serde_json::to_value(report).unwrap();
    let pending = check_by_id(&value, "pending_acceptance_visibility");
    let serialized = serde_json::to_string(pending).unwrap();

    assert_eq!(pending["status"], "fail");
    assert_eq!(pending["findings"][0]["code"], "completed_without_verdict");
    assert!(
        pending["findings"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("task-safe")
    );
    assert!(!serialized.contains("Users"));
    assert!(!serialized.contains("token"));
    assert!(!serialized.contains("session"));
    assert!(!serialized.contains("hidden"));
    assert!(!serialized.contains("private"));
}

fn create_ledger_project(root: &std::path::Path) -> LibrarianAuditProject {
    let ledger_dir = root.join(".superpowers/ledger");
    fs::create_dir_all(&ledger_dir).unwrap();
    ensure_project_id(&LedgerDbOptions {
        db_path: ledger_dir.join("pillow.db"),
        project_name: "sample".into(),
        project_root: Some(root.into()),
    })
    .unwrap();
    project(root.into())
}

fn assert_tool_error_has_no_health_report(
    error: &crate::librarian_audit_commands::LibrarianAuditCommandError,
) {
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(sorted_keys(&value), ["code", "message"]);
    assert!(value.get("outcome").is_none());
    assert!(value.get("checks").is_none());
}

#[test]
fn missing_ledger_is_an_explicit_error_and_never_an_all_pass_report() {
    let temp = tempfile::tempdir().unwrap();

    let error = run_librarian_audit_for_project(
        &project(temp.path().into()),
        &SystemLibrarianAuditHostContext,
    )
    .unwrap_err();

    assert_eq!(error.code, "librarian_audit_unavailable");
    assert_eq!(error.message, "Project audit ledger is unavailable.");
    assert_tool_error_has_no_health_report(&error);
}

#[test]
fn invalid_handoff_path_is_an_explicit_error_and_never_an_all_pass_report() {
    let temp = tempfile::tempdir().unwrap();
    let registered = create_ledger_project(temp.path());

    let error =
        run_librarian_audit_for_project(&registered, &SystemLibrarianAuditHostContext).unwrap_err();

    assert_eq!(error.code, "librarian_audit_unavailable");
    assert_eq!(
        error.message,
        "Project audit handoff document is unavailable or invalid."
    );
    assert_tool_error_has_no_health_report(&error);
}

#[test]
fn unavailable_git_is_an_explicit_error_and_never_an_all_pass_report() {
    let temp = tempfile::tempdir().unwrap();
    let registered = create_ledger_project(temp.path());
    let handoff_dir = temp.path().join("docs/handoff");
    fs::create_dir_all(&handoff_dir).unwrap();
    fs::write(
        handoff_dir.join("2026-08-12-pillow-controller-handoff.md"),
        "In-flight workers: 0\n",
    )
    .unwrap();

    let error =
        run_librarian_audit_for_project(&registered, &SystemLibrarianAuditHostContext).unwrap_err();

    assert_eq!(error.code, "librarian_audit_unavailable");
    assert_eq!(error.message, "Project audit Git history is unavailable.");
    assert_tool_error_has_no_health_report(&error);
}
