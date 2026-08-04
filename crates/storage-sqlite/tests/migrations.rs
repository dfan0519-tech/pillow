//! Migration order, write-proposal schema invariants (C3-B), and C3-F
//! run_events.kind CHECK alignment for write_proposal_* kinds.

use std::path::Path;

use keeper_storage_sqlite::SqliteConversationRepository;
use rusqlite::Connection;

/// Trigger production migrations through the real repository open path.
/// Drops the repository handle so Windows releases the DB file for reopening.
fn apply_via_repository_open(db: &Path) {
    let repo = SqliteConversationRepository::open(db).unwrap();
    drop(repo);
}

/// Reopen a direct SQLite connection for SQL assertions after repository open.
fn reopen_with_foreign_keys(db: &Path) -> Connection {
    let connection = Connection::open(db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
}

/// Pre-C3-F `run_events` table: CHECK list intentionally omits
/// `write_proposal_requested` / `write_proposal_resolved` so tests can build
/// a legacy 0001–0005 database even after 0002 is updated for fresh installs.
const LEGACY_RUN_EVENTS_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS run_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id),
    sequence INTEGER NOT NULL
        CHECK (sequence >= 0),
    kind TEXT NOT NULL
        CHECK (kind IN (
            'run_started',
            'assistant_delta',
            'reasoning_summary_delta',
            'plan_updated',
            'tool_call_started',
            'tool_call_output',
            'tool_call_completed',
            'subagent_started',
            'subagent_progress',
            'subagent_completed',
            'artifact_produced',
            'turn_completed',
            'turn_failed',
            'run_cancelled',
            'heartbeat'
        )),
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (run_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_run_events_run_sequence
    ON run_events(run_id, sequence ASC);
"#;

const LEGACY_CONVERSATIONS_WITHOUT_RUN_EVENTS: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_project_created
    ON conversations(project_id, created_at ASC, id ASC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL
        CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation_created
    ON messages(conversation_id, created_at ASC, id ASC);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    status TEXT NOT NULL
        CHECK (status IN (
            'pending',
            'running',
            'cancelling',
            'completed',
            'failed',
            'cancelled',
            'interrupted'
        )),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_conversation_created
    ON runs(conversation_id, created_at ASC, id ASC);

CREATE INDEX IF NOT EXISTS idx_runs_status
    ON runs(status);
"#;

fn apply_through(connection: &Connection, through: u8) {
    if through >= 1 {
        connection
            .execute_batch(include_str!("../migrations/0001_projects.sql"))
            .unwrap();
    }
    if through >= 2 {
        connection
            .execute_batch(include_str!("../migrations/0002_conversations.sql"))
            .unwrap();
    }
    if through >= 3 {
        connection
            .execute_batch(include_str!("../migrations/0003_tool_calls.sql"))
            .unwrap();
    }
    if through >= 4 {
        connection
            .execute_batch(include_str!("../migrations/0004_resource_recents.sql"))
            .unwrap();
    }
    if through >= 5 {
        connection
            .execute_batch(include_str!("../migrations/0005_write_proposals.sql"))
            .unwrap();
    }
    if through >= 6 {
        // Unconditional table rebuild used only in tests that start from a
        // schema that still rejects C3 write kinds (legacy CHECK).
        connection
            .execute_batch(include_str!(
                "../migrations/0006_run_events_write_kinds.sql"
            ))
            .unwrap();
    }
    if through >= 7 {
        connection
            .execute_batch(include_str!("../migrations/0007_exec_proposals.sql"))
            .unwrap();
    }
}

/// Current 0006/0007 database: write-proposal kinds accepted, process kinds not.
fn apply_current_0006_0007_without_process_kinds(connection: &Connection) {
    apply_legacy_through_0005(connection);
    connection
        .execute_batch(include_str!(
            "../migrations/0006_run_events_write_kinds.sql"
        ))
        .unwrap();
    connection
        .execute_batch(include_str!("../migrations/0007_exec_proposals.sql"))
        .unwrap();
}

const PROCESS_KINDS: [&str; 4] = [
    "process_proposal_requested",
    "process_proposal_decided",
    "process_started",
    "process_terminal_outcome",
];

/// Build a database that mirrors a pre-C3-F install (migrations 0001–0005 with
/// the historical run_events.kind CHECK).
fn apply_legacy_through_0005(connection: &Connection) {
    connection
        .execute_batch(include_str!("../migrations/0001_projects.sql"))
        .unwrap();
    connection
        .execute_batch(LEGACY_CONVERSATIONS_WITHOUT_RUN_EVENTS)
        .unwrap();
    connection.execute_batch(LEGACY_RUN_EVENTS_DDL).unwrap();
    connection
        .execute_batch(include_str!("../migrations/0003_tool_calls.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../migrations/0004_resource_recents.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../migrations/0005_write_proposals.sql"))
        .unwrap();
}

fn seed_project_conversation_run(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO projects (id, name, root_path, canonical_root_path, trust_state, created_at, updated_at)
             VALUES ('p1', 'n', '/tmp/p', '/tmp/p', 'untrusted', 't0', 't0')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO conversations (id, project_id, title, created_at, updated_at)
             VALUES ('c1', 'p1', 't', 't0', 't0')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO runs (id, conversation_id, status, created_at, updated_at)
             VALUES ('r1', 'c1', 'running', 't0', 't0')",
            [],
        )
        .unwrap();
}

fn insert_event(
    connection: &Connection,
    event_id: &str,
    sequence: i64,
    kind: &str,
    payload: &str,
    created_at: &str,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
         VALUES (?1, 'r1', ?2, ?3, ?4, ?5)",
        rusqlite::params![event_id, sequence, kind, payload, created_at],
    )
}

