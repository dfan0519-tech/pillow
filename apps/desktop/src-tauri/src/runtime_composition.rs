//! Desktop-only runtime composition: select Fake or Grok ACP and build the
//! conversation service. Provider wire types stay in the adapter crate; this
//! module is the sole product dependency on `keeper-adapter-grok-acp`.
//!
//! Runtime selection and path authority (S0):
//! ```text
//! env(allow-list) → project → global → default
//! ```
//! Resolution is owned by `keeper_storage_sqlite::SqliteSettingsRepository::
//! resolve_runtime`. This module maps resolved values into adapter construction
//! and **must not** re-implement the adapter label parser.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::AgentRuntimeAdapter;
use keeper_runtime_host::HostConfig;
use keeper_storage_sqlite::{
    ResolvedRuntimeSettings, RuntimeAdapterSetting, SettingsError, SqliteSettingsRepository,
    process_env_map,
};

use crate::conversation_events::RuntimeEventEmitter;
use crate::conversation_service::{ConversationCommandError, ConversationService};

/// Environment variable selecting the desktop runtime adapter.
///
/// - unset / `grok` / `grok-acp` → real Grok ACP (production default)
/// - `fake` → deterministic fake (explicit development / test only)
///
/// Same allow-list name as storage `ENV_RUNTIME_ADAPTER` / `PILLOW_RUNTIME`.
pub const ENV_RUNTIME_SELECTION: &str = "PILLOW_RUNTIME";

/// Non-secret path to the Grok CLI executable.
pub const ENV_GROK_EXECUTABLE: &str = "PILLOW_GROK_EXECUTABLE";

/// Process home for the Grok child (`HOME` / `USERPROFILE` override).
/// Path only — never open or inspect auth file contents or account identity.
///
/// When unset, the current-user home is resolved from `USERPROFILE` (Windows)
/// or `HOME` (other platforms). A missing home fails startup with
/// `runtime_profile_missing` — never silent Fake fallback.
pub const ENV_GROK_PROFILE: &str = "PILLOW_GROK_PROFILE";

/// First industrial Grok connection: one concurrent prompt, no fixture delays.
pub fn production_grok_host_config() -> HostConfig {
    HostConfig::new(1, false, 64)
}

/// Which adapter the desktop process will compose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSelection {
    /// Real `GrokAcpAdapter` (production default).
    GrokAcp,
    /// Deterministic fake — only when explicitly selected.
    Fake,
}

impl RuntimeSelection {
    /// Parse a selection string. Unknown values fail closed (no silent fake).
    ///
    /// **Single source of truth:** label acceptance is owned by
    /// [`RuntimeAdapterSetting::parse`] in `keeper-storage-sqlite`. This method
    /// only maps the storage enum into the desktop composition type.
    pub fn parse(raw: &str) -> Result<Self, ConversationCommandError> {
        match RuntimeAdapterSetting::parse(raw) {
            Ok(adapter) => Ok(Self::from_adapter(adapter)),
            Err(SettingsError::InvalidRuntimeSelection { value }) => {
                Err(ConversationCommandError::new(
                    "invalid_runtime_selection",
                    format!(
                        "Unknown runtime selection `{value}`. Use `grok-acp` (default) or explicit `fake`."
                    ),
                ))
            }
            Err(other) => Err(ConversationCommandError::new(
                other.code(),
                other.to_string(),
            )),
        }
    }

    /// Map the storage-layer adapter enum into desktop composition selection.
    pub fn from_adapter(adapter: RuntimeAdapterSetting) -> Self {
        match adapter {
            RuntimeAdapterSetting::GrokAcp => Self::GrokAcp,
            RuntimeAdapterSetting::Fake => Self::Fake,
        }
    }

