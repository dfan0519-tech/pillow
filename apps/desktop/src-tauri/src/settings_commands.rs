//! S0 non-secret settings Tauri commands.
//!
//! Read / write global and project runtime settings, plus global UI theme.
//! Renderer DTOs carry only non-secret fields (adapter enum label, path strings,
//! theme label, source attribution). No secret, token, credential, or auth
//! fields exist in S0 — asserted in tests.

use std::sync::Arc;

use keeper_storage_sqlite::{
    KEY_GROK_EXECUTABLE, KEY_GROK_PROFILE, KEY_RUNTIME_ADAPTER, KEY_UI_THEME,
    ResolvedRuntimeSettings, ResolvedSetting, RuntimeAdapterSetting, SettingScope, SettingSource,
    SettingsError, SettingsUpdatedBy, SqliteSettingsRepository, UiThemeSetting, process_env_map,
};
use serde::Serialize;
use tauri::State;

use crate::app_state::{AppState, CommandError};
use crate::runtime_composition::{RuntimeOpenOutcome, RuntimeSelection, fake_adapter_selectable};

// ---------------------------------------------------------------------------
// Public DTOs (renderer-facing; camelCase; no secret fields)
// ---------------------------------------------------------------------------

/// One resolved setting with explicit source attribution for UI override state.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSettingDto {
    /// Effective value as a string. Adapter: `grok-acp` / `fake`. Paths: absolute
    /// path string. `null` means discovery default (no stored/env path).
    pub value: Option<String>,
    /// Which layer won: `env` | `project` | `global` | `default`.
    pub source: String,
    /// Allow-listed env var name when `source == "env"` (for UI copy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
}

/// Effective S0 runtime settings for the settings surface.
///
/// **No secret fields.** S0 only exposes adapter + two path strings + sources
/// + whether Fake is selectable in this build (ADR-0003 Q2).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsDto {
    pub adapter: ResolvedSettingDto,
    pub grok_executable: ResolvedSettingDto,
    pub grok_profile: ResolvedSettingDto,
    /// Scope context used for resolution (`project` when project_id provided).
    pub project_id: Option<String>,
    /// Host authority: Fake may appear in the adapter dropdown (dev builds).
    /// Renderer must not hard-code env detection — filter options from this flag.
    pub fake_adapter_selectable: bool,
}

/// Process-startup conversation runtime status for the shell.
///
/// When degraded, conversation is unavailable but Settings remains reachable.
/// **Never** reports a silent Fake fallback.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStartupDto {
    /// True when a conversation service was composed successfully.
    pub conversation_available: bool,
    /// Effective selection when available: `grok-acp` | `fake`.
    pub selection: Option<String>,
    /// Stable error code when degraded (e.g. `runtime_executable_missing`).
    pub degraded_code: Option<String>,
    /// Technical message when degraded.
    pub degraded_message: Option<String>,
    /// User-facing guidance pointing at Settings fields.
    pub degraded_guidance: Option<String>,
}

/// Managed process state: conversation open result (ready or degraded).
///
/// Populated once in `lib::run` setup. ConversationService is only managed
/// when ready; degraded mode deliberately does **not** open Fake.
#[derive(Debug, Clone)]
pub struct RuntimeStartupState {
    pub conversation_available: bool,
    pub selection: Option<RuntimeSelection>,
    pub degraded_code: Option<&'static str>,
    pub degraded_message: Option<String>,
    pub degraded_guidance: Option<String>,
}

impl RuntimeStartupState {
    pub fn ready(selection: RuntimeSelection) -> Self {
        Self {
            conversation_available: true,
            selection: Some(selection),
            degraded_code: None,
            degraded_message: None,
            degraded_guidance: None,
        }
    }

    pub fn degraded(code: &'static str, message: String, guidance: String) -> Self {
        Self {
            conversation_available: false,
            selection: None,
            degraded_code: Some(code),
            degraded_message: Some(message),
            degraded_guidance: Some(guidance),
        }
    }

