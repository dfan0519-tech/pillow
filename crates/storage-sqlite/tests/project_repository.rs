use keeper_project::CreateProject;
use keeper_storage_sqlite::{ProjectStoreError, SqliteProjectRepository};

#[test]
fn creates_and_lists_a_project() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let created = repo
        .create(CreateProject {
            name: "Keeper IDE".into(),
            root_path: r"<workspace>/VaultSystem\keeper-ide".into(),
        })
        .unwrap();

    let projects = repo.list().unwrap();
    assert_eq!(projects, vec![created]);
}

#[test]
fn rejects_the_same_canonical_root_twice() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let input = CreateProject {
        name: "Keeper IDE".into(),
        root_path: r"<workspace>/VaultSystem\keeper-ide".into(),
    };
    repo.create(input.clone()).unwrap();
    let error = repo.create(input).unwrap_err();
    assert!(matches!(error, ProjectStoreError::DuplicateRoot));
}
