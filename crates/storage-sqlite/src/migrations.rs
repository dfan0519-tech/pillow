use rusqlite::Connection;

const PROJECTS_MIGRATION: &str = include_str!("../migrations/0001_projects.sql");
const CONVERSATIONS_MIGRATION: &str = include_str!("../migrations/0002_conversations.sql");
const TOOL_CALLS_MIGRATION: &str = include_str!("../migrations/0003_tool_calls.sql");
const RESOURCE_RECENTS_MIGRATION: &str = include_str!("../migrations/0004_resource_recents.sql");
const WRITE_PROPOSALS_MIGRATION: &str = include_str!("../migrations/0005_write_proposals.sql");
const RUN_EVENTS_WRITE_KINDS_MIGRATION: &str =
    include_str!("../migrations/0006_run_events_write_kinds.sql");
const EXEC_PROPOSALS_MIGRATION: &str = include_str!("../migrations/0007_exec_proposals.sql");
const RUN_EVENTS_PROCESS_KINDS_MIGRATION: &str =
    include_str!("../migrations/0008_run_events_process_kinds.sql");
const SETTINGS_MIGRATION: &str = include_str!("../migrations/0009_settings.sql");
const KNOWLEDGE_MIGRATION: &str = include_str!("../migrations/0010_knowledge.sql");
const KNOWLEDGE_TASKS_MIGRATION: &str = include_str!("../migrations/0011_knowledge_tasks.sql");
const PERMISSION_RULES_MIGRATION: &str = include_str!("../migrations/0012_permission_rules.sql");
const EXEC_PERMISSION_RULE_SEGMENTS_MIGRATION: &str =
    include_str!("../migrations/0013_exec_permission_rule_segments.sql");
const PERMISSION_MODES_MIGRATION: &str = include_str!("../migrations/0014_permission_modes.sql");
const RUNTIME_SLOTS_MIGRATION: &str = include_str!("../migrations/0015_runtime_slots.sql");
const RUNTIME_SLOT_LOGIN_MODELS_MIGRATION: &str =
    include_str!("../migrations/0016_runtime_slot_login_models.sql");
const UNIFORM_SLOT_MODEL_MIGRATION: &str =
    include_str!("../migrations/0017_uniform_slot_model.sql");

/// Apply all schema migrations in order. Each base migration is idempotent
/// (`CREATE TABLE/INDEX IF NOT EXISTS`). Migration 0006 conditionally rebuilds
/// `run_events` when the live kind CHECK still rejects C3 write-proposal kinds.
/// Migration 0007 adds the metadata-only process execution ledger.
/// Migration 0008 conditionally rebuilds `run_events` when the live kind CHECK
/// still rejects any of the four C4 process event kinds.
/// Migration 0009 adds non-secret settings (global + project scopes).
/// Migration 0010 adds the knowledge document registry and multi-valued links.
/// Migration 0011 adds the dispatch / acceptance ledger (tasks + verdicts).
/// Migration 0012 adds permission rules and rule-attributed decision receipts.
/// Migration 0013 adds ordered per-segment process-rule receipt snapshots.
/// Migration 0014 adds the global-only permission-mode guard.
/// Migration 0015 adds project-scoped runtime slots with non-secret metadata.
/// Migration 0016 lets login-auth Grok slots persist an optional selected model.
/// Migration 0017 flattens slots to auth+role; governance is derived, not stored.
pub fn apply(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(PROJECTS_MIGRATION)?;
    connection.execute_batch(CONVERSATIONS_MIGRATION)?;
    connection.execute_batch(TOOL_CALLS_MIGRATION)?;
    connection.execute_batch(RESOURCE_RECENTS_MIGRATION)?;
    connection.execute_batch(WRITE_PROPOSALS_MIGRATION)?;
    upgrade_run_events_write_proposal_kinds(connection)?;
    connection.execute_batch(EXEC_PROPOSALS_MIGRATION)?;
    upgrade_run_events_process_kinds(connection)?;
    connection.execute_batch(SETTINGS_MIGRATION)?;
    connection.execute_batch(KNOWLEDGE_MIGRATION)?;
    connection.execute_batch(KNOWLEDGE_TASKS_MIGRATION)?;
    connection.execute_batch(PERMISSION_RULES_MIGRATION)?;
    connection.execute_batch(EXEC_PERMISSION_RULE_SEGMENTS_MIGRATION)?;
    connection.execute_batch(PERMISSION_MODES_MIGRATION)?;
    connection.execute_batch(RUNTIME_SLOTS_MIGRATION)?;
    upgrade_runtime_slot_login_models(connection)?;
    upgrade_uniform_slot_model(connection)?;
    Ok(())
}

