//! Permission-rule SQLite authority: migration, CRUD, tighten-only scope and
//! effective-set resolution. Rules are loaded from this repository only.

use keeper_agent_protocol::canonical_arguments_digest;
use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    AddPermissionRule, PermissionRule, PermissionRuleAction, PermissionRuleError,
    PermissionRulePrefix, PermissionRuleScope, SqlitePermissionRuleRepository,
    SqliteProjectRepository, UnreadablePermissionRuleReason,
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

// —— Quarantine: one unreadable row must not disable the rule surface ————————

/// The digest body of a stored row, spelled out rather than obtained from the
/// crate's own helper: a test that asked production code for the digest could
/// not tell a row whose digest genuinely describes it from one the helper agrees
/// with by construction.
fn stored_digest(
    scope: &str,
    project_id: Option<&str>,
    prefix: &str,
    action: &str,
    pattern: &str,
) -> String {
    let body = serde_json::json!({
        "schema": "permission-rule-v1",
        "scope": scope,
        "project_id": project_id,
        "prefix": prefix,
        "action": action,
        "pattern": pattern,
    });
    canonical_arguments_digest(&body)
        .expect("digest body fits the canonical bound")
        .as_str()
        .to_owned()
}

/// A row written by a caller that bypassed the repository — the EA-03
/// sequence-B shape, and the shape a hand-edited database has.
struct RawRow<'a> {
    rule_id: &'a str,
    scope: &'a str,
    project_id: Option<&'a str>,
    prefix: &'a str,
    pattern: &'a str,
    action: &'a str,
    /// `None` writes a digest that correctly describes the row, so the row's
    /// only defect is the one under test.
    content_digest: Option<&'a str>,
    created_at: &'a str,
}

fn insert_raw(path: &std::path::Path, row: &RawRow<'_>) {
    let digest = row.content_digest.map(str::to_owned).unwrap_or_else(|| {
        stored_digest(
            row.scope,
            row.project_id,
            row.prefix,
            row.action,
            row.pattern,
        )
    });
    Connection::open(path)
        .unwrap()
        .execute(
            "INSERT INTO permission_rules (
                rule_id, scope, project_id, prefix, pattern, action,
                content_digest, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                row.rule_id,
                row.scope,
                row.project_id,
                row.prefix,
                row.pattern,
                row.action,
                digest,
                row.created_at,
            ],
        )
        .expect("bypassing caller writes the row the product would reject");
}

fn patterns(rules: &[PermissionRule]) -> Vec<&str> {
    rules.iter().map(|rule| rule.pattern.as_str()).collect()
}

fn stored_row_count(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM permission_rules", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn an_empty_pattern_row_is_quarantined_and_the_rest_of_the_store_still_loads() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    let project_id = seed_project(&temp, "project-quarantine");
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();

    let allow = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "node --version",
            PermissionRuleAction::Allow,
        ))
        .unwrap();
    let deny = repo
        .add(add(
            PermissionRuleScope::Project,
            Some(&project_id),
            PermissionRulePrefix::Edit,
            "secrets/**",
            PermissionRuleAction::Deny,
        ))
        .unwrap();

    // EA-03 sequence B: a caller that skipped the gateway stored a blank
    // pattern, which kernel policy reads as match-all.
    let planted = "11111111-1111-4111-8111-111111111111";
    insert_raw(
        &path,
        &RawRow {
            rule_id: planted,
            scope: "global",
            project_id: None,
            prefix: "process",
            pattern: "",
            action: "allow",
            content_digest: None,
            created_at: "2026-08-12T10:00:00Z",
        },
    );

    // The surface still renders, and it renders the user's own rules.
    assert_eq!(
        repo.list(PermissionRuleScope::Global, None).unwrap(),
        vec![allow.clone()]
    );
    assert_eq!(
        repo.list(PermissionRuleScope::Project, Some(&project_id))
            .unwrap(),
        vec![deny.clone()]
    );

    // Fail closed: the planted match-all pattern never reaches evaluation.
    let effective = repo.list_effective(Some(&project_id)).unwrap();
    assert_eq!(patterns(&effective), ["node --version", "secrets/**"]);
    assert!(
        !effective.iter().any(|rule| rule.rule_id == planted),
        "a match-all pattern must never load into the effective set"
    );

    let unreadable = repo.list_unreadable().unwrap();
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].rule_id, planted);
    assert_eq!(
        unreadable[0].reason,
        UnreadablePermissionRuleReason::Pattern
    );
    assert_eq!(unreadable[0].scope.as_deref(), Some("global"));
    assert_eq!(unreadable[0].prefix.as_deref(), Some("process"));
    assert_eq!(unreadable[0].action.as_deref(), Some("allow"));

    // Recovery without a database editor.
    assert!(repo.delete_unreadable(planted).unwrap());
    assert!(repo.list_unreadable().unwrap().is_empty());
    assert_eq!(
        repo.list(PermissionRuleScope::Global, None).unwrap(),
        vec![allow]
    );
    assert_eq!(
        repo.list(PermissionRuleScope::Project, Some(&project_id))
            .unwrap(),
        vec![deny]
    );
}

