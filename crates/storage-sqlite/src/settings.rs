//! Non-secret settings storage and resolution (S0).
//!
//! Durable authority lives in SQLite (`settings_entries`). Process environment
//! is an allow-listed override only. Precedence is documented and testable:
//!
//! ```text
//! env(allow-list) → project → global → default
//! ```
//!
//! Every resolved value reports which layer won (`SettingSource`) so UI can
//! surface "overridden by environment". Invalid stored enum values fail closed
//! and never silently fall back to Fake.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

/// S0 key: preferred runtime adapter (`PILLOW_RUNTIME`).
pub const KEY_RUNTIME_ADAPTER: &str = "runtime.adapter";
/// S0 key: Grok CLI executable path (`PILLOW_GROK_EXECUTABLE`).
pub const KEY_GROK_EXECUTABLE: &str = "runtime.grok.executable";
/// S0 key: Grok process-home / profile directory (`PILLOW_GROK_PROFILE`).
pub const KEY_GROK_PROFILE: &str = "runtime.grok.profile";
/// UI theme preference (`PILLOW_THEME`). Values: `dark` | `light` only.
pub const KEY_UI_THEME: &str = "ui.theme";

/// Whether the user has dismissed first-run onboarding.
///
/// Deliberately has no env override: onboarding is a per-install fact about
/// what this person has already seen, not machine configuration, and an env
/// var could only serve to hide guidance from someone who never dismissed it.
pub const KEY_ONBOARDING_DISMISSED: &str = "ui.onboarding.dismissed";

/// Allow-listed env override for [`KEY_RUNTIME_ADAPTER`].
pub const ENV_RUNTIME_ADAPTER: &str = "PILLOW_RUNTIME";
/// Allow-listed env override for [`KEY_GROK_EXECUTABLE`].
pub const ENV_GROK_EXECUTABLE: &str = "PILLOW_GROK_EXECUTABLE";
/// Allow-listed env override for [`KEY_GROK_PROFILE`].
pub const ENV_GROK_PROFILE: &str = "PILLOW_GROK_PROFILE";
/// Allow-listed env override for [`KEY_UI_THEME`].
pub const ENV_UI_THEME: &str = "PILLOW_THEME";

const VALUE_KIND_ENUM: &str = "enum";
const VALUE_KIND_PATH: &str = "path";
const VALUE_KIND_FLAG: &str = "flag";

/// Setting scope: installation-wide or one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    Global,
    Project,
}

impl SettingScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    fn parse(raw: &str) -> Result<Self, SettingsError> {
        match raw {
            "global" => Ok(Self::Global),
            "project" => Ok(Self::Project),
            other => Err(SettingsError::InvalidScope {
                scope: other.to_owned(),
            }),
        }
    }
}

/// Which layer produced the effective value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    /// Allow-listed process environment override.
    Env,
    /// Project-scoped SQLite row.
    Project,
    /// Installation-wide SQLite row.
    Global,
    /// Built-in code default.
    Default,
}

impl SettingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Project => "project",
            Self::Global => "global",
            Self::Default => "default",
        }
    }
}

/// Constrained runtime adapter enum (mirrors desktop `RuntimeSelection` labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAdapterSetting {
    GrokAcp,
    Fake,
}

impl RuntimeAdapterSetting {
    /// Parse adapter label. Unknown values fail closed (no silent Fake).
    pub fn parse(raw: &str) -> Result<Self, SettingsError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "grok" | "grok-acp" | "acp" => Ok(Self::GrokAcp),
            "fake" | "fixture" => Ok(Self::Fake),
            other => Err(SettingsError::InvalidRuntimeSelection {
                value: other.to_owned(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GrokAcp => "grok-acp",
            Self::Fake => "fake",
        }
    }

    pub fn default_value() -> Self {
        Self::GrokAcp
    }
}

/// Constrained UI theme enum. Intentionally no `system` (E2E screenshot determinism).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiThemeSetting {
    Dark,
    Light,
}

