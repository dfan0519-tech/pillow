use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    GrokSlotAuth, NewRuntimeSlot, RuntimeSlotGovernance, RuntimeSlotKind, SqliteProjectRepository,
    SqliteRuntimeSlotRepository, UpdateGrokApiKeySlot, UpdateRuntimeSlotModel,
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
fn update_model_persists_for_login_and_api_key_slots_but_rejects_external_cli() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let login = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Login".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login { model_id: None },
            },
        })
        .unwrap();
    let api_secret_ref = uuid::Uuid::new_v4().to_string();
    let api = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "API".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url: "https://example.invalid/v1".into(),
                    model_id: "api-old".into(),
                    secret_ref: api_secret_ref.clone(),
                },
            },
        })
        .unwrap();
    let external = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "External".into(),
            governance: RuntimeSlotGovernance::None,
            kind: RuntimeSlotKind::ExternalCli {
                executable_path: "C:\\tools\\external.exe".into(),
                launch_args: vec!["run".into()],
                status_args: vec!["status".into()],
            },
        })
        .unwrap();

    let login = repo
        .update_model(UpdateRuntimeSlotModel {
            project_id: project.id,
            slot_id: login.id,
            model_id: "login-new".into(),
        })
        .unwrap();
    assert!(matches!(
        login.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login {
                model_id: Some(ref model_id)
            }
        } if model_id == "login-new"
    ));

    let api = repo
        .update_model(UpdateRuntimeSlotModel {
            project_id: project.id,
            slot_id: api.id,
            model_id: "api-new".into(),
        })
        .unwrap();
    assert!(matches!(
        api.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::ApiKey {
                ref model_id,
                ref secret_ref,
                ..
            }
        } if model_id == "api-new" && secret_ref == &api_secret_ref
    ));

    let error = repo
        .update_model(UpdateRuntimeSlotModel {
            project_id: project.id,
            slot_id: external.id,
            model_id: "not-applicable".into(),
        })
        .unwrap_err();
    assert_eq!(error.code(), "runtime_slot_shape_invalid");
}

#[test]
fn legacy_runtime_slot_rows_survive_login_model_schema_upgrade_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("legacy.db");
    let project_id = keeper_project::ProjectId(uuid::Uuid::new_v4());
    let login_id = uuid::Uuid::new_v4();
    let api_id = uuid::Uuid::new_v4();
    let secret_ref = uuid::Uuid::new_v4().to_string();
    let root = temp.path().join("legacy-project");
    std::fs::create_dir_all(&root).unwrap();
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_projects.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0015_runtime_slots.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO projects (
                    id, name, root_path, canonical_root_path, trust_state, created_at, updated_at
                 ) VALUES (?1, 'legacy', ?2, ?2, 'untrusted', ?3, ?3)",
                params![
                    project_id.0.to_string(),
                    root.to_string_lossy().into_owned(),
                    "2026-08-10T00:00:00Z",
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO runtime_slots (
                    slot_id, project_id, display_name, kind, governance, auth_mode,
                    base_url, model_id, secret_ref, executable_path,
                    launch_args_json, status_args_json, created_at, updated_at
                 ) VALUES (?1, ?2, 'Login', 'governed_grok', 'full', 'login',
                           NULL, NULL, NULL, NULL, NULL, NULL, ?3, ?3)",
                params![
                    login_id.to_string(),
                    project_id.0.to_string(),
                    "2026-08-10T00:00:00Z",
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO runtime_slots (
                    slot_id, project_id, display_name, kind, governance, auth_mode,
                    base_url, model_id, secret_ref, executable_path,
                    launch_args_json, status_args_json, created_at, updated_at
                 ) VALUES (?1, ?2, 'API', 'governed_grok', 'full', 'api_key',
                           'https://example.invalid/v1', 'api-old', ?3,
                           NULL, NULL, NULL, ?4, ?4)",
                params![
                    api_id.to_string(),
                    project_id.0.to_string(),
                    secret_ref,
                    "2026-08-10T00:00:00Z",
                ],
            )
            .unwrap();
    }

    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let before = repo.list(project_id).unwrap();
    assert_eq!(before.len(), 2);
    assert!(matches!(
        before[0].kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login { model_id: None }
        }
    ));
    repo.update_model(UpdateRuntimeSlotModel {
        project_id,
        slot_id: keeper_storage_sqlite::RuntimeSlotId(login_id),
        model_id: "login-new".into(),
    })
    .unwrap();
    drop(repo);

    let reopened = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let after = reopened.list(project_id).unwrap();
    assert_eq!(after.len(), 2);
    assert!(matches!(
        after[0].kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login {
                model_id: Some(ref model_id)
            }
        } if model_id == "login-new"
    ));
    assert!(matches!(
        after[1].kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::ApiKey {
                ref model_id,
                secret_ref: ref persisted_ref,
                ..
            }
        } if model_id == "api-old" && persisted_ref == &secret_ref
    ));
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
                auth: GrokSlotAuth::Login { model_id: None },
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
            auth: GrokSlotAuth::Login { model_id: None }
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
                auth: GrokSlotAuth::Login { model_id: None },
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
            auth: GrokSlotAuth::Login { model_id: None },
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

