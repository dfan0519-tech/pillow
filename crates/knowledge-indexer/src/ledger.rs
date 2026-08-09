//! K0e: non-interactive ledger write/read helpers for the knowledge-indexer CLI.
//!
//! Wraps K0d storage (`dispatch_task` / `set_task_status` / `record_verdict`) so
//! the controller can record dispatch and acceptance as a byproduct (ADR-0004 §2).
//! Never stores or prints lease ids, session ids, or host user paths.

use std::path::{Path, PathBuf};

use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    DispatchTask, KnowledgeError, KnowledgeTask, KnowledgeVerdict, RecordVerdict, SetTaskStatus,
    SqliteKnowledgeRepository, SqliteProjectRepository, TaskStatus, VerdictOutcome,
};

/// Options shared by ledger CLI commands.
#[derive(Debug, Clone)]
pub struct LedgerDbOptions {
    pub db_path: PathBuf,
    /// Human project name (resolved to FK `projects.id`).
    pub project_name: String,
    /// Optional root used when creating the project row (FK). Portable-ish.
    pub project_root: Option<PathBuf>,
}

/// Input for `task dispatch`.
#[derive(Debug, Clone)]
pub struct DispatchInput {
    pub title: String,
    pub brief_path: Option<String>,
    /// Slot label only (e.g. `grok3`).
    pub worker_label: String,
}

/// Input for `task complete` (or other explicit status set).
#[derive(Debug, Clone)]
pub struct CompleteInput {
    pub task_id: String,
    pub status: TaskStatus,
    pub completion_signal: Option<String>,
}

/// Input for `verdict record`.
#[derive(Debug, Clone)]
pub struct VerdictInput {
    pub task_id: Option<String>,
    pub outcome: VerdictOutcome,
    pub gate_result: Option<String>,
    pub e2e_result: Option<String>,
    pub summary: String,
    pub residuals: Option<String>,
}

/// Snapshot used by `ledger status` and stale detection.
#[derive(Debug, Clone)]
pub struct LedgerSnapshot {
    pub project_id: String,
    pub project_name: String,
    pub tasks: Vec<KnowledgeTask>,
    pub verdicts: Vec<KnowledgeVerdict>,
    pub completed_without_verdict: Vec<KnowledgeTask>,
}

