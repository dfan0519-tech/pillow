use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use keeper_ide_desktop::app_state::AppState;
use keeper_ide_desktop::directory_ops::{create_child_directory, project_name_from_path};

fn test_state() -> (AppState, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!(
        "keeper-ide-project-commands-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_root).expect("test root should be created");
    let state = AppState::open(temp_root.join("keeper.db")).expect("test state should open");

    (state, temp_root)
}

#[test]
fn selected_project_must_exist() {
    let (state, _temp_root) = test_state();

    let error = state
        .select_project("00000000-0000-0000-0000-000000000001".to_owned())
        .expect_err("an unknown project must be rejected");

    assert_eq!(error.code, "project_not_found");
}

#[test]
fn selection_is_not_persisted_as_global_project_status() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("project-a");
    fs::create_dir_all(&project_root).expect("project root should be created");
    let project = state
        .create_project("A".to_owned(), project_root.to_string_lossy().into_owned())
        .expect("project should be created");

    state
        .select_project(project.id.clone())
        .expect("created project should be selectable");

    assert_eq!(
        state.list_projects().expect("projects should list"),
        vec![project]
    );
}

#[test]
fn open_or_register_selects_existing_canonical_path() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("shared-app");
    fs::create_dir_all(&project_root).expect("project root");
    let created = state
        .create_project(
            "Shared".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect("create");

    let reopened = state
        .open_or_register_project(project_root.join(".").to_string_lossy().into_owned())
        .expect("open existing");

    assert_eq!(reopened.id, created.id);
    assert_eq!(state.list_projects().unwrap().len(), 1);
}

#[test]
fn open_or_register_registers_new_folder_using_leaf_name() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("brand-new");
    fs::create_dir_all(&project_root).expect("project root");

    let opened = state
        .open_or_register_project(project_root.to_string_lossy().into_owned())
        .expect("register");

    assert_eq!(opened.name, "brand-new");
    assert_eq!(
        project_name_from_path(&project_root).unwrap(),
        "brand-new"
    );
    assert_eq!(state.list_projects().unwrap().len(), 1);
}

#[test]
fn create_project_with_folder_makes_directory_and_registers() {
    let (state, temp_root) = test_state();
    let parent = temp_root.join("parents");
    fs::create_dir_all(&parent).expect("parent");

    let project = state
        .create_project_with_folder("New App".to_owned(), parent.to_string_lossy().into_owned())
        .expect("create with folder");

    let expected = parent.join("New App");
    assert!(expected.is_dir());
    assert_eq!(project.name, "New App");
    assert_eq!(
        PathBuf::from(&project.root_path)
            .canonicalize()
            .unwrap(),
        expected.canonicalize().unwrap()
    );
}

#[test]
fn create_child_directory_service_is_testable_without_os_dialog() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent = std::env::temp_dir().join(format!(
        "keeper-ide-create-dir-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&parent).unwrap();
    let child = create_child_directory(&parent, "widget").unwrap();
    assert!(child.is_dir());
    let _ = fs::remove_dir_all(&parent);
}
