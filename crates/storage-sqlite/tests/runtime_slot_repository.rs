use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    DEFAULT_CUSTOM_ROLE_NAME, GrokSlotAuth, NewRuntimeSlot, RuntimeSlotGovernance, RuntimeSlotId,
    RuntimeSlotKind, SlotAuth, SlotRole, SqliteProjectRepository, SqliteRuntimeSlotRepository,
    UpdateGrokApiKeySlot, UpdateRuntimeSlot, UpdateRuntimeSlotModel, UpdateSlotAuth,
    governance_from_auth,
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

fn new_login(project_id: keeper_project::ProjectId, name: &str) -> NewRuntimeSlot {
    NewRuntimeSlot {
        project_id,
        display_name: name.into(),
        auth: SlotAuth::GrokLogin { model_id: None },
        role: SlotRole::default_unassigned(),
    }
}

fn new_api_key(
    project_id: keeper_project::ProjectId,
    name: &str,
    secret_ref: String,
) -> NewRuntimeSlot {
    NewRuntimeSlot {
        project_id,
        display_name: name.into(),
        auth: SlotAuth::ApiKey {
            base_url: "https://example.invalid/v1".into(),
            model_id: "model-one".into(),
            secret_ref,
        },
        role: SlotRole::default_unassigned(),
    }
}

fn new_local_cli(project_id: keeper_project::ProjectId, name: &str) -> NewRuntimeSlot {
    NewRuntimeSlot {
        project_id,
        display_name: name.into(),
        auth: SlotAuth::LocalCli {
            executable_path: "C:\\tools\\codex.exe".into(),
            launch_args: vec!["exec".into()],
            status_args: vec!["login".into(), "status".into()],
        },
        role: SlotRole::default_unassigned(),
    }
}

#[test]
fn governance_derivation_maps_all_three_auth_variants() {
    assert_eq!(
        governance_from_auth(&SlotAuth::ApiKey {
            base_url: "https://example.invalid/v1".into(),
            model_id: "m".into(),
            secret_ref: uuid::Uuid::new_v4().to_string(),
        }),
        RuntimeSlotGovernance::Full
    );
    assert_eq!(
        governance_from_auth(&SlotAuth::GrokLogin { model_id: None }),
        RuntimeSlotGovernance::Full
    );
    assert_eq!(
        governance_from_auth(&SlotAuth::LocalCli {
            executable_path: "C:\\tools\\x.exe".into(),
            launch_args: vec![],
            status_args: vec!["status".into()],
        }),
        RuntimeSlotGovernance::None
    );
}

#[test]
fn controller_requires_governed_auth_positive_and_negative() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    // Positive: ApiKey + Controller is accepted.
    let ok = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Controller".into(),
            auth: SlotAuth::ApiKey {
                base_url: "https://example.invalid/v1".into(),
                model_id: "model-one".into(),
                secret_ref: uuid::Uuid::new_v4().to_string(),
            },
            role: SlotRole::Controller,
        })
        .unwrap();
    assert_eq!(ok.role, SlotRole::Controller);
    assert_eq!(ok.governance(), RuntimeSlotGovernance::Full);

    // Negative: LocalCli + Controller is rejected (half-open trap).
    let err = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Bad controller".into(),
            auth: SlotAuth::LocalCli {
                executable_path: "C:\\tools\\codex.exe".into(),
                launch_args: vec![],
                status_args: vec!["status".into()],
            },
            role: SlotRole::Controller,
        })
        .unwrap_err();
    assert_eq!(err.code(), "runtime_slot_controller_requires_governed_auth");

    // Positive: GrokLogin + Controller works after demoting the first by delete.
    repo.delete(project.id, ok.id).unwrap();
    let login_controller = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Login controller".into(),
            auth: SlotAuth::GrokLogin { model_id: None },
            role: SlotRole::Controller,
        })
        .unwrap();
    assert_eq!(login_controller.role, SlotRole::Controller);
    assert!(login_controller.auth.is_governed());
}

#[test]
fn second_controller_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    repo.create(NewRuntimeSlot {
        project_id: project.id,
        display_name: "First".into(),
        auth: SlotAuth::GrokLogin { model_id: None },
        role: SlotRole::Controller,
    })
    .unwrap();

    let err = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Second".into(),
            auth: SlotAuth::GrokLogin { model_id: None },
            role: SlotRole::Controller,
        })
        .unwrap_err();
    assert_eq!(err.code(), "runtime_slot_controller_already_exists");
}

#[test]
fn custom_role_rejects_blank_name() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let err = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Named".into(),
            auth: SlotAuth::GrokLogin { model_id: None },
            role: SlotRole::Custom("   ".into()),
        })
        .unwrap_err();
    assert_eq!(err.code(), "runtime_slot_shape_invalid");
}

