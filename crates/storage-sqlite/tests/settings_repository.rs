//! S0 settings storage + resolution: migration, precedence, restart, fail-closed.

use std::collections::HashMap;

use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    ENV_GROK_EXECUTABLE, ENV_GROK_PROFILE, ENV_RUNTIME_ADAPTER, KEY_GROK_EXECUTABLE,
    KEY_GROK_PROFILE, KEY_RUNTIME_ADAPTER, RuntimeAdapterSetting, SettingScope, SettingSource,
    SettingsError, SettingsUpdatedBy, SqliteProjectRepository, SqliteSettingsRepository,
};
use rusqlite::Connection;

fn open_settings(temp: &tempfile::TempDir) -> SqliteSettingsRepository {
    SqliteSettingsRepository::open(temp.path().join("keeper.db")).expect("open settings")
}

fn seed_project(temp: &tempfile::TempDir, name: &str) -> String {
    let projects = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let root = temp.path().join(name);
    std::fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    project.id.0.to_string()
}

fn empty_env() -> HashMap<String, String> {
    HashMap::new()
}

fn env_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Migration idempotent
// ---------------------------------------------------------------------------

#[test]
fn migration_0009_is_idempotent_and_creates_settings_entries() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");

    let first = SqliteSettingsRepository::open(&db).expect("first open");
    drop(first);
    let second = SqliteSettingsRepository::open(&db).expect("second open");
    drop(second);

    let connection = Connection::open(&db).unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'settings_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);

    let mut statement = connection
        .prepare("PRAGMA table_info(settings_entries)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();

    for required in [
        "setting_id",
        "scope",
        "project_id",
        "key",
        "value_kind",
        "value_json",
        "updated_at",
        "updated_by",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "missing column {required}"
        );
    }

    // Partial unique indexes present.
    let index_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index'
               AND name IN (
                 'idx_settings_entries_global_key',
                 'idx_settings_entries_project_key'
               )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 2);

    // Scope shape CHECK: global with project_id rejected.
    let err = connection.execute(
        "INSERT INTO settings_entries (
            setting_id, scope, project_id, key, value_kind, value_json, updated_at, updated_by
         ) VALUES ('s1', 'global', 'p1', 'runtime.adapter', 'enum', '\"grok-acp\"', 't', 'user')",
        [],
    );
    assert!(err.is_err(), "global+project_id must fail CHECK");

    // Scope shape CHECK: project without project_id rejected.
    let err = connection.execute(
        "INSERT INTO settings_entries (
            setting_id, scope, project_id, key, value_kind, value_json, updated_at, updated_by
         ) VALUES ('s2', 'project', NULL, 'runtime.adapter', 'enum', '\"grok-acp\"', 't', 'user')",
        [],
    );
    assert!(err.is_err(), "project without project_id must fail CHECK");
}

// ---------------------------------------------------------------------------
// 6. Schema has no secret fields (explicit assertion)
// ---------------------------------------------------------------------------

#[test]
fn settings_schema_has_no_secret_fields() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);
    drop(repo);

    let connection = Connection::open(temp.path().join("keeper.db")).unwrap();
    let mut statement = connection
        .prepare("PRAGMA table_info(settings_entries)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();

    let forbidden_substrings = [
        "secret",
        "token",
        "password",
        "credential",
        "api_key",
        "apikey",
        "auth",
        "private_key",
        "privatekey",
    ];
    for col in &columns {
        let lower = col.to_ascii_lowercase();
        for needle in forbidden_substrings {
            assert!(
                !lower.contains(needle),
                "settings_entries must not have secret-like column `{col}`"
            );
        }
    }

    // Table name itself is not a secret store.
    let tables: Vec<String> = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name LIKE '%secret%'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(
        tables.is_empty(),
        "S0 must not create secret tables, found {tables:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Precedence: each layer wins once + source attribution
// ---------------------------------------------------------------------------

#[test]
fn precedence_default_wins_when_nothing_set() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);
    let resolved = repo.resolve_runtime(None, &empty_env()).unwrap();

    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::GrokAcp);
    assert_eq!(resolved.adapter.source, SettingSource::Default);
    assert_eq!(resolved.grok_executable.value, None);
    assert_eq!(resolved.grok_executable.source, SettingSource::Default);
    assert_eq!(resolved.grok_profile.value, None);
    assert_eq!(resolved.grok_profile.source, SettingSource::Default);
}

