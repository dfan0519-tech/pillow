//! Read-only Tauri host boundary for the built-in librarian audit.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use keeper_knowledge_indexer::{
    LedgerSnapshot, LibrarianAuditInput, LibrarianCheck, LibrarianCheckKind, load_git_commit_ids,
    load_snapshot_read_only, run_librarian_audit as run_indexer_librarian_audit,
};
use keeper_project::{Project, validate_portable_path};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

const LEDGER_PATH: &str = ".superpowers/ledger/pillow.db";
const HANDOFF_DIRECTORY: &str = "docs/handoff";
const HANDOFF_SUFFIX: &str = "-pillow-controller-handoff.md";
const STALE_AFTER_HOURS: i64 = 24;
const MAX_SUMMARY_CHARS: usize = 1_000;
const MAX_FINDINGS: usize = 256;

#[derive(Debug, Clone)]
pub struct LibrarianAuditProject {
    pub project_id: String,
    pub project_name: String,
    pub root: PathBuf,
}

impl From<Project> for LibrarianAuditProject {
    fn from(project: Project) -> Self {
        Self {
            project_id: project.id.0.to_string(),
            project_name: project.name,
            root: PathBuf::from(project.root_path),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LibrarianAuditHandoff {
    pub label: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The shared `Unavailable` suffix is the point: each variant names a distinct
/// missing source, and the brief requires them to stay separate so a missing
/// ledger is never reported as a healthy audit. Collapsing the names would
/// make call sites read as if the source were the error.
#[allow(clippy::enum_variant_names)]
pub enum LibrarianAuditSourceError {
    LedgerUnavailable,
    HandoffUnavailable,
    GitUnavailable,
}

pub trait LibrarianAuditHostContext {
    fn now(&self) -> DateTime<Utc>;

    fn load_snapshot(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<LedgerSnapshot, LibrarianAuditSourceError>;

    fn load_handoff(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<LibrarianAuditHandoff, LibrarianAuditSourceError>;

    fn load_git_commit_ids(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<Vec<String>, LibrarianAuditSourceError>;
}

pub struct SystemLibrarianAuditHostContext;

impl LibrarianAuditHostContext for SystemLibrarianAuditHostContext {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn load_snapshot(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<LedgerSnapshot, LibrarianAuditSourceError> {
        load_snapshot_read_only(&project.root.join(LEDGER_PATH), &project.project_name)
            .map_err(|_| LibrarianAuditSourceError::LedgerUnavailable)
    }

    fn load_handoff(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<LibrarianAuditHandoff, LibrarianAuditSourceError> {
        load_latest_handoff(&project.root)
    }

    fn load_git_commit_ids(
        &self,
        project: &LibrarianAuditProject,
    ) -> Result<Vec<String>, LibrarianAuditSourceError> {
        load_git_commit_ids(&project.root).map_err(|_| LibrarianAuditSourceError::GitUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibrarianAuditCommandError {
    pub code: &'static str,
    pub message: &'static str,
}

impl LibrarianAuditCommandError {
    fn project_unavailable() -> Self {
        Self {
            code: "librarian_audit_project_unavailable",
            message: "The requested project is unavailable.",
        }
    }

    fn from_source(error: LibrarianAuditSourceError) -> Self {
        let message = match error {
            LibrarianAuditSourceError::LedgerUnavailable => "Project audit ledger is unavailable.",
            LibrarianAuditSourceError::HandoffUnavailable => {
                "Project audit handoff document is unavailable or invalid."
            }
            LibrarianAuditSourceError::GitUnavailable => {
                "Project audit Git history is unavailable."
            }
        };
        Self {
            code: "librarian_audit_unavailable",
            message,
        }
    }

    fn audit_failed() -> Self {
        Self {
            code: "librarian_audit_failed",
            message: "Project audit failed.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarianAuditCheckIdDto {
    InflightHandoffConsistency,
    PendingAcceptanceVisibility,
    HandoffLedgerReferences,
    InflightInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarianAuditStatusDto {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibrarianAuditFindingDto {
    pub code: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibrarianAuditCheckDto {
    pub id: LibrarianAuditCheckIdDto,
    pub status: LibrarianAuditStatusDto,
    pub summary: String,
    pub findings: Vec<LibrarianAuditFindingDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarianAuditReportDto {
    pub project_id: String,
    pub outcome: LibrarianAuditStatusDto,
    pub checked_at: String,
    pub checks: Vec<LibrarianAuditCheckDto>,
}

#[tauri::command]
pub fn run_librarian_audit(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<LibrarianAuditReportDto, LibrarianAuditCommandError> {
    let project = state
        .get_registered_project(&project_id)
        .map_err(|_| LibrarianAuditCommandError::project_unavailable())?;
    run_librarian_audit_for_project(&project.into(), &SystemLibrarianAuditHostContext)
}

pub fn run_librarian_audit_for_project(
    project: &LibrarianAuditProject,
    context: &dyn LibrarianAuditHostContext,
) -> Result<LibrarianAuditReportDto, LibrarianAuditCommandError> {
    if project.project_id.is_empty() || project.project_id.chars().count() > 128 {
        return Err(LibrarianAuditCommandError::project_unavailable());
    }

    let now = context.now();
    let snapshot = context
        .load_snapshot(project)
        .map_err(LibrarianAuditCommandError::from_source)?;
    let handoff = context
        .load_handoff(project)
        .map_err(LibrarianAuditCommandError::from_source)?;
    let git_commit_ids = context
        .load_git_commit_ids(project)
        .map_err(LibrarianAuditCommandError::from_source)?;
    let report = run_indexer_librarian_audit(LibrarianAuditInput {
        snapshot: &snapshot,
        handoff_markdown: &handoff.markdown,
        handoff_label: &handoff.label,
        now,
        stale_after: TimeDelta::hours(STALE_AFTER_HOURS),
        git_commit_ids: &git_commit_ids,
    })
    .map_err(|_| LibrarianAuditCommandError::audit_failed())?;

    let document = required_check(&report, LibrarianCheckKind::DocumentLedgerConsistency)?;
    let pending = required_check(&report, LibrarianCheckKind::CompletedWithoutVerdict)?;
    let commits = required_check(&report, LibrarianCheckKind::AcceptedVerdictCommit)?;
    let stale = required_check(&report, LibrarianCheckKind::StaleInflight)?;

    let inflight_mismatches = findings_with_tag(document, "inflight_count_mismatch");
    let ghost_references = findings_with_tag(document, "doc_task_missing_from_ledger");
    let mut reference_findings: Vec<_> = ghost_references
        .iter()
        .map(|finding| (document.kind, *finding))
        .collect();
    reference_findings.extend(
        commits
            .findings
            .iter()
            .map(|finding| (commits.kind, finding.as_str())),
    );

    let checks = vec![
        build_check(
            LibrarianAuditCheckIdDto::InflightHandoffConsistency,
            format!(
                "inflight_count_mismatches={} {}",
                inflight_mismatches.len(),
                document.summary
            ),
            inflight_mismatches
                .into_iter()
                .map(|finding| (document.kind, finding))
                .collect(),
        ),
        build_check(
            LibrarianAuditCheckIdDto::PendingAcceptanceVisibility,
            pending.summary.clone(),
            pending
                .findings
                .iter()
                .map(|finding| (pending.kind, finding.as_str()))
                .collect(),
        ),
        build_check(
            LibrarianAuditCheckIdDto::HandoffLedgerReferences,
            format!(
                "handoff_ledger_findings={} {}",
                ghost_references.len(),
                commits.summary
            ),
            reference_findings,
        ),
        build_check(
            LibrarianAuditCheckIdDto::InflightInventory,
            format!(
                "inflight_tasks={} {}",
                snapshot.dispatched().len(),
                stale.summary
            ),
            stale
                .findings
                .iter()
                .map(|finding| (stale.kind, finding.as_str()))
                .collect(),
        ),
    ];
    let outcome = if checks
        .iter()
        .any(|check| check.status == LibrarianAuditStatusDto::Fail)
    {
        LibrarianAuditStatusDto::Fail
    } else {
        LibrarianAuditStatusDto::Pass
    };

    Ok(LibrarianAuditReportDto {
        project_id: project.project_id.clone(),
        outcome,
        checked_at: report.now.to_rfc3339(),
        checks,
    })
}

fn load_latest_handoff(root: &Path) -> Result<LibrarianAuditHandoff, LibrarianAuditSourceError> {
    let directory = root.join(HANDOFF_DIRECTORY);
    let entries =
        fs::read_dir(directory).map_err(|_| LibrarianAuditSourceError::HandoffUnavailable)?;
    let mut labels = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| LibrarianAuditSourceError::HandoffUnavailable)?;
        if !entry
            .file_type()
            .map_err(|_| LibrarianAuditSourceError::HandoffUnavailable)?
            .is_file()
        {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let label = format!("{HANDOFF_DIRECTORY}/{name}");
        if name.ends_with(HANDOFF_SUFFIX) && validate_portable_path(&label).is_ok() {
            labels.push(label);
        }
    }
    labels.sort_unstable();
    let label = labels
        .pop()
        .ok_or(LibrarianAuditSourceError::HandoffUnavailable)?;
    let markdown = fs::read_to_string(root.join(&label))
        .map_err(|_| LibrarianAuditSourceError::HandoffUnavailable)?;
    Ok(LibrarianAuditHandoff { label, markdown })
}

fn required_check(
    report: &keeper_knowledge_indexer::LibrarianAuditReport,
    kind: LibrarianCheckKind,
) -> Result<&LibrarianCheck, LibrarianAuditCommandError> {
    report
        .check(kind)
        .ok_or_else(LibrarianAuditCommandError::audit_failed)
}

fn findings_with_tag<'a>(check: &'a LibrarianCheck, code: &str) -> Vec<&'a str> {
    check
        .findings
        .iter()
        .filter(|finding| tagged_value(finding, "code=") == Some(code))
        .map(String::as_str)
        .collect()
}

fn build_check(
    id: LibrarianAuditCheckIdDto,
    summary: String,
    source_findings: Vec<(LibrarianCheckKind, &str)>,
) -> LibrarianAuditCheckDto {
    let status = if source_findings.is_empty() {
        LibrarianAuditStatusDto::Pass
    } else {
        LibrarianAuditStatusDto::Fail
    };
    let mut findings: Vec<_> = source_findings
        .iter()
        .take(MAX_FINDINGS)
        .map(|(kind, finding)| map_finding(*kind, finding))
        .collect();
    if source_findings.len() > MAX_FINDINGS {
        let omitted = source_findings.len() - (MAX_FINDINGS - 1);
        findings.truncate(MAX_FINDINGS - 1);
        findings.push(LibrarianAuditFindingDto {
            code: "additional_findings".into(),
            summary: format!("additional_findings={omitted}"),
        });
    }
    LibrarianAuditCheckDto {
        id,
        status,
        summary: bounded_text(&summary),
        findings,
    }
}

fn map_finding(kind: LibrarianCheckKind, raw: &str) -> LibrarianAuditFindingDto {
    let code = match kind {
        LibrarianCheckKind::StaleInflight => "stale_inflight",
        LibrarianCheckKind::CompletedWithoutVerdict => "completed_without_verdict",
        LibrarianCheckKind::DocumentLedgerConsistency => {
            tagged_value(raw, "code=").unwrap_or("document_ledger_consistency")
        }
        LibrarianCheckKind::AcceptedVerdictCommit => {
            tagged_value(raw, "reason=").unwrap_or("accepted_verdict_commit")
        }
    };
    let allowed = raw
        .split_ascii_whitespace()
        .filter(|token| {
            [
                "code=",
                "reason=",
                "task_id=",
                "slot=",
                "commit=",
                "age_seconds=",
                "threshold_seconds=",
                "claimed=",
                "ledger_dispatched=",
                "mention_kind=",
                "marker_count=",
                "expected=",
                "matches=",
            ]
            .iter()
            .any(|prefix| token.starts_with(prefix))
        })
        // An allowed key does not make its value safe. Slot labels and task
        // titles are free text from the ledger, so `slot=<anything>` passed the
        // key filter and carried a host path straight into the DTO. Vet the
        // value too, and drop the whole token when it fails.
        .filter(|token| token_value_is_publishable(token))
        .collect::<Vec<_>>()
        .join(" ");
    LibrarianAuditFindingDto {
        code: code.to_owned(),
        summary: bounded_text(if allowed.is_empty() {
            "audit finding recorded"
        } else {
            &allowed
        }),
    }
}

/// True when a `key=value` token's value is safe to publish.
///
/// Conservative on purpose: identifiers, numbers and backtick-quoted ids only.
/// Anything carrying a path separator, drive letter, whitespace or quote is
/// dropped rather than trimmed — a partially redacted host path is still a
/// host path, and this DTO reaches the renderer.
fn token_value_is_publishable(token: &str) -> bool {
    let Some((_, value)) = token.split_once('=') else {
        return false;
    };
    let value = value.trim_matches('`');
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    if value.contains('/') || value.contains('\\') || value.contains(':') {
        return false;
    }
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'<' | b'>'))
}

fn tagged_value<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.split_ascii_whitespace()
        .find_map(|token| token.strip_prefix(prefix))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn bounded_text(text: &str) -> String {
    text.chars().take(MAX_SUMMARY_CHARS).collect()
}
