use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use chrono::{DateTime, TimeDelta, Utc};
use keeper_knowledge_indexer::{
    CompleteInput, DispatchInput, LedgerDbOptions, LibrarianAuditInput, LibrarianCheckKind,
    LibrarianCheckOutcome, VerdictInput, dispatch_task, format_librarian_audit_report,
    load_snapshot, record_verdict, run_librarian_audit, set_task_status,
};
use keeper_storage_sqlite::{KnowledgeTask, KnowledgeVerdict, TaskStatus, VerdictOutcome};

fn utc(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap()
        .with_timezone(&Utc)
}

fn dispatched_task(dispatched_at: &str) -> KnowledgeTask {
    KnowledgeTask {
        task_id: "task-stale-boundary".into(),
        project_id: "project-1".into(),
        title: "Boundary task".into(),
        brief_path: Some("docs/execution/boundary-task.md".into()),
        worker_label: "codex1".into(),
        dispatched_at: utc(dispatched_at),
        status: TaskStatus::Dispatched,
        completion_signal: None,
    }
}

fn completed_task() -> KnowledgeTask {
    KnowledgeTask {
        task_id: "task-awaiting-verdict".into(),
        project_id: "project-1".into(),
        title: "Delivered task".into(),
        brief_path: Some("docs/execution/delivered-task.md".into()),
        worker_label: "codex2".into(),
        dispatched_at: utc("2026-08-12T00:00:00Z"),
        status: TaskStatus::Completed,
        completion_signal: Some("DELIVERED".into()),
    }
}

fn accepted_verdict(summary: &str) -> KnowledgeVerdict {
    KnowledgeVerdict {
        verdict_id: "verdict-1".into(),
        task_id: Some("task-accepted".into()),
        project_id: "project-1".into(),
        outcome: VerdictOutcome::Accepted,
        gate_result: Some("cargo test exit 0".into()),
        e2e_result: None,
        summary: summary.into(),
        residuals: None,
        decided_at: utc("2026-08-11T16:00:00Z"),
    }
}

#[test]
fn stale_inflight_passes_at_threshold_and_fails_one_second_over() {
    let task = dispatched_task("2026-08-11T00:00:00Z");
    let snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: vec![task],
        verdicts: Vec::new(),
        completed_without_verdict: Vec::new(),
    };
    let handoff = "在途 Worker: 1\ntask-id: task-stale-boundary\n";
    let git_commits: Vec<String> = Vec::new();

    let boundary = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: handoff,
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    assert_eq!(
        boundary
            .check(LibrarianCheckKind::StaleInflight)
            .unwrap()
            .outcome,
        LibrarianCheckOutcome::Pass,
        "age exactly equal to the threshold is not stale"
    );
    assert!(format_librarian_audit_report(&boundary).contains("`stale_inflight` | **PASS**"));

    let overdue = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: handoff,
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:01Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let stale = overdue.check(LibrarianCheckKind::StaleInflight).unwrap();
    assert_eq!(stale.outcome, LibrarianCheckOutcome::Fail);
    assert!(
        stale
            .findings
            .iter()
            .any(|finding| finding.contains("task-stale-boundary") && finding.contains("codex1"))
    );
    assert!(format_librarian_audit_report(&overdue).contains("`stale_inflight` | **FAIL**"));
}

#[test]
fn stale_inflight_does_not_flag_a_dispatched_task_with_a_verdict() {
    let task = dispatched_task("2026-08-09T00:00:00Z");
    let commit = "c".repeat(40);
    let mut verdict = accepted_verdict(&format!("Accepted commit={commit}"));
    verdict.task_id = Some(task.task_id.clone());
    let snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: vec![task],
        verdicts: vec![verdict],
        completed_without_verdict: Vec::new(),
    };
    let commits = vec![commit];
    let report = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: "在途 Worker: 1\ntask-id: task-stale-boundary\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &commits,
    })
    .unwrap();

    let check = report.check(LibrarianCheckKind::StaleInflight).unwrap();
    assert_eq!(check.outcome, LibrarianCheckOutcome::Pass);
    assert!(check.findings.is_empty());
}