#[test]
fn precedence_global_wins_over_default() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);

    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::Fake,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Global,
        None,
        KEY_GROK_EXECUTABLE,
        r"C:\tools\grok.exe",
        SettingsUpdatedBy::User,
    )
    .unwrap();

    let resolved = repo.resolve_runtime(None, &empty_env()).unwrap();
    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::Fake);
    assert_eq!(resolved.adapter.source, SettingSource::Global);
    assert_eq!(
        resolved.grok_executable.value.as_deref(),
        Some(r"C:\tools\grok.exe")
    );
    assert_eq!(resolved.grok_executable.source, SettingSource::Global);
}

#[test]
fn precedence_project_wins_over_global() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_settings(&temp);

    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::GrokAcp,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Global,
        None,
        KEY_GROK_PROFILE,
        r"C:\global-profile",
        SettingsUpdatedBy::User,
    )
    .unwrap();

    repo.upsert_runtime_adapter(
        SettingScope::Project,
        Some(&project_id),
        RuntimeAdapterSetting::Fake,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Project,
        Some(&project_id),
        KEY_GROK_PROFILE,
        r"C:\project-profile",
        SettingsUpdatedBy::User,
    )
    .unwrap();

    let resolved = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .unwrap();
    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::Fake);
    assert_eq!(resolved.adapter.source, SettingSource::Project);
    assert_eq!(
        resolved.grok_profile.value.as_deref(),
        Some(r"C:\project-profile")
    );
    assert_eq!(resolved.grok_profile.source, SettingSource::Project);

    // Without project context, global still wins.
    let global_only = repo.resolve_runtime(None, &empty_env()).unwrap();
    assert_eq!(global_only.adapter.value, RuntimeAdapterSetting::GrokAcp);
    assert_eq!(global_only.adapter.source, SettingSource::Global);
    assert_eq!(
        global_only.grok_profile.value.as_deref(),
        Some(r"C:\global-profile")
    );
    assert_eq!(global_only.grok_profile.source, SettingSource::Global);
}

#[test]
fn precedence_env_wins_over_project_and_global() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-env");
    let repo = open_settings(&temp);

    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::GrokAcp,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_runtime_adapter(
        SettingScope::Project,
        Some(&project_id),
        RuntimeAdapterSetting::Fake,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Project,
        Some(&project_id),
        KEY_GROK_EXECUTABLE,
        r"C:\project\grok.exe",
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Global,
        None,
        KEY_GROK_PROFILE,
        r"C:\global-profile",
        SettingsUpdatedBy::User,
    )
    .unwrap();

    let env = env_with(&[
        (ENV_RUNTIME_ADAPTER, "grok-acp"),
        (ENV_GROK_EXECUTABLE, r"C:\env\grok.exe"),
        (ENV_GROK_PROFILE, r"C:\env\profile"),
    ]);
    let resolved = repo.resolve_runtime(Some(&project_id), &env).unwrap();

    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::GrokAcp);
    assert_eq!(resolved.adapter.source, SettingSource::Env);
    assert_eq!(
        resolved.grok_executable.value.as_deref(),
        Some(r"C:\env\grok.exe")
    );
    assert_eq!(resolved.grok_executable.source, SettingSource::Env);
    assert_eq!(
        resolved.grok_profile.value.as_deref(),
        Some(r"C:\env\profile")
    );
    assert_eq!(resolved.grok_profile.source, SettingSource::Env);
}