impl UiThemeSetting {
    /// Parse theme label. Unknown values fail closed (no silent dark fallback).
    pub fn parse(raw: &str) -> Result<Self, SettingsError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            other => Err(SettingsError::InvalidThemeSelection {
                value: other.to_owned(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn default_value() -> Self {
        Self::Dark
    }
}

/// Durable settings row (non-secret only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSetting {
    pub setting_id: String,
    pub scope: SettingScope,
    pub project_id: Option<String>,
    pub key: String,
    pub value_kind: String,
    pub value_json: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

/// A resolved setting with explicit source attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSetting<T> {
    pub value: T,
    pub source: SettingSource,
}

/// Effective S0 runtime settings for composition / UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimeSettings {
    pub adapter: ResolvedSetting<RuntimeAdapterSetting>,
    /// Path only; `None` means discovery / built-in default path logic.
    pub grok_executable: ResolvedSetting<Option<String>>,
    /// Path only; `None` means current-user home discovery.
    pub grok_profile: ResolvedSetting<Option<String>>,
}

/// Who wrote a settings row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsUpdatedBy {
    User,
    System,
    Migration,
}

impl SettingsUpdatedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Migration => "migration",
        }
    }
}

/// Input for upserting a setting at a given scope.
#[derive(Debug, Clone)]
pub struct UpsertSetting {
    pub scope: SettingScope,
    pub project_id: Option<String>,
    pub key: String,
    pub value_kind: String,
    pub value_json: String,
    pub updated_by: SettingsUpdatedBy,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("settings error code={code}: {message}")]
    Coded { code: &'static str, message: String },
    #[error("invalid settings scope: {scope}")]
    InvalidScope { scope: String },
    #[error("project scope requires a non-empty project_id; global requires none")]
    ScopeShape,
    #[error("unknown settings key: {key}")]
    UnknownKey { key: String },
    #[error("invalid runtime selection `{value}` (fail-closed; never falls back to Fake)")]
    InvalidRuntimeSelection { value: String },
    #[error("invalid theme selection `{value}` (fail-closed; never falls back to dark)")]
    InvalidThemeSelection { value: String },
    #[error("invalid path setting value for key {key}")]
    InvalidPathValue { key: String },
    #[error("invalid settings value kind `{kind}` for key {key}")]
    InvalidValueKind { key: String, kind: String },
    #[error("invalid settings JSON for key {key}")]
    InvalidValueJson { key: String },
    #[error("SQLite settings store error")]
    Sqlite,
    #[error("settings store lock was poisoned")]
    LockPoisoned,
}

impl SettingsError {
    /// Stable error code for UI / command mapping.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Coded { code, .. } => code,
            Self::InvalidScope { .. } => "settings_invalid_scope",
            Self::ScopeShape => "settings_scope_shape",
            Self::UnknownKey { .. } => "settings_unknown_key",
            Self::InvalidRuntimeSelection { .. } => "invalid_runtime_selection",
            Self::InvalidThemeSelection { .. } => "invalid_theme_selection",
            Self::InvalidPathValue { .. } => "settings_invalid_path",
            Self::InvalidValueKind { .. } => "settings_invalid_value_kind",
            Self::InvalidValueJson { .. } => "settings_invalid_value_json",
            Self::Sqlite => "settings_sqlite",
            Self::LockPoisoned => "settings_lock_poisoned",
        }
    }
}

/// SQLite-backed settings repository with resolution helpers.
pub struct SqliteSettingsRepository {
    connection: Mutex<Connection>,
}