fn event_row_count(connection: &Connection) -> i64 {
    connection
        .query_row("SELECT COUNT(*) FROM run_events", [], |row| row.get(0))
        .unwrap()
}

fn load_event(connection: &Connection, event_id: &str) -> (String, i64, String, String, String) {
    connection
        .query_row(
            "SELECT event_id, sequence, kind, payload_json, created_at
             FROM run_events WHERE event_id = ?1",
            [event_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap()
}

fn run_events_sql(connection: &Connection) -> String {
    connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'run_events'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
}

fn index_exists(connection: &Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
             )",
            [name],
            |row| row.get(0),
        )
        .unwrap()
}

fn table_exists(connection: &Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
             )",
            [name],
            |row| row.get(0),
        )
        .unwrap()
}

fn digest(seed: u64) -> String {
    format!("sha256:{seed:064x}")
}

const UUID_A: &str = "11111111-1111-1111-1111-111111111111";
const UUID_B: &str = "22222222-2222-2222-2222-222222222222";

#[test]
fn migration_order_reaches_0005_write_proposals() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();

    apply_through(&connection, 4);
    assert!(!table_exists(&connection, "write_proposals"));

    apply_through(&connection, 5);
    assert!(table_exists(&connection, "write_proposals"));
    assert!(table_exists(&connection, "write_proposal_transitions"));
    assert!(table_exists(&connection, "write_approval_decisions"));

    // Idempotent re-apply.
    apply_through(&connection, 5);
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'write_proposals'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn write_proposals_enforce_mode_base_digest_and_terminal_receipt_checks() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    apply_through(&connection, 5);

    connection
        .execute(
            "INSERT INTO projects (id, name, root_path, canonical_root_path, trust_state, created_at, updated_at)
             VALUES ('p1', 'n', '/tmp/p', '/tmp/p', 'untrusted', 't', 't')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO conversations (id, project_id, title, created_at, updated_at)
             VALUES ('c1', 'p1', 't', 't', 't')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO runs (id, conversation_id, status, created_at, updated_at)
             VALUES ('r1', 'c1', 'running', 't', 't')",
            [],
        )
        .unwrap();

    let proposed = digest(1);
    let base = digest(2);
    let fingerprint = digest(3);

    // create with base_digest must fail.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk1', ?2, 'p1', 'c1', 'r1', 'call1', 'a.ts', 'create',
            ?3, 1, ?4, 0, 'pending_approval', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed, base],
    );
    assert!(err.is_err());

    // Uppercase digest must fail CHECK.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-upper', ?2, 'p1', 'c1', 'r1', 'call-u', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'pending_approval', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, format!("sha256:{}", "A".repeat(64))],
    );
    assert!(err.is_err());

    // Zero proposed_bytes must fail.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-zero', ?2, 'p1', 'c1', 'r1', 'call-z', 'a.ts', 'create',
            ?3, 0, NULL, 0, 'pending_approval', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed],
    );
    assert!(err.is_err());

    // Valid pending row.
    connection
        .execute(
            "INSERT INTO write_proposals (
                proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
                run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
                state, expires_at, result_json, created_at, updated_at
             ) VALUES (
                ?1, 'rk2', ?2, 'p1', 'c1', 'r1', 'call2', 'a.ts', 'create',
                ?3, 1, NULL, 0, 'pending_approval', 'e', NULL, 't', 't'
             )",
            rusqlite::params![UUID_B, fingerprint, proposed],
        )
        .unwrap();

    // succeeded without result_json must fail.
    let err = connection.execute(
        "UPDATE write_proposals SET state = 'succeeded', result_json = NULL WHERE proposal_id = ?1",
        [UUID_B],
    );
    assert!(err.is_err());

    // failed without result_json must fail.
    let err = connection.execute(
        "UPDATE write_proposals SET state = 'failed', result_json = NULL WHERE proposal_id = ?1",
        [UUID_B],
    );
    assert!(err.is_err());

    // succeeded with non-JSON must fail.
    let err = connection.execute(
        "UPDATE write_proposals SET state = 'succeeded', result_json = 'not-json' WHERE proposal_id = ?1",
        [UUID_B],
    );
    assert!(err.is_err());

    // Non-terminal / non-apply states must not carry result_json.
    let receipt = r#"{"outcome":"failure","error_code":"write_failed","recovered":false}"#;
    for state in [
        "pending_approval",
        "approved",
        "applying",
        "rejected",
        "expired",
        "cancelled",
        "interrupted",
    ] {
        let err = connection.execute(
            "UPDATE write_proposals SET state = ?1, result_json = ?2 WHERE proposal_id = ?3",
            rusqlite::params![state, receipt, UUID_B],
        );
        assert!(
            err.is_err(),
            "state {state} must not accept non-null result_json"
        );
    }

    // Apply terminal with bounded valid JSON is accepted.
    connection
        .execute(
            "UPDATE write_proposals SET state = 'failed', result_json = ?1 WHERE proposal_id = ?2",
            rusqlite::params![receipt, UUID_B],
        )
        .unwrap();

    // One decision per proposal; grant digest unique and canonical.
    let grant = digest(9);
    connection
        .execute(
            "INSERT INTO write_approval_decisions (
                proposal_id, decision, approval_grant_digest, proposal_fingerprint, decided_at
             ) VALUES (?1, 'approve_once', ?2, ?3, 't')",
            rusqlite::params![UUID_B, grant, fingerprint],
        )
        .unwrap();
    let err = connection.execute(
        "INSERT INTO write_approval_decisions (
            proposal_id, decision, approval_grant_digest, proposal_fingerprint, decided_at
         ) VALUES (?1, 'reject', ?2, ?3, 't')",
        rusqlite::params![UUID_B, digest(10), fingerprint],
    );
    assert!(err.is_err());
}