fn login_model_accepted_by_runtime_slots_check(connection: &Connection) -> rusqlite::Result<bool> {
    connection.execute("SAVEPOINT runtime_slot_login_model_probe", [])?;
    let insert = connection.execute(
        "INSERT INTO runtime_slots (
            slot_id, project_id, display_name, kind, governance, auth_mode,
            base_url, model_id, secret_ref, executable_path,
            launch_args_json, status_args_json, created_at, updated_at
         ) VALUES (
            '__pillow_runtime_slot_login_model_probe__',
            '__pillow_runtime_slot_missing_project__',
            'probe', 'governed_grok', 'full', 'login',
            NULL, 'probe-model', NULL, NULL, NULL, NULL,
            '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z'
         )",
        [],
    );
    connection.execute("ROLLBACK TO runtime_slot_login_model_probe", [])?;
    connection.execute("RELEASE runtime_slot_login_model_probe", [])?;
    match insert {
        Ok(_) => Ok(true),
        Err(rusqlite::Error::SqliteFailure(error, message)) => {
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_CHECK {
                Ok(false)
            } else if error.code == rusqlite::ErrorCode::ConstraintViolation {
                // Foreign-key rejection means the row passed the table CHECK.
                Ok(true)
            } else {
                Err(rusqlite::Error::SqliteFailure(error, message))
            }
        }
        Err(error) => Err(error),
    }
}

fn upgrade_runtime_slot_login_models(connection: &Connection) -> rusqlite::Result<()> {
    // Must check uniform schema before the login-model probe: the probe INSERTs
    // legacy columns (`kind`, `governance`) that no longer exist after 0017.
    if !table_exists(connection, "runtime_slots")?
        || uniform_slot_schema_present(connection)?
        || login_model_accepted_by_runtime_slots_check(connection)?
    {
        return Ok(());
    }
    connection.execute("BEGIN IMMEDIATE", [])?;
    if let Err(error) = connection.execute_batch(RUNTIME_SLOT_LOGIN_MODELS_MIGRATION) {
        let _ = connection.execute("ROLLBACK", []);
        return Err(error);
    }
    match login_model_accepted_by_runtime_slots_check(connection) {
        Ok(true) => {
            connection.execute("COMMIT", [])?;
            Ok(())
        }
        Ok(false) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::Unknown,
                    extended_code: 1,
                },
                Some("runtime slot login-model upgrade did not accept a model".into()),
            ))
        }
        Err(error) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(error)
        }
    }
}

fn uniform_slot_schema_present(connection: &Connection) -> rusqlite::Result<bool> {
    if !table_exists(connection, "runtime_slots")? {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(runtime_slots)")?;
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == "auth_kind"))
}

fn upgrade_uniform_slot_model(connection: &Connection) -> rusqlite::Result<()> {
    if !table_exists(connection, "runtime_slots")? || uniform_slot_schema_present(connection)? {
        return Ok(());
    }
    connection.execute("BEGIN IMMEDIATE", [])?;
    if let Err(error) = connection.execute_batch(UNIFORM_SLOT_MODEL_MIGRATION) {
        let _ = connection.execute("ROLLBACK", []);
        return Err(error);
    }
    match uniform_slot_schema_present(connection) {
        Ok(true) => {
            connection.execute("COMMIT", [])?;
            Ok(())
        }
        Ok(false) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::Unknown,
                    extended_code: 1,
                },
                Some("uniform slot model upgrade did not produce auth_kind".into()),
            ))
        }
        Err(error) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(error)
        }
    }
}

fn table_exists(connection: &Connection, name: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
         )",
        [name],
        |row| row.get(0),
    )
}

