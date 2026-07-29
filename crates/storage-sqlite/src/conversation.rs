use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use keeper_project::ProjectId;
use keeper_session::{
    Conversation, ConversationId, ConversationRepository, EventId, Message, MessageId, MessageRole,
    NewConversation, NewRun, NewRunEvent, Run, RunEvent, RunId, RunStatus, SessionError, Timeline,
    validate_run_transition,
};
use rusqlite::{Connection, TransactionBehavior, params};

use crate::conversation_rows::{
    conversation_from_row, is_unique_constraint, load_conversation, load_event_by_sequence,
    load_run, map_sqlite, message_from_row, next_expected_sequence, run_event_from_row,
    run_from_row, sequence_to_sqlite,
};
use crate::migrations;

pub struct SqliteConversationRepository {
    connection: Mutex<Connection>,
}

impl SqliteConversationRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        // Dual-connection writers serialize on IMMEDIATE transactions; wait
        // rather than surface SQLITE_BUSY to callers during brief contention.
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(map_sqlite)?;
        migrations::apply(&connection).map_err(map_sqlite)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, SessionError> {
        self.connection
            .lock()
            .map_err(|_| SessionError::Store("conversation store lock was poisoned".into()))
    }

    /// Append a runtime event and, when `target_status` is set, apply the legal
    /// resulting run transition in a **single** IMMEDIATE SQLite transaction.
    ///
    /// Preserves strict sequence, idempotent exact-sequence replay, i64 bounds,
    /// and `validate_run_transition` rules. Cancelled from Running walks through
    /// Cancelling inside the same commit so a crash cannot leave a durable
    /// terminal event with a non-terminal run status.
    pub fn append_event_with_status(
        &self,
        event: NewRunEvent,
        target_status: Option<RunStatus>,
    ) -> Result<RunEvent, SessionError> {
        let sequence_i64 = sequence_to_sqlite(event.sequence)?;

        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;

        let (recorded, inserted) = append_event_in_tx(&tx, event, sequence_i64)?;

        if let Some(target) = target_status {
            apply_status_in_tx(&tx, recorded.run_id, target)?;
        }

        if inserted {
            tx.commit().map_err(map_sqlite)?;
        } else {
            // Idempotent replay: still commit when a status transition was applied;
            // if neither insert nor status change occurred, drop the empty txn.
            tx.commit().map_err(map_sqlite)?;
        }

        Ok(recorded)
    }
}

impl ConversationRepository for SqliteConversationRepository {
    fn create_conversation(&self, input: NewConversation) -> Result<Conversation, SessionError> {
        let connection = self.connection()?;
        ensure_project_exists(&connection, input.project_id)?;

        let now = Utc::now();
        let conversation = Conversation {
            id: ConversationId::new(),
            project_id: input.project_id,
            title: input.title,
            created_at: now,
            updated_at: now,
        };

        connection
            .execute(
                "INSERT INTO conversations (id, project_id, title, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    conversation.id.0.to_string(),
                    conversation.project_id.0.to_string(),
                    conversation.title,
                    conversation.created_at.to_rfc3339(),
                    conversation.updated_at.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;

        Ok(conversation)
    }

    fn list_conversations(&self, project_id: ProjectId) -> Result<Vec<Conversation>, SessionError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, project_id, title, created_at, updated_at
                 FROM conversations
                 WHERE project_id = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(map_sqlite)?;

        let rows = statement
            .query_map([project_id.0.to_string()], conversation_from_row)
            .map_err(map_sqlite)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(map_sqlite)
    }

    fn create_run(&self, input: NewRun) -> Result<Run, SessionError> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;

        ensure_conversation_exists(&tx, input.conversation_id)?;

