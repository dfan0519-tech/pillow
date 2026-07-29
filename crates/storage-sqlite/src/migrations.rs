use rusqlite::Connection;

const PROJECTS_MIGRATION: &str = include_str!("../migrations/0001_projects.sql");

pub fn apply(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(PROJECTS_MIGRATION)
}