impl SqliteSettingsRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Read one row by scope + key. Global: `project_id` must be `None`.
    pub fn get(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        key: &str,
    ) -> Result<Option<StoredSetting>, SettingsError> {
        validate_scope_shape(scope, project_id)?;
        let connection = self.connection()?;
        let row = match scope {
            SettingScope::Global => connection
                .query_row(
                    "SELECT setting_id, scope, project_id, key, value_kind, value_json,
                            updated_at, updated_by
                     FROM settings_entries
                     WHERE scope = 'global' AND project_id IS NULL AND key = ?1",
                    params![key],
                    setting_from_row,
                )
                .optional()
                .map_err(map_sqlite)?,
            SettingScope::Project => {
                let project_id = project_id.expect("validated non-empty project_id");
                connection
                    .query_row(
                        "SELECT setting_id, scope, project_id, key, value_kind, value_json,
                                updated_at, updated_by
                         FROM settings_entries
                         WHERE scope = 'project' AND project_id = ?1 AND key = ?2",
                        params![project_id, key],
                        setting_from_row,
                    )
                    .optional()
                    .map_err(map_sqlite)?
            }
        };
        Ok(row)
    }

    /// List all rows for a scope. Project scope requires `project_id`.
    pub fn list(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
    ) -> Result<Vec<StoredSetting>, SettingsError> {
        validate_scope_shape(scope, project_id)?;
        let connection = self.connection()?;
        let mut statement = match scope {
            SettingScope::Global => connection
                .prepare(
                    "SELECT setting_id, scope, project_id, key, value_kind, value_json,
                            updated_at, updated_by
                     FROM settings_entries
                     WHERE scope = 'global' AND project_id IS NULL
                     ORDER BY key ASC",
                )
                .map_err(map_sqlite)?,
            SettingScope::Project => connection
                .prepare(
                    "SELECT setting_id, scope, project_id, key, value_kind, value_json,
                            updated_at, updated_by
                     FROM settings_entries
                     WHERE scope = 'project' AND project_id = ?1
                     ORDER BY key ASC",
                )
                .map_err(map_sqlite)?,
        };
        let rows = match scope {
            SettingScope::Global => statement
                .query_map([], setting_from_row)
                .map_err(map_sqlite)?,
            SettingScope::Project => {
                let project_id = project_id.expect("validated non-empty project_id");
                statement
                    .query_map(params![project_id], setting_from_row)
                    .map_err(map_sqlite)?
            }
        };
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// Insert or replace a setting at the given scope.
    ///
    /// S0 known keys are validated on write. Unknown keys are rejected so the
    /// store cannot accumulate ungoverned free-form configuration.
    pub fn upsert(&self, input: UpsertSetting) -> Result<StoredSetting, SettingsError> {
        validate_scope_shape(input.scope, input.project_id.as_deref())?;
        validate_known_key_write(&input)?;

        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let connection = self.connection()?;

        let existing = match input.scope {
            SettingScope::Global => connection
                .query_row(
                    "SELECT setting_id FROM settings_entries
                     WHERE scope = 'global' AND project_id IS NULL AND key = ?1",
                    params![input.key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(map_sqlite)?,
            SettingScope::Project => {
                let project_id = input.project_id.as_deref().expect("validated");
                connection
                    .query_row(
                        "SELECT setting_id FROM settings_entries
                         WHERE scope = 'project' AND project_id = ?1 AND key = ?2",
                        params![project_id, input.key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(map_sqlite)?
            }
        };

        let setting_id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
        connection
            .execute(
                "INSERT INTO settings_entries (
                    setting_id, scope, project_id, key, value_kind, value_json,
                    updated_at, updated_by
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(setting_id) DO UPDATE SET
                    value_kind = excluded.value_kind,
                    value_json = excluded.value_json,
                    updated_at = excluded.updated_at,
                    updated_by = excluded.updated_by",
                params![
                    setting_id,
                    input.scope.as_str(),
                    input.project_id,
                    input.key,
                    input.value_kind,
                    input.value_json,
                    now_text,
                    input.updated_by.as_str(),
                ],
            )
            .map_err(map_sqlite)?;

        Ok(StoredSetting {
            setting_id,
            scope: input.scope,
            project_id: input.project_id,
            key: input.key,
            value_kind: input.value_kind,
            value_json: input.value_json,
            updated_at: now,
            updated_by: input.updated_by.as_str().to_owned(),
        })
    }

    /// Convenience: set runtime adapter at a scope.
    pub fn upsert_runtime_adapter(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        adapter: RuntimeAdapterSetting,
        updated_by: SettingsUpdatedBy,
    ) -> Result<StoredSetting, SettingsError> {
        self.upsert(UpsertSetting {
            scope,
            project_id: project_id.map(str::to_owned),
            key: KEY_RUNTIME_ADAPTER.to_owned(),
            value_kind: VALUE_KIND_ENUM.to_owned(),
            value_json: json_string(adapter.as_str()),
            updated_by,
        })
    }

    /// Convenience: set a path setting (`runtime.grok.executable` / profile).
    pub fn upsert_path(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        key: &str,
        path: &str,
        updated_by: SettingsUpdatedBy,
    ) -> Result<StoredSetting, SettingsError> {
        self.upsert(UpsertSetting {
            scope,
            project_id: project_id.map(str::to_owned),
            key: key.to_owned(),
            value_kind: VALUE_KIND_PATH.to_owned(),
            value_json: json_string(path),
            updated_by,
        })
    }

    /// Convenience: set UI theme at a scope (same `settings_entries` table as runtime).
    pub fn upsert_ui_theme(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        theme: UiThemeSetting,
        updated_by: SettingsUpdatedBy,
    ) -> Result<StoredSetting, SettingsError> {
        self.upsert(UpsertSetting {
            scope,
            project_id: project_id.map(str::to_owned),
            key: KEY_UI_THEME.to_owned(),
            value_kind: VALUE_KIND_ENUM.to_owned(),
            value_json: json_string(theme.as_str()),
            updated_by,
        })
    }

    /// Convenience: record whether first-run onboarding has been dismissed.
    ///
    /// Global scope only in practice — "this person has seen the guidance" is a
    /// fact about the install, not about one project.
    pub fn upsert_onboarding_dismissed(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        dismissed: bool,
        updated_by: SettingsUpdatedBy,
    ) -> Result<StoredSetting, SettingsError> {
        self.upsert(UpsertSetting {
            scope,
            project_id: project_id.map(str::to_owned),
            key: KEY_ONBOARDING_DISMISSED.to_owned(),
            value_kind: VALUE_KIND_FLAG.to_owned(),
            value_json: if dismissed { "true" } else { "false" }.to_owned(),
            updated_by,
        })
    }

    /// Resolve the onboarding flag. Absent means **not** dismissed, so a fresh
    /// install shows guidance rather than silently skipping it.
    pub fn resolve_onboarding_dismissed(
        &self,
        project_id: Option<&str>,
    ) -> Result<ResolvedSetting<bool>, SettingsError> {
        for (scope, id) in [
            (SettingScope::Project, project_id),
            (SettingScope::Global, None),
        ] {
            if scope == SettingScope::Project && id.is_none() {
                continue;
            }
            if let Some(row) = self.get(scope, id, KEY_ONBOARDING_DISMISSED)? {
                let value = match row.value_json.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(SettingsError::InvalidValueJson {
                            key: KEY_ONBOARDING_DISMISSED.to_owned(),
                        });
                    }
                };
                return Ok(ResolvedSetting {
                    value,
                    source: match scope {
                        SettingScope::Project => SettingSource::Project,
                        SettingScope::Global => SettingSource::Global,
                    },
                });
            }
        }
        Ok(ResolvedSetting {
            value: false,
            source: SettingSource::Default,
        })
    }

    /// Delete one key at a scope.
    pub fn delete(
        &self,
        scope: SettingScope,
        project_id: Option<&str>,
        key: &str,
    ) -> Result<bool, SettingsError> {
        validate_scope_shape(scope, project_id)?;
        let connection = self.connection()?;
        let changed = match scope {
            SettingScope::Global => connection
                .execute(
                    "DELETE FROM settings_entries
                     WHERE scope = 'global' AND project_id IS NULL AND key = ?1",
                    params![key],
                )
                .map_err(map_sqlite)?,
            SettingScope::Project => {
                let project_id = project_id.expect("validated");
                connection
                    .execute(
                        "DELETE FROM settings_entries
                         WHERE scope = 'project' AND project_id = ?1 AND key = ?2",
                        params![project_id, key],
                    )
                    .map_err(map_sqlite)?
            }
        };
        Ok(changed > 0)
    }

    /// Delete every setting for a project scope.
    pub fn delete_project_scope(&self, project_id: &str) -> Result<usize, SettingsError> {
        if project_id.is_empty() {
            return Err(SettingsError::ScopeShape);
        }
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "DELETE FROM settings_entries WHERE scope = 'project' AND project_id = ?1",
                params![project_id],
            )
            .map_err(map_sqlite)?;
        Ok(changed)
    }

    /// Resolve S0 runtime settings with documented precedence.
    ///
    /// `env` is the allow-list source (tests inject a map; production uses
    /// [`process_env_map`]). Only the allow-listed env names are consulted.
    pub fn resolve_runtime(
        &self,
        project_id: Option<&str>,
        env: &HashMap<String, String>,
    ) -> Result<ResolvedRuntimeSettings, SettingsError> {
        Ok(ResolvedRuntimeSettings {
            adapter: self.resolve_adapter(project_id, env)?,
            grok_executable: self.resolve_path_setting(
                KEY_GROK_EXECUTABLE,
                ENV_GROK_EXECUTABLE,
                project_id,
                env,
            )?,
            grok_profile: self.resolve_path_setting(
                KEY_GROK_PROFILE,
                ENV_GROK_PROFILE,
                project_id,
                env,
            )?,
        })
    }

    /// Resolve `ui.theme` with the same precedence chain as runtime settings:
    /// `env → project → global → default`.
    ///
    /// Product UI only exposes global scope for theme, but resolution reuses the
    /// full key/scope/precedence machinery (no parallel theme store).
    pub fn resolve_theme(
        &self,
        project_id: Option<&str>,
        env: &HashMap<String, String>,
    ) -> Result<ResolvedSetting<UiThemeSetting>, SettingsError> {
        if let Some(raw) = env.get(ENV_UI_THEME) {
            let value = UiThemeSetting::parse(raw)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Env,
            });
        }

        if let Some(project_id) = project_id
            && let Some(row) = self.get(SettingScope::Project, Some(project_id), KEY_UI_THEME)?
        {
            let value = parse_stored_theme(&row)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Project,
            });
        }

        if let Some(row) = self.get(SettingScope::Global, None, KEY_UI_THEME)? {
            let value = parse_stored_theme(&row)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Global,
            });
        }

        Ok(ResolvedSetting {
            value: UiThemeSetting::default_value(),
            source: SettingSource::Default,
        })
    }

    fn resolve_adapter(
        &self,
        project_id: Option<&str>,
        env: &HashMap<String, String>,
    ) -> Result<ResolvedSetting<RuntimeAdapterSetting>, SettingsError> {
        if let Some(raw) = env.get(ENV_RUNTIME_ADAPTER) {
            // Present env key always wins, even if empty (empty → default parse).
            let value = RuntimeAdapterSetting::parse(raw)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Env,
            });
        }

        if let Some(project_id) = project_id
            && let Some(row) =
                self.get(SettingScope::Project, Some(project_id), KEY_RUNTIME_ADAPTER)?
        {
            let value = parse_stored_adapter(&row)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Project,
            });
        }

        if let Some(row) = self.get(SettingScope::Global, None, KEY_RUNTIME_ADAPTER)? {
            let value = parse_stored_adapter(&row)?;
            return Ok(ResolvedSetting {
                value,
                source: SettingSource::Global,
            });
        }

        Ok(ResolvedSetting {
            value: RuntimeAdapterSetting::default_value(),
            source: SettingSource::Default,
        })
    }

    fn resolve_path_setting(
        &self,
        key: &str,
        env_name: &str,
        project_id: Option<&str>,
        env: &HashMap<String, String>,
    ) -> Result<ResolvedSetting<Option<String>>, SettingsError> {
        if let Some(raw) = env.get(env_name) {
            let path = raw.trim();
            if path.is_empty() {
                return Err(SettingsError::InvalidPathValue {
                    key: key.to_owned(),
                });
            }
            return Ok(ResolvedSetting {
                value: Some(path.to_owned()),
                source: SettingSource::Env,
            });
        }

        if let Some(project_id) = project_id
            && let Some(row) = self.get(SettingScope::Project, Some(project_id), key)?
        {
            let path = parse_stored_path(&row)?;
            return Ok(ResolvedSetting {
                value: Some(path),
                source: SettingSource::Project,
            });
        }

        if let Some(row) = self.get(SettingScope::Global, None, key)? {
            let path = parse_stored_path(&row)?;
            return Ok(ResolvedSetting {
                value: Some(path),
                source: SettingSource::Global,
            });
        }

        Ok(ResolvedSetting {
            value: None,
            source: SettingSource::Default,
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, SettingsError> {
        self.connection
            .lock()
            .map_err(|_| SettingsError::LockPoisoned)
    }
}

/// Snapshot of allow-listed settings env vars from the process environment.
///
/// Only these names are read; presence (even empty) is recorded so resolve can
/// treat them as explicit overrides.
pub fn process_env_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for name in [
        ENV_RUNTIME_ADAPTER,
        ENV_GROK_EXECUTABLE,
        ENV_GROK_PROFILE,
        ENV_UI_THEME,
    ] {
        if let Ok(value) = std::env::var(name) {
            map.insert(name.to_owned(), value);
        }
    }
    map
}

fn validate_scope_shape(
    scope: SettingScope,
    project_id: Option<&str>,
) -> Result<(), SettingsError> {
    match (scope, project_id) {
        (SettingScope::Global, None) => Ok(()),
        (SettingScope::Project, Some(id)) if !id.is_empty() => Ok(()),
        _ => Err(SettingsError::ScopeShape),
    }
}

fn validate_known_key_write(input: &UpsertSetting) -> Result<(), SettingsError> {
    match input.key.as_str() {
        KEY_RUNTIME_ADAPTER => {
            if input.value_kind != VALUE_KIND_ENUM {
                return Err(SettingsError::InvalidValueKind {
                    key: input.key.clone(),
                    kind: input.value_kind.clone(),
                });
            }
            let label = parse_json_string(&input.value_json).map_err(|_| {
                SettingsError::InvalidValueJson {
                    key: input.key.clone(),
                }
            })?;
            // Fail closed on write as well; never store free-form adapter labels.
            let _ = RuntimeAdapterSetting::parse(&label)?;
            Ok(())
        }
        KEY_UI_THEME => {
            if input.value_kind != VALUE_KIND_ENUM {
                return Err(SettingsError::InvalidValueKind {
                    key: input.key.clone(),
                    kind: input.value_kind.clone(),
                });
            }
            let label = parse_json_string(&input.value_json).map_err(|_| {
                SettingsError::InvalidValueJson {
                    key: input.key.clone(),
                }
            })?;
            // Fail closed on write; never store free-form or system themes.
            let _ = UiThemeSetting::parse(&label)?;
            Ok(())
        }
        KEY_ONBOARDING_DISMISSED => {
            if input.value_kind != VALUE_KIND_FLAG {
                return Err(SettingsError::InvalidValueKind {
                    key: input.key.clone(),
                    kind: input.value_kind.clone(),
                });
            }
            // Only the two literals. A free-form flag would let a typo read as
            // "not dismissed" forever, which is the failure the user notices.
            match input.value_json.as_str() {
                "true" | "false" => Ok(()),
                _ => Err(SettingsError::InvalidValueJson {
                    key: input.key.clone(),
                }),
            }
        }
        KEY_GROK_EXECUTABLE | KEY_GROK_PROFILE => {
            if input.value_kind != VALUE_KIND_PATH {
                return Err(SettingsError::InvalidValueKind {
                    key: input.key.clone(),
                    kind: input.value_kind.clone(),
                });
            }
            let path = parse_json_string(&input.value_json).map_err(|_| {
                SettingsError::InvalidValueJson {
                    key: input.key.clone(),
                }
            })?;
            validate_path_setting_value(&input.key, &path)
        }
        other => Err(SettingsError::UnknownKey {
            key: other.to_owned(),
        }),
    }
}

fn parse_stored_adapter(row: &StoredSetting) -> Result<RuntimeAdapterSetting, SettingsError> {
    if row.value_kind != VALUE_KIND_ENUM {
        return Err(SettingsError::InvalidValueKind {
            key: row.key.clone(),
            kind: row.value_kind.clone(),
        });
    }
    let label =
        parse_json_string(&row.value_json).map_err(|_| SettingsError::InvalidValueJson {
            key: row.key.clone(),
        })?;
    // Hard requirement: illegal stored enum fails closed; never Fake fallback.
    RuntimeAdapterSetting::parse(&label)
}

fn parse_stored_theme(row: &StoredSetting) -> Result<UiThemeSetting, SettingsError> {
    if row.value_kind != VALUE_KIND_ENUM {
        return Err(SettingsError::InvalidValueKind {
            key: row.key.clone(),
            kind: row.value_kind.clone(),
        });
    }
    let label =
        parse_json_string(&row.value_json).map_err(|_| SettingsError::InvalidValueJson {
            key: row.key.clone(),
        })?;
    // Hard requirement: illegal stored enum fails closed; never silent dark.
    UiThemeSetting::parse(&label)
}

fn parse_stored_path(row: &StoredSetting) -> Result<String, SettingsError> {
    if row.value_kind != VALUE_KIND_PATH {
        return Err(SettingsError::InvalidValueKind {
            key: row.key.clone(),
            kind: row.value_kind.clone(),
        });
    }
    let path = parse_json_string(&row.value_json).map_err(|_| SettingsError::InvalidValueJson {
        key: row.key.clone(),
    })?;
    if path.trim().is_empty() {
        return Err(SettingsError::InvalidPathValue {
            key: row.key.clone(),
        });
    }
    Ok(path)
}

/// Hard-reject path writes that would brick startup after restart.
///
/// - `runtime.grok.executable`: must be an **existing file**
/// - `runtime.grok.profile`: must be an **existing directory**
///
/// Empty / oversize paths keep the existing `settings_invalid_path` code.
/// Existence failures use the same code with an actionable message (expected
/// vs current). Intentionally not a soft warning — see product self-brick fix.
fn validate_path_setting_value(key: &str, raw: &str) -> Result<(), SettingsError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err(SettingsError::InvalidPathValue {
            key: key.to_owned(),
        });
    }
    let path = PathBuf::from(trimmed);
    match key {
        KEY_GROK_EXECUTABLE => {
            if path.is_file() {
                Ok(())
            } else {
                Err(SettingsError::Coded {
                    code: "settings_invalid_path",
                    message: format!(
                        "Grok executable path must be an existing file. \
                         Expected: a file on disk that Pillow can launch. \
                         Current value: `{trimmed}` (not an existing file). \
                         Install the Grok CLI or choose the correct path, then save again."
                    ),
                })
            }
        }
        KEY_GROK_PROFILE => {
            if path.is_dir() {
                Ok(())
            } else {
                Err(SettingsError::Coded {
                    code: "settings_invalid_path",
                    message: format!(
                        "Grok profile path must be an existing directory. \
                         Expected: a process-home directory that already exists. \
                         Current value: `{trimmed}` (not an existing directory). \
                         Create the directory or choose an existing profile path, then save again."
                    ),
                })
            }
        }
        _ => Err(SettingsError::InvalidPathValue {
            key: key.to_owned(),
        }),
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("string always serializes")
}

