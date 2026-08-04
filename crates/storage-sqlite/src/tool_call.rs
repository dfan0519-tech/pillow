use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_agent_protocol::{ToolExecutionState, ValidatedToolProposal, ValidatedToolResult};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredToolCall {
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: String,
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub arguments_digest: String,
    pub proposal_json: String,
    pub state: ToolExecutionState,
    pub result_json: Option<String>,
    pub execution_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallReservationOutcome {
    Reserved(StoredToolCall),
    Replay(StoredToolCall),
    Conflict(StoredToolCall),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionClaimOutcome {
    Claimed(StoredToolCall),
    Busy(StoredToolCall),
    Terminal(StoredToolCall),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolCallStoreError {
    #[error("tool call context does not exist in the durable project conversation run graph")]
    MissingContext,
    #[error("tool call transition is not permitted")]
    InvalidTransition,
    #[error("tool call result does not match the requested terminal state")]
    InvalidResult,
    #[error("tool call proposal conflicts with its durable reservation")]
    ReservationConflict,
    #[error("SQLite tool call store error")]
    Sqlite,
    #[error("tool call store lock was poisoned")]
    LockPoisoned,
}

pub struct SqliteToolCallRepository {
    connection: Mutex<Connection>,
}

impl SqliteToolCallRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ToolCallStoreError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn reserve(
        &self,
        proposal: &ValidatedToolProposal,
    ) -> Result<ToolCallReservationOutcome, ToolCallStoreError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        ensure_context_exists(&transaction, proposal)?;

        let reservation_key = proposal.reservation().key().as_str();
        if let Some(existing) = load_call(&transaction, reservation_key)? {
            transaction.commit().map_err(map_sqlite)?;
            return if existing.proposal_fingerprint == proposal.fingerprint().as_str() {
                Ok(ToolCallReservationOutcome::Replay(existing))
            } else {
                Ok(ToolCallReservationOutcome::Conflict(existing))
            };
        }

        let now = Utc::now();
        let proposal_json = serde_json::to_string(proposal.proposal())
            .map_err(|_| ToolCallStoreError::InvalidResult)?;
        transaction
            .execute(
                "INSERT INTO tool_calls (
                    reservation_key, proposal_fingerprint, project_id, conversation_id,
                    run_id, call_id, tool_id, tool_version, arguments_digest,
                    proposal_json, state, result_json, created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'proposed', NULL, ?11, ?11
                 )",
                params![
                    reservation_key,
                    proposal.fingerprint().as_str(),
                    proposal.project_id(),
                    proposal.conversation_id(),
                    proposal.run_id().0.to_string(),
                    proposal.call_id(),
                    proposal.tool_id(),
                    i64::from(proposal.tool_version()),
                    proposal.normalized_arguments_digest().as_str(),
                    proposal_json,
                    now.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;
        transaction
            .execute(
                "INSERT INTO tool_call_transitions (
                    reservation_key, state, result_json, created_at
                 ) VALUES (?1, 'proposed', NULL, ?2)",
                params![reservation_key, now.to_rfc3339()],
            )
            .map_err(map_sqlite)?;
        let stored = load_call(&transaction, reservation_key)?.ok_or(ToolCallStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(ToolCallReservationOutcome::Reserved(stored))
    }

    pub fn transition(
        &self,
        proposal: &ValidatedToolProposal,
        next: ToolExecutionState,
        result: Option<&ValidatedToolResult>,
    ) -> Result<StoredToolCall, ToolCallStoreError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let reservation_key = proposal.reservation().key().as_str();
        let current = load_call(&transaction, reservation_key)?
            .ok_or(ToolCallStoreError::ReservationConflict)?;
        if current.proposal_fingerprint != proposal.fingerprint().as_str() {
            return Err(ToolCallStoreError::ReservationConflict);
        }

        let result_json = validate_transition_result(proposal, next, result)?;
        if current.state == next {
            if current.result_json == result_json {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(current);
            }
            return Err(ToolCallStoreError::InvalidResult);
        }
        if !transition_allowed(current.state, next) {
            return Err(ToolCallStoreError::InvalidTransition);
        }
        if is_terminal(next) && current.execution_owner.is_some() {
            return Err(ToolCallStoreError::ReservationConflict);
        }

        let now = Utc::now();
        transaction
            .execute(
                "UPDATE tool_calls
                 SET state = ?1, result_json = ?2, execution_owner = NULL,
                     lease_expires_at = NULL, updated_at = ?3
                 WHERE reservation_key = ?4 AND proposal_fingerprint = ?5",
                params![
                    state_text(next),
                    result_json,
                    now.to_rfc3339(),
                    reservation_key,
                    proposal.fingerprint().as_str(),
                ],
            )
            .map_err(map_sqlite)?;
        transaction
            .execute(
                "INSERT INTO tool_call_transitions (
                    reservation_key, state, result_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    reservation_key,
                    state_text(next),
                    result_json,
                    now.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;
        let updated =
            load_call(&transaction, reservation_key)?.ok_or(ToolCallStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(updated)
    }

    pub fn history(
        &self,
        proposal: &ValidatedToolProposal,
    ) -> Result<Vec<ToolExecutionState>, ToolCallStoreError> {
        let connection = self.connection()?;
        let reservation_key = proposal.reservation().key().as_str();
        let current = load_call(&connection, reservation_key)?
            .ok_or(ToolCallStoreError::ReservationConflict)?;
        if current.proposal_fingerprint != proposal.fingerprint().as_str() {
            return Err(ToolCallStoreError::ReservationConflict);
        }
        let mut statement = connection
            .prepare(
                "SELECT state FROM tool_call_transitions
                 WHERE reservation_key = ?1 ORDER BY id ASC",
            )
            .map_err(map_sqlite)?;
        let states = statement
            .query_map([reservation_key], |row| {
                let state: String = row.get(0)?;
                parse_state(&state, 0)
            })
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;
        Ok(states)
    }

    pub fn list_terminal_calls(&self) -> Result<Vec<StoredToolCall>, ToolCallStoreError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT reservation_key, proposal_fingerprint, project_id, conversation_id,
                        run_id, call_id, tool_id, tool_version, arguments_digest,
                        proposal_json, state, result_json, execution_owner,
                        lease_expires_at, created_at, updated_at
                 FROM tool_calls
                 WHERE state IN ('succeeded', 'failed', 'cancelled')
                 ORDER BY run_id ASC, created_at ASC, reservation_key ASC",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([], stored_call_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)
    }

    pub fn claim_execution(
        &self,
        proposal: &ValidatedToolProposal,
        owner: &str,
        lease_ms: u64,
    ) -> Result<ToolExecutionClaimOutcome, ToolCallStoreError> {
        validate_owner(owner)?;
        let lease_ms = lease_ms.min(300_000);
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let reservation_key = proposal.reservation().key().as_str();
        let current = load_call(&transaction, reservation_key)?
            .ok_or(ToolCallStoreError::ReservationConflict)?;
        if current.proposal_fingerprint != proposal.fingerprint().as_str() {
            return Err(ToolCallStoreError::ReservationConflict);
        }
        if is_terminal(current.state) {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ToolExecutionClaimOutcome::Terminal(current));
        }
        if current.state == ToolExecutionState::Proposed {
            return Err(ToolCallStoreError::InvalidTransition);
        }

        let now = Utc::now();
        let lease_is_live = current
            .lease_expires_at
            .is_some_and(|expires_at| expires_at > now);
        if current.state == ToolExecutionState::Executing
            && lease_is_live
            && current.execution_owner.as_deref() != Some(owner)
        {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ToolExecutionClaimOutcome::Busy(current));
        }
        if current.state == ToolExecutionState::Executing
            && lease_is_live
            && current.execution_owner.as_deref() == Some(owner)
        {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ToolExecutionClaimOutcome::Claimed(current));
        }

        let expires_at =
            now + chrono::Duration::milliseconds(i64::try_from(lease_ms).unwrap_or(300_000));
        transaction
            .execute(
                "UPDATE tool_calls
                 SET state = 'executing', execution_owner = ?1,
                     lease_expires_at = ?2, updated_at = ?3
                 WHERE reservation_key = ?4 AND proposal_fingerprint = ?5",
                params![
                    owner,
                    expires_at.to_rfc3339(),
                    now.to_rfc3339(),
                    reservation_key,
                    proposal.fingerprint().as_str(),
                ],
            )
            .map_err(map_sqlite)?;
        if current.state == ToolExecutionState::Validated {
            transaction
                .execute(
                    "INSERT INTO tool_call_transitions (
                        reservation_key, state, result_json, created_at
                     ) VALUES (?1, 'executing', NULL, ?2)",
                    params![reservation_key, now.to_rfc3339()],
                )
                .map_err(map_sqlite)?;
        }
        let claimed =
            load_call(&transaction, reservation_key)?.ok_or(ToolCallStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(ToolExecutionClaimOutcome::Claimed(claimed))
    }

    pub fn complete_execution(
        &self,
        proposal: &ValidatedToolProposal,
        owner: &str,
        result: &ValidatedToolResult,
    ) -> Result<StoredToolCall, ToolCallStoreError> {
        validate_owner(owner)?;
        let next = result.state();
        let result_json = validate_transition_result(proposal, next, Some(result))?
            .ok_or(ToolCallStoreError::InvalidResult)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let reservation_key = proposal.reservation().key().as_str();
        let current = load_call(&transaction, reservation_key)?
            .ok_or(ToolCallStoreError::ReservationConflict)?;
        if current.proposal_fingerprint != proposal.fingerprint().as_str() {
            return Err(ToolCallStoreError::ReservationConflict);
        }
        if is_terminal(current.state) {
            if current.state == next && current.result_json.as_deref() == Some(&result_json) {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(current);
            }
            return Err(ToolCallStoreError::InvalidResult);
        }
        if current.state != ToolExecutionState::Executing
            || current.execution_owner.as_deref() != Some(owner)
        {
            return Err(ToolCallStoreError::ReservationConflict);
        }

        let now = Utc::now();
        transaction
            .execute(
                "UPDATE tool_calls
                 SET state = ?1, result_json = ?2, execution_owner = NULL,
                     lease_expires_at = NULL, updated_at = ?3
                 WHERE reservation_key = ?4
                   AND proposal_fingerprint = ?5
                   AND execution_owner = ?6",
                params![
                    state_text(next),
                    result_json,
                    now.to_rfc3339(),
                    reservation_key,
                    proposal.fingerprint().as_str(),
                    owner,
                ],
            )
            .map_err(map_sqlite)?;
        transaction
            .execute(
                "INSERT INTO tool_call_transitions (
                    reservation_key, state, result_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    reservation_key,
                    state_text(next),
                    result_json,
                    now.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;
        let terminal =
            load_call(&transaction, reservation_key)?.ok_or(ToolCallStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(terminal)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, ToolCallStoreError> {
        self.connection
            .lock()
            .map_err(|_| ToolCallStoreError::LockPoisoned)
    }
}

fn ensure_context_exists(
    connection: &Connection,
    proposal: &ValidatedToolProposal,
) -> Result<(), ToolCallStoreError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM runs
                JOIN conversations ON conversations.id = runs.conversation_id
                JOIN projects ON projects.id = conversations.project_id
                WHERE runs.id = ?1
                  AND conversations.id = ?2
                  AND projects.id = ?3
             )",
            params![
                proposal.run_id().0.to_string(),
                proposal.conversation_id(),
                proposal.project_id(),
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite)?;
    if exists {
        Ok(())
    } else {
        Err(ToolCallStoreError::MissingContext)
    }
}