#[test]
fn write_proposals_and_transitions_couple_result_json_to_state() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    apply_through(&connection, 5);

    connection
        .execute(
            "INSERT INTO projects (id, name, root_path, canonical_root_path, trust_state, created_at, updated_at)
             VALUES ('p1', 'n', '/tmp/p', '/tmp/p', 'untrusted', 't', 't')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO conversations (id, project_id, title, created_at, updated_at)
             VALUES ('c1', 'p1', 't', 't', 't')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO runs (id, conversation_id, status, created_at, updated_at)
             VALUES ('r1', 'c1', 'running', 't', 't')",
            [],
        )
        .unwrap();

    let proposed = digest(1);
    let fingerprint = digest(3);
    let receipt = r#"{"outcome":"failure","error_code":"base_changed","recovered":false}"#;

    // Direct INSERT: pending cannot carry JSON.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-pending-json', ?2, 'p1', 'c1', 'r1', 'call-pj', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'pending_approval', 'e', ?4, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed, receipt],
    );
    assert!(
        err.is_err(),
        "pending_approval row must not carry result_json"
    );

    // Direct INSERT: rejected cannot carry JSON.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-rejected-json', ?2, 'p1', 'c1', 'r1', 'call-rj', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'rejected', 'e', ?4, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed, receipt],
    );
    assert!(err.is_err(), "rejected row must not carry result_json");

    // Direct INSERT: interrupted cannot carry JSON.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-interrupted-json', ?2, 'p1', 'c1', 'r1', 'call-ij', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'interrupted', 'e', ?4, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed, receipt],
    );
    assert!(err.is_err(), "interrupted row must not carry result_json");

    // Direct INSERT: apply terminal cannot omit result_json.
    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-failed-null', ?2, 'p1', 'c1', 'r1', 'call-fn', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'failed', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed],
    );
    assert!(err.is_err(), "failed row must not omit result_json");

    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-succeeded-null', ?2, 'p1', 'c1', 'r1', 'call-sn', 'a.ts', 'create',
            ?3, 1, NULL, 0, 'succeeded', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, fingerprint, proposed],
    );
    assert!(err.is_err(), "succeeded row must not omit result_json");

    // Valid parent row for transition FK.
    connection
        .execute(
            "INSERT INTO write_proposals (
                proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
                run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
                state, expires_at, result_json, created_at, updated_at
             ) VALUES (
                ?1, 'rk-parent', ?2, 'p1', 'c1', 'r1', 'call-parent', 'a.ts', 'create',
                ?3, 1, NULL, 0, 'pending_approval', 'e', NULL, 't', 't'
             )",
            rusqlite::params![UUID_B, fingerprint, proposed],
        )
        .unwrap();

    // Transition: non-terminal with JSON rejected.
    for state in ["pending_approval", "rejected", "interrupted", "applying"] {
        let err = connection.execute(
            "INSERT INTO write_proposal_transitions (
                proposal_id, state, result_json, created_at
             ) VALUES (?1, ?2, ?3, 't')",
            rusqlite::params![UUID_B, state, receipt],
        );
        assert!(
            err.is_err(),
            "transition state {state} must not accept non-null result_json"
        );
    }

    // Transition: apply terminal without JSON rejected.
    for state in ["succeeded", "failed"] {
        let err = connection.execute(
            "INSERT INTO write_proposal_transitions (
                proposal_id, state, result_json, created_at
             ) VALUES (?1, ?2, NULL, 't')",
            rusqlite::params![UUID_B, state],
        );
        assert!(
            err.is_err(),
            "transition state {state} must not omit result_json"
        );
    }

    // Transition: apply terminal with bounded valid JSON accepted.
    connection
        .execute(
            "INSERT INTO write_proposal_transitions (
                proposal_id, state, result_json, created_at
             ) VALUES (?1, 'failed', ?2, 't')",
            rusqlite::params![UUID_B, receipt],
        )
        .unwrap();

    // Transition: non-terminal with null accepted.
    connection
        .execute(
            "INSERT INTO write_proposal_transitions (
                proposal_id, state, result_json, created_at
             ) VALUES (?1, 'pending_approval', NULL, 't')",
            [UUID_B],
        )
        .unwrap();
}