#[test]
fn precedence_each_layer_wins_once_for_adapter() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-layers");
    let repo = open_settings(&temp);

    // Default layer.
    let d = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .unwrap();
    assert_eq!(d.adapter.source, SettingSource::Default);
    assert_eq!(d.adapter.value, RuntimeAdapterSetting::GrokAcp);

    // Global wins.
    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::Fake,
        SettingsUpdatedBy::System,
    )
    .unwrap();
    let g = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .unwrap();
    assert_eq!(g.adapter.source, SettingSource::Global);
    assert_eq!(g.adapter.value, RuntimeAdapterSetting::Fake);

    // Project wins over global.
    repo.upsert_runtime_adapter(
        SettingScope::Project,
        Some(&project_id),
        RuntimeAdapterSetting::GrokAcp,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    let p = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .unwrap();
    assert_eq!(p.adapter.source, SettingSource::Project);
    assert_eq!(p.adapter.value, RuntimeAdapterSetting::GrokAcp);

    // Env wins over project.
    let env = env_with(&[(ENV_RUNTIME_ADAPTER, "fake")]);
    let e = repo.resolve_runtime(Some(&project_id), &env).unwrap();
    assert_eq!(e.adapter.source, SettingSource::Env);
    assert_eq!(e.adapter.value, RuntimeAdapterSetting::Fake);
}

// ---------------------------------------------------------------------------
// 4. Restart recovery
// ---------------------------------------------------------------------------

#[test]
fn settings_survive_repository_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project_id = {
        let projects = SqliteProjectRepository::open(&db).unwrap();
        let root = temp.path().join("restart-proj");
        std::fs::create_dir_all(&root).unwrap();
        projects
            .create(CreateProject {
                name: "restart".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap()
            .id
            .0
            .to_string()
    };

    {
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::Fake,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Project,
            Some(&project_id),
            KEY_GROK_EXECUTABLE,
            r"D:\recovered\grok.exe",
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);
    }

    let repo = SqliteSettingsRepository::open(&db).unwrap();
    let resolved = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .unwrap();
    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::Fake);
    assert_eq!(resolved.adapter.source, SettingSource::Global);
    assert_eq!(
        resolved.grok_executable.value.as_deref(),
        Some(r"D:\recovered\grok.exe")
    );
    assert_eq!(resolved.grok_executable.source, SettingSource::Project);

    let global = repo
        .get(SettingScope::Global, None, KEY_RUNTIME_ADAPTER)
        .unwrap()
        .expect("global row restored");
    assert_eq!(global.key, KEY_RUNTIME_ADAPTER);
}

// ---------------------------------------------------------------------------
// 5. Illegal enum fail-closed (no Fake fallback)
// ---------------------------------------------------------------------------

#[test]
fn illegal_stored_adapter_fails_closed_never_fake() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);
    drop(repo);

    // Bypass typed upsert: inject an illegal enum value that would be catastrophic
    // if silently treated as Fake.
    let connection = Connection::open(temp.path().join("keeper.db")).unwrap();
    connection
        .execute(
            "INSERT INTO settings_entries (
                setting_id, scope, project_id, key, value_kind, value_json, updated_at, updated_by
             ) VALUES (
                'bad-adapter', 'global', NULL, ?1, 'enum', '\"codex-auto\"',
                '2026-08-08T00:00:00Z', 'user'
             )",
            [KEY_RUNTIME_ADAPTER],
        )
        .unwrap();
    drop(connection);

    let repo = open_settings(&temp);
    let err = repo
        .resolve_runtime(None, &empty_env())
        .expect_err("illegal stored adapter must fail closed");

    assert!(
        matches!(err, SettingsError::InvalidRuntimeSelection { .. }),
        "expected InvalidRuntimeSelection, got {err:?}"
    );
    assert_eq!(err.code(), "invalid_runtime_selection");
    // Explicitly not a silent Fake success path.
    assert!(!matches!(
        repo.resolve_runtime(None, &empty_env()),
        Ok(resolved) if resolved.adapter.value == RuntimeAdapterSetting::Fake
    ));
}

