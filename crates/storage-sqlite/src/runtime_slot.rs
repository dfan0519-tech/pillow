//! Project-scoped, non-secret runtime-slot metadata.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_project::ProjectId;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use thiserror::Error;
use uuid::Uuid;

const DISPLAY_NAME_MAX_CHARS: usize = 120;
const CONFIG_VALUE_MAX_CHARS: usize = 4_096;
const ARG_COUNT_MAX: usize = 128;
const ARG_MAX_CHARS: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeSlotId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSlotGovernance {
    Full,
    None,
}

impl RuntimeSlotGovernance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrokSlotAuth {
    Login,
    ApiKey {
        base_url: String,
        model_id: String,
        secret_ref: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSlotKind {
    GovernedGrok {
        auth: GrokSlotAuth,
    },
    ExternalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSlot {
    pub id: RuntimeSlotId,
    pub project_id: ProjectId,
    pub display_name: String,
    pub governance: RuntimeSlotGovernance,
    pub kind: RuntimeSlotKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRuntimeSlot {
    pub project_id: ProjectId,
    pub display_name: String,
    pub governance: RuntimeSlotGovernance,
    pub kind: RuntimeSlotKind,
}

/// Metadata update for an existing governed Grok API-key slot.
///
/// `secret_ref` is always the authoritative reference after the update: callers
/// that keep the prior key pass the existing opaque ref; callers that rotate
/// pass a newly minted ref after writing the new ciphertext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateGrokApiKeySlot {
    pub project_id: ProjectId,
    pub slot_id: RuntimeSlotId,
    pub display_name: String,
    pub base_url: String,
    pub model_id: String,
    pub secret_ref: String,
}

#[derive(Debug, Error)]
pub enum RuntimeSlotStoreError {
    #[error("runtime slot shape is invalid")]
    InvalidShape,
    #[error("runtime slot was not found in this project")]
    NotFound,
    #[error("stored runtime slot is invalid")]
    InvalidStoredSlot,
    #[error("SQLite runtime slot store error")]
    Sqlite(#[source] rusqlite::Error),
    #[error("runtime slot store lock was poisoned")]
    LockPoisoned,
}

impl RuntimeSlotStoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidShape => "runtime_slot_shape_invalid",
            Self::NotFound => "runtime_slot_not_found",
            Self::InvalidStoredSlot => "runtime_slot_stored_invalid",
            Self::Sqlite(_) => "runtime_slot_store_unavailable",
            Self::LockPoisoned => "runtime_slot_store_unavailable",
        }
    }
}

impl From<rusqlite::Error> for RuntimeSlotStoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

pub struct SqliteRuntimeSlotRepository {
    connection: Mutex<Connection>,
}

impl SqliteRuntimeSlotRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RuntimeSlotStoreError> {
        let connection = Connection::open(path).map_err(RuntimeSlotStoreError::Sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(RuntimeSlotStoreError::Sqlite)?;
        crate::migrations::apply(&connection).map_err(RuntimeSlotStoreError::Sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create(&self, input: NewRuntimeSlot) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_new_slot(&input)?;
        let now = Utc::now();
        let slot = RuntimeSlot {
            id: RuntimeSlotId(Uuid::new_v4()),
            project_id: input.project_id,
            display_name: input.display_name,
            governance: input.governance,
            kind: input.kind,
            created_at: now,
            updated_at: now,
        };
        let encoded = EncodedSlot::from_slot(&slot)?;
        self.connection()?.execute(
            "INSERT INTO runtime_slots (
                slot_id, project_id, display_name, kind, governance, auth_mode,
                base_url, model_id, secret_ref, executable_path,
                launch_args_json, status_args_json, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
             )",
            params![
                slot.id.0.to_string(),
                slot.project_id.0.to_string(),
                slot.display_name,
                encoded.kind,
                slot.governance.as_str(),
                encoded.auth_mode,
                encoded.base_url,
                encoded.model_id,
                encoded.secret_ref,
                encoded.executable_path,
                encoded.launch_args_json,
                encoded.status_args_json,
                slot.created_at.to_rfc3339(),
                slot.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(slot)
    }

    pub fn get(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<Option<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let slot = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![project_id.0.to_string(), slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        Ok(slot)
    }

    pub fn list(&self, project_id: ProjectId) -> Result<Vec<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id = ?1 ORDER BY created_at ASC, rowid ASC",
            SELECT_SLOT
        ))?;
        let slots = statement
            .query_map([project_id.0.to_string()], runtime_slot_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(slots)
    }

    /// Update display name, endpoint, model, and opaque secret reference for a
    /// governed Grok API-key slot. Login and external CLI slots are rejected.
    pub fn update_grok_api_key(
        &self,
        input: UpdateGrokApiKeySlot,
    ) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_text(&input.display_name, DISPLAY_NAME_MAX_CHARS)?;
        validate_text(&input.base_url, CONFIG_VALUE_MAX_CHARS)?;
        validate_text(&input.model_id, CONFIG_VALUE_MAX_CHARS)?;
        Uuid::parse_str(&input.secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;

        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![input.project_id.0.to_string(), input.slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        let existing = existing.ok_or(RuntimeSlotStoreError::NotFound)?;
        match &existing.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey { .. },
            } => {}
            _ => return Err(RuntimeSlotStoreError::InvalidShape),
        }

        let now = Utc::now();
        let updated = RuntimeSlot {
            id: existing.id,
            project_id: existing.project_id,
            display_name: input.display_name,
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url: input.base_url,
                    model_id: input.model_id,
                    secret_ref: input.secret_ref,
                },
            },
            created_at: existing.created_at,
            updated_at: now,
        };
        let encoded = EncodedSlot::from_slot(&updated)?;
        let changed = connection.execute(
            "UPDATE runtime_slots SET
                display_name = ?1,
                kind = ?2,
                governance = ?3,
                auth_mode = ?4,
                base_url = ?5,
                model_id = ?6,
                secret_ref = ?7,
                executable_path = NULL,
                launch_args_json = NULL,
                status_args_json = NULL,
                updated_at = ?8
             WHERE project_id = ?9 AND slot_id = ?10",
            params![
                updated.display_name,
                encoded.kind,
                updated.governance.as_str(),
                encoded.auth_mode,
                encoded.base_url,
                encoded.model_id,
                encoded.secret_ref,
                updated.updated_at.to_rfc3339(),
                updated.project_id.0.to_string(),
                updated.id.0.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(updated)
    }

    /// Delete a project-scoped slot row. Returns the removed slot so callers can
    /// destroy any associated secret material. Missing rows yield `Ok(None)`.
    pub fn delete(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<Option<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![project_id.0.to_string(), slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        let Some(slot) = existing else {
            return Ok(None);
        };
        let changed = connection.execute(
            "DELETE FROM runtime_slots WHERE project_id = ?1 AND slot_id = ?2",
            params![project_id.0.to_string(), slot_id.0.to_string()],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(Some(slot))
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, RuntimeSlotStoreError> {
        self.connection
            .lock()
            .map_err(|_| RuntimeSlotStoreError::LockPoisoned)
    }
}

const SELECT_SLOT: &str = "SELECT slot_id, project_id, display_name, kind, governance, auth_mode,
            base_url, model_id, secret_ref, executable_path,
            launch_args_json, status_args_json, created_at, updated_at
     FROM runtime_slots";

struct EncodedSlot {
    kind: &'static str,
    auth_mode: Option<&'static str>,
    base_url: Option<String>,
    model_id: Option<String>,
    secret_ref: Option<String>,
    executable_path: Option<String>,
    launch_args_json: Option<String>,
    status_args_json: Option<String>,
}

impl EncodedSlot {
    fn from_slot(slot: &RuntimeSlot) -> Result<Self, RuntimeSlotStoreError> {
        match &slot.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            } => Ok(Self {
                kind: "governed_grok",
                auth_mode: Some("login"),
                base_url: None,
                model_id: None,
                secret_ref: None,
                executable_path: None,
                launch_args_json: None,
                status_args_json: None,
            }),
            RuntimeSlotKind::GovernedGrok {
                auth:
                    GrokSlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref,
                    },
            } => Ok(Self {
                kind: "governed_grok",
                auth_mode: Some("api_key"),
                base_url: Some(base_url.clone()),
                model_id: Some(model_id.clone()),
                secret_ref: Some(secret_ref.clone()),
                executable_path: None,
                launch_args_json: None,
                status_args_json: None,
            }),
            RuntimeSlotKind::ExternalCli {
                executable_path,
                launch_args,
                status_args,
            } => Ok(Self {
                kind: "external_cli",
                auth_mode: None,
                base_url: None,
                model_id: None,
                secret_ref: None,
                executable_path: Some(executable_path.clone()),
                launch_args_json: Some(
                    serde_json::to_string(launch_args)
                        .map_err(|_| RuntimeSlotStoreError::InvalidShape)?,
                ),
                status_args_json: Some(
                    serde_json::to_string(status_args)
                        .map_err(|_| RuntimeSlotStoreError::InvalidShape)?,
                ),
            }),
        }
    }
}

fn validate_new_slot(input: &NewRuntimeSlot) -> Result<(), RuntimeSlotStoreError> {
    validate_text(&input.display_name, DISPLAY_NAME_MAX_CHARS)?;
    match (&input.governance, &input.kind) {
        (
            RuntimeSlotGovernance::Full,
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            },
        ) => Ok(()),
        (
            RuntimeSlotGovernance::Full,
            RuntimeSlotKind::GovernedGrok {
                auth:
                    GrokSlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref,
                    },
            },
        ) => {
            validate_text(base_url, CONFIG_VALUE_MAX_CHARS)?;
            validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
            Uuid::parse_str(secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
            Ok(())
        }
        (
            RuntimeSlotGovernance::None,
            RuntimeSlotKind::ExternalCli {
                executable_path,
                launch_args,
                status_args,
            },
        ) => {
            validate_text(executable_path, CONFIG_VALUE_MAX_CHARS)?;
            validate_args(launch_args)?;
            validate_args(status_args)?;
            if status_args.is_empty() {
                return Err(RuntimeSlotStoreError::InvalidShape);
            }
            Ok(())
        }
        _ => Err(RuntimeSlotStoreError::InvalidShape),
    }
}