impl LedgerSnapshot {
    pub fn dispatched(&self) -> Vec<&KnowledgeTask> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Dispatched)
            .collect()
    }

    pub fn pending_acceptance(&self) -> &[KnowledgeTask] {
        &self.completed_without_verdict
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerCliError {
    #[error("project name must be nonempty")]
    InvalidProjectName,
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("project store error")]
    Project,
    #[error("knowledge store: {0}")]
    Knowledge(#[from] KnowledgeError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Open knowledge + project repos, ensure project row exists (create if missing).
pub fn ensure_project_id(opts: &LedgerDbOptions) -> Result<String, LedgerCliError> {
    if opts.project_name.trim().is_empty() {
        return Err(LedgerCliError::InvalidProjectName);
    }
    let projects =
        SqliteProjectRepository::open(&opts.db_path).map_err(|_| LedgerCliError::Project)?;
    let existing = projects.list().map_err(|_| LedgerCliError::Project)?;
    if let Some(p) = existing.into_iter().find(|p| p.name == opts.project_name) {
        return Ok(p.id.0.to_string());
    }
    // Create: need a root path for the project row.
    let root = opts
        .project_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let root_str = root
        .canonicalize()
        .unwrap_or(root)
        .to_string_lossy()
        .into_owned();
    let project = projects
        .create(CreateProject {
            name: opts.project_name.clone(),
            root_path: root_str,
        })
        .map_err(|_| LedgerCliError::Project)?;
    Ok(project.id.0.to_string())
}

/// Resolve project id by name; do **not** create (for status / stale / complete).
pub fn resolve_project_id(opts: &LedgerDbOptions) -> Result<String, LedgerCliError> {
    if opts.project_name.trim().is_empty() {
        return Err(LedgerCliError::InvalidProjectName);
    }
    let projects =
        SqliteProjectRepository::open(&opts.db_path).map_err(|_| LedgerCliError::Project)?;
    let existing = projects.list().map_err(|_| LedgerCliError::Project)?;
    existing
        .into_iter()
        .find(|p| p.name == opts.project_name)
        .map(|p| p.id.0.to_string())
        .ok_or_else(|| LedgerCliError::ProjectNotFound(opts.project_name.clone()))
}

/// Open knowledge repository at `db_path`.
pub fn open_knowledge(db_path: &Path) -> Result<SqliteKnowledgeRepository, LedgerCliError> {
    Ok(SqliteKnowledgeRepository::open(db_path)?)
}

/// Record a dispatch (byproduct of controller dispatch).
pub fn dispatch_task(
    opts: &LedgerDbOptions,
    input: DispatchInput,
) -> Result<KnowledgeTask, LedgerCliError> {
    let project_id = ensure_project_id(opts)?;
    let repo = open_knowledge(&opts.db_path)?;
    let task = repo.dispatch_task(DispatchTask {
        project_id,
        title: input.title,
        brief_path: input.brief_path,
        worker_label: input.worker_label,
    })?;
    Ok(task)
}

/// Explicit status update (complete / blocked / abandoned). Never auto-migrated.
pub fn set_task_status(
    opts: &LedgerDbOptions,
    input: CompleteInput,
) -> Result<KnowledgeTask, LedgerCliError> {
    // Ensure DB opens (project may already exist via task FK).
    let _ = open_knowledge(&opts.db_path)?;
    let repo = open_knowledge(&opts.db_path)?;
    let task = repo.set_task_status(SetTaskStatus {
        task_id: input.task_id,
        status: input.status,
        completion_signal: input.completion_signal,
    })?;
    Ok(task)
}

/// Record a controller acceptance verdict.
pub fn record_verdict(
    opts: &LedgerDbOptions,
    input: VerdictInput,
) -> Result<KnowledgeVerdict, LedgerCliError> {
    let project_id = ensure_project_id(opts)?;
    let repo = open_knowledge(&opts.db_path)?;
    let verdict = repo.record_verdict(RecordVerdict {
        project_id,
        task_id: input.task_id,
        outcome: input.outcome,
        gate_result: input.gate_result,
        e2e_result: input.e2e_result,
        summary: input.summary,
        residuals: input.residuals,
    })?;
    Ok(verdict)
}

/// Load full ledger snapshot for a project (status + stale inputs).
pub fn load_snapshot(opts: &LedgerDbOptions) -> Result<LedgerSnapshot, LedgerCliError> {
    let project_id = resolve_project_id(opts)?;
    let repo = open_knowledge(&opts.db_path)?;
    let tasks = repo.list_tasks_by_project(&project_id)?;
    let verdicts = repo.list_verdicts_by_project(&project_id)?;
    let completed_without_verdict = repo.list_completed_without_verdict(&project_id)?;
    Ok(LedgerSnapshot {
        project_id,
        project_name: opts.project_name.clone(),
        tasks,
        verdicts,
        completed_without_verdict,
    })
}

/// Format `ledger status` stdout (slot labels only; no lease/session/host paths).
pub fn format_ledger_status(snap: &LedgerSnapshot) -> String {
    let mut out = String::new();
    out.push_str("# Ledger status\n\n");
    out.push_str(&format!(
        "project=`{}` project_id=`{}`\n\n",
        snap.project_name, snap.project_id
    ));
    out.push_str(
        "Privacy: slot labels and portable paths only — never lease ids, session ids, or host user paths.\n\n",
    );

    let inflight = snap.dispatched();
    out.push_str("## In-flight dispatch (在途派单)\n\n");
    out.push_str(&format!("**Count:** {}\n\n", inflight.len()));
    if inflight.is_empty() {
        out.push_str("_None recorded._\n\n");
    } else {
        out.push_str("| task_id | title | slot | brief | dispatched_at |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for t in &inflight {
            let brief = t
                .brief_path
                .as_deref()
                .map(|p| format!("`{p}`"))
                .unwrap_or_else(|| "_none_".into());
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {brief} | `{}` |\n",
                t.task_id,
                escape_cell(&t.title),
                t.worker_label,
                t.dispatched_at.to_rfc3339()
            ));
        }
        out.push('\n');
    }

    let pending = snap.pending_acceptance();
    out.push_str("## Pending acceptance (已完成但未验收)\n\n");
    out.push_str(&format!("**Count:** {}\n\n", pending.len()));
    if pending.is_empty() {
        out.push_str("_None — every completed task has a verdict, or none completed._\n\n");
    } else {
        out.push_str("| task_id | title | slot | completion_signal |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for t in pending {
            let signal = t
                .completion_signal
                .as_deref()
                .map(escape_cell)
                .unwrap_or_else(|| "_none_".into());
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {signal} |\n",
                t.task_id,
                escape_cell(&t.title),
                t.worker_label
            ));
        }
        out.push('\n');
    }

    out.push_str("## Recent verdicts (最近验收结论)\n\n");
    out.push_str(&format!("**Count:** {}\n\n", snap.verdicts.len()));
    if snap.verdicts.is_empty() {
        out.push_str("_None recorded._\n\n");
    } else {
        // Already ordered decided_at DESC from storage.
        let limit = snap.verdicts.len().min(20);
        for v in snap.verdicts.iter().take(limit) {
            let task_note = match v.task_id.as_deref() {
                Some(tid) => match snap.tasks.iter().find(|t| t.task_id == tid) {
                    Some(t) => format!("task=`{tid}` ({}) slot=`{}`", t.title, t.worker_label),
                    None => format!("task=`{tid}` (missing)"),
                },
                None => "task=_none_ (controller-self)".to_owned(),
            };
            out.push_str(&format!(
                "- outcome=`{}` decided_at=`{}` {task_note}\n",
                v.outcome.as_str(),
                v.decided_at.to_rfc3339()
            ));
            out.push_str(&format!("  summary: {}\n", v.summary));
            if let Some(ref g) = v.gate_result {
                out.push_str(&format!("  gate: `{g}`\n"));
            }
            if let Some(ref e) = v.e2e_result {
                out.push_str(&format!("  e2e: `{e}`\n"));
            }
            if let Some(ref r) = v.residuals {
                out.push_str(&format!("  residuals: {r}\n"));
            }
        }
        out.push('\n');
    }

    // Status inventory
    out.push_str("## Task status inventory\n\n");
    let mut dispatched = 0usize;
    let mut completed = 0usize;
    let mut blocked = 0usize;
    let mut abandoned = 0usize;
    for t in &snap.tasks {
        match t.status {
            TaskStatus::Dispatched => dispatched += 1,
            TaskStatus::Completed => completed += 1,
            TaskStatus::Blocked => blocked += 1,
            TaskStatus::Abandoned => abandoned += 1,
        }
    }
    out.push_str(&format!(
        "| status | count |\n| --- | ---: |\n| `dispatched` | {dispatched} |\n| `completed` | {completed} |\n| `blocked` | {blocked} |\n| `abandoned` | {abandoned} |\n"
    ));
    out
}