#[test]
fn illegal_project_adapter_fails_closed_even_when_global_valid() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-bad");
    let repo = open_settings(&temp);
    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::GrokAcp,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    drop(repo);

    let connection = Connection::open(temp.path().join("keeper.db")).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
        .execute(
            "INSERT INTO settings_entries (
                setting_id, scope, project_id, key, value_kind, value_json, updated_at, updated_by
             ) VALUES (
                'bad-proj-adapter', 'project', ?1, ?2, 'enum', '\"unknown-runtime\"',
                '2026-08-08T00:00:00Z', 'user'
             )",
            rusqlite::params![project_id, KEY_RUNTIME_ADAPTER],
        )
        .unwrap();
    drop(connection);

    let repo = open_settings(&temp);
    let err = repo
        .resolve_runtime(Some(&project_id), &empty_env())
        .expect_err("project illegal enum must not fall through to global or Fake");
    assert_eq!(err.code(), "invalid_runtime_selection");
}

#[test]
fn illegal_env_adapter_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);
    let env = env_with(&[(ENV_RUNTIME_ADAPTER, "not-a-runtime")]);
    let err = repo.resolve_runtime(None, &env).unwrap_err();
    assert_eq!(err.code(), "invalid_runtime_selection");
}

// ---------------------------------------------------------------------------
// Repository: upsert / delete / list
// ---------------------------------------------------------------------------

#[test]
fn upsert_get_delete_and_list_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-crud");
    let repo = open_settings(&temp);

    let written = repo
        .upsert_path(
            SettingScope::Project,
            Some(&project_id),
            KEY_GROK_PROFILE,
            r"C:\profiles\a",
            SettingsUpdatedBy::User,
        )
        .unwrap();
    assert_eq!(written.key, KEY_GROK_PROFILE);
    assert_eq!(written.scope, SettingScope::Project);

    let again = repo
        .upsert_path(
            SettingScope::Project,
            Some(&project_id),
            KEY_GROK_PROFILE,
            r"C:\profiles\b",
            SettingsUpdatedBy::User,
        )
        .unwrap();
    assert_eq!(again.setting_id, written.setting_id, "upsert keeps id");

    let got = repo
        .get(SettingScope::Project, Some(&project_id), KEY_GROK_PROFILE)
        .unwrap()
        .unwrap();
    assert!(got.value_json.contains("profiles\\\\b") || got.value_json.contains(r"profiles\b"));

    let listed = repo.list(SettingScope::Project, Some(&project_id)).unwrap();
    assert_eq!(listed.len(), 1);

    assert!(
        repo.delete(SettingScope::Project, Some(&project_id), KEY_GROK_PROFILE)
            .unwrap()
    );
    assert!(
        repo.get(SettingScope::Project, Some(&project_id), KEY_GROK_PROFILE)
            .unwrap()
            .is_none()
    );
}

#[test]
fn delete_project_scope_removes_all_project_keys() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-del");
    let repo = open_settings(&temp);

    repo.upsert_runtime_adapter(
        SettingScope::Project,
        Some(&project_id),
        RuntimeAdapterSetting::Fake,
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_path(
        SettingScope::Project,
        Some(&project_id),
        KEY_GROK_EXECUTABLE,
        r"C:\x\grok.exe",
        SettingsUpdatedBy::User,
    )
    .unwrap();
    repo.upsert_runtime_adapter(
        SettingScope::Global,
        None,
        RuntimeAdapterSetting::GrokAcp,
        SettingsUpdatedBy::User,
    )
    .unwrap();

    let removed = repo.delete_project_scope(&project_id).unwrap();
    assert_eq!(removed, 2);
    assert!(
        repo.list(SettingScope::Project, Some(&project_id))
            .unwrap()
            .is_empty()
    );
    assert!(
        repo.get(SettingScope::Global, None, KEY_RUNTIME_ADAPTER)
            .unwrap()
            .is_some(),
        "global must remain"
    );
}

#[test]
fn unknown_key_rejected_on_write() {
    let temp = tempfile::tempdir().unwrap();
    let repo = open_settings(&temp);
    let err = repo
        .upsert(keeper_storage_sqlite::UpsertSetting {
            scope: SettingScope::Global,
            project_id: None,
            key: "ui.theme".into(),
            value_kind: "enum".into(),
            value_json: "\"dark\"".into(),
            updated_by: SettingsUpdatedBy::User,
        })
        .unwrap_err();
    assert!(matches!(err, SettingsError::UnknownKey { .. }));
}