        let now = Utc::now();
        let message = Message {
            id: MessageId::new(),
            conversation_id: input.conversation_id,
            role: MessageRole::User,
            content: input.user_message,
            created_at: now,
        };
        let run = Run {
            id: RunId::new(),
            conversation_id: input.conversation_id,
            status: RunStatus::Pending,
            created_at: now,
            updated_at: now,
        };

        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message.id.0.to_string(),
                message.conversation_id.0.to_string(),
                message.role.as_str(),
                message.content,
                message.created_at.to_rfc3339(),
            ],
        )
        .map_err(map_sqlite)?;

        tx.execute(
            "INSERT INTO runs (id, conversation_id, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.id.0.to_string(),
                run.conversation_id.0.to_string(),
                run.status.as_str(),
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339(),
            ],
        )
        .map_err(map_sqlite)?;

        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), input.conversation_id.0.to_string()],
        )
        .map_err(map_sqlite)?;

        tx.commit().map_err(map_sqlite)?;
        Ok(run)
    }

    fn append_event(&self, event: NewRunEvent) -> Result<RunEvent, SessionError> {
        let sequence_i64 = sequence_to_sqlite(event.sequence)?;
        let run_id = event.run_id;
        let sequence = event.sequence;

        let mut connection = self.connection()?;
        // IMMEDIATE takes the write lock before the existence/max reads so a
        // second connection cannot race a deferred read-then-insert path.
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;

        match append_event_in_tx(&tx, event, sequence_i64) {
            Ok((recorded, true)) => {
                tx.commit().map_err(map_sqlite)?;
                Ok(recorded)
            }
            Ok((recorded, false)) => {
                // Exact duplicate replay: no write needed.
                drop(tx);
                Ok(recorded)
            }
            Err(SessionError::Store(message)) if message.starts_with("unique constraint") => {
                // Defense-in-depth: transaction is failed — roll back, re-read winner.
                drop(tx);
                load_event_by_sequence(&connection, run_id, sequence)?.ok_or_else(|| {
                    SessionError::Store(format!(
                        "unique constraint on run event but row missing for sequence {sequence}"
                    ))
                })
            }
            Err(error) => Err(error),
        }
    }

    fn load_timeline(&self, conversation_id: ConversationId) -> Result<Timeline, SessionError> {
        let connection = self.connection()?;
        let conversation = load_conversation(&connection, conversation_id)?
            .ok_or(SessionError::ConversationNotFound(conversation_id))?;

        let mut message_stmt = connection
            .prepare(
                "SELECT id, conversation_id, role, content, created_at
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(map_sqlite)?;
        let messages = message_stmt
            .query_map([conversation_id.0.to_string()], message_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;

        let mut run_stmt = connection
            .prepare(
                "SELECT id, conversation_id, status, created_at, updated_at
                 FROM runs
                 WHERE conversation_id = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(map_sqlite)?;
        let runs = run_stmt
            .query_map([conversation_id.0.to_string()], run_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;

        // Group by owning run conversation order, then per-run sequence.
        // Do not interleave multi-run timelines by global sequence + event_id.
        let mut event_stmt = connection
            .prepare(
                "SELECT e.event_id, e.run_id, e.sequence, e.kind, e.payload_json, e.created_at
                 FROM run_events e
                 INNER JOIN runs r ON r.id = e.run_id
                 WHERE r.conversation_id = ?1
                 ORDER BY r.created_at ASC, r.id ASC, e.sequence ASC",
            )
            .map_err(map_sqlite)?;
        let events = event_stmt
            .query_map([conversation_id.0.to_string()], run_event_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;

        Ok(Timeline {
            conversation,
            messages,
            runs,
            events,
        })
    }

    fn transition_run(&self, run_id: RunId, next: RunStatus) -> Result<Run, SessionError> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;

        let updated = transition_run_in_tx(&tx, run_id, next)?;
        tx.commit().map_err(map_sqlite)?;
        Ok(updated)
    }

    fn mark_inflight_interrupted(&self) -> Result<usize, SessionError> {
        let connection = self.connection()?;
        let now = Utc::now();
        let changed = connection
            .execute(
                "UPDATE runs
                 SET status = 'interrupted', updated_at = ?1
                 WHERE status IN ('pending', 'running', 'cancelling')",
                params![now.to_rfc3339()],
            )
            .map_err(map_sqlite)?;
        Ok(changed)
    }
}

/// Insert (or idempotently load) a run event inside an open transaction.
/// Returns `(event, inserted)` where `inserted` is false on exact-sequence replay.
fn append_event_in_tx(
    tx: &rusqlite::Transaction<'_>,
    event: NewRunEvent,
    sequence_i64: i64,
) -> Result<(RunEvent, bool), SessionError> {
    ensure_run_exists(tx, event.run_id)?;

    if let Some(existing) = load_event_by_sequence(tx, event.run_id, event.sequence)? {
        return Ok((existing, false));
    }

    let expected = next_expected_sequence(tx, event.run_id)?;
    if event.sequence != expected {
        return Err(SessionError::InvalidSequence {
            run_id: event.run_id,
            expected,
            got: event.sequence,
        });
    }

    let recorded = RunEvent {
        event_id: EventId::new(),
        run_id: event.run_id,
        sequence: event.sequence,
        kind: event.kind,
        payload_json: event.payload_json.clone(),
        created_at: Utc::now(),
    };

    let insert = tx.execute(
        "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            recorded.event_id.0.to_string(),
            recorded.run_id.0.to_string(),
            sequence_i64,
            recorded.kind.as_str(),
            recorded.payload_json,
            recorded.created_at.to_rfc3339(),
        ],
    );

    match insert {
        Ok(_) => Ok((recorded, true)),
        Err(error) if is_unique_constraint(&error) => Err(SessionError::Store(format!(
            "unique constraint on run event for sequence {}",
            event.sequence
        ))),
        Err(error) => Err(map_sqlite(error)),
    }
}

fn transition_run_in_tx(
    tx: &rusqlite::Transaction<'_>,
    run_id: RunId,
    next: RunStatus,
) -> Result<Run, SessionError> {
    let current = load_run(tx, run_id)?.ok_or(SessionError::RunNotFound(run_id))?;
    validate_run_transition(current.status, next)?;

    let now = Utc::now();
    let updated_rows = tx
        .execute(
            "UPDATE runs SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![next.as_str(), now.to_rfc3339(), run_id.0.to_string()],
        )
        .map_err(map_sqlite)?;

    if updated_rows != 1 {
        return Err(SessionError::Store(format!(
            "expected one run row update for {run_id:?}, got {updated_rows}"
        )));
    }

    Ok(Run {
        id: current.id,
        conversation_id: current.conversation_id,
        status: next,
        created_at: current.created_at,
        updated_at: now,
    })
}

/// Apply resulting status for a runtime event inside the open transaction.
/// Same-status, already-terminal, and illegal transitions are no-ops (match
/// service-layer tolerance for cancel races).
fn apply_status_in_tx(
    tx: &rusqlite::Transaction<'_>,
    run_id: RunId,
    target: RunStatus,
) -> Result<(), SessionError> {
    let current = load_run(tx, run_id)?.ok_or(SessionError::RunNotFound(run_id))?;
    if current.status == target || current.status.is_terminal() {
        return Ok(());
    }

    // Cancel terminal may arrive while still Running if cancel raced past
    // the explicit Cancelling transition.
    if target == RunStatus::Cancelled && current.status == RunStatus::Running {
        match transition_run_in_tx(tx, run_id, RunStatus::Cancelling) {
            Ok(_) => {}
            Err(SessionError::IllegalTransition { .. }) => return Ok(()),
            Err(error) => return Err(error),
        }
        let after = load_run(tx, run_id)?.ok_or(SessionError::RunNotFound(run_id))?;
        if after.status == target || after.status.is_terminal() {
            return Ok(());
        }
    }

    match transition_run_in_tx(tx, run_id, target) {
        Ok(_) => Ok(()),
        Err(SessionError::IllegalTransition { .. }) => Ok(()),
        Err(error) => Err(error),
    }
}

fn ensure_project_exists(
    connection: &Connection,
    project_id: ProjectId,
) -> Result<(), SessionError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            [project_id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(map_sqlite)?;

    if exists {
        Ok(())
    } else {
        Err(SessionError::ProjectNotFound(project_id))
    }
}

fn ensure_conversation_exists(
    connection: &Connection,
    conversation_id: ConversationId,
) -> Result<(), SessionError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
            [conversation_id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(map_sqlite)?;

    if exists {
        Ok(())
    } else {
        Err(SessionError::ConversationNotFound(conversation_id))
    }
}

fn ensure_run_exists(connection: &Connection, run_id: RunId) -> Result<(), SessionError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1)",
            [run_id.0.to_string()],
            |row| row.get(0),
        )
        .map_err(map_sqlite)?;

    if exists {
        Ok(())
    } else {
        Err(SessionError::RunNotFound(run_id))
    }
}