#[test]
fn completed_without_verdict_has_explicit_pass_and_fail_states() {
    let clean_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: Vec::new(),
        completed_without_verdict: Vec::new(),
    };
    let git_commits: Vec<String> = Vec::new();
    let clean = run_librarian_audit(LibrarianAuditInput {
        snapshot: &clean_snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let clean_check = clean
        .check(LibrarianCheckKind::CompletedWithoutVerdict)
        .unwrap();
    assert_eq!(clean_check.outcome, LibrarianCheckOutcome::Pass);
    assert_eq!(clean_check.findings, Vec::<String>::new());
    assert!(
        format_librarian_audit_report(&clean).contains("`completed_without_verdict` | **PASS**")
    );

    let task = completed_task();
    let pending_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: vec![task.clone()],
        verdicts: Vec::new(),
        completed_without_verdict: vec![task],
    };
    let pending = run_librarian_audit(LibrarianAuditInput {
        snapshot: &pending_snapshot,
        handoff_markdown: "在途 Worker: 0\ntask-id: task-awaiting-verdict\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let pending_check = pending
        .check(LibrarianCheckKind::CompletedWithoutVerdict)
        .unwrap();
    assert_eq!(pending_check.outcome, LibrarianCheckOutcome::Fail);
    assert!(
        pending_check
            .findings
            .iter()
            .any(|finding| finding.contains("task-awaiting-verdict") && finding.contains("codex2"))
    );
    assert!(
        format_librarian_audit_report(&pending).contains("`completed_without_verdict` | **FAIL**")
    );
}

#[test]
fn historical_completed_without_verdict_is_visibly_exempt_before_fixed_cutoff_only() {
    let mut historical_task = completed_task();
    historical_task.task_id = "task-historical-debt".into();
    historical_task.dispatched_at = utc("2026-08-11T23:59:59Z");
    let historical_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: vec![historical_task.clone()],
        verdicts: Vec::new(),
        completed_without_verdict: vec![historical_task],
    };
    let git_commits: Vec<String> = Vec::new();
    let historical = run_librarian_audit(LibrarianAuditInput {
        snapshot: &historical_snapshot,
        handoff_markdown: "In-flight workers: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T12:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let completed = historical
        .check(LibrarianCheckKind::CompletedWithoutVerdict)
        .unwrap();
    assert_eq!(completed.outcome, LibrarianCheckOutcome::Pass);
    assert!(completed.findings.is_empty());
    assert!(
        completed.summary.contains(
            "completed_without_verdict=0 total_without_verdict=1 historical_exemptions=1"
        )
    );
    assert!(
        completed
            .summary
            .contains("exemption_basis=dispatched_at<2026-08-12T00:00:00Z")
    );
    let document = historical
        .check(LibrarianCheckKind::DocumentLedgerConsistency)
        .unwrap();
    assert_eq!(document.outcome, LibrarianCheckOutcome::Pass);
    assert!(document.findings.is_empty());
    assert!(
        document
            .summary
            .contains("historical_completed_exemptions=1")
    );
    assert!(
        format_librarian_audit_report(&historical)
            .contains("historical_exemptions=1 exemption_basis=dispatched_at<2026-08-12")
    );

    let current_task = completed_task();
    let current_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: vec![current_task.clone()],
        verdicts: Vec::new(),
        completed_without_verdict: vec![current_task],
    };
    let current = run_librarian_audit(LibrarianAuditInput {
        snapshot: &current_snapshot,
        handoff_markdown: "In-flight workers: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T12:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    assert_eq!(
        current
            .check(LibrarianCheckKind::CompletedWithoutVerdict)
            .unwrap()
            .outcome,
        LibrarianCheckOutcome::Fail,
        "the fixed cutoff is exclusive"
    );
    assert_eq!(
        current
            .check(LibrarianCheckKind::DocumentLedgerConsistency)
            .unwrap()
            .outcome,
        LibrarianCheckOutcome::Fail,
        "current missing verdicts must remain visible in document consistency"
    );
}

#[test]
fn document_ledger_consistency_has_explicit_pass_and_fail_states() {
    let snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: Vec::new(),
        completed_without_verdict: Vec::new(),
    };
    let git_commits: Vec<String> = Vec::new();
    let fresh = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let fresh_check = fresh
        .check(LibrarianCheckKind::DocumentLedgerConsistency)
        .unwrap();
    assert_eq!(fresh_check.outcome, LibrarianCheckOutcome::Pass);
    assert!(fresh_check.findings.is_empty());
    assert!(
        format_librarian_audit_report(&fresh).contains("`document_ledger_consistency` | **PASS**")
    );

    let stale = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: "在途 Worker: 2\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let stale_check = stale
        .check(LibrarianCheckKind::DocumentLedgerConsistency)
        .unwrap();
    assert_eq!(stale_check.outcome, LibrarianCheckOutcome::Fail);
    assert!(
        stale_check
            .findings
            .iter()
            .any(|finding| finding.contains("inflight_count_mismatch")
                && finding.contains("claimed=2")
                && finding.contains("ledger_dispatched=0"))
    );
    assert!(
        format_librarian_audit_report(&stale).contains("`document_ledger_consistency` | **FAIL**")
    );
}

#[test]
fn audit_report_does_not_echo_host_paths_from_handoff_mentions() {
    let snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: Vec::new(),
        completed_without_verdict: Vec::new(),
    };
    let host_path = format!("{}:\\{}\\{}\\private.txt", "C", "Users", "someone");
    let handoff = format!("在途 Worker: 0\ntask-title: {host_path}\n");
    let report = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: &handoff,
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &[],
    })
    .unwrap();
    assert_eq!(
        report
            .check(LibrarianCheckKind::DocumentLedgerConsistency)
            .unwrap()
            .outcome,
        LibrarianCheckOutcome::Fail
    );

    let formatted = format_librarian_audit_report(&report);
    assert!(
        !formatted.contains(&host_path),
        "host path leaked:\n{formatted}"
    );
    assert!(
        !formatted.contains("someone"),
        "host user leaked:\n{formatted}"
    );
    assert!(
        !formatted.contains("private.txt"),
        "host filename leaked:\n{formatted}"
    );
    assert!(formatted.contains("code=doc_task_missing_from_ledger"));
}

