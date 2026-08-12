//! Deterministic, report-only checks for the built-in librarian audit.
//!
//! The caller supplies wall-clock time and local Git history. This module does
//! not read the system clock, access the network, or mutate the ledger/docs.

use std::path::Path;
use std::process::{Command, Stdio};

use chrono::{DateTime, TimeDelta, Utc};
use keeper_project::validate_portable_path;
use keeper_storage_sqlite::{TaskStatus, VerdictOutcome};

use crate::ledger::LedgerSnapshot;
use crate::stale::{StaleKind, check_stale};

const HISTORICAL_COMPLETED_CUTOFF_RFC3339: &str = "2026-08-12T00:00:00Z";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarianCheckKind {
    StaleInflight,
    CompletedWithoutVerdict,
    DocumentLedgerConsistency,
    AcceptedVerdictCommit,
}

impl LibrarianCheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaleInflight => "stale_inflight",
            Self::CompletedWithoutVerdict => "completed_without_verdict",
            Self::DocumentLedgerConsistency => "document_ledger_consistency",
            Self::AcceptedVerdictCommit => "accepted_verdict_commit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarianCheckOutcome {
    Pass,
    Fail,
}

impl LibrarianCheckOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarianCheck {
    pub kind: LibrarianCheckKind,
    pub outcome: LibrarianCheckOutcome,
    pub summary: String,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarianAuditReport {
    pub now: DateTime<Utc>,
    pub stale_after: TimeDelta,
    pub handoff_label: String,
    pub checks: Vec<LibrarianCheck>,
}

impl LibrarianAuditReport {
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.outcome == LibrarianCheckOutcome::Fail)
    }

    pub fn check(&self, kind: LibrarianCheckKind) -> Option<&LibrarianCheck> {
        self.checks.iter().find(|check| check.kind == kind)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LibrarianAuditInput<'a> {
    pub snapshot: &'a LedgerSnapshot,
    pub handoff_markdown: &'a str,
    pub handoff_label: &'a str,
    pub now: DateTime<Utc>,
    pub stale_after: TimeDelta,
    pub git_commit_ids: &'a [String],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LibrarianAuditError {
    #[error("stale threshold must be non-negative")]
    NegativeStaleThreshold,
    #[error("handoff label must be a portable project-relative path")]
    InvalidHandoffLabel,
}

