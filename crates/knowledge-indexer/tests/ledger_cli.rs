//! K0e acceptance: ledger CLI write path + stale detection.
//!
//! Covers:
//! 1. dispatch → complete → verdict round-trip
//! 2. ledger status lists in-flight / pending acceptance
//! 3. stale detection cases 1/2/3
//! 4. negative control (consistent doc not flagged)
//! 5. privacy: output free of lease / session / host paths
//! 6. non-interactive: library + CLI helpers never read stdin

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use keeper_knowledge_indexer::{
    CompleteInput, DispatchInput, LedgerDbOptions, VerdictInput, check_stale, dispatch_task,
    format_ledger_status, format_stale_report, load_snapshot, record_verdict, set_task_status,
};
use keeper_storage_sqlite::{TaskStatus, VerdictOutcome};

fn opts(temp: &tempfile::TempDir, name: &str) -> LedgerDbOptions {
    LedgerDbOptions {
        db_path: temp.path().join("k0e.db"),
        project_name: name.into(),
        project_root: Some(temp.path().to_path_buf()),
    }
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

/// Forbidden needles assembled at runtime so sources never hold contiguous
/// secret-gate samples (aligned with K0d / check-secrets.mjs).
fn privacy_needles() -> (String, String, String, String, String) {
    let lease = format!("{}-{}-{}", "grok", "lease", "abcdef0123456789");
    let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
    let win = format!("{}:\\{}\\{}", "C", "Users", "someone");
    let msys = format!("/{}/{}/{}", "c", "Users", "someone");
    let cyg = format!("/cygdrive/{}/{}/{}", "c", "Users", "someone");
    (lease, session, win, msys, cyg)
}

fn assert_privacy_clean(text: &str) {
    let (lease, session, win, msys, cyg) = privacy_needles();
    assert!(
        !text.contains(&lease) && !text.contains(&format!("{}-{}-", "grok", "lease")),
        "must not contain lease id:\n{text}"
    );
    assert!(
        !text.contains(&session),
        "must not contain session id:\n{text}"
    );
    assert!(
        !text.contains(&win) && !text.contains(&msys) && !text.contains(&cyg),
        "must not contain host user path:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// 1. Round-trip: dispatch → complete → verdict
// ---------------------------------------------------------------------------

#[test]
fn dispatch_complete_verdict_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    let task = dispatch_task(
        &o,
        DispatchInput {
            title: "K0e ledger CLI implementation".into(),
            brief_path: Some("docs/execution/k0e-brief.md".into()),
            worker_label: "grok3".into(),
        },
    )
    .expect("dispatch");
    assert_eq!(task.status, TaskStatus::Dispatched);
    assert_eq!(task.worker_label, "grok3");
    assert!(task.brief_path.as_deref().unwrap().contains("k0e-brief"));

    let done = set_task_status(
        &o,
        CompleteInput {
            task_id: task.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("K0E-LEDGER-CLI-COMPLETE".into()),
        },
    )
    .expect("complete");
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(
        done.completion_signal.as_deref(),
        Some("K0E-LEDGER-CLI-COMPLETE")
    );

    let verdict = record_verdict(
        &o,
        VerdictInput {
            task_id: Some(task.task_id.clone()),
            outcome: VerdictOutcome::Accepted,
            gate_result: Some("worker-gate exit 0".into()),
            e2e_result: Some("skipped (controller-owned)".into()),
            summary: "CLI + stale detection accepted".into(),
            residuals: Some("full gate may flap on parallel workers".into()),
        },
    )
    .expect("verdict");
    assert_eq!(verdict.outcome, VerdictOutcome::Accepted);
    assert_eq!(verdict.task_id.as_deref(), Some(task.task_id.as_str()));

    let snap = load_snapshot(&o).unwrap();
    assert!(
        snap.dispatched().is_empty(),
        "accepted task must not stay in-flight"
    );
    assert!(
        snap.pending_acceptance().is_empty(),
        "verdict must clear completed-without-verdict"
    );
    assert_eq!(snap.verdicts.len(), 1);
    assert_eq!(snap.tasks.len(), 1);
    assert_eq!(snap.tasks[0].status, TaskStatus::Completed);
}

// ---------------------------------------------------------------------------
// 2. ledger status lists in-flight + pending acceptance
// ---------------------------------------------------------------------------

#[test]
fn ledger_status_lists_inflight_and_pending() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    let inflight = dispatch_task(
        &o,
        DispatchInput {
            title: "Still flying worker".into(),
            brief_path: None,
            worker_label: "grok2".into(),
        },
    )
    .unwrap();

    let done = dispatch_task(
        &o,
        DispatchInput {
            title: "Finished but no verdict yet".into(),
            brief_path: Some("docs/execution/done-brief.md".into()),
            worker_label: "grok4".into(),
        },
    )
    .unwrap();
    set_task_status(
        &o,
        CompleteInput {
            task_id: done.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("WORKER-DONE".into()),
        },
    )
    .unwrap();

    let snap = load_snapshot(&o).unwrap();
    let md = format_ledger_status(&snap);

    assert!(
        md.contains("In-flight") || md.contains("在途"),
        "status must have in-flight section:\n{md}"
    );
    assert!(
        md.contains("Pending acceptance") || md.contains("未验收"),
        "status must have pending section:\n{md}"
    );
    assert!(md.contains("Still flying worker"), "in-flight title:\n{md}");
    assert!(md.contains(&inflight.task_id), "in-flight task_id:\n{md}");
    assert!(md.contains("`grok2`"), "slot label:\n{md}");
    assert!(
        md.contains("Finished but no verdict yet"),
        "pending title:\n{md}"
    );
    assert!(md.contains(&done.task_id), "pending task_id:\n{md}");
    assert!(
        md.contains("**Count:** 1"),
        "counts should show 1 for sections:\n{md}"
    );

    assert_privacy_clean(&md);
}

// ---------------------------------------------------------------------------
// 3. Stale detection — three required cases
// ---------------------------------------------------------------------------

#[test]
fn stale_case1_inflight_count_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    // Ledger: 0 dispatched (empty), doc claims 1 — the ADR failure mode.
    // Ensure project exists via a completed task so snapshot resolves.
    let t = dispatch_task(
        &o,
        DispatchInput {
            title: "Already finished hours ago".into(),
            brief_path: None,
            worker_label: "grok1".into(),
        },
    )
    .unwrap();
    set_task_status(
        &o,
        CompleteInput {
            task_id: t.task_id,
            status: TaskStatus::Completed,
            completion_signal: Some("done".into()),
        },
    )
    .unwrap();
    // Record verdict so case 2 does not also fire for this task.
    record_verdict(
        &o,
        VerdictInput {
            task_id: Some(load_snapshot(&o).unwrap().tasks[0].task_id.clone()),
            outcome: VerdictOutcome::Accepted,
            gate_result: None,
            e2e_result: None,
            summary: "finished earlier".into(),
            residuals: None,
        },
    )
    .unwrap();

    let doc = "\
# Controller handoff\n\
\n\
Round summary. **在途 Worker:1。**\n\
\n\
(Still claiming UI Worker in flight — but it finished hours ago.)\n\
";
    let snap = load_snapshot(&o).unwrap();
    assert_eq!(snap.dispatched().len(), 0);
    let report = check_stale(doc, &snap);
    assert!(report.is_stale(), "must detect inflight mismatch");
    let msgs: Vec<String> = report.findings.iter().map(|f| f.message()).collect();
    assert!(
        msgs.iter().any(|m| m.contains("In-flight count mismatch")
            && m.contains("claims 1")
            && m.contains("0")),
        "actionable mismatch message required: {msgs:?}"
    );
    let formatted = format_stale_report(&report, "docs/handoff/stale.md");
    assert!(formatted.contains("STALE") || formatted.contains("stale"));
    assert!(formatted.contains("inflight_count_mismatch"));
    assert_privacy_clean(&formatted);
}