#[test]
fn accepted_verdict_commit_has_explicit_pass_and_fail_states() {
    let existing_commit = "a".repeat(40);
    let accepted = accepted_verdict(&format!("Accepted after review commit={existing_commit}"));
    let snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: vec![accepted],
        completed_without_verdict: Vec::new(),
    };
    let git_commits = vec![existing_commit.clone()];
    let aligned = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let aligned_check = aligned
        .check(LibrarianCheckKind::AcceptedVerdictCommit)
        .unwrap();
    assert_eq!(aligned_check.outcome, LibrarianCheckOutcome::Pass);
    assert!(aligned_check.findings.is_empty());
    assert!(
        format_librarian_audit_report(&aligned).contains("`accepted_verdict_commit` | **PASS**")
    );

    let missing_commit = "b".repeat(40);
    let missing_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: vec![accepted_verdict(&format!(
            "Accepted after review commit={missing_commit}"
        ))],
        completed_without_verdict: Vec::new(),
    };
    let misaligned = run_librarian_audit(LibrarianAuditInput {
        snapshot: &missing_snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let misaligned_check = misaligned
        .check(LibrarianCheckKind::AcceptedVerdictCommit)
        .unwrap();
    assert_eq!(misaligned_check.outcome, LibrarianCheckOutcome::Fail);
    assert!(
        misaligned_check.findings.iter().any(
            |finding| finding.contains("task-accepted") && finding.contains("commit_not_found")
        )
    );
    assert!(
        format_librarian_audit_report(&misaligned).contains("`accepted_verdict_commit` | **FAIL**")
    );

    let unlinked_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: vec![accepted_verdict(
            "Accepted without an explicit commit marker",
        )],
        completed_without_verdict: Vec::new(),
    };
    let unlinked = run_librarian_audit(LibrarianAuditInput {
        snapshot: &unlinked_snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let unlinked_check = unlinked
        .check(LibrarianCheckKind::AcceptedVerdictCommit)
        .unwrap();
    // No marker means the link was never recorded, which is not the same as a
    // recorded link that turned out to be wrong. Failing on absence made this
    // check unable to pass on any real ledger, so it reported nothing usable.
    assert_eq!(unlinked_check.outcome, LibrarianCheckOutcome::Pass);
    assert!(unlinked_check.findings.is_empty());
    assert!(
        unlinked_check.summary.contains("unrecorded=1"),
        "unrecorded verdicts must stay visible in the summary: {}",
        unlinked_check.summary
    );

    let embedded_snapshot = keeper_knowledge_indexer::LedgerSnapshot {
        project_id: "project-1".into(),
        project_name: "pillow".into(),
        tasks: Vec::new(),
        verdicts: vec![accepted_verdict(&format!(
            "Accepted but notcommit={existing_commit} is not the marker grammar"
        ))],
        completed_without_verdict: Vec::new(),
    };
    let embedded = run_librarian_audit(LibrarianAuditInput {
        snapshot: &embedded_snapshot,
        handoff_markdown: "在途 Worker: 0\n",
        handoff_label: "docs/handoff/controller.md",
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &git_commits,
    })
    .unwrap();
    let embedded_check = embedded
        .check(LibrarianCheckKind::AcceptedVerdictCommit)
        .unwrap();
    // `notcommit=` must not be parsed as a marker. That leaves the verdict
    // unrecorded rather than misaligned — the point of this case is the
    // parser's strictness, not a failure verdict.
    assert_eq!(embedded_check.outcome, LibrarianCheckOutcome::Pass);
    assert!(embedded_check.findings.is_empty());
    assert!(
        embedded_check.summary.contains("unrecorded=1"),
        "near-miss grammar must not count as a recorded link: {}",
        embedded_check.summary
    );
}

#[test]
fn verdict_record_commit_flag_is_strict_and_increases_verified_count() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::Builder::new()
        .prefix("verdict-commit-")
        .tempdir_in(repo_root.join("target"))
        .unwrap();
    let db_path = temp.path().join("ledger.db");
    let handoff_path = temp.path().join("handoff.md");
    let handoff_label = handoff_path
        .strip_prefix(&repo_root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(&handoff_path, "In-flight workers: 0\n").unwrap();

    let head = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "rev-parse", "HEAD"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(head.status.success());
    let head = String::from_utf8(head.stdout).unwrap().trim().to_owned();
    let bin = env!("CARGO_BIN_EXE_knowledge-indexer");

    let recorded = Command::new(bin)
        .args([
            "verdict",
            "record",
            "--db",
            db_path.to_str().unwrap(),
            "--project",
            "pillow",
            "--repo",
            repo_root.to_str().unwrap(),
            "--outcome",
            "accepted",
            "--summary",
            "Accepted through the commit flag",
            "--commit",
            &head,
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        recorded.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&recorded.stderr)
    );

    let opts = LedgerDbOptions {
        db_path: db_path.clone(),
        project_name: "pillow".into(),
        project_root: Some(repo_root.clone()),
    };
    let snapshot = load_snapshot(&opts).unwrap();
    assert_eq!(snapshot.verdicts.len(), 1);
    assert_eq!(
        snapshot.verdicts[0].summary,
        format!("Accepted through the commit flag commit={head}")
    );
    let commits = vec![head];
    let report = run_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: "In-flight workers: 0\n",
        handoff_label: &handoff_label,
        now: utc("2026-08-12T00:00:00Z"),
        stale_after: TimeDelta::hours(24),
        git_commit_ids: &commits,
    })
    .unwrap();
    let commit_check = report
        .check(LibrarianCheckKind::AcceptedVerdictCommit)
        .unwrap();
    assert_eq!(commit_check.outcome, LibrarianCheckOutcome::Pass);
    assert!(
        commit_check
            .summary
            .contains("accepted_verdicts=1 verified=1 unrecorded=0 commit_findings=0")
    );

    for malformed in ["abcdef".to_owned(), "123456g".to_owned(), "a".repeat(65)] {
        let rejected = Command::new(bin)
            .args([
                "verdict",
                "record",
                "--db",
                db_path.to_str().unwrap(),
                "--project",
                "pillow",
                "--outcome",
                "accepted",
                "--summary",
                "Must not be recorded",
                "--commit",
                &malformed,
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            rejected.status.code(),
            Some(1),
            "malformed={malformed} stdout={} stderr={}",
            String::from_utf8_lossy(&rejected.stdout),
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert!(
            String::from_utf8_lossy(&rejected.stderr)
                .contains("invalid --commit: expected 7..64 hexadecimal characters")
        );
    }
    assert_eq!(load_snapshot(&opts).unwrap().verdicts.len(), 1);
}

#[test]
fn doctor_runs_with_defaults_records_effective_now_and_preserves_exit_codes() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::Builder::new()
        .prefix("doctor-defaults-")
        .tempdir_in(repo_root.join("target"))
        .unwrap();
    let ledger_dir = temp.path().join(".superpowers/ledger");
    let handoff_dir = temp.path().join("docs/handoff");
    fs::create_dir_all(&ledger_dir).unwrap();
    fs::create_dir_all(&handoff_dir).unwrap();
    fs::write(
        handoff_dir.join("2026-08-10-pillow-controller-handoff.md"),
        "In-flight workers: 9\n",
    )
    .unwrap();
    fs::write(
        handoff_dir.join("2026-08-12-pillow-controller-handoff.md"),
        "In-flight workers: 0\n",
    )
    .unwrap();

    let opts = LedgerDbOptions {
        db_path: ledger_dir.join("pillow.db"),
        project_name: "pillow".into(),
        project_root: Some(temp.path().to_path_buf()),
    };
    record_verdict(
        &opts,
        VerdictInput {
            task_id: None,
            outcome: VerdictOutcome::Rejected,
            gate_result: None,
            e2e_result: None,
            summary: "Rejected seed verdict".into(),
            residuals: None,
        },
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_knowledge-indexer");
    let defaults = Command::new(bin)
        .arg("doctor")
        .current_dir(temp.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        defaults.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&defaults.stdout),
        String::from_utf8_lossy(&defaults.stderr)
    );
    let defaults_stdout = String::from_utf8(defaults.stdout).unwrap();
    let invocation = defaults_stdout
        .lines()
        .find(|line| line.starts_with("invocation:"))
        .expect("doctor must print its resolved invocation");
    assert!(invocation.contains("mode=`doctor`"));
    assert!(invocation.contains("now_source=`system_clock_default`"));
    assert!(invocation.contains("stale_after_hours=24"));
    let effective_now = invocation
        .split_ascii_whitespace()
        .find_map(|token| token.strip_prefix("now=`"))
        .and_then(|token| token.strip_suffix('`'))
        .expect("doctor must print the effective now value");
    DateTime::parse_from_rfc3339(effective_now).expect("effective now must be RFC3339");
    assert!(
        defaults_stdout.contains("handoff=`docs/handoff/2026-08-12-pillow-controller-handoff.md`")
    );

    let injected = Command::new(bin)
        .args(["doctor", "--now", "2026-08-12T00:00:00Z"])
        .current_dir(temp.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(injected.status.code(), Some(0));
    let injected_stdout = String::from_utf8(injected.stdout).unwrap();
    assert!(
        injected_stdout.contains(
            "invocation: mode=`doctor` now=`2026-08-12T00:00:00+00:00` now_source=`--now`"
        )
    );

    let finding = Command::new(bin)
        .args([
            "doctor",
            "--doc",
            "docs/handoff/2026-08-10-pillow-controller-handoff.md",
            "--now",
            "2026-08-12T00:00:00Z",
        ])
        .current_dir(temp.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(finding.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&finding.stdout).contains("result: **FAIL**"));

    let tool_error = Command::new(bin)
        .args(["doctor", "--now", "not-rfc3339"])
        .current_dir(temp.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(tool_error.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&tool_error.stderr).contains("invalid --now"));
}

#[test]
fn audit_cli_distinguishes_pass_findings_and_tool_error_without_writes() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::Builder::new()
        .prefix("librarian-audit-")
        .tempdir_in(repo_root.join("target"))
        .unwrap();
    let db_path = temp.path().join("audit.db");
    let handoff_path = temp.path().join("handoff.md");
    let handoff_label = handoff_path
        .strip_prefix(&repo_root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let opts = LedgerDbOptions {
        db_path: db_path.clone(),
        project_name: "pillow".into(),
        project_root: Some(repo_root.clone()),
    };

    let head = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "rev-parse", "HEAD"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(head.status.success());
    let head = String::from_utf8(head.stdout).unwrap().trim().to_owned();

    let task = dispatch_task(
        &opts,
        DispatchInput {
            title: "Audit CLI accepted task".into(),
            brief_path: Some("docs/execution/audit-cli.md".into()),
            worker_label: "codex1".into(),
        },
    )
    .unwrap();
    set_task_status(
        &opts,
        CompleteInput {
            task_id: task.task_id.clone(),
            status: TaskStatus::Completed,
            completion_signal: Some("AUDIT-CLI-DONE".into()),
        },
    )
    .unwrap();
    record_verdict(
        &opts,
        VerdictInput {
            task_id: Some(task.task_id.clone()),
            outcome: VerdictOutcome::Accepted,
            gate_result: Some("cargo test exit 0".into()),
            e2e_result: None,
            summary: format!("Accepted audit CLI commit={head}"),
            residuals: None,
        },
    )
    .unwrap();

    let fresh_doc = format!("在途 Worker: 0\ntask-id: {}\n", task.task_id);
    fs::write(&handoff_path, &fresh_doc).unwrap();
    let before_snapshot = load_snapshot(&opts).unwrap();
    let bin = env!("CARGO_BIN_EXE_knowledge-indexer");
    let common = [
        "audit",
        "--db",
        db_path.to_str().unwrap(),
        "--project",
        "pillow",
        "--repo",
        repo_root.to_str().unwrap(),
        "--doc",
        &handoff_label,
        "--now",
        "2026-08-12T00:00:00Z",
        "--stale-after-hours",
        "24",
    ];

    let pass = Command::new(bin)
        .args(common)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        pass.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&pass.stderr)
    );
    let pass_stdout = String::from_utf8(pass.stdout).unwrap();
    assert!(pass_stdout.contains("result: **PASS**"));
    for check in [
        "stale_inflight",
        "completed_without_verdict",
        "document_ledger_consistency",
        "accepted_verdict_commit",
    ] {
        assert!(
            pass_stdout.contains(&format!("`{check}` | **PASS**")),
            "missing PASS for {check}:\n{pass_stdout}"
        );
    }
    assert!(
        !pass_stdout.contains(&repo_root.to_string_lossy().to_string()),
        "must not print the host repository path: {pass_stdout}"
    );
    assert_eq!(fs::read_to_string(&handoff_path).unwrap(), fresh_doc);
    assert_eq!(load_snapshot(&opts).unwrap().tasks, before_snapshot.tasks);
    assert_eq!(
        load_snapshot(&opts).unwrap().verdicts,
        before_snapshot.verdicts
    );

    let stale_doc = format!("在途 Worker: 4\ntask-id: {}\n", task.task_id);
    fs::write(&handoff_path, &stale_doc).unwrap();
    let fail = Command::new(bin)
        .args(common)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(fail.status.code(), Some(3));
    let fail_stdout = String::from_utf8(fail.stdout).unwrap();
    assert!(fail_stdout.contains("result: **FAIL**"));
    assert!(fail_stdout.contains("`document_ledger_consistency` | **FAIL**"));
    assert_eq!(fs::read_to_string(&handoff_path).unwrap(), stale_doc);

    fs::write(&handoff_path, &fresh_doc).unwrap();
    fs::write(temp.path().join(".git"), "gitdir: missing-git-directory\n").unwrap();
    let tool_error = Command::new(bin)
        .args([
            "audit",
            "--db",
            db_path.to_str().unwrap(),
            "--project",
            "pillow",
            "--repo",
            temp.path().to_str().unwrap(),
            "--doc",
            "handoff.md",
            "--now",
            "2026-08-12T00:00:00Z",
            "--stale-after-hours",
            "24",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(tool_error.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&tool_error.stderr).contains("git log"));
    assert_eq!(fs::read_to_string(&handoff_path).unwrap(), fresh_doc);
}