fn validate_text(value: &str, max_chars: usize) -> Result<(), RuntimeSlotStoreError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    Ok(())
}

fn validate_args(args: &[String]) -> Result<(), RuntimeSlotStoreError> {
    if args.len() > ARG_COUNT_MAX
        || args
            .iter()
            .any(|arg| arg.chars().count() > ARG_MAX_CHARS || arg.chars().any(|c| c == '\0'))
    {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    Ok(())
}

fn runtime_slot_from_row(row: &Row<'_>) -> rusqlite::Result<RuntimeSlot> {
    let slot_id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let kind: String = row.get(3)?;
    let governance: String = row.get(4)?;
    let auth_mode: Option<String> = row.get(5)?;
    let base_url: Option<String> = row.get(6)?;
    let model_id: Option<String> = row.get(7)?;
    let secret_ref: Option<String> = row.get(8)?;
    let executable_path: Option<String> = row.get(9)?;
    let launch_args_json: Option<String> = row.get(10)?;
    let status_args_json: Option<String> = row.get(11)?;
    let created_at: String = row.get(12)?;
    let updated_at: String = row.get(13)?;

    let governance = match governance.as_str() {
        "full" => RuntimeSlotGovernance::Full,
        "none" => RuntimeSlotGovernance::None,
        _ => return Err(invalid_stored(4)),
    };
    let slot_kind = match (kind.as_str(), governance, auth_mode.as_deref()) {
        ("governed_grok", RuntimeSlotGovernance::Full, Some("login"))
            if base_url.is_none()
                && model_id.is_none()
                && secret_ref.is_none()
                && executable_path.is_none()
                && launch_args_json.is_none()
                && status_args_json.is_none() =>
        {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            }
        }
        ("governed_grok", RuntimeSlotGovernance::Full, Some("api_key"))
            if executable_path.is_none()
                && launch_args_json.is_none()
                && status_args_json.is_none() =>
        {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url: base_url.ok_or_else(|| invalid_stored(6))?,
                    model_id: model_id.ok_or_else(|| invalid_stored(7))?,
                    secret_ref: secret_ref.ok_or_else(|| invalid_stored(8))?,
                },
            }
        }
        ("external_cli", RuntimeSlotGovernance::None, None)
            if base_url.is_none() && model_id.is_none() && secret_ref.is_none() =>
        {
            RuntimeSlotKind::ExternalCli {
                executable_path: executable_path.ok_or_else(|| invalid_stored(9))?,
                launch_args: parse_args(launch_args_json, 10)?,
                status_args: parse_args(status_args_json, 11)?,
            }
        }
        _ => return Err(invalid_stored(3)),
    };

    Ok(RuntimeSlot {
        id: RuntimeSlotId(parse_uuid(&slot_id, 0)?),
        project_id: ProjectId(parse_uuid(&project_id, 1)?),
        display_name: row.get(2)?,
        governance,
        kind: slot_kind,
        created_at: parse_timestamp(&created_at, 12)?,
        updated_at: parse_timestamp(&updated_at, 13)?,
    })
}

fn parse_args(value: Option<String>, index: usize) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(&value.ok_or_else(|| invalid_stored(index))?)
        .map_err(|_| invalid_stored(index))
}

fn parse_uuid(value: &str, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn parse_timestamp(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

fn invalid_stored(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(RuntimeSlotStoreError::InvalidStoredSlot),
    )
}
