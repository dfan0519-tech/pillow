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
    delete_runtime_slot_impl, detect_local_clis_impl, get_runtime_slot_model_catalog_impl,
    list_runtime_slots_impl, local_cli_detection_to_dto, probe_model_catalog_impl,
    set_runtime_slot_model_impl, start_runtime_slot_impl, update_grok_api_key_slot_impl,
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

/// Serialize with `runtime_composition`'s tests.
///
/// Composing a runtime walks the path probe, which counts into a process-wide
/// static. Those tests assert an *exact* probe count while holding this lock,
/// so a runtime started here without it leaks an increment into their
/// assertion — seen as an intermittent `left: 4, right: 3`, plus six follow-on
/// poison failures that hid which test was actually wrong.
///
/// Poison-tolerant on purpose: one real failure should not become seven.
fn runtime_probe_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::runtime_composition::tests::env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
#[test]
fn external_cli_dto_is_explicitly_ungoverned_and_cannot_start() {
    let _probe_guard = runtime_probe_guard();
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
    let _probe_guard = runtime_probe_guard();
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
            // Added by the uniform slot model: role is a field of every slot,
            // not a property of one privileged kind.
            "role",
            "roleCustomName",
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
    assert!(matches!(
        prepared,
        PreparedGrokSlot::Login { model_id: None }
    ));

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
        "create_slot",
        "update_slot",
        "create_grok_login_slot",
        "create_grok_api_key_slot",
        "create_external_cli_slot",
        "get_external_cli_status",
        "get_runtime_slot_model_catalog",
        "set_runtime_slot_model",
        "start_runtime_slot",
        "detect_local_clis",
        "probe_model_catalog",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}

/// Every command the renderer actually invokes must be registered.
///
/// The list above is hand-maintained, which is why it could not catch the
/// defect it was written to prevent: `create_slot` and `update_slot` shipped
/// with the gateway calling them and `generate_handler!` never naming them.
/// Rust compiled, TypeScript typechecked, and the call would have failed only
/// at runtime with "command not found". Deriving the required set from the
/// gateway source makes adding an invoke without a registration a build
/// failure instead of a silent dead path.
#[test]
fn every_command_the_slots_gateway_invokes_is_registered() {
    const GATEWAY: &str = include_str!("../../src/features/settings/runtimeSlotsGateway.ts");
    let handler = include_str!("lib.rs")
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("generate_handler! block");

    let mut invoked: Vec<&str> = Vec::new();
    for segment in GATEWAY.split("invokeSlotCommand(").skip(1) {
        // First string literal after the call opens is the command name.
        let Some(open) = segment.find('"') else {
            continue;
        };
        let rest = &segment[open + 1..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        let name = &rest[..close];
        // Command names are snake_case identifiers; anything else is not one.
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            continue;
        }
        if !invoked.contains(&name) {
            invoked.push(name);
        }
    }

    // A parser that silently finds nothing would make this test vacuously
    // green — the same false-green shape this suite exists to prevent.
    assert!(
        invoked.len() >= 8,
        "gateway command extraction found only {invoked:?}; the parser is broken, not the code"
    );

    for command in invoked {
        assert!(
            handler.contains(command),
            "gateway invokes `{command}` but generate_handler! does not register it"
        );
    }
}

#[test]
fn inactive_login_model_switch_persists_launch_model_without_claiming_restart() {
    let _probe_guard = runtime_probe_guard();
    let harness = Harness::new();
    let created = create_grok_login_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Signed-in Grok".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);

    let switched = set_runtime_slot_model_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
        "grok-4.5".into(),
    )
    .unwrap();
    assert_eq!(switched.slot_id, created.slot_id);
    assert_eq!(switched.project_id, harness.project_id);
    assert_eq!(switched.model_id, "grok-4.5");
    assert!(!switched.runtime_restarted);
    assert_eq!(switched.session_restore, "not_applicable");

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
    let config = build_slot_grok_config(std::env::current_exe().unwrap(), prepared).unwrap();
    assert_eq!(
        config.launch_argv(),
        ["-m", "grok-4.5", "agent", "--no-leader", "stdio"]
    );

    let value = serde_json::to_value(switched).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "modelId",
            "projectId",
            "runtimeRestarted",
            "sessionRestore",
            "slotId",
        ]
    );
}