fn validate_transition_result(
    proposal: &ValidatedToolProposal,
    next: ToolExecutionState,
    result: Option<&ValidatedToolResult>,
) -> Result<Option<String>, ToolCallStoreError> {
    let terminal = matches!(
        next,
        ToolExecutionState::Succeeded | ToolExecutionState::Failed | ToolExecutionState::Cancelled
    );
    if !terminal {
        return if result.is_none() {
            Ok(None)
        } else {
            Err(ToolCallStoreError::InvalidResult)
        };
    }

    let result = result.ok_or(ToolCallStoreError::InvalidResult)?;
    if result.state() != next
        || result.context() != proposal.context()
        || result.proposal_fingerprint() != proposal.fingerprint()
        || result.call_id() != proposal.call_id()
    {
        return Err(ToolCallStoreError::InvalidResult);
    }
    let json =
        serde_json::to_string(result.result()).map_err(|_| ToolCallStoreError::InvalidResult)?;
    let maximum = proposal
        .manifest()
        .limits
        .output_byte_cap
        .saturating_add(32_768);
    if json.len() as u64 > maximum {
        return Err(ToolCallStoreError::InvalidResult);
    }
    Ok(Some(json))
}

fn transition_allowed(current: ToolExecutionState, next: ToolExecutionState) -> bool {
    matches!(
        (current, next),
        (ToolExecutionState::Proposed, ToolExecutionState::Validated)
            | (ToolExecutionState::Validated, ToolExecutionState::Executing)
            | (
                ToolExecutionState::Executing,
                ToolExecutionState::Succeeded
                    | ToolExecutionState::Failed
                    | ToolExecutionState::Cancelled
            )
    )
}