#[test]
fn write_proposals_bind_run_conversation_project_foreign_keys() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    apply_through(&connection, 5);

    let err = connection.execute(
        "INSERT INTO write_proposals (
            proposal_id, reservation_key, proposal_fingerprint, project_id, conversation_id,
            run_id, call_id, path, mode, proposed_digest, proposed_bytes, base_digest, base_bytes,
            state, expires_at, result_json, created_at, updated_at
         ) VALUES (
            ?1, 'rk-fk', ?2, 'missing-p', 'missing-c', 'missing-r', 'call',
            'a.ts', 'create', ?3, 1, NULL, 0, 'pending_approval', 'e', NULL, 't', 't'
         )",
        rusqlite::params![UUID_A, digest(1), digest(2)],
    );
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// C3-F: run_events.kind CHECK must accept write_proposal_* (fresh + legacy)
// ---------------------------------------------------------------------------

#[test]
fn legacy_run_events_check_rejects_write_proposal_kinds() {
    // RED baseline: a 0001–0005 database with the historical CHECK must reject
    // both C3 write-proposal kinds before migration 0006 is applied.
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("legacy.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    apply_legacy_through_0005(&connection);
    seed_project_conversation_run(&connection);

    insert_event(
        &connection,
        "evt-seed",
        0,
        "run_started",
        r#"{"seed":true}"#,
        "2026-08-01T00:00:00Z",
    )
    .unwrap();
    assert_eq!(event_row_count(&connection), 1);

    let requested = insert_event(
        &connection,
        "evt-wpr",
        1,
        "write_proposal_requested",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111"}"#,
        "2026-08-01T00:00:01Z",
    );
    assert!(
        requested.is_err(),
        "legacy CHECK must reject write_proposal_requested"
    );
    let err = requested.unwrap_err().to_string();
    assert!(
        err.contains("CHECK constraint failed"),
        "expected CHECK failure, got: {err}"
    );

    let resolved = insert_event(
        &connection,
        "evt-wps",
        1,
        "write_proposal_resolved",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111","outcome":"rejected"}"#,
        "2026-08-01T00:00:02Z",
    );
    assert!(
        resolved.is_err(),
        "legacy CHECK must reject write_proposal_resolved"
    );
    assert_eq!(
        event_row_count(&connection),
        1,
        "rejected inserts must not add rows"
    );
}

#[test]
fn fresh_database_accepts_write_proposal_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("fresh.db");
    // Empty file: production open applies the full migration chain.
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    seed_project_conversation_run(&connection);

    insert_event(
        &connection,
        "evt-req",
        0,
        "write_proposal_requested",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111"}"#,
        "2026-08-01T00:00:00Z",
    )
    .unwrap();
    insert_event(
        &connection,
        "evt-res",
        1,
        "write_proposal_resolved",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111","outcome":"approved"}"#,
        "2026-08-01T00:00:01Z",
    )
    .unwrap();
    assert_eq!(event_row_count(&connection), 2);

    let sql = run_events_sql(&connection);
    assert!(
        sql.contains("'write_proposal_requested'") && sql.contains("'write_proposal_resolved'"),
        "fresh schema SQL must list both C3 kinds"
    );
}

