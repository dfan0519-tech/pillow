use std::path::PathBuf;
use std::sync::Arc;

use keeper_storage_sqlite::{
    KEY_GROK_EXECUTABLE, SettingScope, SettingsUpdatedBy, SqliteSettingsRepository,
};
use pillow_runtime_slots::{
    DpapiFileSecretStore, PreparedGrokSlot, RuntimeSlotService, SecretStore,
};
use serde_json::Value;

use crate::app_state::AppState;
use crate::conversation_events::NoopEmitter;
use crate::runtime_composition::build_slot_grok_config;
use crate::runtime_service_state::{RuntimeServiceState, require_project_scope};
use crate::runtime_slot_commands::{
    create_external_cli_slot_impl, create_grok_api_key_slot_impl, create_grok_login_slot_impl,
    delete_runtime_slot_impl, list_runtime_slots_impl, start_runtime_slot_impl,
    update_grok_api_key_slot_impl,
};

struct Harness {
    app: AppState,
    slots: RuntimeSlotService,
    project_id: String,
    database_path: PathBuf,
    _temp: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let db = temp.path().join("keeper.db");
        let app = AppState::open(&db).unwrap();
        app.register_picker_selection("main", &root).unwrap();
        let project = app
            .create_project_from_picker_for_window(
                "main",
                "Runtime slots".into(),
                root.to_string_lossy().into_owned(),
            )
            .unwrap();
        let slots = RuntimeSlotService::open(&db, temp.path().join("app-data")).unwrap();
        Self {
            app,
            slots,
            project_id: project.id,
            database_path: db,
            _temp: temp,
        }
    }
}

#[cfg(windows)]
#[test]
fn external_cli_dto_is_explicitly_ungoverned_and_cannot_start() {
    let harness = Harness::new();
    let dto = create_external_cli_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Status-only CLI".into(),
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        vec!["launch-is-not-enabled".into()],
        vec!["--help".into()],
    )
    .unwrap();
    assert_eq!(dto.kind, "external_cli");
    assert_eq!(dto.governance, "none");

    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    let error = start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        dto.slot_id,
    )
    .unwrap_err();
    assert_eq!(error.code, "external_cli_launch_unsupported");
}

#[cfg(windows)]
#[test]
fn governed_api_slot_command_activates_a_project_scoped_runtime() {
    let harness = Harness::new();
    let dto = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        "pillow-test-canary-not-a-real-key".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let executable = std::env::current_exe().unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &executable.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);

    let started = start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        dto.slot_id.clone(),
    )
    .unwrap();
    assert_eq!(started.slot_id, dto.slot_id);
    assert_eq!(started.project_id, harness.project_id);
    assert_eq!(started.kind, "governed_grok");
    assert_eq!(started.governance, "full");
    assert_eq!(started.status, "active");
    runtime
        .with_service(|_service, active_project_id| {
            assert_eq!(active_project_id, Some(started.project_id.as_str()));
            Ok(())
        })
        .unwrap();
    assert!(runtime.shutdown_if_active().unwrap());
    assert!(!runtime.shutdown_if_active().unwrap());
}

#[cfg(windows)]
#[test]
fn serialized_slot_dto_has_an_exact_allowlist_and_never_contains_the_key() {
    let harness = Harness::new();
    let marker = "pillow-test-canary-not-a-real-key";
    let dto = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        marker.into(),
    )
    .unwrap();

    let value = serde_json::to_value(dto).unwrap();
    let object = value.as_object().unwrap();
    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "authMode",
            "baseUrl",
            "createdAt",
            "displayName",
            "executablePath",
            "governance",
            "kind",
            "launchArgs",
            "modelId",
            "projectId",
            "secretConfigured",
            "secretRef",
            "slotId",
            "statusArgs",
            "updatedAt",
        ]
    );
    let serialized = value.to_string();
    assert!(!serialized.contains(marker));
    assert!(object.get("secretRef").and_then(Value::as_str).is_some());
    assert_eq!(object.get("secretConfigured"), Some(&Value::Bool(true)));
    assert_eq!(
        object.get("governance").and_then(Value::as_str),
        Some("full")
    );
    assert!(object.get("apiKey").is_none());
}

