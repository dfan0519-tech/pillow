use std::path::PathBuf;
use std::sync::Arc;

use keeper_storage_sqlite::{
    KEY_GROK_EXECUTABLE, SettingScope, SettingsUpdatedBy, SqliteSettingsRepository,
};
use pillow_runtime_slots::{PreparedGrokSlot, RuntimeSlotService};
use serde_json::Value;

use crate::app_state::AppState;
use crate::conversation_events::NoopEmitter;
use crate::runtime_composition::build_slot_grok_config;
use crate::runtime_service_state::{RuntimeServiceState, require_project_scope};
use crate::runtime_slot_commands::{
    create_external_cli_slot_impl, create_grok_api_key_slot_impl, create_grok_login_slot_impl,
    start_runtime_slot_impl,
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
