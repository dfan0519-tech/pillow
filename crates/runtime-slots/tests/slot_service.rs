use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use keeper_project::CreateProject;
use keeper_storage_sqlite::{RuntimeSlotGovernance, SlotAuth, SlotRole, SqliteProjectRepository};
use pillow_runtime_slots::{
    ACCESS_TOKEN_REFRESH_SKEW, CreateSlotAuth, DpapiFileSecretStore, GROK_API_KEY_ENV,
    OauthTokenClient, PreparedGrokSlot, RuntimeSlotError, RuntimeSlotService, SecretBytes,
    SecretStore, access_token_needs_refresh,
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

#[test]
fn login_slot_model_update_is_persisted_and_prepared_for_launch() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();
    let created = service
        .create_grok_login(project.id, "Signed-in Grok".into())
        .unwrap();

    let updated = service
        .update_model(project.id, created.slot.id, "grok-4.5".into())
        .unwrap();
    assert!(matches!(
        updated.slot.auth,
        SlotAuth::GrokLogin {
            model_id: Some(ref model_id)
        } if model_id == "grok-4.5"
    ));
    assert!(matches!(
        service.prepare_grok(project.id, created.slot.id).unwrap(),
        PreparedGrokSlot::Login {
            model_id: Some(ref model_id)
        } if model_id == "grok-4.5"
    ));
}

#[cfg(windows)]
#[test]
fn api_key_model_update_keeps_the_secret_and_prepares_the_new_model() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();
    let marker = "pillow-test-model-switch-key";
    let created = service
        .create_grok_api_key(
            project.id,
            "Private".into(),
            "https://example.invalid/v1".into(),
            "model-old".into(),
            SecretBytes::new(marker.into()),
        )
        .unwrap();

    service
        .update_model(project.id, created.slot.id, "model-new".into())
        .unwrap();
    match service.prepare_grok(project.id, created.slot.id).unwrap() {
        PreparedGrokSlot::ApiKey {
            model_id, api_key, ..
        } => {
            assert_eq!(model_id, "model-new");
            assert_eq!(api_key.as_bytes(), marker.as_bytes());
        }
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
    }
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
    assert_eq!(created.slot.governance(), RuntimeSlotGovernance::Full);
    assert!(created.secret_configured);

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    let (private_home, model_id, api_key) = match prepared {
        PreparedGrokSlot::ApiKey {
            private_home,
            model_id,
            api_key,
        } => (private_home, model_id, api_key),
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
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
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
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
    assert!(matches!(created.slot.auth, SlotAuth::GrokLogin { .. }));
    assert!(matches!(
        service.prepare_grok(project.id, created.slot.id).unwrap(),
        PreparedGrokSlot::Login { model_id: None }
    ));
}

#[cfg(windows)]
#[test]
fn external_cli_dto_is_explicitly_ungoverned_and_cannot_start() {
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
    // LocalCli is still ungoverned — now as a derived property of auth, not a second slot kind.
    assert_eq!(created.slot.governance(), RuntimeSlotGovernance::None);
    assert!(matches!(created.slot.auth, SlotAuth::LocalCli { .. }));

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
        .expect("local CLI slot must not prepare as Grok");
    assert_eq!(error.code(), "runtime_slot_not_governed_grok");
}

#[test]
fn create_slot_rejects_local_cli_as_controller() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();
    let executable = std::env::current_exe().unwrap();

    let err = service
        .create_slot(
            project.id,
            "Bad controller".into(),
            CreateSlotAuth::LocalCli {
                executable_path: executable.to_string_lossy().into_owned(),
                launch_args: vec![],
                status_args: vec!["--help".into()],
            },
            SlotRole::Controller,
        )
        .unwrap_err();
    assert_eq!(err.code(), "runtime_slot_controller_requires_governed_auth");
}