fn is_terminal(state: ToolExecutionState) -> bool {
    matches!(
        state,
        ToolExecutionState::Succeeded | ToolExecutionState::Failed | ToolExecutionState::Cancelled
    )
}

fn validate_owner(owner: &str) -> Result<(), ToolCallStoreError> {
    if owner.is_empty()
        || owner.len() > 128
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ToolCallStoreError::ReservationConflict);
    }
    Ok(())
}

fn load_call(
    connection: &Connection,
    reservation_key: &str,
) -> Result<Option<StoredToolCall>, ToolCallStoreError> {
    connection
        .query_row(
            "SELECT reservation_key, proposal_fingerprint, project_id, conversation_id,
                    run_id, call_id, tool_id, tool_version, arguments_digest,
                    proposal_json, state, result_json, execution_owner,
                    lease_expires_at, created_at, updated_at
             FROM tool_calls WHERE reservation_key = ?1",
            [reservation_key],
            stored_call_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn stored_call_from_row(row: &Row<'_>) -> rusqlite::Result<StoredToolCall> {
    let state: String = row.get(10)?;
    let lease_expires_at: Option<String> = row.get(13)?;
    let created_at: String = row.get(14)?;
    let updated_at: String = row.get(15)?;
    let version: i64 = row.get(7)?;
    let tool_version = u16::try_from(version).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, Type::Integer, Box::new(error))
    })?;
    Ok(StoredToolCall {
        reservation_key: row.get(0)?,
        proposal_fingerprint: row.get(1)?,
        project_id: row.get(2)?,
        conversation_id: row.get(3)?,
        run_id: row.get(4)?,
        call_id: row.get(5)?,
        tool_id: row.get(6)?,
        tool_version,
        arguments_digest: row.get(8)?,
        proposal_json: row.get(9)?,
        state: parse_state(&state, 10)?,
        result_json: row.get(11)?,
        execution_owner: row.get(12)?,
        lease_expires_at: lease_expires_at
            .map(|value| parse_timestamp(&value, 13))
            .transpose()?,
        created_at: parse_timestamp(&created_at, 14)?,
        updated_at: parse_timestamp(&updated_at, 15)?,
    })
}

fn parse_state(value: &str, index: usize) -> rusqlite::Result<ToolExecutionState> {
    match value {
        "proposed" => Ok(ToolExecutionState::Proposed),
        "validated" => Ok(ToolExecutionState::Validated),
        "executing" => Ok(ToolExecutionState::Executing),
        "succeeded" => Ok(ToolExecutionState::Succeeded),
        "failed" => Ok(ToolExecutionState::Failed),
        "cancelled" => Ok(ToolExecutionState::Cancelled),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid tool execution state",
            )),
        )),
    }
}

fn state_text(state: ToolExecutionState) -> &'static str {
    match state {
        ToolExecutionState::Proposed => "proposed",
        ToolExecutionState::Validated => "validated",
        ToolExecutionState::Executing => "executing",
        ToolExecutionState::Succeeded => "succeeded",
        ToolExecutionState::Failed => "failed",
        ToolExecutionState::Cancelled => "cancelled",
    }
}

fn parse_timestamp(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

fn map_sqlite(_: rusqlite::Error) -> ToolCallStoreError {
    ToolCallStoreError::Sqlite
}
