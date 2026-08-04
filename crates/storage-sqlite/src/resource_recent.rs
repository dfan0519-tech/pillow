//! Durable, project-scoped recent resource paths.
//!
//! Path syntax is validated here; filesystem authorization is left to
//! `ProjectResourceCapability` in `keeper-resource`.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use thiserror::Error;

/// Caller list limit is clamped to this hard cap.
pub const MAX_RECENT_LIST: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRecent {
    pub project_id: String,
    pub path: String,
    pub opened_at: DateTime<Utc>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResourceRecentError {
    #[error("project id must be a nonempty portable identifier")]
    InvalidProjectId,
    #[error("resource path must be a portable relative path")]
    InvalidPath,
    #[error("SQLite resource recent store error")]
    Sqlite,
    #[error("resource recent store lock was poisoned")]
    LockPoisoned,
}

pub struct SqliteResourceRecentRepository {
    connection: Mutex<Connection>,
}

impl SqliteResourceRecentRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ResourceRecentError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Record (or refresh) a portable relative path for a project.
    pub fn record(&self, project_id: &str, path: &str) -> Result<(), ResourceRecentError> {
        validate_project_id(project_id)?;
        validate_portable_path(path)?;
        let now = Utc::now().to_rfc3339();
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO resource_recents (project_id, path, opened_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(project_id, path) DO UPDATE SET opened_at = excluded.opened_at",
                params![project_id, path, now],
            )
            .map_err(map_sqlite)?;
        Ok(())
    }

    /// List recents for a project, newest-first with path ASC tie-break.
    /// Caller `limit` is clamped to [`MAX_RECENT_LIST`].
    pub fn list(
        &self,
        project_id: &str,
        limit: u32,
    ) -> Result<Vec<ResourceRecent>, ResourceRecentError> {
        validate_project_id(project_id)?;
        let limit = limit.min(MAX_RECENT_LIST) as i64;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT project_id, path, opened_at
                 FROM resource_recents
                 WHERE project_id = ?1
                 ORDER BY opened_at DESC, path ASC
                 LIMIT ?2",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![project_id, limit], |row| {
                let project_id: String = row.get(0)?;
                let path: String = row.get(1)?;
                let opened_at: String = row.get(2)?;
                let opened_at = DateTime::parse_from_rfc3339(&opened_at)
                    .map(|ts| ts.with_timezone(&Utc))
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(ResourceRecent {
                    project_id,
                    path,
                    opened_at,
                })
            })
            .map_err(map_sqlite)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(map_sqlite)?);
        }
        Ok(results)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, ResourceRecentError> {
        self.connection
            .lock()
            .map_err(|_| ResourceRecentError::LockPoisoned)
    }
}

fn validate_project_id(project_id: &str) -> Result<(), ResourceRecentError> {
    if project_id.is_empty() {
        return Err(ResourceRecentError::InvalidProjectId);
    }
    Ok(())
}

/// Same portable relative path rules as the resource core: `.` for root, no
/// absolute/drive/backslash/`..`/`./` components.
fn validate_portable_path(path: &str) -> Result<(), ResourceRecentError> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > 4_096 {
        return Err(ResourceRecentError::InvalidPath);
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(ResourceRecentError::InvalidPath);
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ResourceRecentError::InvalidPath);
        }
        if component.len() > 255 {
            return Err(ResourceRecentError::InvalidPath);
        }
    }
    Ok(())
}

fn map_sqlite(_: rusqlite::Error) -> ResourceRecentError {
    ResourceRecentError::Sqlite
}
