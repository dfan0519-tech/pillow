//! Native permission-rule command contract tests.

use std::fs;

use keeper_project::CreateProject;
use keeper_storage_sqlite::{SqlitePermissionRuleRepository, SqliteProjectRepository};

use crate::app_state::AppState;
use crate::permission_rule_commands::{
    add_permission_rule_impl, delete_permission_rule_impl, list_permission_rules_impl,
};

fn harness() -> (
    tempfile::TempDir,
    AppState,
    SqlitePermissionRuleRepository,
    String,
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(&root).unwrap();
    let db = temp.path().join("pillow.db");
    let app = AppState::open(&db).unwrap();
    let project = SqliteProjectRepository::open(&db)
        .unwrap()
        .create(CreateProject {
            name: "permission rules".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let rules = SqlitePermissionRuleRepository::open(&db).unwrap();
    (temp, app, rules, project.id.0.to_string())
}

#[test]
fn command_crud_uses_exact_rule_view_and_snake_case_wire_contract() {
    let (_temp, app, rules, project_id) = harness();
    let global = add_permission_rule_impl(
        &rules,
        &app,
        "global".into(),
        Some(project_id.clone()),
        "process".into(),
        "cargo test *".into(),
        "allow".into(),
    )
    .unwrap();
    assert_eq!(global.scope, "global");
    assert_eq!(global.project_id, None);

    let project = add_permission_rule_impl(
        &rules,
        &app,
        "project".into(),
        Some(project_id.clone()),
        "edit".into(),
        "secrets/**".into(),
        "deny".into(),
    )
    .unwrap();
    assert_eq!(project.project_id.as_deref(), Some(project_id.as_str()));

    let json = serde_json::to_value(&project).unwrap();
    for key in [
        "rule_id",
        "scope",
        "project_id",
        "prefix",
        "pattern",
        "action",
        "created_at",
    ] {
        assert!(json.get(key).is_some(), "missing exact wire field {key}");
    }
    assert!(json.get("ruleId").is_none());

    assert_eq!(
        list_permission_rules_impl(&rules, &app, "global".into(), None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        list_permission_rules_impl(&rules, &app, "project".into(), Some(project_id))
            .unwrap()
            .len(),
        1
    );
    delete_permission_rule_impl(&rules, project.rule_id).unwrap();
}

#[test]
fn command_explicitly_rejects_project_allow() {
    let (_temp, app, rules, project_id) = harness();
    let error = add_permission_rule_impl(
        &rules,
        &app,
        "project".into(),
        Some(project_id),
        "write".into(),
        "src/**".into(),
        "allow".into(),
    )
    .unwrap_err();
    assert_eq!(error.code, "permission_rules_project_allow_forbidden");
}

#[test]
fn project_scope_requires_a_registered_project() {
    let (_temp, app, rules, _project_id) = harness();
    let error = list_permission_rules_impl(
        &rules,
        &app,
        "project".into(),
        Some("00000000-0000-0000-0000-000000000099".into()),
    )
    .unwrap_err();
    assert_eq!(error.code, "project_not_found");
}

#[test]
fn desktop_invoke_handler_registers_exact_permission_rule_commands() {
    let source = include_str!("lib.rs");
    let handler = source
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("desktop invoke handler must exist");
    for command in [
        "list_permission_rules",
        "add_permission_rule",
        "delete_permission_rule",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}