#[test]
fn create_slot_accepts_governed_auth_as_controller() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();

    let created = service
        .create_slot(
            project.id,
            "Controller".into(),
            CreateSlotAuth::GrokLogin { model_id: None },
            SlotRole::Controller,
        )
        .unwrap();
    assert_eq!(created.slot.role, SlotRole::Controller);
    assert_eq!(created.slot.governance(), RuntimeSlotGovernance::Full);
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
        let secret_ref = match &created.slot.auth {
            SlotAuth::ApiKey { secret_ref, .. } => secret_ref,
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
    let original_ref = match &created.slot.auth {
        SlotAuth::ApiKey { secret_ref, .. } => secret_ref.clone(),
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
    let kept_ref = match &updated.slot.auth {
        SlotAuth::ApiKey {
            base_url,
            model_id,
            secret_ref,
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
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
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
    let original_ref = match &created.slot.auth {
        SlotAuth::ApiKey { secret_ref, .. } => secret_ref.clone(),
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
    let new_ref = match &updated.slot.auth {
        SlotAuth::ApiKey { secret_ref, .. } => secret_ref.clone(),
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
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
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
    let secret_ref = match &created.slot.auth {
        SlotAuth::ApiKey { secret_ref, .. } => secret_ref.clone(),
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

fn runtime_token(prefix: &str, suffix: &str) -> String {
    format!("{prefix}{suffix}")
}

struct FakeOauthClient {
    bodies: Mutex<Vec<String>>,
    /// Captured form bodies (may contain refresh) — tests assert they never reach grok config.
    seen_forms: Mutex<Vec<String>>,
    fail: bool,
}

impl FakeOauthClient {
    fn with_body(body: String) -> Arc<Self> {
        Arc::new(Self {
            bodies: Mutex::new(vec![body]),
            seen_forms: Mutex::new(Vec::new()),
            fail: false,
        })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            bodies: Mutex::new(Vec::new()),
            seen_forms: Mutex::new(Vec::new()),
            fail: true,
        })
    }
}

impl OauthTokenClient for FakeOauthClient {
    fn post_token_form(
        &self,
        _token_endpoint: &str,
        form_body: &str,
    ) -> Result<String, RuntimeSlotError> {
        self.seen_forms.lock().unwrap().push(form_body.to_owned());
        if self.fail {
            return Err(RuntimeSlotError::OauthRefreshFailed);
        }
        let mut bodies = self.bodies.lock().unwrap();
        if bodies.is_empty() {
            return Err(RuntimeSlotError::OauthRefreshFailed);
        }
        Ok(bodies.remove(0))
    }
}

#[cfg(windows)]
#[test]
fn oauth_prepare_injects_only_access_token_never_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();

    let access = runtime_token("acc-", "ess-prepare-only");
    let refresh = runtime_token("ref-", "resh-must-not-leak");
    let created = service
        .create_slot(
            project.id,
            "OAuth slot".into(),
            CreateSlotAuth::Oauth {
                provider_id: "example-oidc".into(),
                issuer: "https://issuer.example.invalid".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://issuer.example.invalid/token".into(),
                account_label: Some("label".into()),
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "model-oauth".into(),
                access_token: SecretBytes::new(access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() + Duration::hours(2),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();
    assert_eq!(created.slot.governance(), RuntimeSlotGovernance::Full);
    assert!(created.secret_configured);
    assert!(matches!(created.slot.auth, SlotAuth::Oauth { .. }));

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    let (private_home, api_key) = match prepared {
        PreparedGrokSlot::ApiKey {
            private_home,
            api_key,
            model_id,
        } => {
            assert_eq!(model_id, "model-oauth");
            (private_home, api_key)
        }
        PreparedGrokSlot::Login { .. } => panic!("expected api-key shaped prepare for oauth"),
    };

    // Only access token is handed to the grok shell.
    assert_eq!(api_key.as_bytes(), access.as_bytes());
    assert_ne!(api_key.as_bytes(), refresh.as_bytes());

    let config = std::fs::read_to_string(private_home.join(".grok").join("config.toml")).unwrap();
    assert!(config.contains(GROK_API_KEY_ENV));
    assert!(!config.contains(&access));
    assert!(!config.contains(&refresh));

    // Slot DTO / Debug never carry token material — only refs.
    let auth_debug = format!("{:?}", created.slot.auth);
    assert!(!auth_debug.contains(&access));
    assert!(!auth_debug.contains(&refresh));
    assert!(auth_debug.contains("access_secret_ref") || auth_debug.contains("Oauth"));

    let secret_debug = format!("{:?}", SecretBytes::new(access.clone()));
    assert_eq!(secret_debug, "SecretBytes([REDACTED])");
    assert!(!secret_debug.contains(&access));
}

#[cfg(windows)]
#[test]
fn oauth_prepare_refreshes_when_expired_and_fails_closed_on_refresh_error() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");

    let old_access = runtime_token("acc-", "ess-old-expired");
    let refresh = runtime_token("ref-", "resh-for-refresh-path");
    let new_access = runtime_token("acc-", "ess-new-after-refresh");
    let new_refresh = runtime_token("ref-", "resh-rotated");

    let body = format!(
        r#"{{"access_token":"{new_access}","refresh_token":"{new_refresh}","expires_in":3600}}"#
    );
    let client = FakeOauthClient::with_body(body);
    let service = RuntimeSlotService::open_with_oauth_client(
        &db,
        &app_data,
        Some(client.clone() as Arc<dyn OauthTokenClient>),
    )
    .unwrap();

    let created = service
        .create_slot(
            project.id,
            "OAuth expired".into(),
            CreateSlotAuth::Oauth {
                provider_id: "example-oidc".into(),
                issuer: "https://issuer.example.invalid".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://issuer.example.invalid/token".into(),
                account_label: None,
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "model-oauth".into(),
                access_token: SecretBytes::new(old_access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                // Already inside the refresh skew window.
                access_expires_at: Utc::now() - Duration::minutes(1),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();

    let prepared = service.prepare_grok(project.id, created.slot.id).unwrap();
    match prepared {
        PreparedGrokSlot::ApiKey { api_key, .. } => {
            assert_eq!(api_key.as_bytes(), new_access.as_bytes());
            assert_ne!(api_key.as_bytes(), old_access.as_bytes());
            assert_ne!(api_key.as_bytes(), refresh.as_bytes());
            assert_ne!(api_key.as_bytes(), new_refresh.as_bytes());
        }
        PreparedGrokSlot::Login { .. } => panic!("expected access token prepare"),
    }

    // Refresh form captured by the fake client must include the refresh token,
    // but that material must never appear in the private grok config.
    let forms = client.seen_forms.lock().unwrap().clone();
    assert_eq!(forms.len(), 1);
    assert!(forms[0].contains("grant_type=refresh_token"));
    assert!(forms[0].contains(&refresh));
    let private_home = app_data
        .join("runtime-slots")
        .join(project.id.0.to_string())
        .join(created.slot.id.0.to_string());
    let config = std::fs::read_to_string(private_home.join(".grok").join("config.toml")).unwrap();
    assert!(!config.contains(&refresh));
    assert!(!config.contains(&new_refresh));
    assert!(!config.contains(&new_access));

    // Fail-closed: expired access + failing transport → no silent reuse of old access.
    let fail_db = temp.path().join("fail.db");
    let fail_app = temp.path().join("fail-app");
    let fail_project = seed_project(&fail_db, &temp, "beta");
    let fail_client = FakeOauthClient::failing();
    let fail_service = RuntimeSlotService::open_with_oauth_client(
        &fail_db,
        &fail_app,
        Some(fail_client as Arc<dyn OauthTokenClient>),
    )
    .unwrap();
    let expired = fail_service
        .create_slot(
            fail_project.id,
            "OAuth fail".into(),
            CreateSlotAuth::Oauth {
                provider_id: "example-oidc".into(),
                issuer: "https://issuer.example.invalid".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://issuer.example.invalid/token".into(),
                account_label: None,
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "model-oauth".into(),
                access_token: SecretBytes::new(old_access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() - Duration::hours(1),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();
    match fail_service.prepare_grok(fail_project.id, expired.slot.id) {
        Ok(_) => panic!("expired oauth must not prepare when refresh fails"),
        Err(err) => assert_eq!(err.code(), "runtime_slot_oauth_refresh_failed"),
    }

    // Fail-closed without any transport configured.
    let no_client = RuntimeSlotService::open(&fail_db, &fail_app).unwrap();
    match no_client.prepare_grok(fail_project.id, expired.slot.id) {
        Ok(_) => panic!("expired oauth must not prepare without refresh client"),
        Err(err) => assert_eq!(err.code(), "runtime_slot_oauth_refresh_failed"),
    }
}

#[test]
fn refresh_decision_helpers_exported_for_callers() {
    use std::time::{Duration as StdDuration, UNIX_EPOCH};
    let now = UNIX_EPOCH + StdDuration::from_secs(10_000);
    let skew = ACCESS_TOKEN_REFRESH_SKEW;
    assert!(access_token_needs_refresh(now, now, skew));
    assert!(!access_token_needs_refresh(
        now + skew + StdDuration::from_secs(5),
        now,
        skew
    ));
}

// --- T1 path selection + T2 governance + T3 credential wiring ---

#[test]
fn path_matrix_auth_x_provider() {
    use pillow_runtime_slots::{
        PathGovernance, PathIncompleteReason, SlotExecutionPath, resolve_execution_path,
    };

    // GrokLogin → GrokShell / Full
    let d = resolve_execution_path(&SlotAuth::GrokLogin { model_id: None }).unwrap();
    assert_eq!(d.path, SlotExecutionPath::GrokShell);
    assert_eq!(d.governance, PathGovernance::Full);

    // ApiKey → GrokShell / Full
    let d = resolve_execution_path(&SlotAuth::ApiKey {
        base_url: "https://api.example.invalid/v1".into(),
        model_id: "m".into(),
        secret_ref: "00000000-0000-0000-0000-000000000010".into(),
    })
    .unwrap();
    assert_eq!(d.path, SlotExecutionPath::GrokShell);
    assert_eq!(d.governance, PathGovernance::Full);

    // OAuth standard OIDC → GrokShell / Full
    let d = resolve_execution_path(&SlotAuth::Oauth {
        provider_id: "example-oidc".into(),
        issuer: "https://issuer.example.invalid".into(),
        client_id: "c".into(),
        scopes: vec!["openid".into()],
        token_endpoint: "https://issuer.example.invalid/token".into(),
        account_label: None,
        base_url: "https://api.example.invalid/v1".into(),
        model_id: "m".into(),
        access_secret_ref: "00000000-0000-0000-0000-000000000011".into(),
        refresh_secret_ref: "00000000-0000-0000-0000-000000000012".into(),
        access_expires_at: Utc::now() + Duration::hours(1),
    })
    .unwrap();
    assert_eq!(d.path, SlotExecutionPath::GrokShell);
    assert_eq!(d.governance, PathGovernance::Full);

    // OAuth chatgpt-codex (rewrite headers) → NativeAdapter / Full (host harness)
    let d = resolve_execution_path(&SlotAuth::Oauth {
        provider_id: "chatgpt-codex".into(),
        issuer: "https://auth.openai.com".into(),
        client_id: "c".into(),
        scopes: vec!["openid".into()],
        token_endpoint: "https://auth.openai.com/oauth/token".into(),
        account_label: Some("acct".into()),
        base_url: "https://api.example.invalid/v1".into(),
        model_id: "gpt-5.4".into(),
        access_secret_ref: "00000000-0000-0000-0000-000000000013".into(),
        refresh_secret_ref: "00000000-0000-0000-0000-000000000014".into(),
        access_expires_at: Utc::now() + Duration::hours(1),
    })
    .unwrap();
    assert_eq!(d.path, SlotExecutionPath::NativeAdapter);
    assert_eq!(d.governance, PathGovernance::Full);
    assert!(d.governance.may_be_controller());

    // LocalCli → NotRunnable / Incomplete (cannot be controller)
    let d = resolve_execution_path(&SlotAuth::LocalCli {
        executable_path: "C:\\tools\\x.exe".into(),
        launch_args: vec![],
        status_args: vec!["--version".into()],
    })
    .unwrap();
    assert_eq!(d.path, SlotExecutionPath::NotRunnable);
    assert!(!d.governance.may_be_controller());
    assert!(matches!(
        d.governance,
        PathGovernance::Incomplete {
            reason: PathIncompleteReason::LocalCliUngoverned
        }
    ));
}

#[cfg(windows)]
#[test]
fn prepare_grok_rejects_codex_rewrite_provider_not_stuffed_into_shell() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();

    let access = runtime_token("codex-acc-", "ess-no-shell");
    let refresh = runtime_token("codex-ref-", "resh-no-shell");
    let created = service
        .create_slot(
            project.id,
            "Codex OAuth".into(),
            CreateSlotAuth::Oauth {
                provider_id: "chatgpt-codex".into(),
                issuer: "https://auth.openai.com".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://auth.openai.com/oauth/token".into(),
                account_label: Some("acct-1".into()),
                // base_url present but cannot express rewrite shape
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "gpt-5.4".into(),
                access_token: SecretBytes::new(access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() + Duration::hours(2),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();

    // Path decision is NativeAdapter.
    let path = service.resolve_path(project.id, created.slot.id).unwrap();
    assert_eq!(path.path.as_str(), "native_adapter");
    assert!(path.governance.is_full());

    // prepare_grok must refuse (no silent grok shell stuffing).
    match service.prepare_grok(project.id, created.slot.id) {
        Ok(_) => panic!("codex must not prepare_grok into shell"),
        Err(err) => {
            assert_eq!(err.code(), "runtime_slot_path_not_grok_shell_expressible");
            let text = format!("{err}");
            assert!(!text.contains(&access));
            assert!(!text.contains(&refresh));
            assert!(
                text.contains("chatgpt-codex")
                    || text.contains("NativeAdapter")
                    || text.contains("not expressible")
                    || text.contains("not_grok")
                    || text.contains("base_url")
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn prepare_slot_native_injects_access_never_refresh_and_keeps_full_governance() {
    use pillow_runtime_slots::{
        NATIVE_ADAPTER_GOVERNANCE_MECHANISM, PreparedSlot, SlotExecutionPath,
    };

    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();

    let access = runtime_token("native-acc-", "ess-prepare-slot");
    let refresh = runtime_token("native-ref-", "resh-must-stay");
    let created = service
        .create_slot(
            project.id,
            "Native Codex".into(),
            CreateSlotAuth::Oauth {
                provider_id: "chatgpt-codex".into(),
                issuer: "https://auth.openai.com".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://auth.openai.com/oauth/token".into(),
                account_label: Some("acct-native".into()),
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "gpt-5.4".into(),
                access_token: SecretBytes::new(access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() + Duration::hours(2),
            },
            SlotRole::Controller,
        )
        .unwrap();
    assert_eq!(created.slot.governance(), RuntimeSlotGovernance::Full);
    assert!(matches!(created.slot.role, SlotRole::Controller));

    let prepared = service.prepare_slot(project.id, created.slot.id).unwrap();
    match prepared {
        PreparedSlot::GrokShell(_) => panic!("expected NativeAdapter package"),
        PreparedSlot::NativeAdapter(native) => {
            assert_eq!(native.path, SlotExecutionPath::NativeAdapter);
            assert!(native.governance.is_full());
            assert!(native.governance.may_be_controller());
            assert_eq!(
                native.governance_mechanism,
                NATIVE_ADAPTER_GOVERNANCE_MECHANISM
            );
            assert_eq!(native.provider_id, "chatgpt-codex");
            assert_eq!(native.model_id, "gpt-5.4");
            // Access injected into Authorization; refresh never present.
            assert_eq!(
                native.request.headers.get("Authorization"),
                Some(format!("Bearer {access}").as_str())
            );
            assert_eq!(
                native.request.headers.get("ChatGPT-Account-Id"),
                Some("acct-native")
            );
            // Codex endpoint — not the grok shell base_url chat completions path.
            assert!(native.request.url.contains("codex/responses"));

            let debug = format!("{native:?}");
            assert!(!debug.contains(&access), "access leaked: {debug}");
            assert!(!debug.contains(&refresh), "refresh leaked: {debug}");

            // Private grok home must NOT have been written for native path.
            let private_home = app_data
                .join("runtime-slots")
                .join(project.id.0.to_string())
                .join(created.slot.id.0.to_string());
            assert!(
                !private_home.join(".grok").join("config.toml").exists(),
                "native path must not write grok shell config"
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn prepare_slot_standard_oauth_still_uses_grok_shell() {
    use pillow_runtime_slots::PreparedSlot;

    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let app_data = temp.path().join("app-data");
    let project = seed_project(&db, &temp, "alpha");
    let service = RuntimeSlotService::open(&db, &app_data).unwrap();

    let access = runtime_token("std-acc-", "ess-shell");
    let refresh = runtime_token("std-ref-", "resh-shell");
    let created = service
        .create_slot(
            project.id,
            "Standard OAuth".into(),
            CreateSlotAuth::Oauth {
                provider_id: "example-oidc".into(),
                issuer: "https://issuer.example.invalid".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://issuer.example.invalid/token".into(),
                account_label: None,
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "model-oauth".into(),
                access_token: SecretBytes::new(access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() + Duration::hours(2),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();

    match service.prepare_slot(project.id, created.slot.id).unwrap() {
        PreparedSlot::NativeAdapter(_) => panic!("standard oauth must stay on grok shell"),
        PreparedSlot::GrokShell(PreparedGrokSlot::ApiKey { api_key, .. }) => {
            assert_eq!(api_key.as_bytes(), access.as_bytes());
            assert_ne!(api_key.as_bytes(), refresh.as_bytes());
        }
        PreparedSlot::GrokShell(PreparedGrokSlot::Login { .. }) => {
            panic!("expected api-key shaped oauth prepare")
        }
    }
}

#[test]
fn native_adapter_governance_mechanism_is_host_harness() {
    // T2 mechanism pin: NativeAdapter keeps tools/write/process on the host.
    assert_eq!(
        pillow_runtime_slots::NATIVE_ADAPTER_GOVERNANCE_MECHANISM,
        "host_harness_tools_write_process"
    );
    // Incomplete paths cannot be controller.
    use pillow_runtime_slots::{PathGovernance, PathIncompleteReason};
    let incomplete = PathGovernance::Incomplete {
        reason: PathIncompleteReason::LocalCliUngoverned,
    };
    assert!(!incomplete.may_be_controller());
    assert!(PathGovernance::Full.may_be_controller());
}