#[test]
fn stale_case2_completed_without_verdict_unmentioned() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    let t = dispatch_task(
        &o,
        DispatchInput {
            title: "UI premium polish wave".into(),
            brief_path: None,
            worker_label: "grok2".into(),
        },
    )
    .unwrap();
    set_task_status(
        &o,
        CompleteInput {
            task_id: t.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("UI-DONE".into()),
        },
    )
    .unwrap();

    // Doc claims 0 in-flight (matches ledger) but never mentions the gap task.
    let doc = "\
# Handoff\n\
在途 Worker: 0\n\
\n\
All quiet. Nothing pending.\n\
";
    let snap = load_snapshot(&o).unwrap();
    assert_eq!(snap.pending_acceptance().len(), 1);
    let report = check_stale(doc, &snap);
    assert!(report.is_stale());
    let msgs: Vec<String> = report.findings.iter().map(|f| f.message()).collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("Completed without verdict") && m.contains("UI premium polish")),
        "must name the unmentioned task: {msgs:?}"
    );
    assert_privacy_clean(&format_stale_report(&report, "handoff.md"));
}

#[test]
fn stale_case3_doc_mentions_task_absent_from_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    // Create project via an unrelated completed+accepted task so ledger is non-empty
    // for project resolve, but the phantom title is absent.
    let t = dispatch_task(
        &o,
        DispatchInput {
            title: "Real recorded task".into(),
            brief_path: None,
            worker_label: "grok3".into(),
        },
    )
    .unwrap();
    set_task_status(
        &o,
        CompleteInput {
            task_id: t.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("ok".into()),
        },
    )
    .unwrap();
    record_verdict(
        &o,
        VerdictInput {
            task_id: Some(t.task_id),
            outcome: VerdictOutcome::Accepted,
            gate_result: None,
            e2e_result: None,
            summary: "ok".into(),
            residuals: None,
        },
    )
    .unwrap();

    let doc = "\
# Handoff\n\
在途 Worker: 0\n\
\n\
task-title: Phantom worker that was never dispatched\n\
task-title: Real recorded task\n\
";
    let snap = load_snapshot(&o).unwrap();
    let report = check_stale(doc, &snap);
    assert!(report.is_stale());
    let msgs: Vec<String> = report.findings.iter().map(|f| f.message()).collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("not present in the ledger") && m.contains("Phantom")),
        "reverse missing task: {msgs:?}"
    );
    // Real task must not be reported missing.
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("Real recorded task") && m.contains("not present")),
        "must not false-positive known task: {msgs:?}"
    );
    assert_privacy_clean(&format_stale_report(&report, "handoff.md"));
}