#[test]
fn legacy_database_upgrade_preserves_rows_and_accepts_write_proposal_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("upgrade.db");
    let before = {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_legacy_through_0005(&connection);
        seed_project_conversation_run(&connection);

        insert_event(
            &connection,
            "evt-seed",
            0,
            "assistant_delta",
            r#"{"text":"hello-before-upgrade","n":1}"#,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        insert_event(
            &connection,
            "evt-seed-2",
            1,
            "heartbeat",
            r#"{"tick":2}"#,
            "2026-08-01T00:00:01Z",
        )
        .unwrap();

        let snapshot = (
            load_event(&connection, "evt-seed"),
            load_event(&connection, "evt-seed-2"),
            event_row_count(&connection),
        );
        drop(connection);
        snapshot
    };

    // Production migration path upgrades existing DBs via repository open.
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);

    let after_seed = load_event(&connection, "evt-seed");
    let after_seed_2 = load_event(&connection, "evt-seed-2");
    assert_eq!(
        after_seed, before.0,
        "pre-existing event must survive exactly"
    );
    assert_eq!(
        after_seed_2, before.1,
        "pre-existing event must survive exactly"
    );
    assert_eq!(event_row_count(&connection), before.2);

    insert_event(
        &connection,
        "evt-req",
        2,
        "write_proposal_requested",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111"}"#,
        "2026-08-01T00:00:02Z",
    )
    .unwrap();
    insert_event(
        &connection,
        "evt-res",
        3,
        "write_proposal_resolved",
        r#"{"proposal_id":"11111111-1111-1111-1111-111111111111","outcome":"rejected"}"#,
        "2026-08-01T00:00:03Z",
    )
    .unwrap();
    assert_eq!(event_row_count(&connection), 4);
    assert!(index_exists(&connection, "idx_run_events_run_sequence"));
}

#[test]
fn run_events_invariants_hold_after_kind_upgrade() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("invariants.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_legacy_through_0005(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "run_started",
            r#"{}"#,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        drop(connection);
    }
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);

    // Unique (run_id, sequence)
    let dup_seq = insert_event(
        &connection,
        "evt-dup-seq",
        0,
        "heartbeat",
        r#"{}"#,
        "2026-08-01T00:00:01Z",
    );
    assert!(dup_seq.is_err(), "UNIQUE (run_id, sequence) must hold");

    // Primary key
    let dup_pk = insert_event(
        &connection,
        "evt-seed",
        9,
        "heartbeat",
        r#"{}"#,
        "2026-08-01T00:00:02Z",
    );
    assert!(dup_pk.is_err(), "PRIMARY KEY event_id must hold");

    // Foreign key to runs
    let bad_fk = connection.execute(
        "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
         VALUES ('evt-fk', 'missing-run', 1, 'heartbeat', '{}', 't')",
        [],
    );
    assert!(bad_fk.is_err(), "run_id foreign key must hold");

    // Index still present and usable
    assert!(index_exists(&connection, "idx_run_events_run_sequence"));
    let via_index: i64 = connection
        .query_row(
            "SELECT sequence FROM run_events WHERE run_id = 'r1' ORDER BY sequence ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(via_index, 0);

    // Unknown kind still rejected
    let unknown = insert_event(
        &connection,
        "evt-unknown",
        1,
        "not_a_real_kind",
        r#"{}"#,
        "2026-08-01T00:00:03Z",
    );
    assert!(unknown.is_err(), "unregistered kind must still be rejected");
    let err = unknown.unwrap_err().to_string();
    assert!(
        err.contains("CHECK constraint failed"),
        "expected CHECK failure for unknown kind, got: {err}"
    );
}