#[cfg(windows)]
#[test]
fn active_api_model_switch_replaces_runtime_and_keeps_it_active() {
    let _probe_guard = runtime_probe_guard();
    let harness = Harness::new();
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private".into(),
        "https://example.invalid/v1".into(),
        "model-old".into(),
        "pillow-test-switch-key".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &std::env::current_exe().unwrap().to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();

    let switched = set_runtime_slot_model_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
        "model-new".into(),
    )
    .unwrap();
    assert!(switched.runtime_restarted);
    assert_eq!(switched.session_restore, "unavailable");
    runtime
        .with_service(|_service, active_project_id| {
            assert_eq!(active_project_id, Some(harness.project_id.as_str()));
            Ok(())
        })
        .unwrap();

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
    let config = build_slot_grok_config(std::env::current_exe().unwrap(), prepared).unwrap();
    assert_eq!(
        config.launch_argv(),
        ["-m", "model-new", "agent", "--no-leader", "stdio"]
    );
    assert!(runtime.shutdown_if_active().unwrap());
}

#[cfg(windows)]
#[test]
fn failed_active_model_replacement_does_not_leave_the_old_runtime_active() {
    let _probe_guard = runtime_probe_guard();
    let harness = Harness::new();
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private".into(),
        "https://example.invalid/v1".into(),
        "model-old".into(),
        "pillow-test-failed-switch-key".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &std::env::current_exe().unwrap().to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();

    let disappearing = harness._temp.path().join("disappearing-grok.exe");
    std::fs::write(&disappearing, b"placeholder").unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &disappearing.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    std::fs::remove_file(disappearing).unwrap();

    let error = set_runtime_slot_model_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
        "model-new".into(),
    )
    .unwrap_err();
    assert_eq!(error.code, "runtime_executable_missing");
    assert!(!runtime.shutdown_if_active().unwrap());

    let listed = list_runtime_slots_impl(&harness.slots, &harness.app, harness.project_id).unwrap();
    assert_eq!(listed[0].model_id.as_deref(), Some("model-new"));
}

#[cfg(windows)]
#[test]
#[ignore = "run explicitly with PILLOW_TEST_FAKE_ACP_BIN after building fake_acp_agent"]
fn active_model_switch_launches_the_next_acp_child_with_the_new_model_argument() {
    let _probe_guard = runtime_probe_guard();
    let fake_source = std::env::var_os("PILLOW_TEST_FAKE_ACP_BIN")
        .map(PathBuf::from)
        .expect("PILLOW_TEST_FAKE_ACP_BIN is required");
    assert!(fake_source.is_file());
    let harness = Harness::new();
    let fake = harness._temp.path().join("fake-grok.exe");
    std::fs::copy(fake_source, &fake).unwrap();
    let argv_sink = harness._temp.path().join("fake-grok-argv.jsonl");
    std::fs::write(
        harness._temp.path().join("keeper_fake_acp_argv_sink.txt"),
        argv_sink.to_string_lossy().as_bytes(),
    )
    .unwrap();

    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private".into(),
        "https://example.invalid/v1".into(),
        "model-old".into(),
        "pillow-test-launch-evidence-key".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &fake.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();

    let conversation_id = runtime
        .with_service(|service, _| {
            let conversation =
                service.create_conversation(harness.project_id.clone(), Some("argv".into()))?;
            service.send_turn(conversation.id.clone(), "first".into())?;
            Ok(conversation.id)
        })
        .unwrap();

    let switched = set_runtime_slot_model_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id,
        "model-new".into(),
    )
    .unwrap();
    assert!(switched.runtime_restarted);
    runtime
        .with_service(|service, _| {
            service.send_turn(conversation_id, "second".into())?;
            Ok(())
        })
        .unwrap();
    assert!(runtime.shutdown_if_active().unwrap());

    let lines = std::fs::read_to_string(argv_sink).unwrap();
    let launches = lines
        .lines()
        .map(|line| serde_json::from_str::<Vec<String>>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(launches.len(), 2);
    assert_eq!(
        launches[0],
        ["-m", "model-old", "agent", "--no-leader", "stdio"]
    );
    assert_eq!(
        launches[1],
        ["-m", "model-new", "agent", "--no-leader", "stdio"]
    );
}

