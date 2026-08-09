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

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::AgentRuntimeAdapter;
use keeper_runtime_host::HostConfig;
use keeper_storage_sqlite::{
    ResolvedRuntimeSettings, RuntimeAdapterSetting, SettingSource, SettingsError,
    SqliteSettingsRepository, process_env_map,
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

/// Whether the Fake adapter may be chosen from stored settings / UI.
///
/// ADR-0003 Q2: Fake is **dev builds only**. Env override
/// (`PILLOW_RUNTIME=fake`) still works in any build (E2E life-line).
pub fn fake_adapter_selectable() -> bool {
    cfg!(debug_assertions)
}

/// Error codes that are **runtime configuration** failures the user can fix
/// in Settings. These degrade the shell instead of aborting setup.
///
/// Other failures (SQLite, illegal parse outside this set when not settings-
/// fixable, etc.) remain fatal. **Never** maps to Fake.
///
/// Probe-failed codes (`runtime_executable_probe_failed` /
/// `runtime_profile_probe_failed`) are degradable so the shell still reaches
/// Settings; they are **not** the same codes as definite missing/invalid.
pub fn is_degradable_runtime_error(code: &str) -> bool {
    matches!(
        code,
        "runtime_executable_missing"
            | "runtime_executable_probe_failed"
            | "runtime_profile_invalid"
            | "runtime_profile_missing"
            | "runtime_profile_probe_failed"
            | "runtime_fake_not_allowed"
    )
}

/// Outcome of desktop conversation runtime open at process startup.
pub enum RuntimeOpenOutcome {
    /// Real conversation service is available (Grok ACP or explicit Fake).
    Ready {
        service: ConversationService,
        selection: RuntimeSelection,
    },
    /// Runtime config is broken; shell must still start with Settings reachable.
    /// Conversation is off. Selection is never Fake as a silent fallback.
    Degraded {
        code: &'static str,
        message: String,
        /// User-facing guidance that points at Settings fields (not raw codes).
        guidance: String,
    },
}

impl RuntimeOpenOutcome {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded { .. })
    }
}

/// Build short guidance that points the user at the settings surface.
pub fn guidance_for_runtime_error(code: &str) -> String {
    match code {
        "runtime_executable_missing" => {
            "打开「设置」，将「Grok 可执行文件路径」改为本机已存在的 Grok CLI 文件，\
             或清空该字段以便使用 PATH；保存后重启 Pillow。"
                .to_owned()
        }
        "runtime_executable_probe_failed" => {
            "Pillow 暂时无法确认 Grok 可执行文件路径（文件系统探测失败，非「确定不存在」）。\
             请稍后重启；若持续出现，打开「设置」核对路径后保存并重启。"
                .to_owned()
        }
        "runtime_profile_invalid" | "runtime_profile_missing" => {
            "打开「设置」，将「Grok profile 路径」改为已存在的目录（进程主目录），\
             或清空该字段以使用当前用户主目录；保存后重启 Pillow。"
                .to_owned()
        }
        "runtime_profile_probe_failed" => {
            "Pillow 暂时无法确认 Grok profile 目录（文件系统探测失败，非「确定不存在」）。\
             请稍后重启；若持续出现，打开「设置」核对路径后保存并重启。"
                .to_owned()
        }
        "runtime_fake_not_allowed" => "打开「设置」，将「运行时适配器」改为 Grok ACP（默认）；\
             生产构建不提供 Fake。保存后重启 Pillow。"
            .to_owned(),
        // Adapter probe class — never opened from this module; phrase/code only
        // on the UI path. Kept here so product copy stays one place if host maps it.
        "authentication_failed" => guidance_for_authentication_failed(),
        _ => "打开「设置」检查运行时适配器与 Grok 路径，修正后重启 Pillow。".to_owned(),
    }
}

/// Fixed guidance when the adapter health class is `authentication_failed`.
///
/// Product claim: Grok login is managed by the Grok CLI; Pillow does not store
/// that login. **Never** open, read, or display auth/token files or account id.
pub fn guidance_for_authentication_failed() -> String {
    "Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。请在终端运行 grok login，\
     完成后回到 Pillow 重试发送。"
        .to_owned()
}

