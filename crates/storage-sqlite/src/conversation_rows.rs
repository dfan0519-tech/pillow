//! Row mapping helpers for conversation domain tables.

use chrono::{DateTime, Utc};
use keeper_project::ProjectId;
use keeper_session::{
    Conversation, ConversationId, EventId, Message, MessageId, MessageRole, Run, RunEvent, RunId,
    RunStatus, RuntimeEventKind, SessionError,
};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, ffi, params};
use uuid::Uuid;

pub(crate) fn load_conversation(
    connection: &Connection,
    conversation_id: ConversationId,
) -> Result<Option<Conversation>, SessionError> {
    connection
        .query_row(
            "SELECT id, project_id, title, created_at, updated_at
             FROM conversations WHERE id = ?1",
            [conversation_id.0.to_string()],
            conversation_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

pub(crate) fn load_run(
    connection: &Connection,
    run_id: RunId,
) -> Result<Option<Run>, SessionError> {
    connection
        .query_row(
            "SELECT id, conversation_id, status, created_at, updated_at
             FROM runs WHERE id = ?1",
            [run_id.0.to_string()],
            run_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

pub(crate) fn load_event_by_sequence(
    connection: &Connection,
    run_id: RunId,
    sequence: u64,
) -> Result<Option<RunEvent>, SessionError> {
    let sequence_i64 = sequence_to_sqlite(sequence)?;
    connection
        .query_row(
            "SELECT event_id, run_id, sequence, kind, payload_json, created_at
             FROM run_events
             WHERE run_id = ?1 AND sequence = ?2",
            params![run_id.0.to_string(), sequence_i64],
            run_event_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

/// Next sequence for a new event on this run: 0 when empty, otherwise max + 1.
pub(crate) fn next_expected_sequence(
    connection: &Connection,
    run_id: RunId,
) -> Result<u64, SessionError> {
    let max: Option<i64> = connection
        .query_row(
            "SELECT MAX(sequence) FROM run_events WHERE run_id = ?1",
            [run_id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(map_sqlite)?;

    match max {
        None => Ok(0),
        Some(value) => {
            if value < 0 {
                return Err(SessionError::Store(format!(
                    "negative sequence {value} stored for run {}",
                    run_id.0
                )));
            }
            let max_u64 = value as u64;
            max_u64.checked_add(1).ok_or_else(|| {
                SessionError::Store(format!("sequence counter overflow for run {}", run_id.0))
            })
        }
    }
}

/// Convert domain `u64` sequence to SQLite INTEGER (`i64`), rejecting overflow.
pub(crate) fn sequence_to_sqlite(sequence: u64) -> Result<i64, SessionError> {
    i64::try_from(sequence).map_err(|_| SessionError::SequenceOutOfRange(sequence))
}

pub(crate) fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;

    Ok(Conversation {
        id: ConversationId(parse_uuid(&id, 0)?),
        project_id: ProjectId(parse_uuid(&project_id, 1)?),
        title: row.get(2)?,
        created_at: parse_timestamp(&created_at, 3)?,
        updated_at: parse_timestamp(&updated_at, 4)?,
    })
}

pub(crate) fn message_from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let id: String = row.get(0)?;
    let conversation_id: String = row.get(1)?;
    let role: String = row.get(2)?;
    let created_at: String = row.get(4)?;

    Ok(Message {
        id: MessageId(parse_uuid(&id, 0)?),
        conversation_id: ConversationId(parse_uuid(&conversation_id, 1)?),
        role: MessageRole::parse(&role).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                Type::Text,
                Box::new(InvalidEnumValue("message role", role)),
            )
        })?,
        content: row.get(3)?,
        created_at: parse_timestamp(&created_at, 4)?,
    })
}

pub(crate) fn run_from_row(row: &Row<'_>) -> rusqlite::Result<Run> {
    let id: String = row.get(0)?;
    let conversation_id: String = row.get(1)?;
    let status: String = row.get(2)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;

    Ok(Run {
        id: RunId(parse_uuid(&id, 0)?),
        conversation_id: ConversationId(parse_uuid(&conversation_id, 1)?),
        status: RunStatus::parse(&status).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                Type::Text,
                Box::new(InvalidEnumValue("run status", status)),
            )
        })?,
        created_at: parse_timestamp(&created_at, 3)?,
        updated_at: parse_timestamp(&updated_at, 4)?,
    })
}

pub(crate) fn run_event_from_row(row: &Row<'_>) -> rusqlite::Result<RunEvent> {
    let event_id: String = row.get(0)?;
    let run_id: String = row.get(1)?;
    let sequence_i64: i64 = row.get(2)?;
    let kind: String = row.get(3)?;
    let created_at: String = row.get(5)?;

    let sequence = u64::try_from(sequence_i64).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            Type::Integer,
            Box::new(InvalidEnumValue(
                "run event sequence",
                sequence_i64.to_string(),
            )),
        )
    })?;

    Ok(RunEvent {
        event_id: EventId(parse_uuid(&event_id, 0)?),
        run_id: RunId(parse_uuid(&run_id, 1)?),
        sequence,
        kind: RuntimeEventKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                Type::Text,
                Box::new(InvalidEnumValue("runtime event kind", kind)),
            )
        })?,
        payload_json: row.get(4)?,
        created_at: parse_timestamp(&created_at, 5)?,
    })
}

pub(crate) fn map_sqlite(error: rusqlite::Error) -> SessionError {
    SessionError::Store(error.to_string())
}

pub(crate) fn is_unique_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(error, _)
            if error.extended_code == ffi::SQLITE_CONSTRAINT_UNIQUE
                || error.extended_code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY
    )
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

#[derive(Debug, thiserror::Error)]
#[error("invalid {0}: {1}")]
struct InvalidEnumValue(&'static str, String);