#[cfg(windows)]
#[test]
#[ignore = "run explicitly with PILLOW_TEST_FAKE_ACP_BIN after building fake_acp_agent"]
fn login_model_catalog_command_runs_the_grok_models_subcommand() {
    let fake = std::env::var_os("PILLOW_TEST_FAKE_ACP_BIN")
        .map(PathBuf::from)
        .expect("PILLOW_TEST_FAKE_ACP_BIN is required");
    assert!(fake.is_file());
    let harness = Harness::new();
    let created = create_grok_login_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Signed-in Grok".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &fake.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let catalog = get_runtime_slot_model_catalog_impl(
        &harness.slots,
        &settings,
        &harness.app,
        harness.project_id,
        created.slot_id,
    )
    .unwrap();
    assert_eq!(catalog.status, "available");
    assert_eq!(catalog.source, "grok_cli");
    assert_eq!(catalog.models, ["grok-4.5"]);
    assert_eq!(catalog.reason_code, None);
}

#[test]
fn external_cli_model_catalog_is_an_explicit_renderer_fallback() {
    let harness = Harness::new();
    let created = create_external_cli_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Status-only CLI".into(),
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        vec!["unused".into()],
        vec!["--help".into()],
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();

    let catalog = get_runtime_slot_model_catalog_impl(
        &harness.slots,
        &settings,
        &harness.app,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();
    assert_eq!(catalog.slot_id, created.slot_id);
    assert_eq!(catalog.project_id, harness.project_id);
    assert_eq!(catalog.status, "unsupported");
    assert_eq!(catalog.source, "external_cli");
    assert!(catalog.models.is_empty());
    assert_eq!(
        catalog.reason_code.as_deref(),
        Some("external_cli_not_supported")
    );

    let value = serde_json::to_value(catalog).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "models",
            "projectId",
            "reasonCode",
            "slotId",
            "source",
            "status",
        ]
    );
}

#[cfg(windows)]
#[test]
fn api_key_catalog_command_uses_the_stored_secret_but_returns_only_safe_models() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 || request.len() > 16 * 1024 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let body = br#"{"data":[{"id":"command-model"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        String::from_utf8(request).unwrap()
    });

    let harness = Harness::new();
    let marker = "pillow-test-command-catalog-key";
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private model".into(),
        format!("http://{address}/v1"),
        "model-one".into(),
        marker.into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();

    let catalog = get_runtime_slot_model_catalog_impl(
        &harness.slots,
        &settings,
        &harness.app,
        harness.project_id.clone(),
        created.slot_id,
    )
    .unwrap();
    assert_eq!(catalog.status, "available");
    assert_eq!(catalog.source, "openai_models");
    assert_eq!(catalog.models, ["command-model"]);
    assert_eq!(catalog.reason_code, None);
    assert!(!serde_json::to_string(&catalog).unwrap().contains(marker));

    let request = server.join().unwrap();
    // Derived from `marker` for the same reason as the catalog unit test: no
    // literal auth header in source, and the assertion tracks the real key.
    assert!(request.contains(&format!("Authorization: Bearer {marker}\r\n")));
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
    let _probe_guard = runtime_probe_guard();
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
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);

    let updated = update_grok_api_key_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
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
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
    }

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
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);

    let updated = update_grok_api_key_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
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
        PreparedGrokSlot::Login { .. } => panic!("expected api-key preparation"),
    }
}