/// Classify free-form error text as a recognized auth failure (phrase match).
/// Same non-secret phrases as the adapter health surface — no file I/O.
pub fn looks_like_authentication_failure_message(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "unauthenticated",
        "unauthorized",
        "authentication required",
        "authentication failed",
        "authentication_failed",
        "not authenticated",
        "auth failed",
        "auth_failed",
        "login required",
        "not logged in",
        "invalid token",
        "expired token",
        "token expired",
        "no credentials",
        "sign in required",
        "signin required",
    ];
    PATTERNS.iter().any(|p| m.contains(p))
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

/// Startup open that **degrades** on fixable runtime-config errors instead of
/// aborting the whole desktop process.
///
/// - `Ok(Ready)` — conversation available
/// - `Ok(Degraded)` — shell may start; conversation off; **not Fake**
/// - `Err` — non-degradable failure (caller must keep fatal behaviour)
pub fn open_conversation_service_or_degrade(
    db_path: impl AsRef<Path>,
    emitter: Arc<dyn RuntimeEventEmitter>,
) -> Result<RuntimeOpenOutcome, ConversationCommandError> {
    match open_conversation_service(db_path, emitter) {
        Ok((service, selection)) => Ok(RuntimeOpenOutcome::Ready { service, selection }),
        Err(error) if is_degradable_runtime_error(error.code) => {
            // Hard rule: degradation never opens Fake.
            debug_assert_ne!(error.code, "using_fake");
            Ok(RuntimeOpenOutcome::Degraded {
                code: error.code,
                message: error.message.clone(),
                guidance: guidance_for_runtime_error(error.code),
            })
        }
        Err(error) => Err(error),
    }
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
            // ADR-0003 Q2: stored Fake only in dev builds. Env (`PILLOW_RUNTIME=fake`)
            // remains the E2E life-line in every build. Never rewrite stored data.
            if resolved.adapter.source != SettingSource::Env && !fake_adapter_selectable() {
                return Err(ConversationCommandError::new(
                    "runtime_fake_not_allowed",
                    "Stored Fake adapter is not available in this build. \
                     Open Settings and choose Grok ACP, or set PILLOW_RUNTIME=fake \
                     for explicit development / automation. User data was not modified.",
                ));
            }
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
    let service = ConversationService::open_with_adapter_factory(
        db_path,
        emitter,
        production_grok_host_config(),
        move |governance| {
            let adapter = GrokAcpAdapter::new(config, governance).map_err(|error| {
                ConversationCommandError::new(
                    "runtime_adapter_unavailable",
                    format!("Failed to construct Grok ACP adapter: {error}"),
                )
            })?;
            Ok(Arc::new(adapter) as Arc<dyn AgentRuntimeAdapter>)
        },
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
        None => match current_user_home_dir()? {
            Some(home) => home,
            None => {
                return Err(ConversationCommandError::new(
                    "runtime_profile_missing",
                    format!(
                        "Grok process home could not be resolved. Set {ENV_GROK_PROFILE} or a settings profile path to an existing directory, or ensure the current-user home ({}) exists. Real runtime never falls back to Fake.",
                        if cfg!(windows) { "USERPROFILE" } else { "HOME" }
                    ),
                ));
            }
        },
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

/// Max filesystem probes per configured path (1 initial + retries).
///
/// Chosen so a brief Windows AV / indexer / sharing lock can clear without
/// turning every startup into a multi-second wait. Only applies when the
/// probe result is **indeterminate** (IO error); confirmed missing/wrong-type
/// and confirmed-ok never sleep and never re-probe.
const PATH_PROBE_MAX_ATTEMPTS: u32 = 3;

/// Backoff after indeterminate attempt 1 and 2 (milliseconds).
///
/// Rationale: metadata ops are cheap; short sleeps (10ms + 30ms ≈ 40ms cap)
/// recover most transient locks without measurably slowing the happy path
/// (happy path: one `metadata` call, zero sleep).
const PATH_PROBE_BACKOFF_MS: [u64; 2] = [10, 30];

/// Expected path kind for startup configuration checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathTypeExpectation {
    File,
    Directory,
}

/// Three-way presence result for a single probe (before retries).
enum PathPresence {
    /// Exists and matches the expected type.
    ConfirmedOk,
    /// Definite NotFound, or exists but wrong type (file vs directory).
    ConfirmedReject,
    /// Cannot decide — permission, sharing violation, Interrupted, etc.
    Indeterminate(io::Error),
}

/// Final outcome after the limited retry loop.
enum PathCheckOutcome {
    Ok,
    Rejected,
    Indeterminate(io::Error),
}

/// IO injection seam for unit tests / `test-support` builds only.
///
/// Entire module is stripped from production builds: no statics, no public
/// inject API, and `metadata_for_path` has no injection branch.
#[cfg(any(test, feature = "test-support"))]
pub mod path_probe_test_hook {
    use std::cell::Cell;
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};

    thread_local! {
        static REMAINING_FORCED_ERRORS: Cell<u32> = const { Cell::new(0) };
        static FORCED_KIND: Cell<Option<io::ErrorKind>> = const { Cell::new(None) };
    }

    static PROBE_COUNT: AtomicU64 = AtomicU64::new(0);

    /// Clear injection state and the probe counter (call under `env_lock`).
    pub fn reset() {
        REMAINING_FORCED_ERRORS.with(|c| c.set(0));
        FORCED_KIND.with(|c| c.set(None));
        PROBE_COUNT.store(0, Ordering::SeqCst);
    }

    /// Force the next `count` metadata probes to return `kind` (then real FS).
    pub fn inject_io_errors(count: u32, kind: io::ErrorKind) {
        REMAINING_FORCED_ERRORS.with(|c| c.set(count));
        FORCED_KIND.with(|c| c.set(Some(kind)));
    }

    /// How many times [`super::metadata_for_path`] ran on this thread's process
    /// counter (tests hold `env_lock` so this is effectively serial).
    pub fn probe_count() -> u64 {
        PROBE_COUNT.load(Ordering::SeqCst)
    }

    pub(super) fn record_probe() {
        PROBE_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn take_forced_error() -> Option<io::Error> {
        REMAINING_FORCED_ERRORS.with(|remaining| {
            let n = remaining.get();
            if n == 0 {
                return None;
            }
            remaining.set(n - 1);
            let kind = FORCED_KIND
                .with(|k| k.get())
                .unwrap_or(io::ErrorKind::Other);
            Some(io::Error::new(
                kind,
                "path_probe_test_hook: injected filesystem probe error",
            ))
        })
    }
}

/// Metadata for a path, with optional test-only IO injection.
///
/// Production: single `std::fs::metadata` call — no counters, no hooks.
fn metadata_for_path(path: &Path) -> io::Result<std::fs::Metadata> {
    #[cfg(any(test, feature = "test-support"))]
    {
        path_probe_test_hook::record_probe();
        if let Some(err) = path_probe_test_hook::take_forced_error() {
            return Err(err);
        }
    }
    std::fs::metadata(path)
}

fn classify_path(path: &Path, expect: PathTypeExpectation) -> PathPresence {
    match metadata_for_path(path) {
        Ok(meta) => {
            let matches = match expect {
                PathTypeExpectation::File => meta.is_file(),
                PathTypeExpectation::Directory => meta.is_dir(),
            };
            if matches {
                PathPresence::ConfirmedOk
            } else {
                PathPresence::ConfirmedReject
            }
        }
        // Definite absence — same fail-closed codes as before (`is_file`/`is_dir`
        // also treated NotFound as false).
        Err(err) if err.kind() == io::ErrorKind::NotFound => PathPresence::ConfirmedReject,
        // Structural path error: an intermediate component is not a directory.
        // Definite configuration problem, not a transient probe failure.
        Err(err) if err.kind() == io::ErrorKind::NotADirectory => PathPresence::ConfirmedReject,
        Err(err) => PathPresence::Indeterminate(err),
    }
}

/// Probe with limited retries **only** on indeterminate IO results.
///
/// Confirmed-ok and confirmed-reject return immediately (no backoff, no extra
/// IO). Uncertain results still fail closed after retries — never treated as
/// pass.
fn require_path_with_retry(path: &Path, expect: PathTypeExpectation) -> PathCheckOutcome {
    let mut last_indeterminate: Option<io::Error> = None;
    for attempt in 0..PATH_PROBE_MAX_ATTEMPTS {
        match classify_path(path, expect) {
            PathPresence::ConfirmedOk => return PathCheckOutcome::Ok,
            PathPresence::ConfirmedReject => return PathCheckOutcome::Rejected,
            PathPresence::Indeterminate(err) => {
                last_indeterminate = Some(err);
                let next = attempt + 1;
                if next < PATH_PROBE_MAX_ATTEMPTS {
                    let backoff_ms = PATH_PROBE_BACKOFF_MS[attempt as usize];
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }
    PathCheckOutcome::Indeterminate(
        last_indeterminate
            .unwrap_or_else(|| io::Error::other("path probe failed without IO detail")),
    )
}

fn format_io_error_kind(kind: io::ErrorKind) -> String {
    // Debug form (e.g. `PermissionDenied`) is stable for log/diagnosis.
    format!("{kind:?}")
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
    match require_path_with_retry(&path, PathTypeExpectation::File) {
        PathCheckOutcome::Ok => Ok(path),
        PathCheckOutcome::Rejected => Err(ConversationCommandError::new(
            "runtime_executable_missing",
            format!(
                "Grok executable is not a file: {}. Set {ENV_GROK_EXECUTABLE} or install `grok` on PATH.",
                path.display()
            ),
        )),
        PathCheckOutcome::Indeterminate(err) => Err(ConversationCommandError::new(
            "runtime_executable_probe_failed",
            format!(
                "Could not verify Grok executable path (io::ErrorKind::{}): {}. \
                 Filesystem probe failed after {PATH_PROBE_MAX_ATTEMPTS} attempts; \
                 this is not a confirmed missing path. Settings remain available; \
                 real runtime never falls back to Fake.",
                format_io_error_kind(err.kind()),
                path.display()
            ),
        )),
    }
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
    // Path existence / type only — never open auth files inside the profile.
    match require_path_with_retry(&path, PathTypeExpectation::Directory) {
        PathCheckOutcome::Ok => Ok(path),
        PathCheckOutcome::Rejected => Err(ConversationCommandError::new(
            "runtime_profile_invalid",
            format!(
                "Grok profile path must be an existing directory: {}",
                path.display()
            ),
        )),
        PathCheckOutcome::Indeterminate(err) => Err(ConversationCommandError::new(
            "runtime_profile_probe_failed",
            format!(
                "Could not verify Grok profile path (io::ErrorKind::{}): {}. \
                 Filesystem probe failed after {PATH_PROBE_MAX_ATTEMPTS} attempts; \
                 this is not a confirmed missing directory. Settings remain available; \
                 real runtime never falls back to Fake.",
                format_io_error_kind(err.kind()),
                path.display()
            ),
        )),
    }
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

    match current_user_home_dir()? {
        Some(home) => Ok(home),
        None => Err(ConversationCommandError::new(
            "runtime_profile_missing",
            format!(
                "Grok process home could not be resolved. Set {ENV_GROK_PROFILE} to an existing directory, or ensure the current-user home ({}) exists. Real runtime never falls back to Fake.",
                if cfg!(windows) { "USERPROFILE" } else { "HOME" }
            ),
        )),
    }
}

/// Current-user home from the platform env var, only when it is a confirmed directory.
///
/// - `Ok(None)` — env unset, or path definitely missing / not a directory
/// - `Ok(Some)` — confirmed existing directory
/// - `Err` — probe indeterminate after retries (`runtime_profile_probe_failed`)
fn current_user_home_dir() -> Result<Option<PathBuf>, ConversationCommandError> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let Some(value) = std::env::var_os(key) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    match require_path_with_retry(&path, PathTypeExpectation::Directory) {
        PathCheckOutcome::Ok => Ok(Some(path)),
        PathCheckOutcome::Rejected => Ok(None),
        PathCheckOutcome::Indeterminate(err) => Err(ConversationCommandError::new(
            "runtime_profile_probe_failed",
            format!(
                "Could not verify current-user home as Grok profile (io::ErrorKind::{}): {}. \
                 Filesystem probe failed after {PATH_PROBE_MAX_ATTEMPTS} attempts; \
                 this is not a confirmed missing directory. Settings remain available; \
                 real runtime never falls back to Fake.",
                format_io_error_kind(err.kind()),
                path.display()
            ),
        )),
    }
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

    /// Core self-brick fix: bad stored executable must not abort the process
    /// composition path as Fake, and must degrade so Settings can fix it.
    #[test]
    fn bad_stored_executable_degrades_not_fake_and_not_fatal() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let missing = temp.path().join("missing-grok-for-degrade.exe");
        let profile = temp.path().join("ok-profile");
        std::fs::create_dir_all(&profile).unwrap();

        // Store a path that once existed (or was injected) then is gone — models
        // the pre-validation self-brick chain without relying on write rejection.
        let once = temp.path().join("once-was-grok.exe");
        std::fs::write(&once, b"").unwrap();
        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &once.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            keeper_storage_sqlite::KEY_GROK_PROFILE,
            &profile.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);
        std::fs::remove_file(&once).unwrap();
        assert!(!missing.exists());
        assert!(!once.exists());

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }

        let emitter = Arc::new(RecordingEmitter::new());
        let outcome = open_conversation_service_or_degrade(&db, emitter as Arc<_>)
            .expect("degradable runtime error must not be fatal Err");
        match outcome {
            RuntimeOpenOutcome::Degraded {
                code,
                message,
                guidance,
            } => {
                assert_eq!(code, "runtime_executable_missing");
                assert!(
                    !message.to_ascii_lowercase().contains("using fake")
                        && !message.to_ascii_lowercase().contains("falling back"),
                    "degrade must not mention Fake fallback: {message}"
                );
                assert!(
                    guidance.contains("设置") || guidance.to_ascii_lowercase().contains("settings"),
                    "guidance must point at settings: {guidance}"
                );
            }
            RuntimeOpenOutcome::Ready { selection, .. } => {
                panic!("must not open Ready (selection={selection:?}); would brick or Fake")
            }
        }

        // Direct open still fails closed (no Fake) — degrade wrapper is the product path.
        let emitter2 = Arc::new(RecordingEmitter::new());
        let direct = match open_conversation_service(&db, emitter2 as Arc<_>) {
            Ok(_) => panic!("direct open must fail closed for missing executable"),
            Err(error) => error,
        };
        assert_eq!(direct.code, "runtime_executable_missing");
    }

    #[test]
    fn bad_stored_profile_degrades_not_fake() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let exe = write_placeholder_executable(temp.path());
        let once_profile = temp.path().join("once-profile");
        std::fs::create_dir_all(&once_profile).unwrap();

        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &exe.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            keeper_storage_sqlite::KEY_GROK_PROFILE,
            &once_profile.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);
        std::fs::remove_dir_all(&once_profile).unwrap();

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }

        let emitter = Arc::new(RecordingEmitter::new());
        let outcome = open_conversation_service_or_degrade(&db, emitter as Arc<_>)
            .expect("profile config errors degrade");
        match outcome {
            RuntimeOpenOutcome::Degraded { code, message, .. } => {
                assert_eq!(code, "runtime_profile_invalid");
                assert!(!message.to_ascii_lowercase().contains("using fake"));
            }
            RuntimeOpenOutcome::Ready { .. } => panic!("must degrade, not Ready"),
        }
    }

    #[test]
    fn degrade_outcome_is_never_fake_selection() {
        // Structural: Degraded has no selection field and Ready with Fake only
        // comes from explicit Fake open (env/store), never from missing Grok.
        assert!(is_degradable_runtime_error("runtime_executable_missing"));
        assert!(is_degradable_runtime_error(
            "runtime_executable_probe_failed"
        ));
        assert!(is_degradable_runtime_error("runtime_profile_invalid"));
        assert!(is_degradable_runtime_error("runtime_profile_missing"));
        assert!(is_degradable_runtime_error("runtime_profile_probe_failed"));
        assert!(is_degradable_runtime_error("runtime_fake_not_allowed"));
        assert!(!is_degradable_runtime_error("settings_sqlite"));
        assert!(!is_degradable_runtime_error("invalid_runtime_selection"));
    }

    // --- Path probe three-way + retry (path-validation robustness) ---------

    #[test]
    fn path_probe_missing_still_fail_closed_legacy_codes() {
        let _guard = env_lock().lock().unwrap();
        path_probe_test_hook::reset();
        let temp = tempdir().unwrap();
        let missing_exe = temp.path().join("definitely-missing-grok.exe");
        let missing_profile = temp.path().join("definitely-missing-profile");
        assert!(!missing_exe.exists());
        assert!(!missing_profile.exists());

        let exe_err = require_executable_file(&missing_exe.to_string_lossy()).unwrap_err();
        assert_eq!(exe_err.code, "runtime_executable_missing");
        assert!(
            !exe_err.message.contains("io::ErrorKind"),
            "confirmed missing must not look like probe failure: {}",
            exe_err.message
        );

        let profile_err = require_profile_dir(&missing_profile.to_string_lossy()).unwrap_err();
        assert_eq!(profile_err.code, "runtime_profile_invalid");
        assert!(
            !profile_err.message.contains("io::ErrorKind"),
            "confirmed missing must not look like probe failure: {}",
            profile_err.message
        );
        // Confirmed reject: one probe each, no retry amplification.
        assert_eq!(path_probe_test_hook::probe_count(), 2);
    }

    #[test]
    fn path_probe_existing_ok_single_probe_no_retry_overhead() {
        let _guard = env_lock().lock().unwrap();
        path_probe_test_hook::reset();
        let temp = tempdir().unwrap();
        let exe = write_placeholder_executable(temp.path());
        let profile = temp.path().join("ok-profile-dir");
        std::fs::create_dir_all(&profile).unwrap();

        path_probe_test_hook::reset();
        let got_exe = require_executable_file(&exe.to_string_lossy()).expect("exe exists");
        assert_eq!(got_exe, exe);
        assert_eq!(
            path_probe_test_hook::probe_count(),
            1,
            "happy path must probe exactly once (no retry)"
        );

        path_probe_test_hook::reset();
        let got_profile = require_profile_dir(&profile.to_string_lossy()).expect("profile exists");
        assert_eq!(got_profile, profile);
        assert_eq!(
            path_probe_test_hook::probe_count(),
            1,
            "happy path must probe exactly once (no retry)"
        );
    }

    #[test]
    fn path_probe_io_error_triggers_retry_then_success() {
        let _guard = env_lock().lock().unwrap();
        path_probe_test_hook::reset();
        let temp = tempdir().unwrap();
        let profile = temp.path().join("retry-then-ok-profile");
        std::fs::create_dir_all(&profile).unwrap();

        // Two injected IO failures → third real metadata succeeds within max attempts.
        path_probe_test_hook::inject_io_errors(2, io::ErrorKind::Interrupted);
        let got = require_profile_dir(&profile.to_string_lossy())
            .expect("transient probe errors must recover");
        assert_eq!(got, profile);
        assert_eq!(
            path_probe_test_hook::probe_count(),
            3,
            "2 injected failures + 1 real success"
        );
    }

    #[test]
    fn path_probe_io_error_exhausted_uses_distinct_code_with_error_kind() {
        let _guard = env_lock().lock().unwrap();
        path_probe_test_hook::reset();
        let temp = tempdir().unwrap();
        // Path exists on disk; all probes forced to fail so we never mis-classify
        // as confirmed missing.
        let profile = temp.path().join("exists-but-unprobeable");
        std::fs::create_dir_all(&profile).unwrap();
        let exe = write_placeholder_executable(temp.path());

        path_probe_test_hook::inject_io_errors(
            PATH_PROBE_MAX_ATTEMPTS,
            io::ErrorKind::PermissionDenied,
        );
        let profile_err = require_profile_dir(&profile.to_string_lossy()).unwrap_err();
        assert_eq!(profile_err.code, "runtime_profile_probe_failed");
        assert_ne!(profile_err.code, "runtime_profile_invalid");
        assert_ne!(profile_err.code, "runtime_profile_missing");
        assert!(
            profile_err
                .message
                .contains("io::ErrorKind::PermissionDenied"),
            "must surface ErrorKind: {}",
            profile_err.message
        );
        assert_eq!(
            path_probe_test_hook::probe_count(),
            u64::from(PATH_PROBE_MAX_ATTEMPTS)
        );

        path_probe_test_hook::reset();
        path_probe_test_hook::inject_io_errors(PATH_PROBE_MAX_ATTEMPTS, io::ErrorKind::TimedOut);
        let exe_err = require_executable_file(&exe.to_string_lossy()).unwrap_err();
        assert_eq!(exe_err.code, "runtime_executable_probe_failed");
        assert_ne!(exe_err.code, "runtime_executable_missing");
        assert!(
            exe_err.message.contains("io::ErrorKind::TimedOut"),
            "must surface ErrorKind: {}",
            exe_err.message
        );
    }

    #[test]
    fn path_probe_failure_degrades_not_fake() {
        let _guard = env_lock().lock().unwrap();
        path_probe_test_hook::reset();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let exe = write_placeholder_executable(temp.path());
        let profile = temp.path().join("probe-fail-profile");
        std::fs::create_dir_all(&profile).unwrap();

        let repo = SqliteSettingsRepository::open(&db).unwrap();
        repo.upsert_runtime_adapter(
            SettingScope::Global,
            None,
            RuntimeAdapterSetting::GrokAcp,
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            KEY_GROK_EXECUTABLE,
            &exe.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        repo.upsert_path(
            SettingScope::Global,
            None,
            keeper_storage_sqlite::KEY_GROK_PROFILE,
            &profile.to_string_lossy(),
            SettingsUpdatedBy::User,
        )
        .unwrap();
        drop(repo);

        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }

        // Force the first configured-path probe (executable) to exhaust retries.
        // Paths exist on disk — failure is probe-class, not confirmed-missing.
        path_probe_test_hook::reset();
        path_probe_test_hook::inject_io_errors(PATH_PROBE_MAX_ATTEMPTS, io::ErrorKind::Interrupted);

        let emitter = Arc::new(RecordingEmitter::new());
        let outcome = open_conversation_service_or_degrade(&db, emitter as Arc<_>)
            .expect("probe failure is degradable, not fatal");
        match outcome {
            RuntimeOpenOutcome::Degraded { code, message, .. } => {
                assert_eq!(code, "runtime_executable_probe_failed");
                assert!(
                    message.contains("io::ErrorKind::Interrupted"),
                    "degrade message must carry ErrorKind: {message}"
                );
                assert!(
                    !message.to_ascii_lowercase().contains("using fake")
                        && !message.to_ascii_lowercase().contains("falling back"),
                    "degrade must not be Fake: {message}"
                );
            }
            RuntimeOpenOutcome::Ready { selection, .. } => {
                panic!("must degrade, not Ready (selection={selection:?}); never Fake fallback")
            }
        }

        // Explicit: Degraded has no selection; Ready+Fake is only env/store Fake.
        // Structural check that probe-failed codes degrade and are not Fake-named.
        assert!(is_degradable_runtime_error(
            "runtime_executable_probe_failed"
        ));
        assert!(is_degradable_runtime_error("runtime_profile_probe_failed"));
        path_probe_test_hook::reset();
    }

    #[test]
    fn path_probe_inject_module_is_test_gated() {
        // Production builds compile without `path_probe_test_hook` (module is
        // `cfg(any(test, feature = "test-support"))` only). Under `cfg(test)`
        // the inject API is available for the cases above.
        assert!(
            cfg!(any(test, feature = "test-support")),
            "this unit test only runs under cfg(test)"
        );
        path_probe_test_hook::reset();
        assert_eq!(path_probe_test_hook::probe_count(), 0);
    }

    #[test]
    fn env_fake_still_opens_ready_not_degraded() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        // Store Grok so without env we would try real path; env fake must win.
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
            std::env::remove_var(ENV_GROK_EXECUTABLE);
            std::env::remove_var(ENV_GROK_PROFILE);
        }
        let emitter = Arc::new(RecordingEmitter::new());
        let outcome = open_conversation_service_or_degrade(&db, emitter as Arc<_>)
            .expect("PILLOW_RUNTIME=fake is the E2E life-line");
        match outcome {
            RuntimeOpenOutcome::Ready { service, selection } => {
                assert_eq!(selection, RuntimeSelection::Fake);
                let probe = service.probe().unwrap();
                assert_eq!(probe.adapter_name, "fake");
                service.shutdown().unwrap();
            }
            RuntimeOpenOutcome::Degraded { code, .. } => {
                panic!("env fake must not degrade: {code}")
            }
        }
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
    }

    #[test]
    fn sqlite_open_failure_remains_fatal_not_degraded() {
        let _guard = env_lock().lock().unwrap();
        let temp = tempdir().unwrap();
        // Point "db" at a directory so rusqlite open fails (not a runtime config path).
        let bad_db = temp.path().join("not-a-sqlite-file");
        std::fs::create_dir_all(&bad_db).unwrap();
        unsafe {
            std::env::remove_var(ENV_RUNTIME_SELECTION);
        }
        let emitter = Arc::new(RecordingEmitter::new());
        let err = match open_conversation_service_or_degrade(&bad_db, emitter as Arc<_>) {
            Ok(RuntimeOpenOutcome::Degraded { code, .. }) => {
                panic!("SQLite failure must not degrade: {code}")
            }
            Ok(RuntimeOpenOutcome::Ready { .. }) => panic!("SQLite failure must not open Ready"),
            Err(error) => error,
        };
        assert_eq!(err.code, "settings_sqlite");
        assert!(!is_degradable_runtime_error(err.code));
    }

    #[test]
    fn fake_adapter_selectable_matches_debug_assertions() {
        assert_eq!(fake_adapter_selectable(), cfg!(debug_assertions));
    }

    #[test]
    fn authentication_failed_guidance_is_actionable_and_secret_free() {
        let guidance = guidance_for_authentication_failed();
        assert!(
            guidance.contains("grok login"),
            "must tell the user to run grok login: {guidance}"
        );
        assert!(
            guidance.contains("Grok CLI") || guidance.contains("Grok 登录"),
            "must state CLI owns login: {guidance}"
        );
        assert!(
            guidance.contains("不存储"),
            "must restate Pillow does not store login: {guidance}"
        );
        let lower = guidance.to_ascii_lowercase();
        for needle in [
            "auth.json",
            "bearer ",
            "sk-",
            "api_key",
            "userprofile",
            "c:\\users",
            "/home/",
            "@",
        ] {
            assert!(
                !lower.contains(needle),
                "guidance must not leak identity/auth material ({needle}): {guidance}"
            );
        }
        // Mapped through the same settings-oriented helper.
        assert_eq!(
            guidance_for_runtime_error("authentication_failed"),
            guidance
        );
    }

    #[test]
    fn auth_failure_phrase_match_never_requires_file_io() {
        assert!(looks_like_authentication_failure_message(
            "acp_request_failed: unauthenticated"
        ));
        assert!(looks_like_authentication_failure_message("login required"));
        assert!(looks_like_authentication_failure_message(
            "authentication_failed"
        ));
        assert!(!looks_like_authentication_failure_message("timeout"));
        assert!(!looks_like_authentication_failure_message(
            "permission denied for write"
        ));
        // Degrade codes remain path/config — not auth-class by themselves.
        assert!(!looks_like_authentication_failure_message(
            "runtime_executable_missing"
        ));
    }

    #[test]
    fn degrade_and_auth_guidance_are_distinct() {
        let degrade = guidance_for_runtime_error("runtime_executable_missing");
        let auth = guidance_for_authentication_failed();
        assert_ne!(degrade, auth);
        assert!(degrade.contains("设置"));
        assert!(!degrade.contains("grok login"));
        assert!(auth.contains("grok login"));
    }
}
