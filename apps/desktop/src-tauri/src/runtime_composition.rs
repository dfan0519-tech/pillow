//! Desktop-only runtime composition: select Fake or Grok ACP and build the
//! conversation service. Provider wire types stay in the adapter crate; this
//! module is the sole product dependency on `keeper-adapter-grok-acp`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::AgentRuntimeAdapter;
use keeper_runtime_host::HostConfig;

use crate::conversation_events::RuntimeEventEmitter;
use crate::conversation_service::{ConversationCommandError, ConversationService};

/// Environment variable selecting the desktop runtime adapter.
///
/// - unset / `grok` / `grok-acp` → real Grok ACP (production default)
/// - `fake` → deterministic fake (explicit development / test only)
pub const ENV_RUNTIME_SELECTION: &str = "KEEPER_IDE_RUNTIME";

/// Non-secret path to the Grok CLI executable.
pub const ENV_GROK_EXECUTABLE: &str = "KEEPER_IDE_GROK_EXECUTABLE";

/// Process home for the Grok child (`HOME` / `USERPROFILE` override).
/// Path only — never open or inspect auth file contents or account identity.
///
/// When unset, the current-user home is resolved from `USERPROFILE` (Windows)
/// or `HOME` (other platforms). A missing home fails startup with
/// `runtime_profile_missing` — never silent Fake fallback.
pub const ENV_GROK_PROFILE: &str = "KEEPER_IDE_GROK_PROFILE";

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
    pub fn parse(raw: &str) -> Result<Self, ConversationCommandError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "grok" | "grok-acp" | "acp" => Ok(Self::GrokAcp),
            "fake" | "fixture" => Ok(Self::Fake),
            other => Err(ConversationCommandError::new(
                "invalid_runtime_selection",
                format!(
                    "Unknown runtime selection `{other}`. Use `grok-acp` (default) or explicit `fake`."
                ),
            )),
        }
    }

    /// Read [`ENV_RUNTIME_SELECTION`]. Unset defaults to [`Self::GrokAcp`].
    ///
    /// Fake is **never** chosen as a silent fallback when Grok is missing.
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

/// Open the conversation service for process startup.
///
/// Production path composes `GrokAcpAdapter`. Fake requires
/// `KEEPER_IDE_RUNTIME=fake` (or equivalent explicit selection).
pub fn open_conversation_service(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    open_conversation_service_with_selection(db_path, emitter, RuntimeSelection::from_env()?)
}

/// Compose with an explicit selection (tests / controlled harnesses).
pub fn open_conversation_service_with_selection(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    selection: RuntimeSelection,
) -> Result<(ConversationService, RuntimeSelection), ConversationCommandError> {
    match selection {
        RuntimeSelection::Fake => {
            // Explicit development / test path only.
            let service = ConversationService::open(db_path, emitter, true)?;
            Ok((service, RuntimeSelection::Fake))
        }
        RuntimeSelection::GrokAcp => {
            let config = discover_grok_config()?;
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
    }
}

/// Discover non-secret Grok launch configuration.
///
/// Sources (in order for the executable):
/// 1. `KEEPER_IDE_GROK_EXECUTABLE` when set
/// 2. Stable default: `grok` / `grok.exe` on `PATH`
///
/// Profile home (always required for real Grok; path only):
/// 1. `KEEPER_IDE_GROK_PROFILE` when set to an existing directory
/// 2. Else current-user home from `USERPROFILE` (Windows) or `HOME` (other)
/// 3. Else fail with `runtime_profile_missing` (never Fake)
///
/// Never reads, logs, or serializes auth file contents or account identity.
pub fn discover_grok_config() -> Result<GrokAcpConfig, ConversationCommandError> {
    let executable = resolve_grok_executable()?;
    let profile = resolve_grok_profile()?;
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

fn resolve_grok_executable() -> Result<PathBuf, ConversationCommandError> {
    match std::env::var(ENV_GROK_EXECUTABLE) {
        Ok(raw) => {
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

/// Resolve the process home path passed to the Grok child.
///
/// Priority: explicit `KEEPER_IDE_GROK_PROFILE` → platform current-user home.
/// Existence check only — never open auth files or inspect account identity.
fn resolve_grok_profile() -> Result<PathBuf, ConversationCommandError> {
    match std::env::var(ENV_GROK_PROFILE) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
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
                return Ok(path);
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
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    use crate::conversation_events::RecordingEmitter;
    use tempfile::tempdir;

    fn env_lock() -> &'static Mutex<()> {
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
            "KEEPER_IDE_GROK_PROFILE must win over platform home"
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
