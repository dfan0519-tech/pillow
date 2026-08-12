//! Permission-rule SQLite authority: migration, CRUD, tighten-only scope and
//! effective-set resolution. Rules are loaded from this repository only.

use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    AddPermissionRule, PermissionRuleAction, PermissionRuleError, PermissionRulePrefix,
    PermissionRuleScope, SqlitePermissionRuleRepository, SqliteProjectRepository,
};
use rusqlite::Connection;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn db_path(temp: &tempfile::TempDir) -> std::path::PathBuf {
    temp.path().join("keeper.db")
}

fn seed_project(temp: &tempfile::TempDir, name: &str) -> String {
    let path = db_path(temp);
    let projects = SqliteProjectRepository::open(&path).expect("open projects");
    let root = temp.path().join(name);
    std::fs::create_dir_all(&root).expect("create project root");
    projects
        .create(CreateProject {
            name: name.to_owned(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("create project")
        .id
        .0
        .to_string()
}

fn add(
    scope: PermissionRuleScope,
    project_id: Option<&str>,
    prefix: PermissionRulePrefix,
    pattern: &str,
    action: PermissionRuleAction,
) -> AddPermissionRule {
    AddPermissionRule {
        scope,
        project_id: project_id.map(str::to_owned),
        prefix,
        pattern: pattern.to_owned(),
        action,
    }
}

#[test]
fn permission_rule_migrations_are_idempotent_and_create_rule_and_receipt_tables() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);

    drop(SqlitePermissionRuleRepository::open(&path).expect("first open"));
    drop(SqlitePermissionRuleRepository::open(&path).expect("second open"));

    let connection = Connection::open(&path).unwrap();
    for table in [
        "permission_rules",
        "write_permission_rule_receipts",
        "exec_permission_rule_receipts",
        "exec_permission_rule_segment_receipts",
    ] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing or duplicate table {table}");
    }

    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(permission_rules)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for required in [
        "rule_id",
        "scope",
        "project_id",
        "prefix",
        "pattern",
        "action",
        "content_digest",
        "created_at",
    ] {
        assert!(
            columns.iter().any(|column| column == required),
            "{required}"
        );
    }
}

#[test]
fn global_rules_roundtrip_restart_list_and_delete() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();

    let rule = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "cargo test *",
            PermissionRuleAction::Allow,
        ))
        .unwrap();
    assert_eq!(rule.scope, PermissionRuleScope::Global);
    assert_eq!(rule.project_id, None);
    assert_eq!(rule.prefix, PermissionRulePrefix::Process);
    assert_eq!(rule.pattern, "cargo test *");
    assert_eq!(rule.action, PermissionRuleAction::Allow);
    assert!(rule.content_digest.starts_with("sha256:"));
    assert_eq!(rule.content_digest.len(), 71);
    drop(repo);

    let reopened = SqlitePermissionRuleRepository::open(&path).unwrap();
    let listed = reopened.list(PermissionRuleScope::Global, None).unwrap();
    assert_eq!(listed, vec![rule.clone()]);
    assert!(reopened.delete(&rule.rule_id).unwrap());
    assert!(!reopened.delete(&rule.rule_id).unwrap());
    assert!(
        reopened
            .list(PermissionRuleScope::Global, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn permission_rule_connection_waits_for_a_short_cross_connection_write_lock() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();
    let blocker = Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let start = Arc::new(Barrier::new(2));
    let worker_start = Arc::clone(&start);
    let worker = thread::spawn(move || {
        worker_start.wait();
        repo.add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "cargo test *",
            PermissionRuleAction::Allow,
        ))
    });

    start.wait();
    thread::sleep(Duration::from_millis(100));
    blocker.execute_batch("ROLLBACK").unwrap();

    let rule = worker
        .join()
        .expect("rule writer thread")
        .expect("repository must wait out a short SQLite writer lock");
    assert_eq!(rule.pattern, "cargo test *");
}

#[test]
fn permission_rule_repository_explicitly_pins_busy_timeout() {
    let source = include_str!("../src/permission_rules.rs");
    assert!(
        source.contains(".busy_timeout(Duration::from_secs(5))"),
        "permission-rule connections must not rely on rusqlite's changeable default busy timeout"
    );
}

