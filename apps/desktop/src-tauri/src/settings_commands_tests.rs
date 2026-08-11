//! Focused tests for S0 settings Tauri command surface.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use keeper_storage_sqlite::{
    ENV_GROK_EXECUTABLE, ENV_RUNTIME_ADAPTER, ENV_UI_THEME, KEY_GROK_EXECUTABLE, KEY_UI_THEME,
    PermissionMode, RuntimeAdapterSetting, SettingScope, SettingSource, SettingsUpdatedBy,
    SqliteSettingsRepository, UiThemeSetting,
};
use serde_json::Value;

use crate::app_state::AppState;
use crate::runtime_composition::{fake_adapter_selectable, tests::env_lock};
use crate::settings_commands::{
    PermissionModeSettingsDto, RuntimeSettingsDto, RuntimeStartupState, ThemeSettingsDto,
    permission_mode_to_dto, runtime_settings_dto_field_names, theme_to_dto, to_dto,
};

/// Owns the fixture tree; removing it is the guard's own `Drop`, never ours.
fn temp_root(label: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("pillow-settings-cmd-{label}-"))
        .tempdir()
        .expect("temp root")
}

struct Harness {
    app: AppState,
    settings: SqliteSettingsRepository,
    project_id: String,
    /// Last field on purpose: fields drop in declaration order, so `app` and
    /// `settings` close their SQLite handles on `keeper.db` before the directory
    /// holding that file is removed. Windows refuses to delete a file that is
    /// still open, and a hand-written `impl Drop for Harness` runs *before* any
    /// field — which is how this fixture used to leak its whole tree.
    temp: TempDir,
}

impl Harness {
    fn new(label: &str) -> Self {
        let temp = temp_root(label);
        let project_root = temp.path().join("project");
        fs::create_dir_all(&project_root).expect("project root");
        let db = temp.path().join("keeper.db");
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
            app,
            settings,
            project_id: project.id,
            temp,
        }
    }

    fn root(&self) -> &Path {
        self.temp.path()
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

    fn resolve_theme_dto(&self, env: &HashMap<String, String>) -> ThemeSettingsDto {
        let resolved = self
            .settings
            .resolve_theme(None, env)
            .expect("resolve theme");
        theme_to_dto(resolved)
    }

    fn resolve_permission_mode_dto(&self) -> PermissionModeSettingsDto {
        permission_mode_to_dto(
            self.settings
                .resolve_permission_mode()
                .expect("resolve permission mode"),
        )
    }
}

fn empty_env() -> HashMap<String, String> {
    HashMap::new()
}

// ---------------------------------------------------------------------------
// Fixture hygiene
// ---------------------------------------------------------------------------

/// Both `app` and `settings` hold `keeper.db` open, so removal must wait for
/// them. It used to run from an `impl Drop for Harness` — before either field
/// was dropped — and the failed delete was discarded, leaking the tree.
#[test]
fn dropping_the_harness_removes_its_temp_root() {
    let root = {
        let h = Harness::new("temp-hygiene");
        let root = h.root().to_path_buf();
        assert!(root.join("keeper.db").is_file(), "fixture db should exist");
        root
    };
    assert!(!root.exists(), "harness left {} behind", root.display());
}

// ---------------------------------------------------------------------------
// 5. Command DTO has no secret fields (explicit assertion)
// ---------------------------------------------------------------------------