#[test]
fn update_grok_api_key_rewrites_metadata_and_secret_ref() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let original_ref = uuid::Uuid::new_v4().to_string();
    let created = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Private model".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url: "https://example.invalid/v1".into(),
                    model_id: "model-one".into(),
                    secret_ref: original_ref.clone(),
                },
            },
        })
        .unwrap();

    let rotated_ref = uuid::Uuid::new_v4().to_string();
    let updated = repo
        .update_grok_api_key(UpdateGrokApiKeySlot {
            project_id: project.id,
            slot_id: created.id,
            display_name: "Renamed".into(),
            base_url: "https://example.invalid/v2".into(),
            model_id: "model-two".into(),
            secret_ref: rotated_ref.clone(),
        })
        .unwrap();

    assert_eq!(updated.display_name, "Renamed");
    assert!(updated.updated_at >= created.updated_at);
    assert!(matches!(
        updated.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::ApiKey {
                ref base_url,
                ref model_id,
                secret_ref: ref stored_ref,
            }
        } if base_url == "https://example.invalid/v2"
            && model_id == "model-two"
            && stored_ref == &rotated_ref
    ));
    assert_eq!(
        repo.get(project.id, created.id).unwrap(),
        Some(updated.clone())
    );
}

#[test]
fn update_grok_api_key_rejects_missing_and_non_api_slots() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let login = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Login".into(),
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login { model_id: None },
            },
        })
        .unwrap();

    let missing = repo
        .update_grok_api_key(UpdateGrokApiKeySlot {
            project_id: project.id,
            slot_id: keeper_storage_sqlite::RuntimeSlotId(uuid::Uuid::new_v4()),
            display_name: "Nope".into(),
            base_url: "https://example.invalid/v1".into(),
            model_id: "model-one".into(),
            secret_ref: uuid::Uuid::new_v4().to_string(),
        })
        .unwrap_err();
    assert_eq!(missing.code(), "runtime_slot_not_found");

    let wrong_kind = repo
        .update_grok_api_key(UpdateGrokApiKeySlot {
            project_id: project.id,
            slot_id: login.id,
            display_name: "Nope".into(),
            base_url: "https://example.invalid/v1".into(),
            model_id: "model-one".into(),
            secret_ref: uuid::Uuid::new_v4().to_string(),
        })
        .unwrap_err();
    assert_eq!(wrong_kind.code(), "runtime_slot_shape_invalid");
}

#[test]
fn delete_removes_the_row_and_is_project_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let alpha = seed_project(&db, &temp, "alpha");
    let beta = seed_project(&db, &temp, "beta");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let secret_ref = uuid::Uuid::new_v4().to_string();
    let slot = repo
        .create(NewRuntimeSlot {
            project_id: alpha.id,
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

    assert!(repo.delete(beta.id, slot.id).unwrap().is_none());
    assert_eq!(repo.get(alpha.id, slot.id).unwrap(), Some(slot.clone()));

    let removed = repo.delete(alpha.id, slot.id).unwrap().unwrap();
    assert_eq!(removed.id, slot.id);
    assert!(matches!(
        removed.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::ApiKey {
                secret_ref: ref stored_ref,
                ..
            }
        } if stored_ref == &secret_ref
    ));
    assert!(repo.get(alpha.id, slot.id).unwrap().is_none());
    assert!(repo.list(alpha.id).unwrap().is_empty());
    assert!(repo.delete(alpha.id, slot.id).unwrap().is_none());
}
