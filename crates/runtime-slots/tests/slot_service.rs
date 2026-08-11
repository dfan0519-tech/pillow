use std::path::Path;

use keeper_project::CreateProject;
use keeper_storage_sqlite::{RuntimeSlotGovernance, RuntimeSlotKind, SqliteProjectRepository};
use pillow_runtime_slots::{
    DpapiFileSecretStore, GROK_API_KEY_ENV, PreparedGrokSlot, RuntimeSlotService, SecretBytes,
    SecretStore,
};

fn seed_project(db: &Path, temp: &tempfile::TempDir, name: &str) -> keeper_project::Project {
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

#[cfg(windows)]
#[test]
fn dpapi_file_round_trip_is_user_scoped_ciphertext_not_plaintext() {
    let temp = tempfile::tempdir().unwrap();
    let store = DpapiFileSecretStore::new(temp.path().join("secrets"));
    let secret_ref = uuid::Uuid::new_v4().to_string();
    let marker = String::from("pillow-test-canary-not-a-real-key");

    store
        .put(&secret_ref, &SecretBytes::new(marker.clone()))
        .unwrap();
    let protected = std::fs::read(
        temp.path()
            .join("secrets")
            .join(format!("{secret_ref}.dpapi")),
    )
    .unwrap();
    assert!(
        !protected
            .windows(marker.len())
            .any(|bytes| bytes == marker.as_bytes())
    );

    let recovered = store.get(&secret_ref).unwrap();
    assert_eq!(recovered.as_bytes(), marker.as_bytes());
    assert!(store.exists(&secret_ref).unwrap());
}

#[cfg(windows)]
#[test]
fn configured_api_slot_generates_secret_free_usable_private_config() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();
    let marker = String::from("pillow-test-canary-not-a-real-key");

    let created = service
        .create_grok_api_key(
            project.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            SecretBytes::new(marker.clone()),
        )
        .unwrap();
    assert_eq!(created.slot.governance, RuntimeSlotGovernance::Full);
    assert!(created.secret_configured);

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    let (private_home, model_id, api_key) = match prepared {
        PreparedGrokSlot::ApiKey {
            private_home,
            model_id,
            api_key,
        } => (private_home, model_id, api_key),
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    };
    assert_eq!(model_id, "model-one");
    assert_eq!(api_key.as_bytes(), marker.as_bytes());
    assert!(private_home.starts_with(app_data.join("runtime-slots")));

    let config_path = private_home.join(".grok").join("config.toml");
    let config = std::fs::read_to_string(config_path).unwrap();
    let parsed: toml::Value = toml::from_str(&config).unwrap();
    let model = &parsed["model"]["model-one"];
    assert_eq!(model["model"].as_str(), Some("model-one"));
    assert_eq!(
        model["base_url"].as_str(),
        Some("https://example.invalid/v1")
    );
    assert_eq!(model["env_key"].as_str(), Some(GROK_API_KEY_ENV));
    assert!(!config.contains(&marker));
    assert!(!private_home.join(".grok").join("auth.json").exists());

    let database = std::fs::read(&db).unwrap();
    assert!(
        !database
            .windows(marker.len())
            .any(|bytes| bytes == marker.as_bytes())
    );
}

#[cfg(windows)]
#[test]
fn api_slot_fails_closed_if_its_private_home_contains_login_auth() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();
    let created = service
        .create_grok_api_key(
            project.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            SecretBytes::new("pillow-test-canary-not-a-real-key".into()),
        )
        .unwrap();
    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    let private_home = match prepared {
        PreparedGrokSlot::ApiKey { private_home, .. } => private_home,
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    };
    std::fs::write(private_home.join(".grok").join("auth.json"), b"forbidden").unwrap();

    let error = service
        .prepare_grok(project.id, created.slot.id)
        .err()
        .expect("login auth contamination must fail closed");
    assert_eq!(error.code(), "runtime_slot_private_home_contaminated");
}