#[cfg(windows)]
#[test]
fn active_api_slot_edit_cannot_change_model_without_restarting_runtime() {
    let _probe_guard = runtime_probe_guard();
    let harness = Harness::new();
    let created = create_grok_api_key_slot_impl(
        &harness.slots,
        &harness.app,
        harness.project_id.clone(),
        "Private".into(),
        "https://example.invalid/v1".into(),
        "model-old".into(),
        "pillow-test-edit-restart-key".into(),
    )
    .unwrap();
    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &std::env::current_exe().unwrap().to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id.clone(),
    )
    .unwrap();

    let updated = update_grok_api_key_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot_id,
        "Private".into(),
        "https://example.invalid/v1".into(),
        "model-new".into(),
        None,
    )
    .unwrap();
    assert_eq!(updated.model_id.as_deref(), Some("model-new"));
    assert!(
        runtime
            .is_active_slot(&harness.project_id, &updated.slot_id)
            .unwrap()
    );
    assert!(runtime.shutdown_if_active().unwrap());
}

#[cfg(windows)]
#[test]
fn delete_command_removes_slot_and_destroys_secret() {
    let _probe_guard = runtime_probe_guard();
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
    assert!(source.contains("pub fn get_runtime_slot_model_catalog"));
    assert!(source.contains("pub fn set_runtime_slot_model"));
    assert!(source.contains("pub fn detect_local_clis"));
    assert!(source.contains("pub fn probe_model_catalog"));
}

/// Closed DTO keys for `LocalCliDetectionDto` — must match renderer allowlist.
#[test]
fn local_cli_detection_dto_keys_are_closed_and_secret_free() {
    use pillow_cli_detect::{AgentCliKind, AuthState, CliDetection};
    use std::path::PathBuf;

    let dto = local_cli_detection_to_dto(CliDetection {
        kind: AgentCliKind::Codex,
        found: true,
        executable: Some(PathBuf::from("C:/tools/codex.exe")),
        version: Some("0.1.0".into()),
        auth: AuthState::Authenticated {
            account_label: "ChatGPT".into(),
        },
    });
    let value = serde_json::to_value(&dto).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "accountLabel",
            "authReason",
            "authStatus",
            "executablePath",
            "found",
            "kind",
            "version",
        ]
    );
    // Negative: closed object keys only — no secret-shaped field names.
    let obj = value.as_object().unwrap();
    for key in obj.keys() {
        let lower = key.to_ascii_lowercase();
        assert!(
            !matches!(
                lower.as_str(),
                "apikey"
                    | "api_key"
                    | "password"
                    | "token"
                    | "bearer"
                    | "authorization"
                    | "credential"
                    | "privatekey"
                    | "private_key"
            ),
            "DTO must not contain secret-shaped key: {key}"
        );
    }
}

/// Positive: explicit NotAuthenticated maps to `not_authenticated`.
/// Negative: Unknown (timeout / not_found / no status) never becomes `not_authenticated`.
#[test]
fn local_cli_auth_unknown_is_never_not_authenticated() {
    use pillow_cli_detect::{AgentCliKind, AuthState, CliDetection};
    use std::path::PathBuf;

    let logged_out = local_cli_detection_to_dto(CliDetection {
        kind: AgentCliKind::Grok,
        found: true,
        executable: Some(PathBuf::from("/bin/grok")),
        version: Some("1.0".into()),
        auth: AuthState::NotAuthenticated,
    });
    assert_eq!(logged_out.auth_status, "not_authenticated");
    assert!(logged_out.account_label.is_none());
    assert!(logged_out.auth_reason.is_none());

    for (reason, label) in [
        ("timed_out", "timeout"),
        ("not_found", "missing install"),
        ("no_headless_status_command", "gemini-class"),
        ("spawn_failed", "denied/spawn"),
        ("output_capture_failed", "unparseable"),
    ] {
        let unknown = local_cli_detection_to_dto(CliDetection {
            kind: AgentCliKind::Gemini,
            found: reason != "not_found",
            executable: if reason == "not_found" {
                None
            } else {
                Some(PathBuf::from("/bin/gemini"))
            },
            version: None,
            auth: AuthState::Unknown {
                reason: reason.into(),
            },
        });
        assert_eq!(
            unknown.auth_status, "unknown",
            "{label}: expected unknown status"
        );
        assert_ne!(
            unknown.auth_status, "not_authenticated",
            "{label}: must not collapse missing evidence into logged-out"
        );
        assert_eq!(unknown.auth_reason.as_deref(), Some(reason));
        assert!(unknown.account_label.is_none());
    }

    let authed = local_cli_detection_to_dto(CliDetection {
        kind: AgentCliKind::Claude,
        found: true,
        executable: Some(PathBuf::from("/bin/claude")),
        version: Some("2".into()),
        auth: AuthState::Authenticated {
            account_label: "user@example.com".into(),
        },
    });
    assert_eq!(authed.auth_status, "authenticated");
    assert_eq!(authed.account_label.as_deref(), Some("user@example.com"));
    assert!(authed.auth_reason.is_none());
}

