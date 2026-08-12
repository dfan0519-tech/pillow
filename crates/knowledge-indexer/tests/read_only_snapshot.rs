use std::fs;
use std::path::{Path, PathBuf};

use keeper_knowledge_indexer::{
    DispatchInput, LedgerDbOptions, ReadOnlyLedgerError, dispatch_task, load_snapshot_read_only,
};

fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut value = db_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[test]
fn read_only_snapshot_loads_current_ledger_without_changing_database_or_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("ledger.db");
    let opts = LedgerDbOptions {
        db_path: db_path.clone(),
        project_name: "sample".into(),
        project_root: Some(temp.path().into()),
    };
    dispatch_task(
        &opts,
        DispatchInput {
            title: "Host audit".into(),
            brief_path: Some("docs/brief.md".into()),
            worker_label: "codex1".into(),
        },
    )
    .unwrap();

    let before = fs::read(&db_path).unwrap();
    let journal = sidecar_path(&db_path, "-journal");
    let wal = sidecar_path(&db_path, "-wal");
    let shm = sidecar_path(&db_path, "-shm");
    assert!(!journal.exists());
    assert!(!wal.exists());
    assert!(!shm.exists());

    let snapshot = load_snapshot_read_only(&db_path, "sample").unwrap();

    assert_eq!(snapshot.project_name, "sample");
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].worker_label, "codex1");
    assert_eq!(fs::read(&db_path).unwrap(), before);
    assert!(!journal.exists());
    assert!(!wal.exists());
    assert!(!shm.exists());
}

#[test]
fn read_only_snapshot_does_not_create_a_missing_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("missing.db");

    let error = load_snapshot_read_only(&db_path, "sample").unwrap_err();

    assert!(matches!(error, ReadOnlyLedgerError::DatabaseUnavailable));
    assert!(!db_path.exists());
}