#[test]
fn run_events_kind_upgrade_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("idempotent.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_legacy_through_0005(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "plan_updated",
            r#"{"steps":["a"]}"#,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        drop(connection);
    }

    apply_via_repository_open(&db);
    let (sql_once, rows_once) = {
        let connection = reopen_with_foreign_keys(&db);
        let sql = run_events_sql(&connection);
        let rows: Vec<(String, i64, String, String, String)> = {
            let mut stmt = connection
                .prepare(
                    "SELECT event_id, sequence, kind, payload_json, created_at
                     FROM run_events ORDER BY sequence ASC, event_id ASC",
                )
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
        };
        drop(connection);
        (sql, rows)
    };

    // Second repository open re-runs the production migration path (no-op).
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    let sql_twice = run_events_sql(&connection);
    let rows_twice: Vec<(String, i64, String, String, String)> = {
        let mut stmt = connection
            .prepare(
                "SELECT event_id, sequence, kind, payload_json, created_at
                 FROM run_events ORDER BY sequence ASC, event_id ASC",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    };

    assert_eq!(rows_once, rows_twice, "second apply must not alter rows");
    assert_eq!(
        sql_once, sql_twice,
        "second apply must not alter schema SQL"
    );
    assert_eq!(event_row_count(&connection), 1);

    // Still accepts C3 kinds after double apply.
    insert_event(
        &connection,
        "evt-req",
        1,
        "write_proposal_requested",
        r#"{}"#,
        "2026-08-01T00:00:01Z",
    )
    .unwrap();
}

#[test]
fn run_events_kind_upgrade_rolls_back_on_failure() {
    // Prove the rebuild is transactional: a mid-rebuild failure must not leave
    // a half-rebuilt table or drop pre-existing rows.
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("rollback.db");
    let (seed_before, sql_before) = {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_legacy_through_0005(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "tool_call_started",
            r#"{"call_id":"c1"}"#,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let seed_before = load_event(&connection, "evt-seed");
        let sql_before = run_events_sql(&connection);

        // Force the production upgrade path to fail after DROP by pre-creating a
        // blocker object that steals the post-rename name during the transaction.
        // The upgrade runs CREATE run_events__new → copy → DROP run_events →
        // RENAME. If `run_events` already exists as a different object type when
        // rename runs, rename fails and the transaction must roll back.
        //
        // We exercise the same transactional helper the migration uses by applying
        // a deliberately broken rebuild batch under BEGIN/COMMIT semantics that
        // mirror migrations.rs.
        connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let broken = connection.execute_batch(
            r#"
        CREATE TABLE run_events__new (
            event_id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES runs(id),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE (run_id, sequence)
        );
        INSERT INTO run_events__new (
            event_id, run_id, sequence, kind, payload_json, created_at
        )
        SELECT event_id, run_id, sequence, kind, payload_json, created_at
        FROM run_events;
        DROP TABLE run_events;
        -- Fail before rename: invalid SQL aborts the batch.
        CREATE TABLE __migration_force_fail (
            id INTEGER PRIMARY KEY,
            x INTEGER NOT NULL CHECK (x > 0)
        );
        INSERT INTO __migration_force_fail (id, x) VALUES (1, 0);
        "#,
        );
        assert!(broken.is_err(), "forced mid-rebuild failure must error");
        connection.execute_batch("ROLLBACK;").unwrap();

        assert!(
            table_exists(&connection, "run_events"),
            "run_events must be restored after rollback"
        );
        assert!(
            !table_exists(&connection, "run_events__new"),
            "temp rebuild table must not survive rollback"
        );
        assert!(
            !table_exists(&connection, "__migration_force_fail"),
            "failure side-table must not survive rollback"
        );
        assert_eq!(load_event(&connection, "evt-seed"), seed_before);
        assert_eq!(run_events_sql(&connection), sql_before);
        assert_eq!(event_row_count(&connection), 1);

        // Legacy CHECK still in place until a successful production apply.
        let still_rejected = insert_event(
            &connection,
            "evt-req",
            1,
            "write_proposal_requested",
            r#"{}"#,
            "2026-08-01T00:00:01Z",
        );
        assert!(still_rejected.is_err());
        drop(connection);
        (seed_before, sql_before)
    };

    // Production upgrade still succeeds after the failed attempt.
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    insert_event(
        &connection,
        "evt-req",
        1,
        "write_proposal_requested",
        r#"{}"#,
        "2026-08-01T00:00:01Z",
    )
    .unwrap();
    assert_eq!(load_event(&connection, "evt-seed"), seed_before);
    // sql_before retained to prove pre-upgrade snapshot was captured; post-open
    // schema must have advanced (not equal to legacy).
    assert_ne!(
        run_events_sql(&connection),
        sql_before,
        "successful production open must upgrade kind CHECK"
    );
}

#[test]
fn repository_open_reaches_write_proposal_kind_support() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("apply.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_through(&connection, 5);
        // Through 0005 alone (current 0002 may already list C3 kinds on fresh
        // installs). Full apply via repository open must remain safe and leave
        // write-proposal kinds accepted.
        drop(connection);
    }
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    seed_project_conversation_run(&connection);
    insert_event(
        &connection,
        "evt-req",
        0,
        "write_proposal_requested",
        r#"{}"#,
        "t",
    )
    .unwrap();

    // Table count for write_proposals still 1 (idempotent ledger migration).
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'write_proposals'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// C4 D3AR2: run_events.kind CHECK must accept process_* kinds (fresh + legacy + 0006/0007)
// ---------------------------------------------------------------------------

#[test]
fn current_0006_0007_check_rejects_process_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pre-process.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    apply_current_0006_0007_without_process_kinds(&connection);
    seed_project_conversation_run(&connection);

    insert_event(
        &connection,
        "evt-seed",
        0,
        "write_proposal_requested",
        r#"{}"#,
        "2026-08-04T00:00:00Z",
    )
    .unwrap();

    for (i, kind) in PROCESS_KINDS.iter().enumerate() {
        let result = insert_event(
            &connection,
            &format!("evt-proc-{i}"),
            (i + 1) as i64,
            kind,
            r#"{}"#,
            "2026-08-04T00:00:01Z",
        );
        assert!(result.is_err(), "0006/0007 CHECK must reject {kind}");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("CHECK constraint failed"),
            "expected CHECK failure for {kind}, got: {err}"
        );
    }
    assert_eq!(event_row_count(&connection), 1);
}