#[test]
fn every_unreadable_row_shape_is_reported_and_removable() {
    let tampered_digest = stored_digest("global", None, "process", "allow", "cargo test *");
    let not_a_uuid = "z".repeat(36);
    let cases: Vec<(&str, RawRow<'_>, UnreadablePermissionRuleReason)> = vec![
        (
            "id that is not a uuid",
            RawRow {
                rule_id: &not_a_uuid,
                scope: "global",
                project_id: None,
                prefix: "process",
                pattern: "cargo test *",
                action: "allow",
                content_digest: None,
                created_at: "2026-08-12T10:00:00Z",
            },
            UnreadablePermissionRuleReason::RuleId,
        ),
        (
            "pattern rewritten after the digest was taken",
            RawRow {
                rule_id: "22222222-2222-4222-8222-222222222222",
                scope: "global",
                project_id: None,
                prefix: "process",
                pattern: "rm -rf *",
                action: "allow",
                content_digest: Some(&tampered_digest),
                created_at: "2026-08-12T10:00:00Z",
            },
            UnreadablePermissionRuleReason::ContentDigest,
        ),
        (
            "whitespace-only pattern",
            RawRow {
                rule_id: "33333333-3333-4333-8333-333333333333",
                scope: "global",
                project_id: None,
                prefix: "process",
                pattern: "   ",
                action: "allow",
                content_digest: None,
                created_at: "2026-08-12T10:00:00Z",
            },
            UnreadablePermissionRuleReason::Pattern,
        ),
        (
            "control characters in the pattern",
            RawRow {
                rule_id: "44444444-4444-4444-8444-444444444444",
                scope: "global",
                project_id: None,
                prefix: "write",
                pattern: "src/**\r\n.env",
                action: "deny",
                content_digest: None,
                created_at: "2026-08-12T10:00:00Z",
            },
            UnreadablePermissionRuleReason::Pattern,
        ),
        (
            "created_at that is not a timestamp",
            RawRow {
                rule_id: "55555555-5555-4555-8555-555555555555",
                scope: "global",
                project_id: None,
                prefix: "edit",
                pattern: "src/**",
                action: "deny",
                content_digest: None,
                created_at: "yesterday",
            },
            UnreadablePermissionRuleReason::CreatedAt,
        ),
    ];

    for (label, row, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = db_path(&temp);
        let repo = SqlitePermissionRuleRepository::open(&path).unwrap();
        let kept = repo
            .add(add(
                PermissionRuleScope::Global,
                None,
                PermissionRulePrefix::Process,
                "cargo test *",
                PermissionRuleAction::Allow,
            ))
            .unwrap();
        insert_raw(&path, &row);

        assert_eq!(
            repo.list(PermissionRuleScope::Global, None).unwrap(),
            vec![kept.clone()],
            "{label}: the readable rule must still load"
        );
        assert_eq!(
            patterns(&repo.list_effective(None).unwrap()),
            ["cargo test *"],
            "{label}: only the readable rule may be evaluated"
        );

        let unreadable = repo.list_unreadable().unwrap();
        assert_eq!(unreadable.len(), 1, "{label}");
        assert_eq!(unreadable[0].rule_id, row.rule_id, "{label}");
        assert_eq!(unreadable[0].reason, expected, "{label}");

        assert!(repo.delete_unreadable(row.rule_id).unwrap(), "{label}");
        assert!(repo.list_unreadable().unwrap().is_empty(), "{label}");
        assert_eq!(
            repo.list(PermissionRuleScope::Global, None).unwrap(),
            vec![kept],
            "{label}: repair must not disturb the readable rule"
        );
    }
}