    /// Read [`ENV_RUNTIME_SELECTION`]. Unset defaults to [`Self::GrokAcp`].
    ///
    /// Fake is **never** chosen as a silent fallback when Grok is missing.
    /// Prefer [`resolve_runtime_selection_from_store`] at process startup so
    /// SQLite global/project settings participate; this helper remains for
    /// env-only probes and focused tests.
    pub fn from_env() -> Result<Self, ConversationCommandError> {
        match std::env::var(ENV_RUNTIME_SELECTION) {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::GrokAcp),
            Err(std::env::VarError::NotUnicode(_)) => Err(ConversationCommandError::new(
                "invalid_runtime_selection",
                format!("{ENV_RUNTIME_SELECTION} must be valid UTF-8."),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GrokAcp => "grok-acp",
            Self::Fake => "fake",
        }
    }
}

/// Resolve S0 runtime settings from SQLite + allow-listed env.
///
/// `project_id` is optional; process startup passes `None` (global + env +
/// default). Invalid stored/env adapter labels fail closed.
pub fn resolve_runtime_settings(
    db_path: impl AsRef<Path>,
    project_id: Option<&str>,
) -> Result<ResolvedRuntimeSettings, ConversationCommandError> {
    let repo = SqliteSettingsRepository::open(db_path.as_ref()).map_err(map_settings_error)?;
    repo.resolve_runtime(project_id, &process_env_map())
        .map_err(map_settings_error)
}

/// Startup selection from the settings store (env still wins via resolve).
pub fn resolve_runtime_selection_from_store(
    db_path: impl AsRef<Path>,
) -> Result<(RuntimeSelection, ResolvedRuntimeSettings), ConversationCommandError> {
    let resolved = resolve_runtime_settings(db_path, None)?;
    Ok((
        RuntimeSelection::from_adapter(resolved.adapter.value),
        resolved,
    ))
}

fn map_settings_error(error: SettingsError) -> ConversationCommandError {
    ConversationCommandError::new(error.code(), error.to_string())
}

/// Open the conversation service for process startup.
///
/// Reads resolved settings (env → global → default). Production path composes
/// `GrokAcpAdapter`. Fake requires explicit selection via env or stored setting.
pub fn open_conversation_service(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    let (selection, resolved) = resolve_runtime_selection_from_store(db_path.as_ref())?;
    open_conversation_service_with_resolved(db_path, emitter, selection, &resolved)
}

/// Compose with an explicit selection (tests / controlled harnesses).
///
/// Path discovery still uses env / PATH / home (legacy test seam). Production
/// startup goes through [`open_conversation_service`] so stored paths apply.
pub fn open_conversation_service_with_selection(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    selection: RuntimeSelection,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    match selection {
        RuntimeSelection::Fake => {
            let service = ConversationService::open(db_path, emitter, true)?;
            Ok((service, RuntimeSelection::Fake))
        }
        RuntimeSelection::GrokAcp => {
            let config = discover_grok_config()?;
            open_with_grok_adapter(db_path, emitter, config)
        }
    }
}

fn open_conversation_service_with_resolved(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    selection: RuntimeSelection,
    resolved: &ResolvedRuntimeSettings,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    match selection {
        RuntimeSelection::Fake => {
            let service = ConversationService::open(db_path, emitter, true)?;
            Ok((service, RuntimeSelection::Fake))
        }
        RuntimeSelection::GrokAcp => {
            let config = discover_grok_config_with_overrides(
                resolved.grok_executable.value.as_deref(),
                resolved.grok_profile.value.as_deref(),
            )?;
            open_with_grok_adapter(db_path, emitter, config)
        }
    }
}

fn open_with_grok_adapter(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    config: GrokAcpConfig,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    let adapter = GrokAcpAdapter::new(config).map_err(|error| {
        ConversationCommandError::new(
            "runtime_adapter_unavailable",
            format!("Failed to construct Grok ACP adapter: {error}"),
        )
    })?;
    let adapter: Arc<dyn AgentRuntimeAdapter> = Arc::new(adapter);
    let service = ConversationService::open_with_adapter(
        db_path,
        emitter,
        adapter,
        production_grok_host_config(),
    )?;
    Ok((service, RuntimeSelection::GrokAcp))
}

/// Discover non-secret Grok launch configuration from env / PATH / home.
///
/// Prefer [`discover_grok_config_with_overrides`] when values already came from
/// `resolve_runtime` so env is not consulted twice under different rules.
pub fn discover_grok_config() -> Result<GrokAcpConfig, ConversationCommandError> {
    let executable = resolve_grok_executable_from_env()?;
    let profile = resolve_grok_profile_from_env()?;
    finish_grok_config(executable, profile)
}

/// Build Grok config from already-resolved path strings (SQLite + env chain).
///
/// - `Some(path)` → use that path (existence checked; fail-closed)
/// - `None` → PATH discovery for executable / current-user home for profile
///
/// Does **not** re-read allow-listed env vars; callers pass values from
/// `resolve_runtime` so env still wins exactly once at the settings layer.
pub fn discover_grok_config_with_overrides(
    executable_override: Option<&str>,
    profile_override: Option<&str>,
) -> Result<GrokAcpConfig, ConversationCommandError> {
    let executable = match executable_override {
        Some(raw) => require_executable_file(raw)?,
        None => find_grok_on_path().ok_or_else(|| {
            ConversationCommandError::new(
                "runtime_executable_missing",
                format!(
                    "Grok executable not found on PATH. Set {ENV_GROK_EXECUTABLE} or a settings path to the Grok CLI, or set {ENV_RUNTIME_SELECTION}=fake for explicit development mode."
                ),
            )
        })?,
    };
    let profile = match profile_override {
        Some(raw) => require_profile_dir(raw)?,
        None => current_user_home_dir().ok_or_else(|| {
            ConversationCommandError::new(
                "runtime_profile_missing",
                format!(
                    "Grok process home could not be resolved. Set {ENV_GROK_PROFILE} or a settings profile path to an existing directory, or ensure the current-user home ({}) exists. Real runtime never falls back to Fake.",
                    if cfg!(windows) { "USERPROFILE" } else { "HOME" }
                ),
            )
        })?,
    };
    finish_grok_config(executable, profile)
}

fn finish_grok_config(
    executable: PathBuf,
    profile: PathBuf,
) -> Result<GrokAcpConfig, ConversationCommandError> {
    let mut config = GrokAcpConfig::new(executable);
    // Always pass a home path to the child. GrokAcpAdapter clears parent
    // HOME/USERPROFILE unless honor_isolated_home is set.
    config.honor_isolated_home = Some(profile);
    config.validate().map_err(|error| {
        ConversationCommandError::new(
            "runtime_config_invalid",
            format!("Grok ACP configuration is invalid: {error}"),
        )
    })?;
    Ok(config)
}

fn require_executable_file(raw: &str) -> Result<PathBuf, ConversationCommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConversationCommandError::new(
            "runtime_executable_missing",
            format!("{ENV_GROK_EXECUTABLE} is empty."),
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_file() {
        return Err(ConversationCommandError::new(
            "runtime_executable_missing",
            format!(
                "Grok executable is not a file: {}. Set {ENV_GROK_EXECUTABLE} or install `grok` on PATH.",
                path.display()
            ),
        ));
    }
    Ok(path)
}

fn require_profile_dir(raw: &str) -> Result<PathBuf, ConversationCommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConversationCommandError::new(
            "runtime_profile_invalid",
            format!("{ENV_GROK_PROFILE} is empty."),
        ));
    }
    let path = PathBuf::from(trimmed);
    // Path existence only — never open auth files inside the profile.
    if !path.is_dir() {
        return Err(ConversationCommandError::new(
            "runtime_profile_invalid",
            format!(
                "Grok profile path must be an existing directory: {}",
                path.display()
            ),
        ));
    }
    Ok(path)
}