#[test]
fn prepared_login_slot_keeps_home_unredirected() {
    let harness = Harness::new();
    let dto = create_grok_login_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Signed-in Grok".into(),
    )
    .unwrap();
    let prepared = harness
        .slots
        .prepare_grok(
            uuid::Uuid::parse_str(&harness.project_id)
                .map(keeper_project::ProjectId)
                .unwrap(),
            uuid::Uuid::parse_str(&dto.slot_id)
                .map(keeper_storage_sqlite::RuntimeSlotId)
                .unwrap(),
        )
        .unwrap();
    assert!(matches!(prepared, PreparedGrokSlot::Login));

    let config =
        build_slot_grok_config(PathBuf::from(std::env::current_exe().unwrap()), prepared).unwrap();
    assert_eq!(config.honor_isolated_home, None);
    assert_eq!(config.launch_argv(), vec!["agent", "--no-leader", "stdio"]);
}

#[cfg(windows)]
#[test]
fn prepared_api_slot_maps_to_model_private_home_and_redacted_launch_secret() {
    let harness = Harness::new();
    let marker = "pillow-test-canary-not-a-real-key";
    let dto = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        marker.into(),
    )
    .unwrap();
    let prepared = harness
        .slots
        .prepare_grok(
            uuid::Uuid::parse_str(&harness.project_id)
                .map(keeper_project::ProjectId)
                .unwrap(),
            uuid::Uuid::parse_str(&dto.slot_id)
                .map(keeper_storage_sqlite::RuntimeSlotId)
                .unwrap(),
        )
        .unwrap();
    let config =
        build_slot_grok_config(PathBuf::from(std::env::current_exe().unwrap()), prepared).unwrap();
    assert!(config.honor_isolated_home.is_some());
    assert_eq!(
        config.launch_argv(),
        vec!["-m", "model-one", "agent", "--no-leader", "stdio"]
    );
    assert!(!format!("{config:?}").contains(marker));
    assert_eq!(
        keeper_adapter_grok_acp::GROK_API_KEY_ENV,
        pillow_runtime_slots::GROK_API_KEY_ENV
    );
}

#[test]
fn desktop_commands_use_replaceable_runtime_state_and_register_slot_surface() {
    for source in [
        include_str!("conversation_commands.rs"),
        include_str!("exec_commands.rs"),
        include_str!("write_commands.rs"),
    ] {
        assert!(!source.contains("State<'_, Arc<ConversationService>>"));
        assert!(source.contains("RuntimeServiceState"));
    }

    let handler = include_str!("lib.rs")
        .split("tauri::generate_handler![")
        .nth(1)
        .unwrap();
    for command in [
        "list_runtime_slots",
        "create_grok_login_slot",
        "create_grok_api_key_slot",
        "create_external_cli_slot",
        "get_external_cli_status",
        "start_runtime_slot",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}

#[test]
fn an_active_slot_is_not_reused_across_project_boundaries() {
    assert!(require_project_scope(None, "project-b").is_ok());
    assert!(require_project_scope(Some("project-a"), "project-a").is_ok());
    let error = require_project_scope(Some("project-a"), "project-b").unwrap_err();
    assert_eq!(error.code, "runtime_slot_project_mismatch");
}

#[cfg(windows)]
#[test]
fn update_omitting_api_key_keeps_secret_usable_for_prepare_and_start() {
    let harness = Harness::new();
    let original = "pillow-test-canary-keep-via-command";
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        original.into(),
    )
    .unwrap();
    let original_ref = created.secret_ref.clone().expect("secret ref");

    let updated = update_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        created.slot_id.clone(),
        "Renamed".into(),
        "https://example.invalid/v2".into(),
        "model-two".into(),
        None,
    )
    .unwrap();
    assert_eq!(updated.display_name, "Renamed");
    assert_eq!(
        updated.base_url.as_deref(),
        Some("https://example.invalid/v2")
    );
    assert_eq!(updated.model_id.as_deref(), Some("model-two"));
    assert_eq!(updated.secret_ref.as_deref(), Some(original_ref.as_str()));
    assert!(updated.secret_configured);
    let serialized = serde_json::to_string(&updated).unwrap();
    assert!(!serialized.contains(original));

    // Prove retained key still decrypts and prepares — not just Ok(()).
    let store = DpapiFileSecretStore::new(harness._temp.path().join("app-data").join("secrets"));
    let recovered = store.get(&original_ref).unwrap();
    assert_eq!(recovered.as_bytes(), original.as_bytes());

    let prepared = harness
        .slots
        .prepare_grok(
            uuid::Uuid::parse_str(&harness.project_id)
                .map(keeper_project::ProjectId)
                .unwrap(),
            uuid::Uuid::parse_str(&created.slot_id)
                .map(keeper_storage_sqlite::RuntimeSlotId)
                .unwrap(),
        )
        .unwrap();
    match prepared {
        PreparedGrokSlot::ApiKey {
            model_id, api_key, ..
        } => {
            assert_eq!(model_id, "model-two");
            assert_eq!(api_key.as_bytes(), original.as_bytes());
        }
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    }

    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let executable = std::env::current_exe().unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &executable.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    let started = start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();
    assert_eq!(started.status, "active");
    assert!(runtime.shutdown_if_active().unwrap());
}