#[test]
fn a_project_scoped_allow_row_is_quarantined_not_evaluated() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    let project_id = seed_project(&temp, "project-allow-row");
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();

    let planted = "66666666-6666-4666-8666-666666666666";
    insert_raw(
        &path,
        &RawRow {
            rule_id: planted,
            scope: "project",
            project_id: Some(&project_id),
            prefix: "process",
            pattern: "cargo *",
            action: "allow",
            content_digest: None,
            created_at: "2026-08-12T10:00:00Z",
        },
    );

    assert!(
        repo.list(PermissionRuleScope::Project, Some(&project_id))
            .unwrap()
            .is_empty(),
        "a project row may only deny; an allow row is not a rule"
    );
    // `list_effective` already filters project rows to deny in SQL. Quarantine
    // is the second gate, so a project allow is unreachable by both.
    assert!(repo.list_effective(Some(&project_id)).unwrap().is_empty());

    let unreadable = repo.list_unreadable().unwrap();
    assert_eq!(unreadable.len(), 1);
    assert_eq!(
        unreadable[0].reason,
        UnreadablePermissionRuleReason::ProjectAllow
    );
    assert_eq!(unreadable[0].project_id.as_deref(), Some(project_id.as_str()));
    assert!(repo.delete_unreadable(planted).unwrap());
}

#[test]
fn rows_with_missing_or_wrong_typed_columns_are_reported_not_fatal() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    {
        // A `permission_rules` table whose CHECK constraints are gone — the
        // shape a hand-repaired database has. Migrations use
        // `CREATE TABLE IF NOT EXISTS`, so this table is adopted as-is and the
        // classifier, not the schema, is what has to hold.
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE permission_rules (
                    rule_id TEXT PRIMARY KEY,
                    scope TEXT,
                    project_id TEXT,
                    prefix TEXT,
                    pattern TEXT,
                    action TEXT,
                    content_digest TEXT,
                    created_at TEXT
                 );",
            )
            .unwrap();
        let digest = stored_digest("global", None, "process", "allow", "cargo *");
        connection
            .execute(
                "INSERT INTO permission_rules VALUES
                   ('77777777-7777-4777-8777-777777777777', 'GLOBAL', NULL,
                    'process', 'cargo *', 'allow', ?1, '2026-08-12T10:00:00Z'),
                   ('88888888-8888-4888-8888-888888888888', 'global', NULL,
                    'process', 'cargo *', 'allow', ?1, NULL),
                   (X'6E6F742D612D75756964', 'global', NULL,
                    'process', 'cargo *', 'allow', ?1, '2026-08-12T10:00:00Z')",
                [&digest],
            )
            .unwrap();
    }

    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();
    let kept = repo
        .add(add(
            PermissionRuleScope::Global,
            None,
            PermissionRulePrefix::Process,
            "cargo test *",
            PermissionRuleAction::Allow,
        ))
        .unwrap();

    assert_eq!(
        repo.list(PermissionRuleScope::Global, None).unwrap(),
        vec![kept],
        "three unreadable rows must not take the readable one down with them"
    );

    let unreadable = repo.list_unreadable().unwrap();
    let mut reasons: Vec<&str> = unreadable
        .iter()
        .map(|row| row.reason.as_str())
        .collect::<Vec<_>>();
    reasons.sort_unstable();
    // The uppercase scope is not returned by either scoped query, so an
    // unscoped recovery list is the only way the user can see it at all.
    assert_eq!(reasons, ["column", "column", "scope"]);

    for row in &unreadable {
        assert!(
            repo.delete_unreadable(&row.rule_id).unwrap(),
            "reported row {} must be removable, including a blob id",
            row.rule_id
        );
    }
    assert!(repo.list_unreadable().unwrap().is_empty());
    assert_eq!(stored_row_count(&path), 1);
}