#[test]
fn login_slot_preparation_does_not_redirect_the_default_profile() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();

    let created = service
        .create_grok_login(project.id, "Signed-in Grok".into())
        .unwrap();
    assert!(matches!(
        created.slot.kind,
        RuntimeSlotKind::GovernedGrok { .. }
    ));
    assert!(matches!(
        service.prepare_grok(project.id, created.slot.id).unwrap(),
        PreparedGrokSlot::Login
    ));
}

#[cfg(windows)]
#[test]
fn external_cli_status_returns_bounded_raw_program_output_and_stays_ungoverned() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();
    let executable = std::env::var("COMSPEC").unwrap();

    let created = service
        .create_external_cli(
            project.id,
            "Account status".into(),
            executable,
            vec!["placeholder".into()],
            vec![
                "/D".into(),
                "/C".into(),
                "echo Logged in using ChatGPT".into(),
            ],
        )
        .unwrap();
    assert_eq!(created.slot.governance, RuntimeSlotGovernance::None);

    let status = service
        .external_status(project.id, created.slot.id)
        .unwrap();
    assert_eq!(status.exit_code, Some(0));
    assert_eq!(status.stdout, "Logged in using ChatGPT\r\n");
    assert!(status.stderr.is_empty());
    assert!(!status.timed_out);
    assert!(!status.truncated);

    let error = service
        .prepare_grok(project.id, created.slot.id)
        .err()
        .expect("external slot must not prepare as Grok");
    assert_eq!(error.code(), "runtime_slot_not_governed_grok");
}

#[test]
fn list_marks_missing_secret_as_not_configured_without_exposing_a_value() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();

    #[cfg(windows)]
    {
        let created = service
            .create_grok_api_key(
                project.id,
                "Private model".into(),
                "https://example.invalid/v1".into(),
                "model-one".into(),
                SecretBytes::new("pillow-test-canary-not-a-real-key".into()),
            )
            .unwrap();
        let secret_ref = match &created.slot.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: keeper_storage_sqlite::GrokSlotAuth::ApiKey { secret_ref, .. },
            } => secret_ref,
            _ => panic!("expected api-key slot"),
        };
        std::fs::remove_file(app_data.join("secrets").join(format!("{secret_ref}.dpapi"))).unwrap();

        let listed = service.list(project.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].secret_configured);
    }
}

#[cfg(windows)]
#[test]
fn update_without_api_key_keeps_the_original_secret_usable() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();
    let original = String::from("pillow-test-canary-keep-original");
    let created = service
        .create_grok_api_key(
            project.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            SecretBytes::new(original.clone()),
        )
        .unwrap();
    let original_ref = match &created.slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth: keeper_storage_sqlite::GrokSlotAuth::ApiKey { secret_ref, .. },
        } => secret_ref.clone(),
        _ => panic!("expected api-key slot"),
    };

    let updated = service
        .update_grok_api_key(
            project.id,
            created.slot.id,
            "Renamed model".into(),
            "https://example.invalid/v2".into(),
            "model-two".into(),
            None,
        )
        .unwrap();
    assert_eq!(updated.slot.display_name, "Renamed model");
    assert!(updated.secret_configured);
    let kept_ref = match &updated.slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth:
                keeper_storage_sqlite::GrokSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref,
                },
        } => {
            assert_eq!(base_url, "https://example.invalid/v2");
            assert_eq!(model_id, "model-two");
            assert_eq!(secret_ref, &original_ref);
            secret_ref.clone()
        }
        _ => panic!("expected api-key slot"),
    };

    // Prove the retained key is still decryptable and usable for prepare —
    // not merely that update returned Ok.
    let store = DpapiFileSecretStore::new(app_data.join("secrets"));
    let recovered = store.get(&kept_ref).unwrap();
    assert_eq!(recovered.as_bytes(), original.as_bytes());

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    match prepared {
        PreparedGrokSlot::ApiKey {
            model_id, api_key, ..
        } => {
            assert_eq!(model_id, "model-two");
            assert_eq!(api_key.as_bytes(), original.as_bytes());
        }
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    }
}