#[test]
fn project_scope_explicitly_rejects_allow_before_sql() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "project-allow-rejected");
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();

    let error = repo
        .add(add(
            PermissionRuleScope::Project,
            Some(&project_id),
            PermissionRulePrefix::Edit,
            "src/**",
            PermissionRuleAction::Allow,
        ))
        .expect_err("project allow must fail closed");

    assert_eq!(error, PermissionRuleError::ProjectAllowForbidden);
    assert_eq!(error.code(), "permission_rule_project_allow_forbidden");
    assert!(
        repo.list(PermissionRuleScope::Project, Some(&project_id))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn effective_rules_are_global_allow_or_deny_plus_current_project_deny_only() {
    let temp = tempfile::tempdir().unwrap();
    let project_a = seed_project(&temp, "project-a");
    let project_b = seed_project(&temp, "project-b");
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();

    let global_allow = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Edit,
            "src/**",
            PermissionRuleAction::Allow,
        ))
        .unwrap();
    let global_deny = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "cargo publish*",
            PermissionRuleAction::Deny,
        ))
        .unwrap();
    let project_a_deny = repo
        .add(add(
            PermissionRuleScope::Project,
            Some(&project_a),
            PermissionRulePrefix::Edit,
            "secrets/**",
            PermissionRuleAction::Deny,
        ))
        .unwrap();
    let project_b_deny = repo
        .add(add(
            PermissionRuleScope::Project,
            Some(&project_b),
            PermissionRulePrefix::Write,
            "generated/**",
            PermissionRuleAction::Deny,
        ))
        .unwrap();

    let effective = repo.list_effective(Some(&project_a)).unwrap();
    let ids: Vec<&str> = effective.iter().map(|rule| rule.rule_id.as_str()).collect();
    assert!(ids.contains(&global_allow.rule_id.as_str()));
    assert!(ids.contains(&global_deny.rule_id.as_str()));
    assert!(ids.contains(&project_a_deny.rule_id.as_str()));
    assert!(!ids.contains(&project_b_deny.rule_id.as_str()));
    assert!(effective.iter().all(|rule| {
        rule.scope == PermissionRuleScope::Global || rule.action == PermissionRuleAction::Deny
    }));
}

#[test]
fn scope_shape_and_bounded_pattern_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();

    let shape = repo
        .add(add(
            PermissionRuleScope::Global,
            Some("unexpected-project"),
            PermissionRulePrefix::Write,
            "src/**",
            PermissionRuleAction::Deny,
        ))
        .unwrap_err();
    assert_eq!(shape, PermissionRuleError::ScopeShape);

    let too_long = "x".repeat(4097);
    let bounded = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Write,
            &too_long,
            PermissionRuleAction::Deny,
        ))
        .unwrap_err();
    assert_eq!(bounded, PermissionRuleError::InvalidPattern);
}

/// EA-03 sequence B: empty / whitespace-only patterns must fail at repository
/// boundary (kernel treats empty pattern as match-all). Not renderer-only.
#[test]
fn empty_and_whitespace_patterns_are_rejected_by_repository() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();

    let empty = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "",
            PermissionRuleAction::Allow,
        ))
        .expect_err("empty pattern must fail closed at repository");
    assert_eq!(empty, PermissionRuleError::InvalidPattern);
    assert_eq!(empty.code(), "permission_rule_invalid_pattern");

    let whitespace = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "   \t  ",
            PermissionRuleAction::Allow,
        ))
        .expect_err("whitespace-only pattern must fail closed at repository");
    assert_eq!(whitespace, PermissionRuleError::InvalidPattern);

    assert!(
        repo.list(PermissionRuleScope::Global, None)
            .unwrap()
            .is_empty(),
        "no empty-pattern global process allow may be stored"
    );
}

/// EA-03: repository trims pattern before persist so leading/trailing space
/// cannot smuggle a different match subject.
#[test]
fn repository_trims_pattern_before_persist() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();
    let rule = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "  cargo test *  ",
            PermissionRuleAction::Allow,
        ))
        .unwrap();
    assert_eq!(rule.pattern, "cargo test *");
}

#[test]
fn permission_rule_authority_has_no_project_directory_config_loader() {
    let source = include_str!("../src/permission_rules.rs").to_ascii_lowercase();
    for forbidden in [
        ".grok",
        ".pillow",
        "permissions.toml",
        "read_to_string",
        "file::open",
    ] {
        assert!(
            !source.contains(forbidden),
            "SQLite-only rule authority must not contain project-file loader token {forbidden}"
        );
    }
}
