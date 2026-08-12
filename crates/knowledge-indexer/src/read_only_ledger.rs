//! Read-only projection of the audit ledger.
//!
//! Unlike the write-capable repositories, this loader never applies schema
//! migrations and never creates a missing database. SQLite opens the
//! authoritative ledger with `SQLITE_OPEN_READ_ONLY`.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Utc};
use keeper_storage_sqlite::{KnowledgeTask, KnowledgeVerdict, TaskStatus, VerdictOutcome};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::ledger::LedgerSnapshot;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReadOnlyLedgerError {
    #[error("ledger database is unavailable")]
    DatabaseUnavailable,
    #[error("ledger project name is invalid")]
    InvalidProjectName,
    #[error("ledger project is unavailable")]
    ProjectUnavailable,
    #[error("ledger query failed")]
    QueryFailed,
    #[error("ledger contains invalid audit data")]
    InvalidData,
}

/// Load the task/verdict snapshot needed by the librarian audit without
/// creating, migrating, or updating the SQLite database.
pub fn load_snapshot_read_only(
    db_path: &Path,
    project_name: &str,
) -> Result<LedgerSnapshot, ReadOnlyLedgerError> {
    if !db_path.is_file() {
        return Err(ReadOnlyLedgerError::DatabaseUnavailable);
    }
    if project_name.trim().is_empty() {
        return Err(ReadOnlyLedgerError::InvalidProjectName);
    }

    let connection = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| ReadOnlyLedgerError::DatabaseUnavailable)?;

    let project_id = connection
        .query_row(
            "SELECT id
             FROM projects
             WHERE name = ?1
             ORDER BY created_at ASC, id ASC
             LIMIT 1",
            [project_name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| ReadOnlyLedgerError::QueryFailed)?
        .ok_or(ReadOnlyLedgerError::ProjectUnavailable)?;

    let tasks = load_tasks(&connection, &project_id)?;
    let verdicts = load_verdicts(&connection, &project_id)?;
    let verdict_task_ids: HashSet<&str> = verdicts
        .iter()
        .filter_map(|verdict| verdict.task_id.as_deref())
        .collect();
    let completed_without_verdict = tasks
        .iter()
        .filter(|task| {
            task.status == TaskStatus::Completed
                && !verdict_task_ids.contains(task.task_id.as_str())
        })
        .cloned()
        .collect();

    Ok(LedgerSnapshot {
        project_id,
        project_name: project_name.to_owned(),
        tasks,
        verdicts,
        completed_without_verdict,
    })
}

fn load_tasks(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<KnowledgeTask>, ReadOnlyLedgerError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, project_id, title, brief_path, worker_label,
                    dispatched_at, status, completion_signal
             FROM knowledge_tasks
             WHERE project_id = ?1
             ORDER BY dispatched_at DESC, task_id ASC",
        )
        .map_err(|_| ReadOnlyLedgerError::QueryFailed)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|_| ReadOnlyLedgerError::QueryFailed)?;

    let mut tasks = Vec::new();
    for row in rows {
        let (
            task_id,
            row_project_id,
            title,
            brief_path,
            worker_label,
            dispatched_at,
            status,
            completion_signal,
        ) = row.map_err(|_| ReadOnlyLedgerError::QueryFailed)?;
        tasks.push(KnowledgeTask {
            task_id,
            project_id: row_project_id,
            title,
            brief_path,
            worker_label,
            dispatched_at: parse_time(&dispatched_at)?,
            status: TaskStatus::parse(&status).map_err(|_| ReadOnlyLedgerError::InvalidData)?,
            completion_signal,
        });
    }
    Ok(tasks)
}

fn load_verdicts(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<KnowledgeVerdict>, ReadOnlyLedgerError> {
    let mut statement = connection
        .prepare(
            "SELECT verdict_id, task_id, project_id, outcome, gate_result,
                    e2e_result, summary, residuals, decided_at
             FROM knowledge_verdicts
             WHERE project_id = ?1
             ORDER BY decided_at DESC, verdict_id ASC",
        )
        .map_err(|_| ReadOnlyLedgerError::QueryFailed)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|_| ReadOnlyLedgerError::QueryFailed)?;

    let mut verdicts = Vec::new();
    for row in rows {
        let (
            verdict_id,
            task_id,
            row_project_id,
            outcome,
            gate_result,
            e2e_result,
            summary,
            residuals,
            decided_at,
        ) = row.map_err(|_| ReadOnlyLedgerError::QueryFailed)?;
        verdicts.push(KnowledgeVerdict {
            verdict_id,
            task_id,
            project_id: row_project_id,
            outcome: VerdictOutcome::parse(&outcome)
                .map_err(|_| ReadOnlyLedgerError::InvalidData)?,
            gate_result,
            e2e_result,
            summary,
            residuals,
            decided_at: parse_time(&decided_at)?,
        });
    }
    Ok(verdicts)
}

fn parse_time(raw: &str) -> Result<DateTime<Utc>, ReadOnlyLedgerError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ReadOnlyLedgerError::InvalidData)
}
