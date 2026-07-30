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
    state
        .register_picker_selection("main", &project_root)
        .expect("picker grant");
    let project = state
        .create_project_from_picker_for_window(
            "main",
            "A".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
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
    state
        .register_picker_selection("main", &project_root)
        .expect("picker grant");
    let created = state
        .create_project_from_picker_for_window(
            "main",
            "Shared".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect("create");
    state
        .register_picker_selection("main", &project_root)
        .expect("picker grant");

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
    state
        .register_picker_selection("main", &project_root)
        .expect("picker grant");

    let opened = state
        .open_or_register_project(project_root.to_string_lossy().into_owned())
        .expect("register");

    assert_eq!(opened.name, "brand-new");
    assert_eq!(project_name_from_path(&project_root).unwrap(), "brand-new");
    assert_eq!(state.list_projects().unwrap().len(), 1);
}

#[test]
fn open_or_register_rejects_existing_folder_without_native_picker_capability() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("renderer-supplied");
    fs::create_dir_all(&project_root).expect("project root");

    let error = state
        .open_or_register_project_for_window(
            "window-a",
            project_root.to_string_lossy().into_owned(),
        )
        .expect_err("renderer input without a picker capability must fail closed");

    assert_eq!(error.code, "path_not_allowed");
    assert!(state.list_projects().unwrap().is_empty());
}

#[test]
fn open_or_register_consumes_picker_capability_and_stores_canonical_root() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("picker-backed");
    fs::create_dir_all(&project_root).expect("project root");
    state
        .register_picker_selection("window-a", &project_root)
        .expect("native picker should mint capability");

    let project = state
        .open_or_register_project_for_window(
            "window-a",
            project_root.to_string_lossy().into_owned(),
        )
        .expect("picker-backed registration should succeed");

    assert_eq!(
        PathBuf::from(&project.root_path),
        project_root,
        "canonical registration must preserve a normal display path"
    );
    assert_eq!(
        state
            .open_or_register_project_for_window(
                "window-a",
                project_root.to_string_lossy().into_owned(),
            )
            .expect_err("picker grant must be one use")
            .code,
        "path_not_allowed"
    );
}

#[test]
fn renderer_create_project_requires_native_picker_capability() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("direct-renderer-root");
    fs::create_dir_all(&project_root).expect("project root");

    let error = state
        .create_project_from_picker_for_window(
            "window-a",
            "Denied".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect_err("direct renderer path must not register without picker authority");
    assert_eq!(error.code, "path_not_allowed");

    state
        .register_picker_selection("window-a", &project_root)
        .expect("picker grant");
    let project = state
        .create_project_from_picker_for_window(
            "window-a",
            "Allowed".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect("picker-backed renderer registration");
    assert_eq!(PathBuf::from(project.root_path), project_root);
}

#[test]
fn create_project_with_folder_makes_directory_and_registers_when_parent_granted() {
    let (state, temp_root) = test_state();
    let parent = temp_root.join("parents");
    fs::create_dir_all(&parent).expect("parent");
    state
        .register_picker_selection("window-a", &parent)
        .expect("picker grant");

    let project = state
        .create_project_with_folder_for_window(
            "window-a",
            "New App".to_owned(),
            parent.to_string_lossy().into_owned(),
        )
        .expect("create with folder");

    let expected = parent.join("New App");
    assert!(expected.is_dir());
    assert_eq!(project.name, "New App");
    assert_eq!(
        PathBuf::from(&project.root_path).canonicalize().unwrap(),
        expected.canonicalize().unwrap()
    );
}

#[test]
fn create_project_with_folder_rejects_parent_without_picker_capability() {
    let (state, temp_root) = test_state();
    let parent = temp_root.join("ungranted-parent");
    fs::create_dir_all(&parent).expect("parent");
    // Intentionally do not grant parent.

    let error = state
        .create_project_with_folder_for_window(
            "window-a",
            "Denied App".to_owned(),
            parent.to_string_lossy().into_owned(),
        )
        .expect_err("ungranted parent must be rejected");

    assert_eq!(error.code, "path_not_allowed");
    assert!(!parent.join("Denied App").exists());
}

#[test]
fn selecting_durable_project_activates_only_its_root_for_that_window() {
    let (state, temp_root) = test_state();
    let project_root = temp_root.join("durable-project");
    let other_root = temp_root.join("other-project");
    fs::create_dir_all(&project_root).expect("project root");
    fs::create_dir_all(&other_root).expect("other root");
    state
        .register_picker_selection("setup-window", &project_root)
        .expect("picker grant");
    let project = state
        .create_project_from_picker_for_window(
            "setup-window",
            "Durable".to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect("project");
    drop(state);
    let state = AppState::open(temp_root.join("keeper.db")).expect("restarted state");

    assert_eq!(
        state
            .require_project_root_for_window("window-a", project.id.clone(), &project_root)
            .expect_err("ephemeral authority must not survive restart")
            .code,
        "path_not_allowed"
    );

    state
        .select_project_for_window("window-a", project.id.clone())
        .expect("durable project should activate explicitly");

    assert!(
        state
            .require_project_root_for_window("window-a", project.id.clone(), &project_root)
            .is_ok()
    );
    assert_eq!(
        state
            .require_project_root_for_window("window-b", project.id.clone(), &project_root)
            .unwrap_err()
            .code,
        "path_not_allowed"
    );
    assert_eq!(
        state
            .require_project_root_for_window("window-a", project.id, &other_root)
            .unwrap_err()
            .code,
        "path_not_allowed"
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

#[test]
fn standalone_directory_creation_derives_one_use_child_picker_capability() {
    let (state, temp_root) = test_state();
    let parent = temp_root.join("standalone-parent");
    fs::create_dir_all(&parent).expect("parent");
    state
        .register_picker_selection("window-a", &parent)
        .expect("picker grant");

    let child = state
        .create_project_directory_from_picker_for_window(
            "window-a",
            parent.to_string_lossy().into_owned(),
            "child".to_owned(),
        )
        .expect("child directory");
    let project = state
        .create_project_from_picker_for_window("window-a", "Child".to_owned(), child.clone())
        .expect("derived child grant should be consumable once");

    assert_eq!(PathBuf::from(project.root_path), PathBuf::from(&child));
    assert_eq!(
        state
            .create_project_from_picker_for_window("window-a", "Replay".to_owned(), child)
            .expect_err("derived child grant must not be reusable")
            .code,
        "path_not_allowed"
    );
}
