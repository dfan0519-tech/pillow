use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    GrokSlotAuth, NewRuntimeSlot, RuntimeSlotGovernance, RuntimeSlotKind, SqliteProjectRepository,
    SqliteRuntimeSlotRepository,
};
use rusqlite::{Connection, params};

fn seed_project(
    db: &std::path::Path,
    temp: &tempfile::TempDir,
    name: &str,
) -> keeper_project::Project {
    let root = temp.path().join(name);
    std::fs::create_dir_all(&root).unwrap();
    SqliteProjectRepository::open(db)
        .unwrap()
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap()
}

#[test]
fn persists_both_slot_kinds_with_explicit_governance() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let login = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Default Grok".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            },
        })
        .unwrap();
    let external = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Codex status".into(),
            governance: RuntimeSlotGovernance::None,
            kind: RuntimeSlotKind::ExternalCli {
                executable_path: "C:\\tools\\codex.exe".into(),
                launch_args: vec!["exec".into()],
                status_args: vec!["login".into(), "status".into()],
            },
        })
        .unwrap();

    assert_eq!(login.governance, RuntimeSlotGovernance::Full);
    assert!(matches!(
        login.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login
        }
    ));
    assert_eq!(external.governance, RuntimeSlotGovernance::None);
    assert!(matches!(external.kind, RuntimeSlotKind::ExternalCli { .. }));

    drop(repo);
    let reopened = SqliteRuntimeSlotRepository::open(&db).unwrap();
    assert_eq!(reopened.list(project.id).unwrap(), vec![login, external]);
}

#[test]
fn api_key_slot_persists_only_an_opaque_secret_reference() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let secret_ref = uuid::Uuid::new_v4().to_string();

    let slot = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Private model".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url: "https://example.invalid/v1".into(),
                    model_id: "model-one".into(),
                    secret_ref: secret_ref.clone(),
                },
            },
        })
        .unwrap();

    assert!(matches!(
        slot.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::ApiKey {
                ref base_url,
                ref model_id,
                secret_ref: ref stored_ref,
            }
        } if base_url == "https://example.invalid/v1"
            && model_id == "model-one"
            && stored_ref == &secret_ref
    ));

    let connection = Connection::open(&db).unwrap();
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(runtime_slots)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!columns.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower == "api_key" || lower == "secret" || lower == "token"
    }));
}

#[test]
fn list_and_get_are_project_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let alpha = seed_project(&db, &temp, "alpha");
    let beta = seed_project(&db, &temp, "beta");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let alpha_slot = repo
        .create(NewRuntimeSlot {
            project_id: alpha.id,
            display_name: "Alpha".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            },
        })
        .unwrap();

    assert_eq!(repo.list(alpha.id).unwrap(), vec![alpha_slot.clone()]);
    assert!(repo.list(beta.id).unwrap().is_empty());
    assert_eq!(
        repo.get(alpha.id, alpha_slot.id).unwrap(),
        Some(alpha_slot.clone())
    );
    assert_eq!(repo.get(beta.id, alpha_slot.id).unwrap(), None);
}

#[test]
fn rejects_kind_governance_mismatch_in_typed_api_and_sql() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let error = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Misleading".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::ExternalCli {
                executable_path: "C:\\tools\\codex.exe".into(),
                launch_args: vec![],
                status_args: vec!["login".into(), "status".into()],
            },
        })
        .unwrap_err();
    assert_eq!(error.code(), "runtime_slot_shape_invalid");

    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    let direct = connection.execute(
        "INSERT INTO runtime_slots (
            slot_id, project_id, display_name, kind, governance, auth_mode,
            base_url, model_id, secret_ref, executable_path,
            launch_args_json, status_args_json, created_at, updated_at
         ) VALUES (?1, ?2, ?3, 'external_cli', 'full', NULL,
                   NULL, NULL, NULL, ?4, '[]', '[\"login\",\"status\"]', ?5, ?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            project.id.0.to_string(),
            "Direct mismatch",
            "C:\\tools\\codex.exe",
            "2026-08-11T00:00:00Z",
        ],
    );
    assert!(
        direct.is_err(),
        "database CHECK must reject misleading governance"
    );
}

#[test]
fn deleting_a_project_cascades_its_slots() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    repo.create(NewRuntimeSlot {
        project_id: project.id,
        display_name: "Default Grok".into(),
        governance: RuntimeSlotGovernance::Full,
        kind: RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login,
        },
    })
    .unwrap();
    drop(repo);

    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
        .execute(
            "DELETE FROM projects WHERE id = ?1",
            [project.id.0.to_string()],
        )
        .unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runtime_slots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