    pub fn from_outcome(outcome: &RuntimeOpenOutcome) -> Self {
        match outcome {
            RuntimeOpenOutcome::Ready { selection, .. } => Self::ready(*selection),
            RuntimeOpenOutcome::Degraded {
                code,
                message,
                guidance,
            } => Self::degraded(code, message.clone(), guidance.clone()),
        }
    }

    pub fn to_dto(&self) -> RuntimeStartupDto {
        RuntimeStartupDto {
            conversation_available: self.conversation_available,
            selection: self.selection.map(|s| s.as_str().to_owned()),
            degraded_code: self.degraded_code.map(str::to_owned),
            degraded_message: self.degraded_message.clone(),
            degraded_guidance: self.degraded_guidance.clone(),
        }
    }
}

/// Effective UI theme (`ui.theme`) for the settings surface and shell.
///
/// Same `settings_entries` store and precedence as runtime settings.
/// **No secret fields.**
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettingsDto {
    pub theme: ResolvedSettingDto,
}

/// First-run onboarding dismiss flag (`ui.onboarding.dismissed`).
///
/// Global install fact only — no env override, no secret fields.
/// Absent row resolves to `dismissed = false` (show guidance on fresh install).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSettingsDto {
    pub dismissed: bool,
    /// Which layer won: `project` | `global` | `default` (never `env`).
    pub source: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Resolve effective runtime settings (env → project → global → default).