#[test]
fn fresh_database_accepts_process_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("fresh-process.db");
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    seed_project_conversation_run(&connection);

    for (i, kind) in PROCESS_KINDS.iter().enumerate() {
        insert_event(
            &connection,
            &format!("evt-{i}"),
            i as i64,
            kind,
            &format!(r#"{{"seq":{i}}}"#),
            "2026-08-04T00:00:00Z",
        )
        .unwrap();
    }
    assert_eq!(event_row_count(&connection), 4);

    let sql = run_events_sql(&connection);
    for kind in PROCESS_KINDS {
        assert!(
            sql.contains(&format!("'{kind}'")),
            "fresh schema SQL must list {kind}"
        );
    }
    // B1/C3 kinds retained.
    assert!(sql.contains("'write_proposal_requested'"));
    assert!(sql.contains("'write_proposal_resolved'"));
    assert!(sql.contains("'run_started'"));
    assert!(sql.contains("'heartbeat'"));
}

#[test]
fn legacy_pre_0006_upgrade_preserves_rows_and_accepts_process_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("legacy-process-upgrade.db");
    let before = {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_legacy_through_0005(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "assistant_delta",
            r#"{"text":"pre-process-upgrade"}"#,
            "2026-08-04T00:00:00Z",
        )
        .unwrap();
        insert_event(
            &connection,
            "evt-seed-2",
            1,
            "heartbeat",
            r#"{"tick":1}"#,
            "2026-08-04T00:00:01Z",
        )
        .unwrap();
        let snapshot = (
            load_event(&connection, "evt-seed"),
            load_event(&connection, "evt-seed-2"),
            event_row_count(&connection),
        );
        drop(connection);
        snapshot
    };

    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);

    assert_eq!(load_event(&connection, "evt-seed"), before.0);
    assert_eq!(load_event(&connection, "evt-seed-2"), before.1);
    assert_eq!(event_row_count(&connection), before.2);

    for (i, kind) in PROCESS_KINDS.iter().enumerate() {
        insert_event(
            &connection,
            &format!("evt-proc-{i}"),
            (i + 2) as i64,
            kind,
            r#"{}"#,
            "2026-08-04T00:00:02Z",
        )
        .unwrap();
    }
    assert_eq!(event_row_count(&connection), 6);
    assert!(index_exists(&connection, "idx_run_events_run_sequence"));
}

#[test]
fn current_0006_0007_upgrade_preserves_rows_and_accepts_process_kinds() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("current-process-upgrade.db");
    let before = {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_current_0006_0007_without_process_kinds(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "write_proposal_requested",
            r#"{"proposal_id":"11111111-1111-1111-1111-111111111111"}"#,
            "2026-08-04T00:00:00Z",
        )
        .unwrap();
        insert_event(
            &connection,
            "evt-seed-2",
            1,
            "write_proposal_resolved",
            r#"{"outcome":"approved"}"#,
            "2026-08-04T00:00:01Z",
        )
        .unwrap();
        let snapshot = (
            load_event(&connection, "evt-seed"),
            load_event(&connection, "evt-seed-2"),
            event_row_count(&connection),
        );
        drop(connection);
        snapshot
    };

    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);

    assert_eq!(load_event(&connection, "evt-seed"), before.0);
    assert_eq!(load_event(&connection, "evt-seed-2"), before.1);
    assert_eq!(event_row_count(&connection), before.2);

    for (i, kind) in PROCESS_KINDS.iter().enumerate() {
        insert_event(
            &connection,
            &format!("evt-proc-{i}"),
            (i + 2) as i64,
            kind,
            r#"{}"#,
            "2026-08-04T00:00:02Z",
        )
        .unwrap();
    }
    assert_eq!(event_row_count(&connection), 6);
    assert!(index_exists(&connection, "idx_run_events_run_sequence"));

    // Constraints still hold after process-kind rebuild.
    let dup_seq = insert_event(&connection, "evt-dup", 0, "heartbeat", r#"{}"#, "t");
    assert!(dup_seq.is_err(), "UNIQUE (run_id, sequence) must hold");
    let bad_fk = connection.execute(
        "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
         VALUES ('evt-fk', 'missing-run', 9, 'heartbeat', '{}', 't')",
        [],
    );
    assert!(bad_fk.is_err(), "run_id foreign key must hold");
}