fn resolve_grok_executable_from_env() -> Result<PathBuf, ConversationCommandError> {
    match std::env::var(ENV_GROK_EXECUTABLE) {
        Ok(raw) => require_executable_file(&raw),
        Err(std::env::VarError::NotPresent) => find_grok_on_path().ok_or_else(|| {
            ConversationCommandError::new(
                "runtime_executable_missing",
                format!(
                    "Grok executable not found on PATH. Set {ENV_GROK_EXECUTABLE} to the Grok CLI, or set {ENV_RUNTIME_SELECTION}=fake for explicit development mode."
                ),
            )
        }),
        Err(std::env::VarError::NotUnicode(_)) => Err(ConversationCommandError::new(
            "runtime_executable_missing",
            format!("{ENV_GROK_EXECUTABLE} must be valid UTF-8."),
        )),
    }
}

/// Resolve the process home path passed to the Grok child (env / platform home).
///
/// Priority: explicit `PILLOW_GROK_PROFILE` → platform current-user home.
/// Existence check only — never open auth files or inspect account identity.
fn resolve_grok_profile_from_env() -> Result<PathBuf, ConversationCommandError> {
    match std::env::var(ENV_GROK_PROFILE) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return require_profile_dir(trimmed);
            }
            // Empty explicit value falls through to current-user home.
        }
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(ConversationCommandError::new(
                "runtime_profile_invalid",
                format!("{ENV_GROK_PROFILE} must be valid UTF-8."),
            ));
        }
    }

    if let Some(home) = current_user_home_dir() {
        return Ok(home);
    }

    Err(ConversationCommandError::new(
        "runtime_profile_missing",
        format!(
            "Grok process home could not be resolved. Set {ENV_GROK_PROFILE} to an existing directory, or ensure the current-user home ({}) exists. Real runtime never falls back to Fake.",
            if cfg!(windows) { "USERPROFILE" } else { "HOME" }
        ),
    ))
}

