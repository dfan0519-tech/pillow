//! Shared helpers for conversation repository integration tests.
#![allow(dead_code)]

use keeper_project::CreateProject;
use keeper_storage_sqlite::{SqliteConversationRepository, SqliteProjectRepository};
use rusqlite::Connection;

pub fn open_pair(
    temp: &tempfile::TempDir,
) -> (SqliteProjectRepository, SqliteConversationRepository) {
    let db = temp.path().join("keeper.db");
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let sessions = SqliteConversationRepository::open(&db).unwrap();
    (projects, sessions)
}

pub fn seed_project(
    projects: &SqliteProjectRepository,
    temp: &tempfile::TempDir,
    name: &str,
) -> keeper_project::Project {
    let root = temp.path().join(name);
    std::fs::create_dir(&root).unwrap();
    projects
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap()
}

pub fn table_exists(connection: &Connection, name: &str) -> bool {
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