// ---------------------------------------------------------------------------
// 4. Negative control — consistent doc does not alarm
// ---------------------------------------------------------------------------

#[test]
fn stale_negative_control_consistent_not_flagged() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    let inflight = dispatch_task(
        &o,
        DispatchInput {
            title: "K0e ledger CLI".into(),
            brief_path: Some("docs/execution/k0e-brief.md".into()),
            worker_label: "grok3".into(),
        },
    )
    .unwrap();

    let done = dispatch_task(
        &o,
        DispatchInput {
            title: "K0d ledger tables".into(),
            brief_path: None,
            worker_label: "grok2".into(),
        },
    )
    .unwrap();
    set_task_status(
        &o,
        CompleteInput {
            task_id: done.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("K0D-DONE".into()),
        },
    )
    .unwrap();
    record_verdict(
        &o,
        VerdictInput {
            task_id: Some(done.task_id.clone()),
            outcome: VerdictOutcome::Accepted,
            gate_result: Some("exit 0".into()),
            e2e_result: None,
            summary: "K0d accepted".into(),
            residuals: None,
        },
    )
    .unwrap();

    // Doc matches: 1 in-flight, mentions both tasks, no completed-without-verdict.
    let doc = format!(
        "# Handoff\n\
在途 Worker: 1\n\
\n\
task-title: K0e ledger CLI\n\
task-title: K0d ledger tables\n\
task-id: {}\n\
",
        inflight.task_id
    );

    let snap = load_snapshot(&o).unwrap();
    assert_eq!(snap.dispatched().len(), 1);
    assert!(snap.pending_acceptance().is_empty());
    let report = check_stale(&doc, &snap);
    assert!(
        !report.is_stale(),
        "negative control failed — findings: {:?}",
        report
            .findings
            .iter()
            .map(|f| f.message())
            .collect::<Vec<_>>()
    );
    let md = format_stale_report(&report, "docs/handoff/fresh.md");
    assert!(md.contains("fresh") || md.contains("no stale"));
    assert_privacy_clean(&md);
}

// ---------------------------------------------------------------------------
// 5. Privacy on CLI-facing formatters (explicit assertions)
// ---------------------------------------------------------------------------