#[test]
fn readable_rules_are_not_reachable_through_the_repair_path() {
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

    assert!(
        !repo.delete_unreadable(&rule.rule_id).unwrap(),
        "the repair path relaxes the id shape, so it must refuse readable rules"
    );
    assert_eq!(
        repo.list(PermissionRuleScope::Global, None).unwrap(),
        vec![rule]
    );

    assert_eq!(
        repo.delete_unreadable("").unwrap_err(),
        PermissionRuleError::InvalidRuleId
    );
    assert_eq!(
        repo.delete_unreadable(&"x".repeat(129)).unwrap_err(),
        PermissionRuleError::InvalidRuleId
    );
    // The ordinary delete path keeps its UUID requirement — that is why the
    // repair path exists at all.
    assert_eq!(
        repo.delete(&"z".repeat(36)).unwrap_err(),
        PermissionRuleError::InvalidRuleId
    );
}

#[test]
fn quarantine_survives_reopen_and_accounts_for_every_stored_row() {
    let temp = tempfile::tempdir().unwrap();
    let path = db_path(&temp);
    let project_id = seed_project(&temp, "project-accounting");
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();
    repo.add(add(
        PermissionRuleScope::Global,
        None,
        PermissionRulePrefix::Process,
        "cargo test *",
        PermissionRuleAction::Allow,
    ))
    .unwrap();
    repo.add(add(
        PermissionRuleScope::Project,
        Some(&project_id),
        PermissionRulePrefix::Write,
        ".env*",
        PermissionRuleAction::Deny,
    ))
    .unwrap();
    for (index, pattern) in ["", "   "].iter().enumerate() {
        insert_raw(
            &path,
            &RawRow {
                rule_id: &format!("9999999{index}-9999-4999-8999-999999999999"),
                scope: "global",
                project_id: None,
                prefix: "process",
                pattern,
                action: "allow",
                content_digest: None,
                created_at: "2026-08-12T10:00:00Z",
            },
        );
    }
    drop(repo);

    // Quarantine is a property of the stored row, not of one process's state.
    let repo = SqlitePermissionRuleRepository::open(&path).unwrap();
    let global = repo.list(PermissionRuleScope::Global, None).unwrap();
    let project = repo
        .list(PermissionRuleScope::Project, Some(&project_id))
        .unwrap();
    let unreadable = repo.list_unreadable().unwrap();
    assert_eq!(unreadable.len(), 2);

    // Every stored row is either readable in its scope or reported for repair:
    // no row can hide from both surfaces.
    assert_eq!(
        (global.len() + project.len() + unreadable.len()) as i64,
        stored_row_count(&path)
    );
}

#[test]
fn add_rejects_empty_and_whitespace_only_patterns() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqlitePermissionRuleRepository::open(db_path(&temp)).unwrap();

    // An empty pattern is match-all in kernel policy, so it can never be a
    // stored rule — not through the gateway and not through this repository.
    for pattern in ["", " ", "\t", "\u{3000}"] {
        assert_eq!(
            repo.add(add(
                PermissionRuleScope::Global,
                None,
                PermissionRulePrefix::Process,
                pattern,
                PermissionRuleAction::Allow,
            ))
            .unwrap_err(),
            PermissionRuleError::InvalidPattern,
            "pattern {pattern:?} must be rejected at write time"
        );
    }
    assert!(
        repo.list(PermissionRuleScope::Global, None)
            .unwrap()
            .is_empty()
    );
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