/// Probe the live `run_events.kind` CHECK by attempting an insert of `kind`.
///
/// Uses a savepoint so no durable row is left behind. A CHECK violation means
/// the kind is rejected; a foreign-key (or other non-CHECK) constraint means
/// the kind was accepted by CHECK. This is stronger than scanning
/// `sqlite_master` SQL text for substrings.
fn kind_accepted_by_run_events_check(
    connection: &Connection,
    kind: &str,
) -> rusqlite::Result<bool> {
    connection.execute("SAVEPOINT run_events_kind_probe", [])?;
    let insert = connection.execute(
        "INSERT INTO run_events (event_id, run_id, sequence, kind, payload_json, created_at)
         VALUES (
            '__pillow_run_events_kind_probe__',
            '__pillow_run_events_kind_probe_run__',
            0,
            ?1,
            '{}',
            '1970-01-01T00:00:00Z'
         )",
        [kind],
    );
    // Always roll the probe back, even when the insert "succeeds".
    connection.execute("ROLLBACK TO run_events_kind_probe", [])?;
    connection.execute("RELEASE run_events_kind_probe", [])?;

    match insert {
        Ok(_) => Ok(true),
        Err(rusqlite::Error::SqliteFailure(error, message)) => {
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_CHECK {
                Ok(false)
            } else if error.code == rusqlite::ErrorCode::ConstraintViolation {
                // FK / PK / UNIQUE: CHECK already accepted the kind value.
                Ok(true)
            } else {
                Err(rusqlite::Error::SqliteFailure(error, message))
            }
        }
        Err(error) => Err(error),
    }
}

fn upgrade_run_events_write_proposal_kinds(connection: &Connection) -> rusqlite::Result<()> {
    if !table_exists(connection, "run_events")? {
        return Ok(());
    }

    // Require both C3 kinds before treating the schema as current.
    let requested_ok = kind_accepted_by_run_events_check(connection, "write_proposal_requested")?;
    let resolved_ok = kind_accepted_by_run_events_check(connection, "write_proposal_resolved")?;
    if requested_ok && resolved_ok {
        return Ok(());
    }

    // Transactional rebuild: any statement failure rolls back the whole upgrade.
    connection.execute("BEGIN IMMEDIATE", [])?;
    if let Err(error) = connection.execute_batch(RUN_EVENTS_WRITE_KINDS_MIGRATION) {
        let _ = connection.execute("ROLLBACK", []);
        return Err(error);
    }

    // Strong post-condition: both kinds must now pass CHECK before commit.
    let requested_post = kind_accepted_by_run_events_check(connection, "write_proposal_requested");
    let resolved_post = kind_accepted_by_run_events_check(connection, "write_proposal_resolved");
    match (requested_post, resolved_post) {
        (Ok(true), Ok(true)) => {
            connection.execute("COMMIT", [])?;
            Ok(())
        }
        (Err(error), _) | (_, Err(error)) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(error)
        }
        (Ok(_), Ok(_)) => {
            let _ = connection.execute("ROLLBACK", []);
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::Unknown,
                    extended_code: 1,
                },
                Some("run_events kind upgrade did not accept write_proposal kinds".to_string()),
            ))
        }
    }
}

const PROCESS_EVENT_KINDS: [&str; 4] = [
    "process_proposal_requested",
    "process_proposal_decided",
    "process_started",
    "process_terminal_outcome",
];

fn upgrade_run_events_process_kinds(connection: &Connection) -> rusqlite::Result<()> {
    if !table_exists(connection, "run_events")? {
        return Ok(());
    }

    // Rebuild only when any of the four process kinds is still rejected.
    let mut all_accepted = true;
    for kind in PROCESS_EVENT_KINDS {
        if !kind_accepted_by_run_events_check(connection, kind)? {
            all_accepted = false;
            break;
        }
    }
    if all_accepted {
        return Ok(());
    }

    connection.execute("BEGIN IMMEDIATE", [])?;
    if let Err(error) = connection.execute_batch(RUN_EVENTS_PROCESS_KINDS_MIGRATION) {
        let _ = connection.execute("ROLLBACK", []);
        return Err(error);
    }

    // Strong post-condition: every process kind must pass CHECK before commit.
    for kind in PROCESS_EVENT_KINDS {
        match kind_accepted_by_run_events_check(connection, kind) {
            Ok(true) => {}
            Ok(false) => {
                let _ = connection.execute("ROLLBACK", []);
                return Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ErrorCode::Unknown,
                        extended_code: 1,
                    },
                    Some(format!(
                        "run_events kind upgrade did not accept process kind {kind}"
                    )),
                ));
            }
            Err(error) => {
                let _ = connection.execute("ROLLBACK", []);
                return Err(error);
            }
        }
    }

    connection.execute("COMMIT", [])?;
    Ok(())
}