#[cfg(windows)]
#[test]
fn update_with_new_api_key_rotates_and_destroys_prior_ciphertext() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();
    let original = String::from("pillow-test-canary-original-key");
    let rotated = String::from("pillow-test-canary-rotated-key");
    let created = service
        .create_grok_api_key(
            project.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            SecretBytes::new(original.clone()),
        )
        .unwrap();
    let original_ref = match &created.slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth: keeper_storage_sqlite::GrokSlotAuth::ApiKey { secret_ref, .. },
        } => secret_ref.clone(),
        _ => panic!("expected api-key slot"),
    };
    let original_path = app_data
        .join("secrets")
        .join(format!("{original_ref}.dpapi"));
    assert!(original_path.is_file());

    let updated = service
        .update_grok_api_key(
            project.id,
            created.slot.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            Some(SecretBytes::new(rotated.clone())),
        )
        .unwrap();
    let new_ref = match &updated.slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth: keeper_storage_sqlite::GrokSlotAuth::ApiKey { secret_ref, .. },
        } => secret_ref.clone(),
        _ => panic!("expected api-key slot"),
    };
    assert_ne!(new_ref, original_ref);
    assert!(!original_path.exists(), "old ciphertext file must be gone");

    let store = DpapiFileSecretStore::new(app_data.join("secrets"));
    let old_lookup = store.get(&original_ref);
    assert!(
        matches!(
            old_lookup,
            Err(pillow_runtime_slots::RuntimeSlotError::SecretMissing)
        ),
        "old secret_ref must not decrypt"
    );
    let new_value = store.get(&new_ref).unwrap();
    assert_eq!(new_value.as_bytes(), rotated.as_bytes());
    assert_ne!(new_value.as_bytes(), original.as_bytes());

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    match prepared {
        PreparedGrokSlot::ApiKey { api_key, .. } => {
            assert_eq!(api_key.as_bytes(), rotated.as_bytes());
        }
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    }
}

#[cfg(windows)]
#[test]
fn delete_destroys_slot_metadata_and_secret_ciphertext() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();
    let marker = String::from("pillow-test-canary-delete-me");
    let created = service
        .create_grok_api_key(
            project.id,
            "Private model".into(),
            "https://example.invalid/v1".into(),
            "model-one".into(),
            SecretBytes::new(marker.clone()),
        )
        .unwrap();
    let secret_ref = match &created.slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth: keeper_storage_sqlite::GrokSlotAuth::ApiKey { secret_ref, .. },
        } => secret_ref.clone(),
        _ => panic!("expected api-key slot"),
    };
    // Materialize private home so delete must clean it too.
    let _ = service.prepare_grok(project.id, created.slot.id).unwrap();
    let secret_path = app_data.join("secrets").join(format!("{secret_ref}.dpapi"));
    let private_home = app_data
        .join("runtime-slots")
        .join(project.id.0.to_string())
        .join(created.slot.id.0.to_string());
    assert!(secret_path.is_file());
    assert!(private_home.is_dir());

    service.delete(project.id, created.slot.id).unwrap();

    assert!(service.list(project.id).unwrap().is_empty());
    assert!(!secret_path.exists());
    let store = DpapiFileSecretStore::new(app_data.join("secrets"));
    assert!(matches!(
        store.get(&secret_ref),
        Err(pillow_runtime_slots::RuntimeSlotError::SecretMissing)
    ));
    assert!(!private_home.exists());
    let missing = service
        .prepare_grok(project.id, created.slot.id)
        .err()
        .expect("deleted slot must not prepare");
    assert_eq!(missing.code(), "runtime_slot_not_found");

    // Ciphertext bytes on disk (if any leftover) must not contain the plaintext marker.
    if let Ok(entries) = std::fs::read_dir(app_data.join("secrets")) {
        for entry in entries.flatten() {
            let bytes = std::fs::read(entry.path()).unwrap_or_default();
            assert!(
                !bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
            );
        }
    }
}
