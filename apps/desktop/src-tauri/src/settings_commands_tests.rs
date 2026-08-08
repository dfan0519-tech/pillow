//! Focused tests for S0 settings Tauri command surface.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use keeper_storage_sqlite::{
    ENV_GROK_EXECUTABLE, ENV_RUNTIME_ADAPTER, KEY_GROK_EXECUTABLE, RuntimeAdapterSetting,
    SettingScope, SettingSource, SettingsUpdatedBy, SqliteSettingsRepository,
};
use serde_json::Value;

use crate::app_state::AppState;
use crate::runtime_composition::tests::env_lock;
use crate::settings_commands::{RuntimeSettingsDto, runtime_settings_dto_field_names, to_dto};

fn temp_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "pillow-settings-cmd-{label}-{nanos}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp root");
    dir
}

struct Harness {
    _temp: PathBuf,
    app: AppState,
    settings: SqliteSettingsRepository,
    project_id: String,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self._temp);
    }
}

impl Harness {
    fn new(label: &str) -> Self {
        let temp = temp_root(label);
        let project_root = temp.join("project");
        fs::create_dir_all(&project_root).expect("project root");
        let db = temp.join("keeper.db");
        let app = AppState::open(&db).expect("app state");
        app.register_picker_selection("main", &project_root)
            .expect("picker");
        let project = app
            .create_project_from_picker_for_window(
                "main",
                label.to_owned(),
                project_root.to_string_lossy().into_owned(),
            )
            .expect("register project");
        let settings = SqliteSettingsRepository::open(&db).expect("settings");
        Self {
            _temp: temp,
            app,
            settings,
            project_id: project.id,
        }
    }

    /// Resolve with an explicit env map (preferred — no process-env races).
    fn resolve_dto(
        &self,
        project_id: Option<&str>,
        env: &HashMap<String, String>,
    ) -> RuntimeSettingsDto {
        let resolved = self
            .settings
            .resolve_runtime(project_id, env)
            .expect("resolve");
        to_dto(resolved, project_id.map(str::to_owned))
    }
}

fn empty_env() -> HashMap<String, String> {
    HashMap::new()
}

// ---------------------------------------------------------------------------
// 5. Command DTO has no secret fields (explicit assertion)
// ---------------------------------------------------------------------------