/// Current-user home from the platform env var, only when it is an existing directory.
fn current_user_home_dir() -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let value = std::env::var_os(key)?;
    let path = PathBuf::from(value);
    if path.is_dir() { Some(path) } else { None }
}

/// Locate `grok` (or `grok.exe` on Windows) on `PATH`.
fn find_grok_on_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("grok");
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join("grok.exe");
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use keeper_storage_sqlite::{
        ENV_RUNTIME_ADAPTER, KEY_GROK_EXECUTABLE, SettingScope, SettingSource, SettingsUpdatedBy,
    };

    use crate::conversation_events::RecordingEmitter;
    use tempfile::tempdir;

    /// Shared across desktop modules that mutate process env in tests so
    /// `settings_commands_tests` and composition tests cannot interleave.
    pub(crate) fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn selection_defaults_to_grok_acp() {
        assert_eq!(
            RuntimeSelection::parse("").unwrap(),
            RuntimeSelection::GrokAcp
        );
        assert_eq!(
            RuntimeSelection::parse("grok-acp").unwrap(),
            RuntimeSelection::GrokAcp
        );
        assert_eq!(
            RuntimeSelection::parse("GROK").unwrap(),
            RuntimeSelection::GrokAcp
        );
    }

    #[test]
    fn selection_requires_explicit_fake() {
        assert_eq!(
            RuntimeSelection::parse("fake").unwrap(),
            RuntimeSelection::Fake
        );
        assert_eq!(
            RuntimeSelection::parse("fixture").unwrap(),
            RuntimeSelection::Fake
        );
        let err = RuntimeSelection::parse("auto").unwrap_err();
        assert_eq!(err.code, "invalid_runtime_selection");
        assert!(!err.message.to_ascii_lowercase().contains("falling back"));
    }

    /// Mechanical drift capture: desktop parse must match storage parse for
    /// every label. If someone edits only one side, this test fails.
    #[test]
    fn runtime_selection_parse_matches_storage_adapter_parse() {
        let labels = [
            "",
            "   ",
            "grok",
            "GROK",
            "grok-acp",
            "Grok-Acp",
            "acp",
            "ACP",
            "fake",
            "FAKE",
            "fixture",
            "  fake  ",
            "auto",
            "codex",
            "unknown-runtime",
            "claude",
            "not-a-runtime",
        ];
        for label in labels {
            let storage = RuntimeAdapterSetting::parse(label);
            let desktop = RuntimeSelection::parse(label);
            match (storage, desktop) {
                (Ok(adapter), Ok(selection)) => {
                    assert_eq!(
                        RuntimeSelection::from_adapter(adapter),
                        selection,
                        "accept mismatch for label {label:?}"
                    );
                    assert_eq!(adapter.as_str(), selection.as_str());
                }
                (Err(storage_err), Err(desktop_err)) => {
                    assert_eq!(
                        storage_err.code(),
                        desktop_err.code,
                        "error code drift for label {label:?}"
                    );
                    assert_eq!(
                        storage_err.code(),
                        "invalid_runtime_selection",
                        "fail-closed code must stay invalid_runtime_selection for {label:?}"
                    );
                    // Neither path may produce Fake on reject.
                    assert!(
                        !matches!(
                            RuntimeAdapterSetting::parse(label),
                            Ok(RuntimeAdapterSetting::Fake)
                        ),
                        "storage must not Ok(Fake) for {label:?}"
                    );
                }
                (Ok(adapter), Err(desktop_err)) => {
                    panic!(
                        "drift: storage Ok({adapter:?}) but desktop Err({}) for {label:?}",
                        desktop_err.code
                    );
                }
                (Err(storage_err), Ok(selection)) => {
                    panic!(
                        "drift: storage Err({}) but desktop Ok({selection:?}) for {label:?}",
                        storage_err.code()
                    );
                }
            }
        }
    }

    #[test]
    fn from_env_defaults_to_grok_never_silent_fake() {
        let _guard = env_lock().lock().unwrap();
        // SAFETY: serialized via env_lock for this test module.
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
        assert_eq!(
            RuntimeSelection::from_env().unwrap(),
            RuntimeSelection::GrokAcp
        );
        unsafe {
            std::env::set_var(ENV_RUNTIME_SELECTION, "fake");
        }
        assert_eq!(
            RuntimeSelection::from_env().unwrap(),
            RuntimeSelection::Fake
        );
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
    }

    #[test]
    fn no_stored_settings_behaves_like_env_only_default() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        // Ensure allow-listed runtime env is unset so store defaults apply.
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }
        let (selection, resolved) = resolve_runtime_selection_from_store(&db).unwrap();
        assert_eq!(selection, RuntimeSelection::GrokAcp);
        assert_eq!(resolved.adapter.source, SettingSource::Default);
        assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::GrokAcp);
        assert_eq!(resolved.grok_executable.value, None);
        assert_eq!(resolved.grok_executable.source, SettingSource::Default);
        assert_eq!(resolved.grok_profile.value, None);
        assert_eq!(resolved.grok_profile.source, SettingSource::Default);
    }

    #[test]
    fn env_adapter_wins_over_stored_value() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        // Store Fake; env must still force GrokAcp (E2E life-line).
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::Fake,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);

        unsafe {
            std::env::set_var(ENV_RUNTIME_SELECTION, "grok-acp");
        }
        let (selection, resolved) = resolve_runtime_selection_from_store(&db).unwrap();
        assert_eq!(selection, RuntimeSelection::GrokAcp);
        assert_eq!(resolved.adapter.source, SettingSource::Env);
        assert_eq!(resolved.adapter.value, RuntimeAdapterSetting::GrokAcp);

        // Also prove Fake env wins over stored GrokAcp (symmetric).
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);
        unsafe {
            std::env::set_var(ENV_RUNTIME_SELECTION, "fake");
        }
        let (selection, resolved) = resolve_runtime_selection_from_store(&db).unwrap();
        assert_eq!(selection, RuntimeSelection::Fake);
        assert_eq!(resolved.adapter.source, SettingSource::Env);

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
    }

    #[test]
    fn stored_adapter_used_when_env_unset() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::Fake,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);

        let (selection, resolved) = resolve_runtime_selection_from_store(&db).unwrap();
        assert_eq!(selection, RuntimeSelection::Fake);
        assert_eq!(resolved.adapter.source, SettingSource::Global);
    }

    #[test]
    fn illegal_env_adapter_fails_closed_never_fake() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        unsafe {
            std::env::set_var(ENV_RUNTIME_SELECTION, "not-a-runtime");
        }
        let err = resolve_runtime_selection_from_store(&db).unwrap_err();
        assert_eq!(err.code, "invalid_runtime_selection");
        assert!(!err.message.to_ascii_lowercase().contains("falling back"));
        assert!(!err.message.to_ascii_lowercase().contains("using fake"));

        // open_conversation_service must also fail closed (not Fake).
        let emitter = Arc::new(RecordingEmitter::new());
        let open_err = match open_conversation_service(&db, emitter as Arc<_>) {
            Ok(_) => panic!("illegal env adapter must not open service"),
            Err(error) => error,
        };
        assert_eq!(open_err.code, "invalid_runtime_selection");

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
    }

    #[test]
    fn illegal_label_parse_never_returns_fake() {
        for label in ["auto", "codex", "not-a-runtime", "claude"] {
            let err = RuntimeSelection::parse(label).unwrap_err();
            assert_eq!(err.code, "invalid_runtime_selection");
            // Symmetric storage path (drift already covered; assert fail-closed).
            assert!(RuntimeAdapterSetting::parse(label).is_err());
        }
    }

    #[test]
    fn explicit_fake_opens_without_grok_binary() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let emitter = Arc::new(RecordingEmitter::new());
        let (service, selection) = open_conversation_service_with_selection(
            &db,
            Arc::clone(&emitter) as Arc<_>,
            RuntimeSelection::Fake,
        )
        .expect("explicit fake must open without Grok");
        assert_eq!(selection, RuntimeSelection::Fake);
        let probe = service.probe().unwrap();
        assert_eq!(probe.adapter_name, "fake");
        service.shutdown().unwrap();
    }

    #[test]
    fn open_service_respects_env_fake_over_stored_grok() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);
        unsafe {
            std::env::set_var(ENV_RUNTIME_SELECTION, "fake");
        }
        let emitter = Arc::new(RecordingEmitter::new());
        let (service, selection) =
            open_conversation_service(&db, emitter as Arc<_>).expect("env fake must open");
        assert_eq!(selection, RuntimeSelection::Fake);
        let probe = service.probe().unwrap();
        assert_eq!(probe.adapter_name, "fake");
        service.shutdown().unwrap();
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
    }

    #[test]
    fn env_path_overrides_win_over_stored_paths() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let stored_exe = temp.path().join("stored-grok.exe");
        let env_exe = temp.path().join("env-grok.exe");
        let stored_profile = temp.path().join("stored-profile");
        let env_profile = temp.path().join("env-profile");
        std::fs::write(&stored_exe, b"").unwrap();
        std::fs::write(&env_exe, b"").unwrap();
        std::fs::create_dir_all(&stored_profile).unwrap();
        std::fs::create_dir_all(&env_profile).unwrap();

        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &stored_exe.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            keeper_storage_sqlite::KEY_GROK_PROFILE,
            &stored_profile.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
            std::env::set_var(ENV_GROK_EXECUTABLE, &env_exe);
            std::env::set_var(ENV_GROK_PROFILE, &env_profile);
        }

        let resolved = resolve_runtime_settings(&db, None).unwrap();
        assert_eq!(resolved.grok_executable.source, SettingSource::Env);
        assert_eq!(
            resolved.grok_executable.value.as_deref(),
            Some(env_exe.to_string_lossy().as_ref())
        );
        assert_eq!(resolved.grok_profile.source, SettingSource::Env);
        assert_eq!(
            resolved.grok_profile.value.as_deref(),
            Some(env_profile.to_string_lossy().as_ref())
        );

        // Direct resolve_runtime map injection also documents the E2E contract.
        let mut env = HashMap::new();
        env.insert(ENV_RUNTIME_ADAPTER.to_owned(), "fake".to_owned());
        env.insert(
            ENV_GROK_EXECUTABLE.to_owned(),
            env_exe.to_string_lossy().into_owned(),
        );
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        let injected = repo.resolve_runtime(None, &env).unwrap();
        assert_eq!(injected.adapter.source, SettingSource::Env);
        assert_eq!(injected.adapter.value, RuntimeAdapterSetting::Fake);

        unsafe {
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }
    }

    #[test]
    fn grok_selection_fails_truthfully_when_executable_missing() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let missing = temp.path().join("no-such-grok-binary.exe");
        let profile = temp.path().join("profile");
        std::fs::create_dir_all(&profile).unwrap();
        unsafe {
            std::env::set_var(ENV_GROK_EXECUTABLE, &missing);
            std::env::set_var(ENV_GROK_PROFILE, &profile);
        }
        let emitter = Arc::new(RecordingEmitter::new());
        let err = match open_conversation_service_with_selection(
            &db,
            emitter as Arc<_>,
            RuntimeSelection::GrokAcp,
        ) {
            Ok(_) => panic!("must not fall back to fake when Grok executable is missing"),
            Err(error) => error,
        };
        assert_eq!(err.code, "runtime_executable_missing");
        assert!(
            !err.message.to_ascii_lowercase().contains("using fake"),
            "must never silently fall back: {}",
            err.message
        );
        unsafe {
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }
    }

    /// Platform home env key used by [`current_user_home_dir`].
    fn platform_home_env_key() -> &'static str {
        if cfg!(windows) { "USERPROFILE" } else { "HOME" }
    }

    /// Touch a non-secret placeholder file so discover_grok_config can treat it as the executable.
    fn write_placeholder_executable(dir: &Path) -> PathBuf {
        let path = if cfg!(windows) {
            dir.join("grok-placeholder.exe")
        } else {
            dir.join("grok-placeholder")
        };
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn profile_explicit_env_is_highest_priority() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let explicit = temp.path().join("explicit-profile");
        let other_home = temp.path().join("other-home");
        std::fs::create_dir_all(&explicit).unwrap();
        std::fs::create_dir_all(&other_home).unwrap();
        let exe = write_placeholder_executable(temp.path());
        let home_key = platform_home_env_key();
        let previous_home = std::env::var_os(home_key);
        unsafe {
            std::env::set_var(ENV_GROK_EXECUTABLE, &exe);
            std::env::set_var(ENV_GROK_PROFILE, &explicit);
            std::env::set_var(home_key, &other_home);
        }
        let config = discover_grok_config().expect("explicit profile must resolve");
        assert_eq!(
            config.honor_isolated_home.as_deref(),
            Some(explicit.as_path()),
            "PILLOW_GROK_PROFILE must win over platform home"
        );
        unsafe {
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
            match previous_home {
                Some(v) => std::env::set_var(home_key, v),
                None => std::env::remove_var(home_key),
            }
        }
    }

    #[test]
    fn profile_defaults_to_current_user_home_when_unset() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let home = temp.path().join("current-user-home");
        std::fs::create_dir_all(&home).unwrap();
        let exe = write_placeholder_executable(temp.path());
        let home_key = platform_home_env_key();
        let previous_home = std::env::var_os(home_key);
        unsafe {
            std::env::set_var(ENV_GROK_EXECUTABLE, &exe);
            std::env::remove_var(ENV_GROK_PROFILE);
            std::env::set_var(home_key, &home);
        }
        let config = discover_grok_config().expect("default home must resolve");
        assert_eq!(
            config.honor_isolated_home.as_deref(),
            Some(home.as_path()),
            "unset profile must use existing current-user home"
        );
        unsafe {
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            match previous_home {
                Some(v) => std::env::set_var(home_key, v),
                None => std::env::remove_var(home_key),
            }
        }
    }

    #[test]
    fn profile_missing_fails_truthfully_never_fake() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let exe = write_placeholder_executable(temp.path());
        let missing_home = temp.path().join("no-such-home-dir");
        let home_key = platform_home_env_key();
        let previous_home = std::env::var_os(home_key);
        unsafe {
            std::env::set_var(ENV_GROK_EXECUTABLE, &exe);
            std::env::remove_var(ENV_GROK_PROFILE);
            std::env::set_var(home_key, &missing_home);
        }
        let emitter = Arc::new(RecordingEmitter::new());
        let err = match open_conversation_service_with_selection(
            &db,
            emitter as Arc<_>,
            RuntimeSelection::GrokAcp,
        ) {
            Ok(_) => panic!("must not open Grok or Fake when profile home is missing"),
            Err(error) => error,
        };
        assert_eq!(err.code, "runtime_profile_missing");
        assert!(
            !err.message.to_ascii_lowercase().contains("using fake")
                && !err.message.to_ascii_lowercase().contains("falling back"),
            "must never silently fall back to Fake: {}",
            err.message
        );
        // Path-only error; must not leak auth file names or account identity.
        let lower = err.message.to_ascii_lowercase();
        assert!(!lower.contains("auth.json"));
        assert!(!lower.contains("api_key"));
        assert!(!lower.contains("token"));
        unsafe {
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            match previous_home {
                Some(v) => std::env::set_var(home_key, v),
                None => std::env::remove_var(home_key),
            }
        }
    }

    #[test]
    fn production_host_config_is_single_prompt() {
        let config = production_grok_host_config();
        assert_eq!(config.max_concurrent_turns, 1);
        assert!(!config.honor_fixture_delays);
        assert_eq!(config.event_queue_capacity, 64);
    }
}