#[test]
fn update_model_persists_for_login_and_api_key_slots_but_rejects_local_cli() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let login = repo.create(new_login(project.id, "Login")).unwrap();
    let api_secret_ref = uuid::Uuid::new_v4().to_string();
    let api = repo
        .create(new_api_key(project.id, "API", api_secret_ref.clone()))
        .unwrap();
    let external = repo.create(new_local_cli(project.id, "External")).unwrap();

    let login = repo
        .update_model(UpdateRuntimeSlotModel {
            project_id: project.id,
            slot_id: login.id,
            model_id: "login-new".into(),
        })
        .unwrap();
    assert!(matches!(
        login.auth,
        SlotAuth::GrokLogin {
            model_id: Some(ref model_id)
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
        api.auth,
        SlotAuth::ApiKey {
            ref model_id,
            ref secret_ref,
            ..
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

/// Round-trip: write legacy shape → migrate (on open) → read fields equal.
#[test]
fn legacy_slot_rows_migrate_round_trip_preserving_secret_ref() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("legacy.db");
    let project_id = keeper_project::ProjectId(uuid::Uuid::new_v4());
    let login_id = uuid::Uuid::new_v4();
    let api_id = uuid::Uuid::new_v4();
    let external_id = uuid::Uuid::new_v4();
    let secret_ref = uuid::Uuid::new_v4().to_string();
    let root = temp.path().join("legacy-project");
    std::fs::create_dir_all(&root).unwrap();
    {
        let connection = Connection::open(&db).unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_projects.sql"))
            .unwrap();
        // Pre-0016 shape (login model_id must be NULL under 0015 CHECK).
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
        // GovernedGrok Login
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
        // GovernedGrok ApiKey — secret_ref must survive migration unchanged.
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
        // ExternalCli → LocalCli
        connection
            .execute(
                "INSERT INTO runtime_slots (
                    slot_id, project_id, display_name, kind, governance, auth_mode,
                    base_url, model_id, secret_ref, executable_path,
                    launch_args_json, status_args_json, created_at, updated_at
                 ) VALUES (?1, ?2, 'External', 'external_cli', 'none', NULL,
                           NULL, NULL, NULL, 'C:\\tools\\codex.exe',
                           '[\"exec\"]', '[\"login\",\"status\"]', ?3, ?3)",
                params![
                    external_id.to_string(),
                    project_id.0.to_string(),
                    "2026-08-10T00:00:00Z",
                ],
            )
            .unwrap();
    }

    // Open applies 0016 + 0017 migrations.
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let slots = repo.list(project_id).unwrap();
    assert_eq!(slots.len(), 3);

    let login = slots.iter().find(|s| s.id.0 == login_id).unwrap();
    assert_eq!(login.auth, SlotAuth::GrokLogin { model_id: None });
    assert_eq!(
        login.role,
        SlotRole::Custom(DEFAULT_CUSTOM_ROLE_NAME.to_owned())
    );
    assert_eq!(login.governance(), RuntimeSlotGovernance::Full);
    assert_eq!(login.display_name, "Login");

    let api = slots.iter().find(|s| s.id.0 == api_id).unwrap();
    assert!(matches!(
        &api.auth,
        SlotAuth::ApiKey {
            base_url,
            model_id,
            secret_ref: stored_ref,
        } if base_url == "https://example.invalid/v1"
            && model_id == "api-old"
            && stored_ref == &secret_ref
    ));
    assert_eq!(
        api.role,
        SlotRole::Custom(DEFAULT_CUSTOM_ROLE_NAME.to_owned())
    );
    assert_eq!(api.governance(), RuntimeSlotGovernance::Full);

    let external = slots.iter().find(|s| s.id.0 == external_id).unwrap();
    assert!(matches!(
        &external.auth,
        SlotAuth::LocalCli {
            executable_path,
            launch_args,
            status_args,
        } if executable_path == "C:\\tools\\codex.exe"
            && launch_args == &vec!["exec".to_owned()]
            && status_args == &vec!["login".to_owned(), "status".to_owned()]
    ));
    assert_eq!(
        external.role,
        SlotRole::Custom(DEFAULT_CUSTOM_ROLE_NAME.to_owned())
    );
    assert_eq!(external.governance(), RuntimeSlotGovernance::None);

    // Re-open: stable after migration (no double-apply damage).
    drop(repo);
    let reopened = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let again = reopened.list(project_id).unwrap();
    assert_eq!(again.len(), 3);
    let api_again = again.iter().find(|s| s.id.0 == api_id).unwrap();
    match &api_again.auth {
        SlotAuth::ApiKey {
            secret_ref: stored, ..
        } => assert_eq!(stored, &secret_ref),
        _ => panic!("expected api key after re-open"),
    }
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
        before[0].auth,
        SlotAuth::GrokLogin { model_id: None }
    ));
    repo.update_model(UpdateRuntimeSlotModel {
        project_id,
        slot_id: RuntimeSlotId(login_id),
        model_id: "login-new".into(),
    })
    .unwrap();
    drop(repo);

    let reopened = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let after = reopened.list(project_id).unwrap();
    assert_eq!(after.len(), 2);
    assert!(matches!(
        after[0].auth,
        SlotAuth::GrokLogin {
            model_id: Some(ref model_id)
        } if model_id == "login-new"
    ));
    assert!(matches!(
        after[1].auth,
        SlotAuth::ApiKey {
            ref model_id,
            secret_ref: ref persisted_ref,
            ..
        } if model_id == "api-old" && persisted_ref == &secret_ref
    ));
}

