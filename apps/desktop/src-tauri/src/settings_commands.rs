//! S0 non-secret settings Tauri commands.
//!
//! Read / write global and project runtime settings. Renderer DTOs carry only
//! non-secret fields (adapter enum label, path strings, source attribution).
//! No secret, token, credential, or auth fields exist in S0 — asserted in tests.

use keeper_storage_sqlite::{
    KEY_GROK_EXECUTABLE, KEY_GROK_PROFILE, KEY_RUNTIME_ADAPTER, ResolvedRuntimeSettings,
    RuntimeAdapterSetting, SettingScope, SettingSource, SettingsError, SettingsUpdatedBy,
    SqliteSettingsRepository, process_env_map,
};
use serde::Serialize;
use tauri::State;

use crate::app_state::{AppState, CommandError};

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
/// **No secret fields.** S0 only exposes adapter + two path strings + sources.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsDto {
    pub adapter: ResolvedSettingDto,
    pub grok_executable: ResolvedSettingDto,
    pub grok_profile: ResolvedSettingDto,
    /// Scope context used for resolution (`project` when project_id provided).
    pub project_id: Option<String>,
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
        KEY_RUNTIME_ADAPTER | KEY_GROK_EXECUTABLE | KEY_GROK_PROFILE => Ok(()),
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