#[test]
fn runtime_settings_dto_has_no_secret_fields() {
    let h = Harness::new("dto-secrets");
    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    h.settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            r"C:\tools\grok.exe",
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let dto = h.resolve_dto(None, &empty_env());
    let json = serde_json::to_value(&dto).expect("serialize");
    let blob = json.to_string().to_ascii_lowercase();
    let forbidden = [
        "secret",
        "token",
        "password",
        "credential",
        "api_key",
        "apikey",
        "auth",
        "private_key",
        "privatekey",
        "bearer",
        "oauth",
    ];
    for needle in forbidden {
        assert!(
            !blob.contains(needle),
            "RuntimeSettingsDto JSON must not contain secret-like substring `{needle}`: {blob}"
        );
    }

    let keys = runtime_settings_dto_field_names(&dto);
    for key in &keys {
        let lower = key.to_ascii_lowercase();
        for needle in forbidden {
            assert!(
                !lower.contains(needle),
                "DTO field path `{key}` looks secret-shaped"
            );
        }
    }

    // Allowed surface only (structural allow-list).
    let top = json.as_object().expect("object");
    let allowed_top = ["adapter", "grokExecutable", "grokProfile", "projectId"];
    for key in top.keys() {
        assert!(
            allowed_top.contains(&key.as_str()),
            "unexpected top-level DTO field `{key}`"
        );
    }
    for field in ["adapter", "grokExecutable", "grokProfile"] {
        let obj = top.get(field).and_then(Value::as_object).expect(field);
        for key in obj.keys() {
            assert!(
                matches!(key.as_str(), "value" | "source" | "envVar"),
                "unexpected nested field `{field}.{key}`"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Env wins over storage (E2E life-line) — explicit map injection
// ---------------------------------------------------------------------------

#[test]
fn env_override_wins_over_stored_and_marks_source_env() {
    let h = Harness::new("env-wins");

    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::Fake,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    h.settings
        .upsert_path(
            SettingScope::Project,
            Some(&h.project_id),
            KEY_GROK_EXECUTABLE,
            r"C:\stored\grok.exe",
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let mut env = HashMap::new();
    env.insert(ENV_RUNTIME_ADAPTER.to_owned(), "grok-acp".to_owned());
    env.insert(
        ENV_GROK_EXECUTABLE.to_owned(),
        r"C:\env\grok.exe".to_owned(),
    );

    let dto = h.resolve_dto(Some(&h.project_id), &env);
    assert_eq!(dto.adapter.value.as_deref(), Some("grok-acp"));
    assert_eq!(dto.adapter.source, "env");
    assert_eq!(dto.adapter.env_var.as_deref(), Some("PILLOW_RUNTIME"));
    assert_eq!(
        dto.grok_executable.value.as_deref(),
        Some(r"C:\env\grok.exe")
    );
    assert_eq!(dto.grok_executable.source, "env");
    assert_eq!(
        dto.grok_executable.env_var.as_deref(),
        Some("PILLOW_GROK_EXECUTABLE")
    );
}

// ---------------------------------------------------------------------------
// No storage → default (matches env-only product default)
// ---------------------------------------------------------------------------

#[test]
fn no_stored_values_resolve_to_defaults() {
    let h = Harness::new("defaults");
    let dto = h.resolve_dto(None, &empty_env());
    assert_eq!(dto.adapter.value.as_deref(), Some("grok-acp"));
    assert_eq!(dto.adapter.source, "default");
    assert!(dto.adapter.env_var.is_none());
    assert_eq!(dto.grok_executable.value, None);
    assert_eq!(dto.grok_executable.source, "default");
    assert_eq!(dto.grok_profile.value, None);
    assert_eq!(dto.grok_profile.source, "default");
}

// ---------------------------------------------------------------------------
// Fail-closed illegal adapter
// ---------------------------------------------------------------------------

#[test]
fn illegal_adapter_parse_fails_closed_never_fake() {
    for label in ["auto", "codex", "not-a-runtime"] {
        let err = RuntimeAdapterSetting::parse(label).unwrap_err();
        assert_eq!(err.code(), "invalid_runtime_selection");
        assert!(!matches!(
            RuntimeAdapterSetting::parse(label),
            Ok(RuntimeAdapterSetting::Fake)
        ));
    }
}

#[test]
fn illegal_env_adapter_resolve_fails_closed() {
    let h = Harness::new("illegal-env");
    let mut env = HashMap::new();
    env.insert(ENV_RUNTIME_ADAPTER.to_owned(), "not-a-runtime".to_owned());
    let err = h.settings.resolve_runtime(None, &env).unwrap_err();
    assert_eq!(err.code(), "invalid_runtime_selection");
    assert!(!matches!(
        h.settings.resolve_runtime(None, &env),
        Ok(r) if r.adapter.value == RuntimeAdapterSetting::Fake
    ));
}

// ---------------------------------------------------------------------------
// Global / project write round-trip
// ---------------------------------------------------------------------------

#[test]
fn global_and_project_writes_round_trip() {
    let h = Harness::new("roundtrip");

    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    h.settings
        .upsert_runtime_adapter(
            SettingScope::Project,
            Some(&h.project_id),
            RuntimeAdapterSetting::Fake,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    h.settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            r"C:\global\grok.exe",
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let global = h.resolve_dto(None, &empty_env());
    assert_eq!(global.adapter.value.as_deref(), Some("grok-acp"));
    assert_eq!(global.adapter.source, "global");
    assert_eq!(
        global.grok_executable.value.as_deref(),
        Some(r"C:\global\grok.exe")
    );

    let project = h.resolve_dto(Some(&h.project_id), &empty_env());
    assert_eq!(project.adapter.value.as_deref(), Some("fake"));
    assert_eq!(project.adapter.source, "project");
    assert_eq!(
        project.grok_executable.value.as_deref(),
        Some(r"C:\global\grok.exe")
    );
    assert_eq!(project.grok_executable.source, "global");
}

#[test]
fn unknown_project_id_fails_closed_on_lookup() {
    let h = Harness::new("unknown-proj");
    let err = h
        .app
        .get_registered_project("00000000-0000-0000-0000-000000000099")
        .expect_err("unknown project");
    assert_eq!(err.code, "project_not_found");
}

#[test]
fn env_map_injection_documents_e2e_lifeline() {
    // Explicit HashMap (same as resolve_runtime production path) — not live env.
    let h = Harness::new("inject");
    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let mut env = HashMap::new();
    env.insert(ENV_RUNTIME_ADAPTER.to_owned(), "fake".to_owned());
    let resolved = h.settings.resolve_runtime(None, &env).unwrap();
    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::Fake);
    assert_eq!(resolved.adapter.source, SettingSource::Env);
    // Stored value would have been GrokAcp without env.
    let without = h.settings.resolve_runtime(None, &empty_env()).unwrap();
    assert_eq!(without.adapter.value, RuntimeAdapterSetting::GrokAcp);
    assert_eq!(without.adapter.source, SettingSource::Global);
}

/// Live process_env_map path still honors env (serialized with composition tests).
#[test]
fn process_env_map_still_wins_when_live() {
    let _guard = env_lock().lock().unwrap();
    let h = Harness::new("live-env");
    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    // SAFETY: serialized via shared env_lock with runtime_composition tests.
    unsafe {
        std::env::set_var(ENV_RUNTIME_ADAPTER, "fake");
    }
    let env = keeper_storage_sqlite::process_env_map();
    let resolved = h.settings.resolve_runtime(None, &env).unwrap();
    assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::Fake);
    assert_eq!(resolved.adapter.source, SettingSource::Env);
    unsafe {
        std::env::remove_var(ENV_RUNTIME_ADAPTER);
    }
}