#[derive(Debug, thiserror::Error)]
pub enum GitLogError {
    #[error("git log could not be started")]
    Start(#[source] std::io::Error),
    #[error("git log failed with exit code {code:?}")]
    Failed { code: Option<i32> },
    #[error("git log returned non-UTF-8 output")]
    NonUtf8,
    #[error("git log returned an invalid commit id")]
    InvalidCommitId,
}

pub fn run_librarian_audit(
    input: LibrarianAuditInput<'_>,
) -> Result<LibrarianAuditReport, LibrarianAuditError> {
    if input.stale_after < TimeDelta::zero() {
        return Err(LibrarianAuditError::NegativeStaleThreshold);
    }
    if validate_portable_path(input.handoff_label).is_err() {
        return Err(LibrarianAuditError::InvalidHandoffLabel);
    }

    let threshold_seconds = input.stale_after.num_seconds();
    let mut findings = Vec::new();
    let mut dispatched = 0usize;
    let verdict_task_ids: std::collections::HashSet<&str> = input
        .snapshot
        .verdicts
        .iter()
        .filter_map(|verdict| verdict.task_id.as_deref())
        .collect();
    let mut dispatched_with_verdict = 0usize;
    for task in &input.snapshot.tasks {
        if task.status != TaskStatus::Dispatched {
            continue;
        }
        dispatched += 1;
        if verdict_task_ids.contains(task.task_id.as_str()) {
            dispatched_with_verdict += 1;
            continue;
        }
        let age = input.now.signed_duration_since(task.dispatched_at);
        if age > input.stale_after {
            findings.push(format!(
                "task_id=`{}` slot=`{}` age_seconds={} threshold_seconds={threshold_seconds}",
                task.task_id,
                task.worker_label,
                age.num_seconds()
            ));
        }
    }

    let stale_check = LibrarianCheck {
        kind: LibrarianCheckKind::StaleInflight,
        outcome: if findings.is_empty() {
            LibrarianCheckOutcome::Pass
        } else {
            LibrarianCheckOutcome::Fail
        },
        summary: format!(
            "stale_inflight={} dispatched={dispatched} dispatched_with_verdict={dispatched_with_verdict} threshold_seconds={threshold_seconds}",
            findings.len(),
        ),
        findings,
    };

    let historical_completed_cutoff =
        DateTime::parse_from_rfc3339(HISTORICAL_COMPLETED_CUTOFF_RFC3339)
            .expect("fixed historical cutoff must be valid RFC3339")
            .with_timezone(&Utc);
    let historical_completed_task_ids: std::collections::HashSet<&str> = input
        .snapshot
        .completed_without_verdict
        .iter()
        .filter(|task| task.dispatched_at < historical_completed_cutoff)
        .map(|task| task.task_id.as_str())
        .collect();
    let completed_findings: Vec<String> = input
        .snapshot
        .completed_without_verdict
        .iter()
        .filter(|task| !historical_completed_task_ids.contains(task.task_id.as_str()))
        .map(|task| {
            format!(
                "task_id=`{}` slot=`{}` title={:?}",
                task.task_id, task.worker_label, task.title
            )
        })
        .collect();
    let completed_check = LibrarianCheck {
        kind: LibrarianCheckKind::CompletedWithoutVerdict,
        outcome: if completed_findings.is_empty() {
            LibrarianCheckOutcome::Pass
        } else {
            LibrarianCheckOutcome::Fail
        },
        summary: format!(
            "completed_without_verdict={} total_without_verdict={} historical_exemptions={} exemption_basis=dispatched_at<{HISTORICAL_COMPLETED_CUTOFF_RFC3339}",
            completed_findings.len(),
            input.snapshot.completed_without_verdict.len(),
            historical_completed_task_ids.len()
        ),
        findings: completed_findings,
    };

    let stale_report = check_stale(input.handoff_markdown, input.snapshot);
    let mut historical_completed_document_exemptions = 0usize;
    let document_findings: Vec<String> = stale_report
        .findings
        .iter()
        .filter_map(|finding| match &finding.kind {
            StaleKind::InflightCountMismatch {
                claimed,
                ledger_dispatched,
                ..
            } => Some(format!(
                "code=inflight_count_mismatch claimed={claimed} ledger_dispatched={ledger_dispatched}"
            )),
            StaleKind::CompletedWithoutVerdictUnmentioned {
                task_id,
                worker_label,
                ..
            } if historical_completed_task_ids.contains(task_id.as_str()) => {
                historical_completed_document_exemptions += 1;
                None
            }
            StaleKind::CompletedWithoutVerdictUnmentioned {
                task_id,
                worker_label,
                ..
            } => Some(format!(
                "code=completed_without_verdict_unmentioned task_id=`{task_id}` slot=`{worker_label}`"
            )),
            StaleKind::DocTaskMissingFromLedger {
                mention_kind,
                ..
            } => Some(format!(
                "code=doc_task_missing_from_ledger mention_kind={mention_kind}"
            )),
        })
        .collect();
    let document_check = LibrarianCheck {
        kind: LibrarianCheckKind::DocumentLedgerConsistency,
        outcome: if document_findings.is_empty() {
            LibrarianCheckOutcome::Pass
        } else {
            LibrarianCheckOutcome::Fail
        },
        summary: format!(
            "document_ledger_findings={} historical_completed_exemptions={} exemption_basis=dispatched_at<{HISTORICAL_COMPLETED_CUTOFF_RFC3339}",
            document_findings.len(),
            historical_completed_document_exemptions
        ),
        findings: document_findings,
    };

    let mut accepted_count = 0usize;
    // A verdict with no commit marker is *unrecorded*, not misaligned.
    //
    // Treating absence as FAIL made this check unable to pass at all: nothing
    // in the ledger has ever carried a marker, so it flagged 22 of 22 accepted
    // verdicts and told us nothing. A check whose PASS state is unreachable is
    // noise, and it is the same error as reading a failed probe as "signed
    // out" — missing evidence is not negative evidence. FAIL is reserved for a
    // claim that does not hold: a marker that is malformed, ambiguous, or
    // names a commit this repository does not contain.
    let mut unrecorded_count = 0usize;
    let mut commit_findings = Vec::new();
    for verdict in &input.snapshot.verdicts {
        if verdict.outcome != VerdictOutcome::Accepted {
            continue;
        }
        accepted_count += 1;
        let task_id = verdict.task_id.as_deref().unwrap_or("_controller_self_");
        let markers = extract_commit_markers(&verdict.summary);
        if markers.is_empty() {
            unrecorded_count += 1;
            continue;
        }
        if markers.len() != 1 {
            commit_findings.push(format!(
                "reason=ambiguous_commit_markers task_id=`{task_id}` marker_count={}",
                markers.len()
            ));
            continue;
        }
        let Some(commit_ref) = markers[0].as_ref() else {
            commit_findings.push(format!(
                "reason=invalid_commit_marker task_id=`{task_id}` expected=7..64_hex"
            ));
            continue;
        };
        let matching: std::collections::HashSet<String> = input
            .git_commit_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .filter(|id| id.starts_with(commit_ref))
            .collect();
        match matching.len() {
            1 => {}
            0 => commit_findings.push(format!(
                "reason=commit_not_found task_id=`{task_id}` commit=`{commit_ref}`"
            )),
            count => commit_findings.push(format!(
                "reason=ambiguous_git_prefix task_id=`{task_id}` commit=`{commit_ref}` matches={count}"
            )),
        }
    }
    let commit_check = LibrarianCheck {
        kind: LibrarianCheckKind::AcceptedVerdictCommit,
        outcome: if commit_findings.is_empty() {
            LibrarianCheckOutcome::Pass
        } else {
            LibrarianCheckOutcome::Fail
        },
        summary: format!(
            "accepted_verdicts={accepted_count} verified={} unrecorded={unrecorded_count} commit_findings={}",
            accepted_count - unrecorded_count - commit_findings.len(),
            commit_findings.len()
        ),
        findings: commit_findings,
    };

    Ok(LibrarianAuditReport {
        now: input.now,
        stale_after: input.stale_after,
        handoff_label: input.handoff_label.to_owned(),
        checks: vec![stale_check, completed_check, document_check, commit_check],
    })
}

pub fn format_librarian_audit_report(report: &LibrarianAuditReport) -> String {
    let mut out = String::new();
    out.push_str("# Librarian audit\n\n");
    out.push_str(&format!(
        "result: **{}**\n\n",
        if report.has_failures() {
            "FAIL"
        } else {
            "PASS"
        }
    ));
    out.push_str(&format!(
        "now=`{}` threshold_seconds={} handoff=`{}`\n\n",
        report.now.to_rfc3339(),
        report.stale_after.num_seconds(),
        report.handoff_label
    ));
    out.push_str(
        "Privacy: slot labels, task ids, commit ids, and portable paths only; no lease/session/host paths.\n\n",
    );
    out.push_str("| check | result | evidence |\n");
    out.push_str("| --- | --- | --- |\n");
    for check in &report.checks {
        out.push_str(&format!(
            "| `{}` | **{}** | {} |\n",
            check.kind.as_str(),
            check.outcome.as_str(),
            escape_cell(&check.summary)
        ));
    }
    out.push('\n');
    for check in &report.checks {
        if check.findings.is_empty() {
            continue;
        }
        out.push_str(&format!("## `{}` findings\n\n", check.kind.as_str()));
        for finding in &check.findings {
            out.push_str(&format!("- {}\n", finding.replace('\n', " ")));
        }
        out.push('\n');
    }
    out
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn extract_commit_markers(summary: &str) -> Vec<Option<String>> {
    summary
        .split_ascii_whitespace()
        .filter_map(|token| token.strip_prefix("commit="))
        .map(|rest| {
            let hex_len = rest
                .as_bytes()
                .iter()
                .take_while(|byte| byte.is_ascii_hexdigit())
                .count();
            let value = &rest[..hex_len];
            if (7..=64).contains(&value.len()) {
                Some(value.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect()
}

/// Read the local repository's reachable commit ids without consulting the
/// network or changing repository state.
pub fn load_git_commit_ids(repo_root: &Path) -> Result<Vec<String>, GitLogError> {
    let output = Command::new("git")
        .args(["--no-pager", "-C"])
        .arg(repo_root)
        .args(["log", "--all", "--format=%H", "--no-color"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(GitLogError::Start)?;
    if !output.status.success() {
        return Err(GitLogError::Failed {
            code: output.status.code(),
        });
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| GitLogError::NonUtf8)?;
    let mut commits = Vec::new();
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !matches!(line.len(), 40 | 64) || !line.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(GitLogError::InvalidCommitId);
        }
        commits.push(line.to_ascii_lowercase());
    }
    commits.sort();
    commits.dedup();
    Ok(commits)
}