#[cfg(windows)]
#[test]
fn update_with_new_api_key_rotates_secret_and_old_ref_is_unrecoverable() {
    let harness = Harness::new();
    let original = "pillow-test-canary-command-original";
    let rotated = "pillow-test-canary-command-rotated";
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        original.into(),
    )
    .unwrap();
    let original_ref = created.secret_ref.clone().expect("secret ref");
    let secrets_dir = harness._temp.path().join("app-data").join("secrets");
    let original_path = secrets_dir.join(format!("{original_ref}.dpapi"));
    assert!(original_path.is_file());

    let updated = update_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        created.slot_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        Some(rotated.into()),
    )
    .unwrap();
    let new_ref = updated.secret_ref.clone().expect("new secret ref");
    assert_ne!(new_ref, original_ref);
    assert!(!original_path.exists());
    let serialized = serde_json::to_string(&updated).unwrap();
    assert!(!serialized.contains(original));
    assert!(!serialized.contains(rotated));

    let store = DpapiFileSecretStore::new(secrets_dir);
    assert!(matches!(
        store.get(&original_ref),
        Err(pillow_runtime_slots::RuntimeSlotError::SecretMissing)
    ));
    let new_value = store.get(&new_ref).unwrap();
    assert_eq!(new_value.as_bytes(), rotated.as_bytes());

    let prepared = harness
        .slots
        .prepare_grok(
            uuid::Uuid::parse_str(&harness.project_id)
                .map(keeper_project::ProjectId)
                .unwrap(),
            uuid::Uuid::parse_str(&created.slot_id)
                .map(keeper_storage_sqlite::RuntimeSlotId)
                .unwrap(),
        )
        .unwrap();
    match prepared {
        PreparedGrokSlot::ApiKey { api_key, .. } => {
            assert_eq!(api_key.as_bytes(), rotated.as_bytes());
        }
        PreparedGrokSlot::Login => panic!("expected api-key preparation"),
    }
}

#[cfg(windows)]
#[test]
fn delete_command_removes_slot_and_destroys_secret() {
    let harness = Harness::new();
    let marker = "pillow-test-canary-command-delete";
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        "https://example.invalid/v1".into(),
        "model-one".into(),
        marker.into(),
    )
    .unwrap();
    let secret_ref = created.secret_ref.clone().expect("secret ref");
    let secrets_dir = harness._temp.path().join("app-data").join("secrets");
    let secret_path = secrets_dir.join(format!("{secret_ref}.dpapi"));
    assert!(secret_path.is_file());

    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let executable = std::env::current_exe().unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &executable.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    let started = start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();
    assert_eq!(started.status, "active");

    delete_runtime_slot_impl(
        &harness.slots,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();

    // Active runtime bound to the deleted slot must have been stopped.
    assert!(!runtime.shutdown_if_active().unwrap());

    let listed =
        list_runtime_slots_impl(&harness.slots, &harness.app, harness.project_id.clone()).unwrap();
    assert!(listed.is_empty());
    assert!(!secret_path.exists());
    let store = DpapiFileSecretStore::new(secrets_dir);
    assert!(matches!(
        store.get(&secret_ref),
        Err(pillow_runtime_slots::RuntimeSlotError::SecretMissing)
    ));
}

#[test]
fn update_and_delete_command_symbols_exist_for_host_registration() {
    // lib.rs registration is owned by the controller merge; assert the command
    // surface is present in this module so the merge can wire generate_handler.
    let source = include_str!("runtime_slot_commands.rs");
    assert!(source.contains("pub fn update_grok_api_key_slot"));
    assert!(source.contains("pub fn delete_runtime_slot"));
}
