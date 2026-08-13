//! Native permission-rule command contract tests.

use std::fs;

use keeper_project::CreateProject;
use keeper_storage_sqlite::{SqlitePermissionRuleRepository, SqliteProjectRepository};

use crate::app_state::AppState;
use crate::permission_rule_commands::{
    add_permission_rule_impl, command_pattern_or_reject, delete_permission_rule_impl,
    delete_unreadable_permission_rule_impl, list_permission_rules_impl,
    list_unreadable_permission_rules_impl,
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
        "list_unreadable_permission_rules",
        "delete_unreadable_permission_rule",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}

/// Plant a row the product would refuse to write: a rule whose digest does not
/// describe its own content. That is the shape a rewritten row has, and one of
/// the pre-existing reasons a whole rule set could fail to load.
fn plant_tampered_row(db: &std::path::Path, rule_id: &str) {
    rusqlite::Connection::open(db)
        .unwrap()
        .execute(
            "INSERT INTO permission_rules (
                rule_id, scope, project_id, prefix, pattern, action,
                content_digest, created_at
             ) VALUES (?1, 'global', NULL, 'process', 'rm -rf *', 'allow', ?2,
                       '2026-08-12T10:00:00Z')",
            rusqlite::params![rule_id, format!("sha256:{}", "0".repeat(64))],
        )
        .unwrap();
}

#[test]
fn unreadable_rows_are_listed_and_removable_without_hiding_readable_rules() {
    let (temp, app, rules, _project_id) = harness();
    let readable = add_permission_rule_impl(
        &rules,
        &app,
        "global".into(),
        None,
        "process".into(),
        "cargo test *".into(),
        "allow".into(),
    )
    .unwrap();
    let planted = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    plant_tampered_row(&temp.path().join("pillow.db"), planted);

    // The rule list still renders, and it renders the user's own rule.
    assert_eq!(
        list_permission_rules_impl(&rules, &app, "global".into(), None).unwrap(),
        vec![readable.clone()]
    );

    let unreadable = list_unreadable_permission_rules_impl(&rules).unwrap();
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].rule_id, planted);
    assert_eq!(unreadable[0].reason, "content_digest");

    // Recognition fields only. The stored pattern is deliberately absent: a row
    // the host cannot vouch for must not be re-displayed as a rule the user
    // wrote, and `rm -rf *` above is exactly why.
    let json = serde_json::to_value(&unreadable[0]).unwrap();
    for key in [
        "rule_id",
        "scope",
        "project_id",
        "prefix",
        "action",
        "created_at",
        "reason",
    ] {
        assert!(json.get(key).is_some(), "missing exact wire field {key}");
    }
    assert!(json.get("pattern").is_none(), "pattern must not be exported");
    assert!(json.get("content_digest").is_none());
    assert!(json.get("ruleId").is_none());

    delete_unreadable_permission_rule_impl(&rules, planted.into()).unwrap();
    assert!(
        list_unreadable_permission_rules_impl(&rules)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        list_permission_rules_impl(&rules, &app, "global".into(), None).unwrap(),
        vec![readable]
    );
}

#[test]
fn the_repair_command_refuses_a_readable_rule() {
    let (_temp, app, rules, _project_id) = harness();
    let readable = add_permission_rule_impl(
        &rules,
        &app,
        "global".into(),
        None,
        "process".into(),
        "cargo test *".into(),
        "allow".into(),
    )
    .unwrap();

    let error =
        delete_unreadable_permission_rule_impl(&rules, readable.rule_id.clone()).unwrap_err();
    assert_eq!(error.code, "permission_rules_not_found");
    assert_eq!(
        list_permission_rules_impl(&rules, &app, "global".into(), None).unwrap(),
        vec![readable]
    );
}

#[test]
fn direct_invoke_rejects_empty_and_whitespace_process_allow_patterns() {
    let (_temp, app, rules, _project_id) = harness();

    let empty = add_permission_rule_impl(
        &rules,
        &app,
        "global".into(),
        None,
        "process".into(),
        "".into(),
        "allow".into(),
    )
    .expect_err("empty pattern must fail at native command boundary");
    assert_eq!(empty.code, "permission_rules_invalid_pattern");

    let whitespace = add_permission_rule_impl(
        &rules,
        &app,
        "global".into(),
        None,
        "process".into(),
        "   \t  ".into(),
        "allow".into(),
    )
    .expect_err("whitespace-only pattern must fail at native command boundary");
    assert_eq!(whitespace.code, "permission_rules_invalid_pattern");

    // Bypass command-layer trim by going through repository is covered in
    // storage tests; here prove the listed global set stays empty after
    // rejected direct invokes.
    assert!(
        list_permission_rules_impl(&rules, &app, "global".into(), None)
            .unwrap()
            .is_empty()
    );
}

/// Pins the command layer on its own.
///
/// `direct_invoke_rejects_empty_and_whitespace_process_allow_patterns` above
/// goes through the repository, which rejects blank patterns as well and
/// reports the same code — so it stays green even with the command-layer check
/// deleted. Verified by mutation: removing that check left the whole suite
/// green. These call the boundary directly, so the second layer of the
/// defence the external audit asked for is actually held in place.
#[test]
fn command_layer_rejects_blank_patterns_without_consulting_the_repository() {
    for blank in ["", "   ", "\t", " \t \r\n "] {
        let error = command_pattern_or_reject(blank)
            .expect_err("command boundary must reject a blank pattern on its own");
        assert_eq!(error.code, "permission_rules_invalid_pattern");
    }
}

#[test]
fn command_layer_trims_before_the_pattern_reaches_storage() {
    // Storage trims too. This pins that the value handed on is already
    // trimmed, so the two layers cannot disagree about what was stored.
    assert_eq!(
        command_pattern_or_reject("  cargo test *  ").expect("non-blank pattern"),
        "cargo test *"
    );
}