#[test]
fn privacy_cli_output_and_stale_report_clean() {
    let temp = tempfile::tempdir().unwrap();
    let o = opts(&temp, "pillow");

    let task = dispatch_task(
        &o,
        DispatchInput {
            title: "Privacy-safe task title".into(),
            brief_path: Some("docs/execution/safe-brief.md".into()),
            worker_label: "grok3".into(),
        },
    )
    .unwrap();

    let snap = load_snapshot(&o).unwrap();
    let status = format_ledger_status(&snap);
    assert!(status.contains("grok3"));
    assert!(status.contains("Privacy-safe task title"));
    assert_privacy_clean(&status);

    let doc = format!(
        "在途 Worker: 1\ntask-title: Privacy-safe task title\ntask-id: {}\n",
        task.task_id
    );
    let report = check_stale(&doc, &snap);
    assert!(!report.is_stale());
    let stale_md = format_stale_report(&report, "docs/handoff/x.md");
    assert_privacy_clean(&stale_md);

    // Also ensure storage rejects lease-as-slot (and we do not need to print it).
    let (lease, _session, _win, _msys, _cyg) = privacy_needles();
    let err = dispatch_task(
        &o,
        DispatchInput {
            title: "Bad slot attempt".into(),
            brief_path: None,
            worker_label: lease,
        },
    );
    assert!(err.is_err(), "lease as worker_label must be rejected");
}

// ---------------------------------------------------------------------------
// 6. Non-interactive: binary does not require stdin
// ---------------------------------------------------------------------------

#[test]
fn cli_binary_noninteractive_dispatch_status_stale() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("cli.db");
    let db_str = db.to_string_lossy().to_string();
    let root = temp.path().to_string_lossy().to_string();

    let bin = env!("CARGO_BIN_EXE_knowledge-indexer");

    // dispatch — stdin null
    let out = Command::new(bin)
        .args([
            "task",
            "dispatch",
            "--db",
            &db_str,
            "--project",
            "pillow",
            "--repo",
            &root,
            "--title",
            "CLI noninteractive task",
            "--slot",
            "grok3",
            "--brief",
            "docs/execution/cli-brief.md",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run dispatch");
    assert!(
        out.status.success(),
        "dispatch failed: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ok dispatched") && stdout.contains("task_id="),
        "stdout={stdout}"
    );
    assert_privacy_clean(&stdout);
    assert_privacy_clean(&String::from_utf8_lossy(&out.stderr));

    // Extract task_id=...
    let task_id = stdout
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("task_id="))
        .expect("task_id in stdout")
        .to_owned();

    // complete
    let out = Command::new(bin)
        .args([
            "task",
            "complete",
            "--db",
            &db_str,
            "--task-id",
            &task_id,
            "--status",
            "completed",
            "--signal",
            "CLI-COMPLETE",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run complete");
    assert!(
        out.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // verdict
    let out = Command::new(bin)
        .args([
            "verdict",
            "record",
            "--db",
            &db_str,
            "--project",
            "pillow",
            "--repo",
            &root,
            "--task-id",
            &task_id,
            "--outcome",
            "accepted",
            "--summary",
            "CLI round-trip accepted",
            "--gate",
            "exit 0",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run verdict");
    assert!(
        out.status.success(),
        "verdict failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok verdict_recorded"));

    // status
    let out = Command::new(bin)
        .args(["ledger", "status", "--db", &db_str, "--project", "pillow"])
        .stdin(Stdio::null())
        .output()
        .expect("run status");
    assert!(out.status.success());
    let status_md = String::from_utf8_lossy(&out.stdout);
    assert!(status_md.contains("Ledger status"));
    assert_privacy_clean(&status_md);

    // stale — consistent
    let doc_path = temp.path().join("handoff.md");
    write(
        &doc_path,
        "在途 Worker: 0\ntask-title: CLI noninteractive task\n",
    );
    let out = Command::new(bin)
        .args([
            "ledger",
            "stale",
            "--db",
            &db_str,
            "--project",
            "pillow",
            "--doc",
            &doc_path.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run stale fresh");
    assert!(
        out.status.success(),
        "fresh stale check should exit 0: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_privacy_clean(&String::from_utf8_lossy(&out.stdout));

    // stale — mismatch → exit 3
    write(
        &doc_path,
        "在途 Worker: 5\ntask-title: Ghost task nowhere in ledger\n",
    );
    let out = Command::new(bin)
        .args([
            "ledger",
            "stale",
            "--db",
            &db_str,
            "--project",
            "pillow",
            "--doc",
            &doc_path.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run stale dirty");
    let code = out.status.code();
    assert_eq!(
        code,
        Some(3),
        "stale findings must exit 3, got {code:?} stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stale_out = String::from_utf8_lossy(&out.stdout);
    assert!(stale_out.contains("STALE") || stale_out.contains("inflight_count_mismatch"));
    assert_privacy_clean(&stale_out);
}
