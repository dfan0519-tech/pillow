use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

const APP_DATA_OVERRIDE: &str = "KEEPER_IDE_APP_DATA_DIR";
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
    pid: u32,
    app_data_dir: &'a Path,
    database_path: &'a Path,
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
    pid: u32,
    app_data_dir: &'a Path,
    database_path: &'a Path,
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
            acceptance,
            write_lock: Mutex::new(()),
        }
    }

    fn mark_ready(&self) -> Result<(), LifecycleCommandError> {
        let evidence = ReadinessEvidence {
            schema_version: 1,
            status: "ready",
            pid: std::process::id(),
            app_data_dir: &self.app_data_dir,
            database_path: &self.database_path,
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
            schema_version: 1,
            phase: &result.phase,
            status: &result.status,
            project_id: &result.project_id,
            restored_project_count: result.restored_project_count,
            error_message: &result.error_message,
            ipc_boundary: "real-tauri-webview",
            pid: std::process::id(),
            app_data_dir: &self.app_data_dir,
            database_path: &self.database_path,
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
pub fn get_native_acceptance_scenario(
    state: State<'_, DesktopLifecycleState>,
) -> Option<NativeAcceptanceScenario> {
    state.acceptance_scenario()
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
