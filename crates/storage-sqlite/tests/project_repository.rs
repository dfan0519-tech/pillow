use keeper_project::CreateProject;
use keeper_storage_sqlite::{ProjectStoreError, SqliteProjectRepository};

#[test]
fn creates_and_lists_a_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("keeper-ide");
    std::fs::create_dir(&project_root).unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let created = repo
        .create(CreateProject {
            name: "Keeper IDE".into(),
            root_path: project_root.to_string_lossy().into_owned(),
        })
        .unwrap();

    let projects = repo.list().unwrap();
    assert_eq!(projects, vec![created]);
}

#[test]
fn rejects_the_same_canonical_root_twice() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("keeper-ide");
    std::fs::create_dir(&project_root).unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    repo.create(CreateProject {
        name: "Keeper IDE".into(),
        root_path: project_root.to_string_lossy().into_owned(),
    })
    .unwrap();

    let error = repo
        .create(CreateProject {
            name: "Keeper IDE alias".into(),
            root_path: project_root.join(".").to_string_lossy().into_owned(),
        })
        .unwrap_err();

    assert!(matches!(error, ProjectStoreError::DuplicateRoot));
}
