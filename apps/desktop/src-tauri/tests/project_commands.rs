use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use keeper_ide_desktop::app_state::AppState;

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