#[test]
fn persists_all_auth_variants_with_derived_governance() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let login = repo.create(new_login(project.id, "Default Grok")).unwrap();
    let external = repo
        .create(new_local_cli(project.id, "Codex status"))
        .unwrap();

    assert_eq!(login.governance(), RuntimeSlotGovernance::Full);
    assert!(matches!(login.auth, SlotAuth::GrokLogin { model_id: None }));
    assert_eq!(external.governance(), RuntimeSlotGovernance::None);
    assert!(matches!(external.auth, SlotAuth::LocalCli { .. }));
    // Compatibility kind view still works.
    assert!(matches!(
        login.kind(),
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login { model_id: None }
        }
    ));

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
        .create(new_api_key(project.id, "Private model", secret_ref.clone()))
        .unwrap();

    assert!(matches!(
        slot.auth,
        SlotAuth::ApiKey {
            ref base_url,
            ref model_id,
            secret_ref: ref stored_ref,
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
        lower == "api_key" || lower == "secret" || lower == "token" || lower == "governance"
    }));
    assert!(columns.iter().any(|name| name == "auth_kind"));
    assert!(columns.iter().any(|name| name == "role_kind"));
}

#[test]
fn list_and_get_are_project_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let alpha = seed_project(&db, &temp, "alpha");
    let beta = seed_project(&db, &temp, "beta");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let alpha_slot = repo.create(new_login(alpha.id, "Alpha")).unwrap();

    assert_eq!(repo.list(alpha.id).unwrap(), vec![alpha_slot.clone()]);
    assert!(repo.list(beta.id).unwrap().is_empty());
    assert_eq!(
        repo.get(alpha.id, alpha_slot.id).unwrap(),
        Some(alpha_slot.clone())
    );
    assert_eq!(repo.get(beta.id, alpha_slot.id).unwrap(), None);
}

#[test]
fn rejecting_controller_on_local_cli_is_typed_not_sql_only() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();

    let error = repo
        .create(NewRuntimeSlot {
            project_id: project.id,
            display_name: "Misleading".into(),
            auth: SlotAuth::LocalCli {
                executable_path: "C:\\tools\\codex.exe".into(),
                launch_args: vec![],
                status_args: vec!["login".into(), "status".into()],
            },
            role: SlotRole::Controller,
        })
        .unwrap_err();
    assert_eq!(
        error.code(),
        "runtime_slot_controller_requires_governed_auth"
    );
}

#[test]
fn deleting_a_project_cascades_its_slots() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    repo.create(new_login(project.id, "Default Grok")).unwrap();
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
        .create(new_api_key(
            project.id,
            "Private model",
            original_ref.clone(),
        ))
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
        updated.auth,
        SlotAuth::ApiKey {
            ref base_url,
            ref model_id,
            secret_ref: ref stored_ref,
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
    let login = repo.create(new_login(project.id, "Login")).unwrap();

    let missing = repo
        .update_grok_api_key(UpdateGrokApiKeySlot {
            project_id: project.id,
            slot_id: RuntimeSlotId(uuid::Uuid::new_v4()),
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
fn uniform_update_can_change_role_without_touching_auth() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let repo = SqliteRuntimeSlotRepository::open(&db).unwrap();
    let created = repo.create(new_login(project.id, "Slot")).unwrap();

    let updated = repo
        .update(UpdateRuntimeSlot {
            project_id: project.id,
            slot_id: created.id,
            display_name: "Slot".into(),
            role: SlotRole::Librarian,
            auth: UpdateSlotAuth::Keep,
        })
        .unwrap();
    assert_eq!(updated.role, SlotRole::Librarian);
    assert!(matches!(
        updated.auth,
        SlotAuth::GrokLogin { model_id: None }
    ));
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
        .create(new_api_key(alpha.id, "Private model", secret_ref.clone()))
        .unwrap();

    assert!(repo.delete(beta.id, slot.id).unwrap().is_none());
    assert_eq!(repo.get(alpha.id, slot.id).unwrap(), Some(slot.clone()));

    let removed = repo.delete(alpha.id, slot.id).unwrap().unwrap();
    assert_eq!(removed.id, slot.id);
    assert!(matches!(
        removed.auth,
        SlotAuth::ApiKey {
            secret_ref: ref stored_ref,
            ..
        } if stored_ref == &secret_ref
    ));
    assert!(repo.get(alpha.id, slot.id).unwrap().is_none());
    assert!(repo.list(alpha.id).unwrap().is_empty());
    assert!(repo.delete(alpha.id, slot.id).unwrap().is_none());
}