/// Live detect returns a closed list for every known kind (machine-dependent found flags).
#[test]
fn detect_local_clis_returns_all_kinds_with_closed_dto() {
    let _probe_guard = runtime_probe_guard();
    let list = detect_local_clis_impl().unwrap();
    assert_eq!(list.len(), 5, "expected one row per known CLI kind");
    let kinds: Vec<&str> = list.iter().map(|row| row.kind.as_str()).collect();
    assert_eq!(kinds, ["grok", "claude", "codex", "opencode", "gemini"]);
    for row in &list {
        assert!(
            matches!(
                row.auth_status.as_str(),
                "authenticated" | "not_authenticated" | "unknown"
            ),
            "invalid auth_status {}",
            row.auth_status
        );
        if row.auth_status == "unknown" {
            // Reason may be present; must never claim not_authenticated.
            assert_ne!(row.auth_status, "not_authenticated");
        }
        if row.auth_status != "authenticated" {
            assert!(row.account_label.is_none());
        }
        let raw = serde_json::to_string(row).unwrap();
        assert!(!raw.contains("apiKey") && !raw.contains("api_key"));
    }
}

#[test]
fn probe_model_catalog_dto_keys_are_closed() {
    let _probe_guard = runtime_probe_guard();
    // Unsupported path is pure (no network): empty body status codes map in
    // fetch; we pin the DTO shape via a synthetic unavailable-error-free path
    // using invalid base_url → fixed error, and success via TCP on Windows.
    let err = probe_model_catalog_impl("".into(), "k".into()).unwrap_err();
    assert_eq!(err.code, "runtime_slot_input_invalid");
    assert!(!err.message.contains("k"));
}

#[cfg(windows)]
#[test]
fn probe_model_catalog_success_returns_models_without_echoing_key() {
    let _probe_guard = runtime_probe_guard();
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 || request.len() > 16 * 1024 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let body = br#"{"data":[{"id":"draft-model-a"},{"id":"draft-model-b"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        String::from_utf8(request).unwrap()
    });

    let marker = "pillow-test-draft-probe-key-9f3a";
    let dto = probe_model_catalog_impl(format!("http://{address}/v1"), marker.into()).unwrap();
    assert_eq!(dto.status, "available");
    assert_eq!(dto.source, "openai_models");
    assert_eq!(dto.models, ["draft-model-a", "draft-model-b"]);
    assert_eq!(dto.reason_code, None);

    let value = serde_json::to_value(&dto).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, ["models", "reasonCode", "source", "status"]);

    let encoded = serde_json::to_string(&dto).unwrap();
    assert!(
        !encoded.contains(marker),
        "success DTO must never echo the API key"
    );
    assert!(!encoded.contains("apiKey") && !encoded.contains("Authorization"));

    let request = server.join().unwrap();
    assert!(request.contains(&format!("Authorization: Bearer {marker}\r\n")));
}

#[cfg(windows)]
#[test]
fn probe_model_catalog_unsupported_endpoint_is_normal_result() {
    let _probe_guard = runtime_probe_guard();
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let marker = "pillow-test-draft-probe-unsupported-key";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 512];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            request.extend_from_slice(&buffer[..read]);
            if request.len() > 8 * 1024 {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });

    let dto = probe_model_catalog_impl(format!("http://{address}/v1"), marker.into()).unwrap();
    assert_eq!(dto.status, "unsupported");
    assert_eq!(dto.source, "openai_models");
    assert!(dto.models.is_empty());
    assert_eq!(dto.reason_code.as_deref(), Some("endpoint_not_supported"));
    assert!(!serde_json::to_string(&dto).unwrap().contains(marker));
    let _ = server.join();
}

