mod conversation;
mod conversation_rows;
mod exec_proposal;
mod migrations;
mod resource_recent;
mod settings;
mod tool_call;
mod write_proposal;

pub use conversation::SqliteConversationRepository;
pub use exec_proposal::{
    EXEC_RESULT_JSON_BYTE_CAP, ExecDecisionOutcome, ExecProposalReserveInput,
    ExecProposalStoreError, ExecReservationOutcome, ExecRunClaimOutcome, ExecTerminalReceipt,
    MAX_RUN_LEASE_MS, SqliteExecProposalRepository, StoredExecProposal,
};
pub use resource_recent::{
    MAX_RECENT_LIST, ResourceRecent, ResourceRecentError, SqliteResourceRecentRepository,
};
pub use settings::{
    ENV_GROK_EXECUTABLE, ENV_GROK_PROFILE, ENV_RUNTIME_ADAPTER, KEY_GROK_EXECUTABLE,
    KEY_GROK_PROFILE, KEY_RUNTIME_ADAPTER, ResolvedRuntimeSettings, ResolvedSetting,
    RuntimeAdapterSetting, SettingScope, SettingSource, SettingsError, SettingsUpdatedBy,
    SqliteSettingsRepository, StoredSetting, UpsertSetting, process_env_map,
};
pub use tool_call::{
    SqliteToolCallRepository, StoredToolCall, ToolCallReservationOutcome, ToolCallStoreError,
    ToolExecutionClaimOutcome,
};
pub use write_proposal::{
    MAX_APPLY_LEASE_MS, RESULT_JSON_BYTE_CAP, SqliteWriteProposalRepository, StoredWriteProposal,
    WriteApplyClaimOutcome, WriteApplyErrorCode, WriteApplyReceipt, WriteApplyReconcileObservation,
    WriteDecisionOutcome, WriteProposalReserveInput, WriteProposalStoreError,
    WriteReservationOutcome,
};

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_project::{CreateProject, Project, ProjectId, ProjectRepository, ProjectTrust};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, ffi, params};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProjectStoreError {
    #[error("a project already exists for this canonical root")]
    DuplicateRoot,
    #[error("failed to canonicalize project root {path}: {source}")]
    Canonicalize {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("SQLite project store error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("project store lock was poisoned")]
    LockPoisoned,
}

pub struct SqliteProjectRepository {
    connection: Mutex<Connection>,
}

impl SqliteProjectRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectStoreError> {
        let connection = Connection::open(path)?;
        migrations::apply(&connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create(&self, input: CreateProject) -> Result<Project, ProjectStoreError> {
        ProjectRepository::create(self, input)
    }

    pub fn list(&self) -> Result<Vec<Project>, ProjectStoreError> {
        ProjectRepository::list(self)
    }

    pub fn get(&self, id: ProjectId) -> Result<Option<Project>, ProjectStoreError> {
        ProjectRepository::get(self, id)
    }

    pub fn find_by_canonical_root(
        &self,
        root_path: &str,
    ) -> Result<Option<Project>, ProjectStoreError> {
        ProjectRepository::find_by_canonical_root(self, root_path)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, ProjectStoreError> {
        self.connection
            .lock()
            .map_err(|_| ProjectStoreError::LockPoisoned)
    }
}

impl ProjectRepository for SqliteProjectRepository {
    type Error = ProjectStoreError;

    fn create(&self, input: CreateProject) -> Result<Project, Self::Error> {
        let canonical_root_path = canonical_root_path(&input.root_path)?;
        let connection = self.connection()?;
        let project = Project {
            id: ProjectId(Uuid::new_v4()),
            name: input.name,
            root_path: input.root_path,
            trust: ProjectTrust::Untrusted,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        connection
            .execute(
                "INSERT INTO projects (
                id, name, root_path, canonical_root_path, trust_state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    project.id.0.to_string(),
                    project.name,
                    project.root_path,
                    canonical_root_path,
                    trust_state(project.trust),
                    project.created_at.to_rfc3339(),
                    project.updated_at.to_rfc3339(),
                ],
            )
            .map_err(map_unique_constraint)?;

        Ok(project)
    }

    fn list(&self) -> Result<Vec<Project>, Self::Error> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, root_path, trust_state, created_at, updated_at
             FROM projects
             ORDER BY created_at ASC, id ASC",
        )?;
        let projects = statement
            .query_map([], project_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(projects)
    }

    fn get(&self, id: ProjectId) -> Result<Option<Project>, Self::Error> {
        let connection = self.connection()?;
        let project = connection
            .query_row(
                "SELECT id, name, root_path, trust_state, created_at, updated_at
                 FROM projects WHERE id = ?1",
                [id.0.to_string()],
                project_from_row,
            )
            .optional()?;

        Ok(project)
    }

    fn find_by_canonical_root(&self, root_path: &str) -> Result<Option<Project>, Self::Error> {
        let canonical = canonical_root_path(root_path)?;
        let connection = self.connection()?;
        let project = connection
            .query_row(
                "SELECT id, name, root_path, trust_state, created_at, updated_at
                 FROM projects WHERE canonical_root_path = ?1",
                [canonical],
                project_from_row,
            )
            .optional()?;

        Ok(project)
    }
}

fn canonical_root_path(root_path: &str) -> Result<String, ProjectStoreError> {
    let path = Path::new(root_path);
    let canonical = path
        .canonicalize()
        .map_err(|source| ProjectStoreError::Canonicalize {
            path: root_path.to_owned(),
            source,
        })?;

    Ok(canonical.to_string_lossy().into_owned())
}

fn map_unique_constraint(error: rusqlite::Error) -> ProjectStoreError {
    match &error {
        rusqlite::Error::SqliteFailure(error, _)
            if error.extended_code == ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            ProjectStoreError::DuplicateRoot
        }
        _ => ProjectStoreError::Sqlite(error),
    }
}

fn trust_state(trust: ProjectTrust) -> &'static str {
    match trust {
        ProjectTrust::Untrusted => "untrusted",
        ProjectTrust::Trusted => "trusted",
    }
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    let trust: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let updated_at: String = row.get(5)?;

    Ok(Project {
        id: ProjectId(parse_uuid(&id, 0)?),
        name: row.get(1)?,
        root_path: row.get(2)?,
        trust: parse_trust_state(&trust)?,
        created_at: parse_timestamp(&created_at, 4)?,
        updated_at: parse_timestamp(&updated_at, 5)?,
    })
}

fn parse_uuid(value: &str, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn parse_trust_state(value: &str) -> rusqlite::Result<ProjectTrust> {
    match value {
        "untrusted" => Ok(ProjectTrust::Untrusted),
        "trusted" => Ok(ProjectTrust::Trusted),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            Type::Text,
            Box::new(InvalidTrustState(value.to_owned())),
        )),
    }
}

fn parse_timestamp(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

#[derive(Debug, Error)]
#[error("invalid project trust state: {0}")]
struct InvalidTrustState(String);

#[cfg(test)]
mod tests {
    use super::{ProjectStoreError, map_unique_constraint};
    use rusqlite::{Error, ffi};

    #[test]
    fn maps_unique_sqlite_constraints_to_duplicate_roots() {
        let error = Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_CONSTRAINT_UNIQUE), None);

        assert!(matches!(
            map_unique_constraint(error),
            ProjectStoreError::DuplicateRoot
        ));
    }

    #[test]
    fn preserves_non_unique_sqlite_constraints() {
        let error = Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_CONSTRAINT_CHECK), None);

        assert!(matches!(
            map_unique_constraint(error),
            ProjectStoreError::Sqlite(Error::SqliteFailure(sqlite_error, _))
                if sqlite_error.extended_code == ffi::SQLITE_CONSTRAINT_CHECK
        ));
    }
}
