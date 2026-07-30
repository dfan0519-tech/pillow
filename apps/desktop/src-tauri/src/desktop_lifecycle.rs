use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{State, WebviewWindow};

use crate::app_state::{AppState, CommandError};

const APP_DATA_OVERRIDE: &str = "KEEPER_IDE_APP_DATA_DIR";
const LAUNCH_NONCE: &str = "KEEPER_IDE_LAUNCH_NONCE";
const STARTED_AT: &str = "KEEPER_IDE_STARTED_AT";
const WEBVIEW_PROFILE_DIR: &str = "WEBVIEW2_USER_DATA_FOLDER";
const ACCEPTANCE_PHASE: &str = "KEEPER_IDE_ACCEPTANCE_PHASE";
const ACCEPTANCE_RESULT: &str = "KEEPER_IDE_ACCEPTANCE_RESULT";
const ACCEPTANCE_PROJECT_NAME: &str = "KEEPER_IDE_ACCEPTANCE_PROJECT_NAME";
const ACCEPTANCE_PROJECT_ROOT: &str = "KEEPER_IDE_ACCEPTANCE_PROJECT_ROOT";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAcceptanceScenario {
    phase: String,
    project_name: String,
    project_root: String,
}

#[derive(Debug, Clone)]
struct NativeAcceptanceConfig {
    scenario: NativeAcceptanceScenario,
    result_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAcceptanceResult {
    phase: String,
    status: String,
    project_id: String,
    restored_project_count: usize,
    error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessEvidence<'a> {
    schema_version: u8,
    status: &'static str,
    nonce: &'a str,
    pid: u32,
    started_at: u128,
    app_data_dir: &'a Path,
    database_path: &'a Path,
    webview_profile_dir: Option<&'a Path>,
    ready_at_unix_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeAcceptanceEvidence<'a> {
    schema_version: u8,
    phase: &'a str,
    status: &'a str,
    project_id: &'a str,
    restored_project_count: usize,
    error_message: &'a Option<String>,
    ipc_boundary: &'static str,
    nonce: &'a str,
    pid: u32,
    started_at: u128,
    app_data_dir: &'a Path,
    database_path: &'a Path,
    webview_profile_dir: Option<&'a Path>,
    completed_at_unix_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleCommandError {
    code: &'static str,
    message: &'static str,
}

pub struct DesktopLifecycleState {
    app_data_dir: PathBuf,
    database_path: PathBuf,
    readiness_path: PathBuf,
    launch_nonce: String,
    started_at: u128,
    webview_profile_dir: Option<PathBuf>,
    acceptance: Option<NativeAcceptanceConfig>,
    write_lock: Mutex<()>,
}

impl DesktopLifecycleState {
    pub fn from_app_data_dir(app_data_dir: PathBuf) -> Self {
        let database_path = app_data_dir.join("keeper.db");
        let readiness_path = app_data_dir.join("desktop-ready.json");
        let acceptance = acceptance_config(&app_data_dir);
        Self {
            app_data_dir,
            database_path,
            readiness_path,
            launch_nonce: std::env::var(LAUNCH_NONCE)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            started_at: std::env::var(STARTED_AT)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(unix_time_ms),
            webview_profile_dir: std::env::var_os(WEBVIEW_PROFILE_DIR)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            acceptance,
            write_lock: Mutex::new(()),
        }
    }

    fn mark_ready(&self) -> Result<(), LifecycleCommandError> {
        let evidence = ReadinessEvidence {
            schema_version: 2,
            status: "ready",
            nonce: &self.launch_nonce,
            pid: std::process::id(),
            started_at: self.started_at,
            app_data_dir: &self.app_data_dir,
            database_path: &self.database_path,
            webview_profile_dir: self.webview_profile_dir.as_deref(),
            ready_at_unix_ms: unix_time_ms(),
        };
        self.write_evidence(&self.readiness_path, &evidence)
    }

    fn acceptance_scenario(&self) -> Option<NativeAcceptanceScenario> {
        self.acceptance
            .as_ref()
            .map(|config| config.scenario.clone())
    }

    fn complete_acceptance(
        &self,
        result: NativeAcceptanceResult,
    ) -> Result<(), LifecycleCommandError> {
        let config = self.acceptance.as_ref().ok_or_else(lifecycle_error)?;
        if result.phase != config.scenario.phase
            || !matches!(result.status.as_str(), "passed" | "failed")
        {
            return Err(lifecycle_error());
        }
        let evidence = NativeAcceptanceEvidence {
            schema_version: 2,
            phase: &result.phase,
            status: &result.status,
            project_id: &result.project_id,
            restored_project_count: result.restored_project_count,
            error_message: &result.error_message,
            ipc_boundary: "real-tauri-webview",
            nonce: &self.launch_nonce,
            pid: std::process::id(),
            started_at: self.started_at,
            app_data_dir: &self.app_data_dir,
            database_path: &self.database_path,
            webview_profile_dir: self.webview_profile_dir.as_deref(),
            completed_at_unix_ms: unix_time_ms(),
        };
        self.write_evidence(&config.result_path, &evidence)
    }

    fn write_evidence<T: Serialize>(
        &self,
        destination: &Path,
        value: &T,
    ) -> Result<(), LifecycleCommandError> {
        let _guard = self.write_lock.lock().map_err(|_| lifecycle_error())?;
        let bytes = serde_json::to_vec(value).map_err(log_lifecycle_error)?;
        let file_name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(lifecycle_error)?;
        let temporary =
            destination.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
        fs::write(&temporary, bytes).map_err(log_lifecycle_error)?;
        if let Err(error) = fs::remove_file(destination)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(log_lifecycle_error(error));
        }
        fs::rename(temporary, destination).map_err(log_lifecycle_error)
    }
}

#[tauri::command]
pub fn mark_project_launcher_ready(
    state: State<'_, DesktopLifecycleState>,
) -> Result<(), LifecycleCommandError> {
    state.mark_ready()
}

#[tauri::command]
pub fn get_native_acceptance_scenario<R: tauri::Runtime>(
    lifecycle: State<'_, DesktopLifecycleState>,
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
) -> Result<Option<NativeAcceptanceScenario>, CommandError> {
    let mut scenario = lifecycle.acceptance_scenario();
    if let Some(scenario) = &mut scenario
        && scenario.phase == "create"
    {
        scenario.project_root =
            state.register_picker_selection(window.label(), Path::new(&scenario.project_root))?;
    }
    Ok(scenario)
}

#[tauri::command]
pub fn complete_native_acceptance(
    state: State<'_, DesktopLifecycleState>,
    result: NativeAcceptanceResult,
) -> Result<(), LifecycleCommandError> {
    state.complete_acceptance(result)
}

pub fn app_data_override() -> Option<PathBuf> {
    std::env::var_os(APP_DATA_OVERRIDE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn acceptance_config(app_data_dir: &Path) -> Option<NativeAcceptanceConfig> {
    let phase = std::env::var(ACCEPTANCE_PHASE).ok()?;
    if !matches!(phase.as_str(), "create" | "restore") {
        return None;
    }
    let result_path = PathBuf::from(std::env::var_os(ACCEPTANCE_RESULT)?);
    if result_path.parent() != Some(app_data_dir) {
        return None;
    }
    let project_name = std::env::var(ACCEPTANCE_PROJECT_NAME).ok()?;
    let project_root = std::env::var(ACCEPTANCE_PROJECT_ROOT).ok()?;
    if project_name.trim().is_empty() || project_root.trim().is_empty() {
        return None;
    }
    Some(NativeAcceptanceConfig {
        scenario: NativeAcceptanceScenario {
            phase,
            project_name,
            project_root,
        },
        result_path,
    })
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn lifecycle_error() -> LifecycleCommandError {
    LifecycleCommandError {
        code: "desktop_lifecycle_unavailable",
        message: "Desktop lifecycle evidence is unavailable.",
    }
}

fn log_lifecycle_error(error: impl std::fmt::Display) -> LifecycleCommandError {
    eprintln!("Keeper IDE desktop lifecycle error: {error}");
    lifecycle_error()
}

/// Pure exit decision for `RunEvent::ExitRequested` after conversation drain.
///
/// Successful shutdown allows the process to exit. Failed shutdown must
/// `prevent_exit` — a later `Exit` event cannot recover a failed drain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationExitDecision {
    AllowExit,
    PreventExit { reason: String },
}

pub fn conversation_exit_decision<E: std::fmt::Display>(
    shutdown_result: Result<(), E>,
) -> ConversationExitDecision {
    match shutdown_result {
        Ok(()) => ConversationExitDecision::AllowExit,
        Err(error) => ConversationExitDecision::PreventExit {
            reason: error.to_string(),
        },
    }
}

/// Write the acceptance shutdown-success marker **only** after
/// `ConversationService::shutdown` returns `Ok`.
///
/// Path is taken from `KEEPER_IDE_CONVERSATION_SHUTDOWN_MARKER` when set
/// (native E2E). Silent no-op when unset so normal desktop runs are unchanged.
pub fn write_conversation_shutdown_success_marker() {
    let Some(raw) = std::env::var_os("KEEPER_IDE_CONVERSATION_SHUTDOWN_MARKER") else {
        return;
    };
    if raw.is_empty() {
        return;
    }
    let path = PathBuf::from(raw);
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "status": "shutdown_success",
        "pid": std::process::id(),
        "completedAtUnixMs": unix_time_ms(),
    });
    let Ok(bytes) = serde_json::to_vec_pretty(&payload) else {
        eprintln!("Keeper IDE: failed to serialize conversation shutdown marker");
        return;
    };
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    if let Err(error) = fs::write(&temporary, &bytes) {
        eprintln!("Keeper IDE: failed to write conversation shutdown marker: {error}");
        return;
    }
    let _ = fs::remove_file(&path);
    if let Err(error) = fs::rename(&temporary, &path) {
        eprintln!("Keeper IDE: failed to publish conversation shutdown marker: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{ConversationExitDecision, conversation_exit_decision};

    #[test]
    fn successful_shutdown_allows_exit() {
        let decision: ConversationExitDecision = conversation_exit_decision(Ok::<(), &str>(()));
        assert_eq!(decision, ConversationExitDecision::AllowExit);
    }

    #[test]
    fn failed_shutdown_prevents_exit_with_reason() {
        let decision = conversation_exit_decision(Err("drain_timeout: pumps still active"));
        assert_eq!(
            decision,
            ConversationExitDecision::PreventExit {
                reason: "drain_timeout: pumps still active".into(),
            }
        );
    }
}