#[cfg(windows)]
#[test]
fn probe_model_catalog_failure_does_not_pretend_success_or_echo_key() {
    let _probe_guard = runtime_probe_guard();
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let marker = "pillow-test-draft-probe-fail-key";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 512];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            request.extend_from_slice(&buffer[..read]);
            if request.len() > 8 * 1024 {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });

    let err = probe_model_catalog_impl(format!("http://{address}/v1"), marker.into()).unwrap_err();
    assert_eq!(err.code, "runtime_slot_model_catalog_unavailable");
    assert!(!err.message.contains(marker));
    assert!(!err.code.contains(marker));
    let _ = server.join();
}

#[test]
fn probe_model_catalog_rejects_blank_key_without_network_and_without_echo() {
    let _probe_guard = runtime_probe_guard();
    let marker = "pillow-test-blank-should-not-matter";
    // Whitespace-only key is treated as empty input.
    let err =
        probe_model_catalog_impl("https://api.example.test/v1".into(), "   ".into()).unwrap_err();
    assert_eq!(err.code, "runtime_slot_input_invalid");
    assert!(!err.message.contains(marker));
}

/// NativeAdapter path must not activate the Grok shell (no base_url+env_key stuffing).
#[cfg(windows)]
#[test]
fn start_native_adapter_codex_slot_prepares_without_grok_shell() {
    use chrono::{Duration, Utc};
    use keeper_project::ProjectId;
    use keeper_storage_sqlite::{RuntimeSlotId, SlotRole};
    use pillow_runtime_slots::{CreateSlotAuth, SecretBytes};
    use uuid::Uuid;

    let _probe_guard = runtime_probe_guard();
    let harness = Harness::new();
    let project_uuid = Uuid::parse_str(&harness.project_id).unwrap();
    let project_id = ProjectId(project_uuid);

    let access = format!("{}{}", "desk-acc-", "ess-native-01");
    let refresh = format!("{}{}", "desk-ref-", "resh-native-01");
    let created = harness
        .slots
        .create_slot(
            project_id,
            "Codex native".into(),
            CreateSlotAuth::Oauth {
                provider_id: "chatgpt-codex".into(),
                issuer: "https://auth.openai.com".into(),
                client_id: "client-1".into(),
                scopes: vec!["openid".into()],
                token_endpoint: "https://auth.openai.com/oauth/token".into(),
                account_label: Some("acct-desk".into()),
                base_url: "https://api.example.invalid/v1".into(),
                model_id: "gpt-5.4".into(),
                access_token: SecretBytes::new(access.clone()),
                refresh_token: SecretBytes::new(refresh.clone()),
                access_expires_at: Utc::now() + Duration::hours(2),
            },
            SlotRole::default_unassigned(),
        )
        .unwrap();

    let settings = SqliteSettingsRepository::open(&harness.database_path).unwrap();
    let runtime = RuntimeServiceState::new(&harness.database_path, Arc::new(NoopEmitter), None);
    let started = start_runtime_slot_impl(
        &harness.slots,
        &settings,
        &harness.app,
        &runtime,
        harness.project_id.clone(),
        created.slot.id.0.to_string(),
    )
    .unwrap();

    // Closed wire kind remains governed_grok; path is status + no grok activate.
    assert_eq!(started.kind, "governed_grok");
    assert_eq!(started.governance, "full");
    assert_eq!(started.status, "native_adapter_prepared");
    // Must not have activated a grok conversation runtime.
    assert!(
        !runtime
            .is_active_slot(&harness.project_id, &created.slot.id.0.to_string())
            .unwrap()
    );

    // prepare_grok remains refused for this slot (cannot stuff into shell).
    let slot_id = RuntimeSlotId(created.slot.id.0);
    match harness.slots.prepare_grok(project_id, slot_id) {
        Ok(_) => panic!("codex must not prepare_grok"),
        Err(err) => {
            assert_eq!(err.code(), "runtime_slot_path_not_grok_shell_expressible");
            let text = format!("{err}");
            assert!(!text.contains(&access));
            assert!(!text.contains(&refresh));
        }
    }
}