#[test]
fn runtime_settings_dto_has_no_secret_fields() {
    // Label must not contain forbidden substrings (path is embedded in DTO values).
    let h = Harness::new("dto-surface");
    h.settings
        .upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let exe = h.root().join("tools-grok.exe");
    fs::write(&exe, b"").unwrap();
    h.settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &exe.to_string_lossy(),
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
    let allowed_top = [
        "adapter",
        "grokExecutable",
        "grokProfile",
        "projectId",
        "fakeAdapterSelectable",
    ];
    for key in top.keys() {
        assert!(
            allowed_top.contains(&key.as_str()),
            "unexpected top-level DTO field `{key}`"
        );
    }
    assert_eq!(
        dto.fake_adapter_selectable,
        fake_adapter_selectable(),
        "DTO must surface host Fake gate"
    );
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
    let stored_exe = h.root().join("stored-grok.exe");
    fs::write(&stored_exe, b"").unwrap();
    h.settings
        .upsert_path(
            SettingScope::Project,
            Some(&h.project_id),
            KEY_GROK_EXECUTABLE,
            &stored_exe.to_string_lossy(),
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
    let global_exe = h.root().join("global-grok.exe");
    fs::write(&global_exe, b"").unwrap();
    let global_exe_str = global_exe.to_string_lossy().into_owned();
    h.settings
        .upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &global_exe_str,
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let global = h.resolve_dto(None, &empty_env());
    assert_eq!(global.adapter.value.as_deref(), Some("grok-acp"));
    assert_eq!(global.adapter.source, "global");
    assert_eq!(
        global.grok_executable.value.as_deref(),
        Some(global_exe_str.as_str())
    );

    let project = h.resolve_dto(Some(&h.project_id), &empty_env());
    assert_eq!(project.adapter.value.as_deref(), Some("fake"));
    assert_eq!(project.adapter.source, "project");
    assert_eq!(
        project.grok_executable.value.as_deref(),
        Some(global_exe_str.as_str())
    );
    assert_eq!(project.grok_executable.source, "global");
}

#[test]
fn startup_state_degraded_reports_not_available_and_not_fake() {
    let state = RuntimeStartupState::degraded(
        "runtime_executable_missing",
        "Grok executable is not a file: missing.exe".to_owned(),
        "打开「设置」修正路径".to_owned(),
    );
    let dto = state.to_dto();
    assert!(!dto.conversation_available);
    assert!(dto.selection.is_none());
    assert_eq!(
        dto.degraded_code.as_deref(),
        Some("runtime_executable_missing")
    );
    assert!(dto.degraded_guidance.as_ref().unwrap().contains("设置"));
    // Explicit: degraded is not Fake.
    assert_ne!(dto.selection.as_deref(), Some("fake"));
}

#[test]
fn startup_state_ready_reports_selection() {
    use crate::runtime_composition::RuntimeSelection;
    let dto = RuntimeStartupState::ready(RuntimeSelection::GrokAcp).to_dto();
    assert!(dto.conversation_available);
    assert_eq!(dto.selection.as_deref(), Some("grok-acp"));
    assert!(dto.degraded_code.is_none());
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

// ---------------------------------------------------------------------------
// ui.theme command DTO surface
// ---------------------------------------------------------------------------

#[test]
fn theme_settings_dto_has_no_secret_fields() {
    let h = Harness::new("theme-dto-secrets");
    h.settings
        .upsert_ui_theme(
            SettingScope::Global,
            None,
            UiThemeSetting::Light,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let dto = h.resolve_theme_dto(&empty_env());
    let json = serde_json::to_value(&dto).expect("serialize");
    let blob = json.to_string().to_ascii_lowercase();
    for needle in [
        "secret",
        "token",
        "password",
        "credential",
        "api_key",
        "apikey",
        "auth",
        "private_key",
        "bearer",
    ] {
        assert!(
            !blob.contains(needle),
            "ThemeSettingsDto must not contain secret-like substring `{needle}`: {blob}"
        );
    }
    let top = json.as_object().expect("object");
    assert_eq!(top.keys().count(), 1);
    assert!(top.contains_key("theme"));
    let theme = top.get("theme").and_then(Value::as_object).expect("theme");
    for key in theme.keys() {
        assert!(
            matches!(key.as_str(), "value" | "source" | "envVar"),
            "unexpected nested field `theme.{key}`"
        );
    }
}

#[test]
fn theme_env_override_wins_and_marks_source_env() {
    let h = Harness::new("theme-env-wins");
    h.settings
        .upsert_ui_theme(
            SettingScope::Global,
            None,
            UiThemeSetting::Dark,
            SettingsUpdatedBy::User,
        )
        .unwrap();

    let mut env = HashMap::new();
    env.insert(ENV_UI_THEME.to_owned(), "light".to_owned());
    let dto = h.resolve_theme_dto(&env);
    assert_eq!(dto.theme.value.as_deref(), Some("light"));
    assert_eq!(dto.theme.source, "env");
    assert_eq!(dto.theme.env_var.as_deref(), Some("PILLOW_THEME"));
}

#[test]
fn theme_default_and_global_round_trip() {
    let h = Harness::new("theme-roundtrip");
    let default = h.resolve_theme_dto(&empty_env());
    assert_eq!(default.theme.value.as_deref(), Some("dark"));
    assert_eq!(default.theme.source, "default");
    assert!(default.theme.env_var.is_none());

    h.settings
        .upsert_ui_theme(
            SettingScope::Global,
            None,
            UiThemeSetting::Light,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    let stored = h.resolve_theme_dto(&empty_env());
    assert_eq!(stored.theme.value.as_deref(), Some("light"));
    assert_eq!(stored.theme.source, "global");
}

#[test]
fn illegal_theme_fails_closed_never_dark() {
    for label in ["system", "auto", "not-a-theme"] {
        let err = UiThemeSetting::parse(label).unwrap_err();
        assert_eq!(err.code(), "invalid_theme_selection");
        assert!(!matches!(
            UiThemeSetting::parse(label),
            Ok(UiThemeSetting::Dark)
        ));
    }

    let h = Harness::new("theme-illegal-env");
    let mut env = HashMap::new();
    env.insert(ENV_UI_THEME.to_owned(), "system".to_owned());
    let err = h.settings.resolve_theme(None, &env).unwrap_err();
    assert_eq!(err.code(), "invalid_theme_selection");
}

#[test]
fn theme_key_is_known_for_clear_path() {
    // clear_runtime_setting validate_known_key includes ui.theme (same store).
    let h = Harness::new("theme-clear");
    h.settings
        .upsert_ui_theme(
            SettingScope::Global,
            None,
            UiThemeSetting::Light,
            SettingsUpdatedBy::User,
        )
        .unwrap();
    assert!(
        h.settings
            .delete(SettingScope::Global, None, KEY_UI_THEME)
            .unwrap()
    );
    let dto = h.resolve_theme_dto(&empty_env());
    assert_eq!(dto.theme.value.as_deref(), Some("dark"));
    assert_eq!(dto.theme.source, "default");
}

#[test]
fn permission_mode_dto_accepts_exact_keys_and_global_values_round_trip() {
    let h = Harness::new("permission-mode-dto");
    let default = h.resolve_permission_mode_dto();
    assert_eq!(default.mode, "default");
    assert_eq!(default.source, "default");

    h.settings
        .upsert_permission_mode(PermissionMode::Bypass, SettingsUpdatedBy::User)
        .unwrap();
    let stored = h.resolve_permission_mode_dto();
    assert_eq!(stored.mode, "bypass");
    assert_eq!(stored.source, "global");

    let json = serde_json::to_value(stored).expect("serialize permission mode dto");
    let keys = json
        .as_object()
        .expect("permission mode dto object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(keys, vec!["mode", "source"]);
}

#[test]
fn permission_mode_command_parser_rejects_unknown_value_without_selecting_bypass() {
    let error = PermissionMode::parse("always-approve").unwrap_err();
    assert_eq!(error.code(), "invalid_permission_mode");
    assert!(!matches!(
        PermissionMode::parse("always-approve"),
        Ok(PermissionMode::Bypass)
    ));
}