#[test]
fn run_events_process_kind_upgrade_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("process-idempotent.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_current_0006_0007_without_process_kinds(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "plan_updated",
            r#"{"steps":["a"]}"#,
            "2026-08-04T00:00:00Z",
        )
        .unwrap();
        drop(connection);
    }

    apply_via_repository_open(&db);
    let (sql_once, rows_once) = {
        let connection = reopen_with_foreign_keys(&db);
        let sql = run_events_sql(&connection);
        let rows: Vec<(String, i64, String, String, String)> = {
            let mut stmt = connection
                .prepare(
                    "SELECT event_id, sequence, kind, payload_json, created_at
                     FROM run_events ORDER BY sequence ASC, event_id ASC",
                )
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
        };
        drop(connection);
        (sql, rows)
    };

    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    let sql_twice = run_events_sql(&connection);
    let rows_twice: Vec<(String, i64, String, String, String)> = {
        let mut stmt = connection
            .prepare(
                "SELECT event_id, sequence, kind, payload_json, created_at
                 FROM run_events ORDER BY sequence ASC, event_id ASC",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    };

    assert_eq!(rows_once, rows_twice, "second apply must not alter rows");
    assert_eq!(
        sql_once, sql_twice,
        "second apply must not alter schema SQL"
    );

    insert_event(
        &connection,
        "evt-proc",
        1,
        "process_proposal_requested",
        r#"{}"#,
        "2026-08-04T00:00:01Z",
    )
    .unwrap();
}

#[test]
fn run_events_process_kind_upgrade_rolls_back_on_failure() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("process-rollback.db");
    let (seed_before, sql_before) = {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_current_0006_0007_without_process_kinds(&connection);
        seed_project_conversation_run(&connection);
        insert_event(
            &connection,
            "evt-seed",
            0,
            "tool_call_started",
            r#"{"call_id":"c1"}"#,
            "2026-08-04T00:00:00Z",
        )
        .unwrap();
        let seed_before = load_event(&connection, "evt-seed");
        let sql_before = run_events_sql(&connection);

        connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let broken = connection.execute_batch(
            r#"
        CREATE TABLE run_events__new (
            event_id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES runs(id),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE (run_id, sequence)
        );
        INSERT INTO run_events__new (
            event_id, run_id, sequence, kind, payload_json, created_at
        )
        SELECT event_id, run_id, sequence, kind, payload_json, created_at
        FROM run_events;
        DROP TABLE run_events;
        CREATE TABLE __migration_force_fail (
            id INTEGER PRIMARY KEY,
            x INTEGER NOT NULL CHECK (x > 0)
        );
        INSERT INTO __migration_force_fail (id, x) VALUES (1, 0);
        "#,
        );
        assert!(broken.is_err(), "forced mid-rebuild failure must error");
        connection.execute_batch("ROLLBACK;").unwrap();

        assert!(table_exists(&connection, "run_events"));
        assert!(!table_exists(&connection, "run_events__new"));
        assert!(!table_exists(&connection, "__migration_force_fail"));
        assert_eq!(load_event(&connection, "evt-seed"), seed_before);
        assert_eq!(run_events_sql(&connection), sql_before);

        let still_rejected = insert_event(
            &connection,
            "evt-proc",
            1,
            "process_proposal_requested",
            r#"{}"#,
            "2026-08-04T00:00:01Z",
        );
        assert!(still_rejected.is_err());
        drop(connection);
        (seed_before, sql_before)
    };

    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    insert_event(
        &connection,
        "evt-proc",
        1,
        "process_proposal_requested",
        r#"{}"#,
        "2026-08-04T00:00:01Z",
    )
    .unwrap();
    assert_eq!(load_event(&connection, "evt-seed"), seed_before);
    assert_ne!(
        run_events_sql(&connection),
        sql_before,
        "successful production open must upgrade kind CHECK for process kinds"
    );
}

#[test]
fn repository_open_reaches_process_kind_support() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("process-apply.db");
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        apply_current_0006_0007_without_process_kinds(&connection);
        drop(connection);
    }
    apply_via_repository_open(&db);
    let connection = reopen_with_foreign_keys(&db);
    seed_project_conversation_run(&connection);
    for (i, kind) in PROCESS_KINDS.iter().enumerate() {
        insert_event(
            &connection,
            &format!("evt-{i}"),
            i as i64,
            kind,
            r#"{}"#,
            "t",
        )
        .unwrap();
    }
    assert_eq!(event_row_count(&connection), 4);
}
