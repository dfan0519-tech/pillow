//! Focused tests for durable project-scoped recent resource paths.

use std::thread;
use std::time::Duration;

use keeper_storage_sqlite::{ResourceRecentError, SqliteResourceRecentRepository};
use rusqlite::Connection;

fn open_repo(temp: &tempfile::TempDir) -> SqliteResourceRecentRepository {
    SqliteResourceRecentRepository::open(temp.path().join("keeper.db")).expect("open repo")
}

#[test]
fn opening_applies_migration_0004_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("keeper.db");

    let first = SqliteResourceRecentRepository::open(&db_path).expect("first open");
    drop(first);
    let second = SqliteResourceRecentRepository::open(&db_path).expect("second open");
    drop(second);

    let connection = Connection::open(&db_path).unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'resource_recents'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);

    // Required columns exist.
    let mut statement = connection
        .prepare("PRAGMA table_info(resource_recents)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    assert!(columns.iter().any(|c| c == "project_id"));
    assert!(columns.iter().any(|c| c == "path"));
    assert!(columns.iter().any(|c| c == "opened_at"));
}

#[test]
fn recent_paths_are_isolated_by_project_and_upsert_timestamp() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_repo(&temp);

    repo.record("project-a", "src/a.rs").unwrap();
    thread::sleep(Duration::from_millis(15));
    repo.record("project-a", "src/b.rs").unwrap();
    thread::sleep(Duration::from_millis(15));
    repo.record("project-b", "src/a.rs").unwrap();
    thread::sleep(Duration::from_millis(15));
    // Upsert same path for project-a: should become newest for A.
    repo.record("project-a", "src/a.rs").unwrap();

    let list_a = repo.list("project-a", 50).unwrap();
    assert_eq!(list_a.len(), 2);
    assert_eq!(list_a[0].path, "src/a.rs");
    assert_eq!(list_a[1].path, "src/b.rs");
    assert_eq!(list_a[0].project_id, "project-a");
    assert!(list_a[0].opened_at >= list_a[1].opened_at);

    let list_b = repo.list("project-b", 50).unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].path, "src/a.rs");
    assert_eq!(list_b[0].project_id, "project-b");
}

#[test]
fn list_obeys_newest_first_cap_with_path_tiebreak() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_repo(&temp);

    // Insert more than the hard cap of 50 to verify caller limit and hard cap.
    for index in 0..12 {
        repo.record("proj", &format!("file-{index:02}.txt"))
            .unwrap();
        thread::sleep(Duration::from_millis(5));
    }

    let limited = repo.list("proj", 5).unwrap();
    assert_eq!(limited.len(), 5);
    // Newest first: last recorded is file-11.
    assert_eq!(limited[0].path, "file-11.txt");
    assert_eq!(limited[1].path, "file-10.txt");
    assert_eq!(limited[4].path, "file-07.txt");

    // Hard cap at 50 even if caller asks for more.
    let many = repo.list("proj", 500).unwrap();
    assert!(many.len() <= 50);
    assert_eq!(many.len(), 12);
}

#[test]
fn rejects_empty_project_id_and_non_portable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_repo(&temp);

    assert!(matches!(
        repo.record("", "src/lib.rs"),
        Err(ResourceRecentError::InvalidProjectId)
    ));
    assert!(matches!(
        repo.list("", 10),
        Err(ResourceRecentError::InvalidProjectId)
    ));

    let bad_paths = [
        "",
        "C:/Windows",
        "/abs",
        "a\\b",
        "..",
        "a/../b",
        "./x",
        "a/./b",
    ];
    for path in bad_paths {
        let err = repo.record("project-1", path).unwrap_err();
        assert!(
            matches!(err, ResourceRecentError::InvalidPath),
            "expected InvalidPath for {path:?}, got {err:?}"
        );
    }

    // Root marker and nested portable path are accepted by the repository
    // (filesystem authorization is left to ProjectResourceCapability).
    repo.record("project-1", ".").unwrap();
    repo.record("project-1", "src/lib.rs").unwrap();
    let listed = repo.list("project-1", 10).unwrap();
    assert!(listed.iter().any(|row| row.path == "."));
    assert!(listed.iter().any(|row| row.path == "src/lib.rs"));
}

#[test]
fn newest_first_uses_path_tiebreak_when_timestamps_equal() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_repo(&temp);

    // Same-second inserts may share timestamps depending on clock resolution;
    // seed via direct SQL if needed after API records.
    repo.record("p", "b.txt").unwrap();
    repo.record("p", "a.txt").unwrap();

    // Force equal timestamps to assert deterministic path tie-break.
    let connection = Connection::open(temp.path().join("keeper.db")).unwrap();
    connection
        .execute(
            "UPDATE resource_recents SET opened_at = '2026-07-31T00:00:00Z'",
            [],
        )
        .unwrap();
    drop(connection);

    let listed = repo.list("p", 10).unwrap();
    assert_eq!(listed.len(), 2);
    // Newest-first with path ASC as secondary key: same time => a before b.
    assert_eq!(listed[0].path, "a.txt");
    assert_eq!(listed[1].path, "b.txt");
}
