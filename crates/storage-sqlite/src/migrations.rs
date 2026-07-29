use rusqlite::Connection;

const PROJECTS_MIGRATION: &str = include_str!("../migrations/0001_projects.sql");
const CONVERSATIONS_MIGRATION: &str = include_str!("../migrations/0002_conversations.sql");

/// Apply all schema migrations in order. Each migration is idempotent
/// (`CREATE TABLE/INDEX IF NOT EXISTS`) so reopening an existing database is safe.
pub fn apply(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(PROJECTS_MIGRATION)?;
    connection.execute_batch(CONVERSATIONS_MIGRATION)?;
    Ok(())
}