#[tauri::command]
pub fn get_runtime_settings(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    project_id: Option<String>,
) -> Result<RuntimeSettingsDto, CommandError> {
    let project_ref = normalize_project_id(project_id.as_deref(), &app)?;
    let resolved = settings
        .resolve_runtime(project_ref, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(to_dto(resolved, project_ref.map(str::to_owned)))
}

/// Process-startup runtime status (ready vs degraded). Shell always available.
#[tauri::command]
pub fn get_runtime_startup(
    startup: State<'_, Arc<RuntimeStartupState>>,
) -> Result<RuntimeStartupDto, CommandError> {
    Ok(startup.to_dto())
}

/// Upsert runtime adapter at global or project scope. Returns fresh resolution.
#[tauri::command]
pub fn set_runtime_adapter(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    adapter: String,
) -> Result<RuntimeSettingsDto, CommandError> {
    let (scope, project_ref) = parse_scope(&scope, project_id.as_deref(), &app)?;
    let parsed = RuntimeAdapterSetting::parse(&adapter).map_err(map_settings_error)?;
    // ADR-0003 Q2: refuse storing Fake when this build does not expose it.
    // Does not rewrite existing stored Fake rows (fail-closed on read/open).
    if parsed == RuntimeAdapterSetting::Fake && !fake_adapter_selectable() {
        return Err(settings_error(
            "runtime_fake_not_allowed",
            "Fake adapter is only available in development builds. \
             Choose Grok ACP, or set PILLOW_RUNTIME=fake for automation. \
             Existing stored values are not modified by this rejection.",
        ));
    }
    settings
        .upsert_runtime_adapter(scope, project_ref, parsed, SettingsUpdatedBy::User)
        .map_err(map_settings_error)?;
    let resolved = settings
        .resolve_runtime(project_ref, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(to_dto(resolved, project_ref.map(str::to_owned)))
}

/// Upsert Grok executable path at global or project scope.
#[tauri::command]
pub fn set_grok_executable(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    path: String,
) -> Result<RuntimeSettingsDto, CommandError> {
    upsert_path_setting(
        &settings,
        &app,
        scope,
        project_id,
        KEY_GROK_EXECUTABLE,
        path,
    )
}

/// Upsert Grok profile / process-home path at global or project scope.
#[tauri::command]
pub fn set_grok_profile(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    path: String,
) -> Result<RuntimeSettingsDto, CommandError> {
    upsert_path_setting(&settings, &app, scope, project_id, KEY_GROK_PROFILE, path)
}

/// Delete one known key at a scope so resolution falls through.
#[tauri::command]
pub fn clear_runtime_setting(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    key: String,
) -> Result<RuntimeSettingsDto, CommandError> {
    let (scope, project_ref) = parse_scope(&scope, project_id.as_deref(), &app)?;
    validate_known_key(&key)?;
    let _ = settings
        .delete(scope, project_ref, &key)
        .map_err(map_settings_error)?;
    let resolved = settings
        .resolve_runtime(project_ref, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(to_dto(resolved, project_ref.map(str::to_owned)))
}

/// Resolve effective UI theme (env → project → global → default).
///
/// Product UI only exposes global scope for edits; resolution still uses the
/// shared precedence chain so env/`PILLOW_THEME` wins for deterministic E2E.
#[tauri::command]
pub fn get_theme_settings(
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    project_id: Option<String>,
) -> Result<ThemeSettingsDto, CommandError> {
    let project_ref = normalize_project_id(project_id.as_deref(), &app)?;
    let resolved = settings
        .resolve_theme(project_ref, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(theme_to_dto(resolved))
}

/// Upsert UI theme at **global** scope only. Returns fresh resolution.
///
/// Project-scoped theme is intentionally not exposed (visual contract stability).
#[tauri::command]
pub fn set_ui_theme(
    settings: State<'_, SqliteSettingsRepository>,
    theme: String,
) -> Result<ThemeSettingsDto, CommandError> {
    let parsed = UiThemeSetting::parse(&theme).map_err(map_settings_error)?;
    settings
        .upsert_ui_theme(SettingScope::Global, None, parsed, SettingsUpdatedBy::User)
        .map_err(map_settings_error)?;
    let resolved = settings
        .resolve_theme(None, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(theme_to_dto(resolved))
}

/// Resolve first-run onboarding dismiss flag (global install fact).
///
/// Default when absent: **not dismissed** so strangers see guidance once.
/// No env override (by design — see storage `KEY_ONBOARDING_DISMISSED`).
#[tauri::command]
pub fn get_onboarding_settings(
    settings: State<'_, SqliteSettingsRepository>,
) -> Result<OnboardingSettingsDto, CommandError> {
    let resolved = settings
        .resolve_onboarding_dismissed(None)
        .map_err(map_settings_error)?;
    Ok(onboarding_to_dto(resolved))
}

/// Persist onboarding dismiss / restore at **global** scope. Returns fresh view.
#[tauri::command]
pub fn set_onboarding_dismissed(
    settings: State<'_, SqliteSettingsRepository>,
    dismissed: bool,
) -> Result<OnboardingSettingsDto, CommandError> {
    settings
        .upsert_onboarding_dismissed(
            SettingScope::Global,
            None,
            dismissed,
            SettingsUpdatedBy::User,
        )
        .map_err(map_settings_error)?;
    let resolved = settings
        .resolve_onboarding_dismissed(None)
        .map_err(map_settings_error)?;
    Ok(onboarding_to_dto(resolved))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn upsert_path_setting(
    settings: &SqliteSettingsRepository,
    app: &AppState,
    scope: String,
    project_id: Option<String>,
    key: &str,
    path: String,
) -> Result<RuntimeSettingsDto, CommandError> {
    let (scope, project_ref) = parse_scope(&scope, project_id.as_deref(), app)?;
    settings
        .upsert_path(scope, project_ref, key, &path, SettingsUpdatedBy::User)
        .map_err(map_settings_error)?;
    let resolved = settings
        .resolve_runtime(project_ref, &process_env_map())
        .map_err(map_settings_error)?;
    Ok(to_dto(resolved, project_ref.map(str::to_owned)))
}

fn parse_scope<'a>(
    scope: &str,
    project_id: Option<&'a str>,
    app: &AppState,
) -> Result<(SettingScope, Option<&'a str>), CommandError> {
    match scope.trim().to_ascii_lowercase().as_str() {
        "global" => {
            if project_id.is_some() {
                // Ignore project_id for global writes; shape is global-only.
            }
            Ok((SettingScope::Global, None))
        }
        "project" => {
            let project_ref = normalize_project_id(project_id, app)?.ok_or_else(|| {
                settings_error(
                    "settings_scope_shape",
                    "Project scope requires a project id.",
                )
            })?;
            Ok((SettingScope::Project, Some(project_ref)))
        }
        other => Err(settings_error(
            "settings_invalid_scope",
            &format!("Invalid settings scope `{other}`. Use `global` or `project`."),
        )),
    }
}

fn normalize_project_id<'a>(
    project_id: Option<&'a str>,
    app: &AppState,
) -> Result<Option<&'a str>, CommandError> {
    match project_id {
        None => Ok(None),
        Some(id) if id.trim().is_empty() => Ok(None),
        Some(id) => {
            // Fail closed: unknown project must not resolve against a ghost scope.
            let _ = app.get_registered_project(id)?;
            Ok(Some(id))
        }
    }
}

fn validate_known_key(key: &str) -> Result<(), CommandError> {
    match key {
        KEY_RUNTIME_ADAPTER | KEY_GROK_EXECUTABLE | KEY_GROK_PROFILE | KEY_UI_THEME => Ok(()),
        other => Err(settings_error(
            "settings_unknown_key",
            &format!("Unknown settings key `{other}`."),
        )),
    }
}

pub(crate) fn to_dto(
    resolved: ResolvedRuntimeSettings,
    project_id: Option<String>,
) -> RuntimeSettingsDto {
    RuntimeSettingsDto {
        adapter: ResolvedSettingDto {
            value: Some(resolved.adapter.value.as_str().to_owned()),
            source: source_str(resolved.adapter.source).to_owned(),
            env_var: env_var_for(resolved.adapter.source, "PILLOW_RUNTIME"),
        },
        grok_executable: ResolvedSettingDto {
            value: resolved.grok_executable.value,
            source: source_str(resolved.grok_executable.source).to_owned(),
            env_var: env_var_for(resolved.grok_executable.source, "PILLOW_GROK_EXECUTABLE"),
        },
        grok_profile: ResolvedSettingDto {
            value: resolved.grok_profile.value,
            source: source_str(resolved.grok_profile.source).to_owned(),
            env_var: env_var_for(resolved.grok_profile.source, "PILLOW_GROK_PROFILE"),
        },
        project_id,
        fake_adapter_selectable: fake_adapter_selectable(),
    }
}

fn source_str(source: SettingSource) -> &'static str {
    source.as_str()
}

fn env_var_for(source: SettingSource, name: &str) -> Option<String> {
    if source == SettingSource::Env {
        Some(name.to_owned())
    } else {
        None
    }
}

pub(crate) fn theme_to_dto(resolved: ResolvedSetting<UiThemeSetting>) -> ThemeSettingsDto {
    ThemeSettingsDto {
        theme: ResolvedSettingDto {
            value: Some(resolved.value.as_str().to_owned()),
            source: source_str(resolved.source).to_owned(),
            env_var: env_var_for(resolved.source, "PILLOW_THEME"),
        },
    }
}

pub(crate) fn onboarding_to_dto(resolved: ResolvedSetting<bool>) -> OnboardingSettingsDto {
    OnboardingSettingsDto {
        dismissed: resolved.value,
        source: source_str(resolved.source).to_owned(),
    }
}

fn map_settings_error(error: SettingsError) -> CommandError {
    CommandError {
        code: error.code(),
        message: error.to_string(),
    }
}

fn settings_error(code: &'static str, message: &str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}

/// Test-only: list serializable field names of the public DTO surface.
#[cfg(test)]
pub(crate) fn runtime_settings_dto_field_names(dto: &RuntimeSettingsDto) -> Vec<String> {
    let value = serde_json::to_value(dto).expect("dto serializes");
    collect_json_keys(&value, String::new())
}

#[cfg(test)]
fn collect_json_keys(value: &serde_json::Value, prefix: String) -> Vec<String> {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys = Vec::new();
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                keys.push(path.clone());
                keys.extend(collect_json_keys(v, path));
            }
            keys
        }
        serde_json::Value::Array(items) => {
            let mut keys = Vec::new();
            for (i, item) in items.iter().enumerate() {
                keys.extend(collect_json_keys(item, format!("{prefix}[{i}]")));
            }
            keys
        }
        _ => Vec::new(),
    }
}
