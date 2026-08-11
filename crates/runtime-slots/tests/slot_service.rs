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