fn parse_json_string(value_json: &str) -> Result<String, ()> {
    serde_json::from_str::<String>(value_json).map_err(|_| ())
}

fn setting_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSetting> {
    let setting_id: String = row.get(0)?;
    let scope_raw: String = row.get(1)?;
    let project_id: Option<String> = row.get(2)?;
    let key: String = row.get(3)?;
    let value_kind: String = row.get(4)?;
    let value_json: String = row.get(5)?;
    let updated_at_raw: String = row.get(6)?;
    let updated_by: String = row.get(7)?;

    let scope = SettingScope::parse(&scope_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_raw)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(StoredSetting {
        setting_id,
        scope,
        project_id,
        key,
        value_kind,
        value_json,
        updated_at,
        updated_by,
    })
}

fn map_sqlite(_: rusqlite::Error) -> SettingsError {
    SettingsError::Sqlite
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn adapter_parse_accepts_known_labels() {
        assert_eq!(
            RuntimeAdapterSetting::parse("grok-acp").unwrap(),
            RuntimeAdapterSetting::GrokAcp
        );
        assert_eq!(
            RuntimeAdapterSetting::parse("FAKE").unwrap(),
            RuntimeAdapterSetting::Fake
        );
        assert_eq!(
            RuntimeAdapterSetting::parse("fixture").unwrap(),
            RuntimeAdapterSetting::Fake
        );
        assert_eq!(
            RuntimeAdapterSetting::parse("").unwrap(),
            RuntimeAdapterSetting::GrokAcp
        );
    }

    #[test]
    fn adapter_parse_rejects_unknown_without_fake_fallback() {
        let err = RuntimeAdapterSetting::parse("auto").unwrap_err();
        assert!(matches!(err, SettingsError::InvalidRuntimeSelection { .. }));
        assert_eq!(err.code(), "invalid_runtime_selection");
        // Fail-closed: Result is Err, never Ok(Fake).
        assert!(RuntimeAdapterSetting::parse("codex").is_err());
        assert!(RuntimeAdapterSetting::parse("auto").is_err());
    }

    #[test]
    fn theme_parse_accepts_dark_and_light_only() {
        assert_eq!(UiThemeSetting::parse("dark").unwrap(), UiThemeSetting::Dark);
        assert_eq!(
            UiThemeSetting::parse("LIGHT").unwrap(),
            UiThemeSetting::Light
        );
        assert_eq!(UiThemeSetting::default_value(), UiThemeSetting::Dark);
    }

    #[test]
    fn theme_parse_rejects_system_and_unknown_without_dark_fallback() {
        let err = UiThemeSetting::parse("system").unwrap_err();
        assert!(matches!(err, SettingsError::InvalidThemeSelection { .. }));
        assert_eq!(err.code(), "invalid_theme_selection");
        assert!(UiThemeSetting::parse("auto").is_err());
        assert!(UiThemeSetting::parse("").is_err());
        // Fail-closed: Result is Err, never Ok(Dark) for illegal input.
        assert!(!matches!(
            UiThemeSetting::parse("system"),
            Ok(UiThemeSetting::Dark)
        ));
    }

    #[test]
    fn path_validation_rejects_missing_executable_file() {
        let err = validate_path_setting_value(
            KEY_GROK_EXECUTABLE,
            r"C:\no-such-pillow-grok-binary-xyz.exe",
        )
        .unwrap_err();
        assert_eq!(err.code(), "settings_invalid_path");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(msg.contains("existing file"), "{msg}");
        assert!(msg.contains("no-such-pillow-grok-binary-xyz"), "{msg}");
    }

    #[test]
    fn path_validation_rejects_missing_profile_directory() {
        let err =
            validate_path_setting_value(KEY_GROK_PROFILE, r"C:\no-such-pillow-profile-dir-xyz")
                .unwrap_err();
        assert_eq!(err.code(), "settings_invalid_path");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(msg.contains("existing directory"), "{msg}");
        assert!(msg.contains("no-such-pillow-profile-dir-xyz"), "{msg}");
    }

    #[test]
    fn path_validation_accepts_existing_file_and_directory() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("grok-placeholder.exe");
        std::fs::write(&exe, b"").unwrap();
        let profile = temp.path().join("profile-home");
        std::fs::create_dir_all(&profile).unwrap();

        validate_path_setting_value(KEY_GROK_EXECUTABLE, &exe.to_string_lossy()).unwrap();
        validate_path_setting_value(KEY_GROK_PROFILE, &profile.to_string_lossy()).unwrap();
    }
}