/// One-line machine-friendly confirmation for write commands.
pub fn format_task_recorded(task: &KnowledgeTask, action: &str) -> String {
    format!(
        "ok {action} task_id={} status={} slot={} title={}",
        task.task_id,
        task.status.as_str(),
        task.worker_label,
        task.title
    )
}

pub fn format_verdict_recorded(v: &KnowledgeVerdict) -> String {
    let task = v.task_id.as_deref().unwrap_or("_none_");
    format!(
        "ok verdict_recorded verdict_id={} outcome={} task_id={}",
        v.verdict_id,
        v.outcome.as_str(),
        task
    )
}

fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn privacy_needles() -> (String, String, String, String) {
        let lease = format!("{}-{}-{}", "grok", "lease", "abcdef012345");
        let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
        let win = format!("{}:\\{}\\{}", "C", "Users", "someone");
        let msys = format!("/{}/{}/{}", "c", "Users", "someone");
        (lease, session, win, msys)
    }

    fn temp_opts(temp: &tempfile::TempDir) -> LedgerDbOptions {
        LedgerDbOptions {
            db_path: temp.path().join("ledger.db"),
            project_name: "pillow".into(),
            project_root: Some(temp.path().to_path_buf()),
        }
    }

    #[test]
    fn dispatch_complete_verdict_round_trip_unit() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path()).unwrap();
        let opts = temp_opts(&temp);

        let task = dispatch_task(
            &opts,
            DispatchInput {
                title: "K0e unit task".into(),
                brief_path: Some("docs/execution/k0e-brief.md".into()),
                worker_label: "grok3".into(),
            },
        )
        .unwrap();
        assert_eq!(task.status, TaskStatus::Dispatched);
        assert_eq!(task.worker_label, "grok3");

        let done = set_task_status(
            &opts,
            CompleteInput {
                task_id: task.task_id.clone(),
                status: TaskStatus::Completed,
                completion_signal: Some("K0E-LEDGER-CLI-COMPLETE".into()),
            },
        )
        .unwrap();
        assert_eq!(done.status, TaskStatus::Completed);

        let v = record_verdict(
            &opts,
            VerdictInput {
                task_id: Some(task.task_id.clone()),
                outcome: VerdictOutcome::Accepted,
                gate_result: Some("exit 0".into()),
                e2e_result: None,
                summary: "Accepted unit round-trip".into(),
                residuals: Some("none".into()),
            },
        )
        .unwrap();
        assert_eq!(v.outcome, VerdictOutcome::Accepted);

        let snap = load_snapshot(&opts).unwrap();
        assert!(snap.dispatched().is_empty());
        assert!(snap.pending_acceptance().is_empty());
        assert_eq!(snap.verdicts.len(), 1);

        let status = format_ledger_status(&snap);
        let (lease, session, win, msys) = privacy_needles();
        assert!(!status.contains(&lease));
        assert!(!status.contains(&session));
        assert!(!status.contains(&win));
        assert!(!status.contains(&msys));
        assert!(status.contains("grok3"));
    }
}
