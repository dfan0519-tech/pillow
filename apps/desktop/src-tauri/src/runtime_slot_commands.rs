//! Host-side runtime-slot commands. Returned DTOs are closed and secret-free.

use std::path::Path;

use keeper_project::ProjectId;
use keeper_storage_sqlite::{
    GrokSlotAuth, RuntimeSlotGovernance, RuntimeSlotId, RuntimeSlotKind, SqliteSettingsRepository,
    process_env_map,
};
use pillow_runtime_slots::{
    ExternalCliStatus, ModelCatalog, ModelCatalogAvailability, ModelCatalogSource,
    ModelCatalogUnsupportedReason, RuntimeSlotError, RuntimeSlotService, RuntimeSlotView,
    SecretBytes,
};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::app_state::{AppState, CommandError};
use crate::runtime_composition::{build_slot_grok_config, discover_grok_executable_with_override};
use crate::runtime_service_state::RuntimeServiceState;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSlotDto {
    pub slot_id: String,
    pub project_id: String,
    pub display_name: String,
    pub kind: String,
    pub governance: String,
    pub auth_mode: Option<String>,
    pub base_url: Option<String>,
    pub model_id: Option<String>,
    pub secret_ref: Option<String>,
    pub secret_configured: bool,
    pub executable_path: Option<String>,
    pub launch_args: Option<Vec<String>>,
    pub status_args: Option<Vec<String>>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCliStatusDto {
    pub slot_id: String,
    pub project_id: String,
    pub governance: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSlotStartDto {
    pub slot_id: String,
    pub project_id: String,
    pub kind: String,
    pub governance: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSlotModelCatalogDto {
    pub slot_id: String,
    pub project_id: String,
    pub status: String,
    pub source: String,
    pub models: Vec<String>,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSlotModelSwitchDto {
    pub slot_id: String,
    pub project_id: String,
    pub model_id: String,
    pub runtime_restarted: bool,
    pub session_restore: String,
}

#[tauri::command]
pub fn list_runtime_slots(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<RuntimeSlotDto>, CommandError> {
    list_runtime_slots_impl(&slots, &app, project_id)
}

#[tauri::command]
pub fn create_grok_login_slot(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
    display_name: String,
) -> Result<RuntimeSlotDto, CommandError> {
    create_grok_login_slot_impl(&slots, &app, project_id, display_name)
}

#[tauri::command]
pub fn create_grok_api_key_slot(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
    display_name: String,
    base_url: String,
    model_id: String,
    api_key: String,
) -> Result<RuntimeSlotDto, CommandError> {
    create_grok_api_key_slot_impl(
        &slots,
        &app,
        project_id,
        display_name,
        base_url,
        model_id,
        api_key,
    )
}

#[tauri::command]
pub fn create_external_cli_slot(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
    display_name: String,
    executable_path: String,
    launch_args: Vec<String>,
    status_args: Vec<String>,
) -> Result<RuntimeSlotDto, CommandError> {
    create_external_cli_slot_impl(
        &slots,
        &app,
        project_id,
        display_name,
        executable_path,
        launch_args,
        status_args,
    )
}

#[tauri::command]
pub fn get_external_cli_status(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
    slot_id: String,
) -> Result<ExternalCliStatusDto, CommandError> {
    get_external_cli_status_impl(&slots, &app, project_id, slot_id)
}

#[tauri::command]
pub fn get_runtime_slot_model_catalog(
    slots: State<'_, RuntimeSlotService>,
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    project_id: String,
    slot_id: String,
) -> Result<RuntimeSlotModelCatalogDto, CommandError> {
    get_runtime_slot_model_catalog_impl(&slots, &settings, &app, project_id, slot_id)
}

#[tauri::command]
pub fn start_runtime_slot(
    slots: State<'_, RuntimeSlotService>,
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
    slot_id: String,
) -> Result<RuntimeSlotStartDto, CommandError> {
    start_runtime_slot_impl(&slots, &settings, &app, &runtime, project_id, slot_id)
}

#[tauri::command]
pub fn set_runtime_slot_model(
    slots: State<'_, RuntimeSlotService>,
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
    slot_id: String,
    model_id: String,
) -> Result<RuntimeSlotModelSwitchDto, CommandError> {
    set_runtime_slot_model_impl(
        &slots, &settings, &app, &runtime, project_id, slot_id, model_id,
    )
}

/// Update a governed Grok API-key slot.
///
/// `api_key` is optional: omit (or null) to keep the stored secret; provide a
/// non-empty value to rotate. Empty/whitespace is treated as omit (keep), never clear.
///
/// Argument count follows the IPC boundary: Tauri commands receive the wire
/// fields flat, so bundling them would change the renderer contract rather than
/// simplify anything. Same precedent as `host/process.rs` and `governed_write.rs`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn update_grok_api_key_slot(
    slots: State<'_, RuntimeSlotService>,
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
    slot_id: String,
    display_name: String,
    base_url: String,
    model_id: String,
    api_key: Option<String>,
) -> Result<RuntimeSlotDto, CommandError> {
    update_grok_api_key_slot_impl(
        &slots,
        &settings,
        &app,
        &runtime,
        project_id,
        slot_id,
        display_name,
        base_url,
        model_id,
        api_key,
    )
}

/// Delete a project-scoped runtime slot and destroy any associated secret material.
/// If this slot is the process-active runtime, the runtime is shut down first.
///
/// See `update_grok_api_key_slot`: `lib.rs` registration is controller-owned.
#[tauri::command]
pub fn delete_runtime_slot(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
    slot_id: String,
) -> Result<(), CommandError> {
    delete_runtime_slot_impl(&slots, &app, &runtime, project_id, slot_id)
}

pub(crate) fn list_runtime_slots_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
) -> Result<Vec<RuntimeSlotDto>, CommandError> {
    let project_id = registered_project_id(app, &project_id)?;
    slots
        .list(project_id)
        .map_err(map_slot_error)
        .map(|views| views.into_iter().map(slot_to_dto).collect())
}

pub(crate) fn create_grok_login_slot_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    display_name: String,
) -> Result<RuntimeSlotDto, CommandError> {
    let project_id = registered_project_id(app, &project_id)?;
    slots
        .create_grok_login(project_id, display_name)
        .map(slot_to_dto)
        .map_err(map_slot_error)
}

pub(crate) fn create_grok_api_key_slot_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    display_name: String,
    base_url: String,
    model_id: String,
    api_key: String,
) -> Result<RuntimeSlotDto, CommandError> {
    let project_id = registered_project_id(app, &project_id)?;
    // Move the inbound String buffer immediately into a non-formatting,
    // zero-on-drop host wrapper. It is never part of a serializable DTO.
    let secret = SecretBytes::new(api_key);
    slots
        .create_grok_api_key(project_id, display_name, base_url, model_id, secret)
        .map(slot_to_dto)
        .map_err(map_slot_error)
}

pub(crate) fn create_external_cli_slot_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    display_name: String,
    executable_path: String,
    launch_args: Vec<String>,
    status_args: Vec<String>,
) -> Result<RuntimeSlotDto, CommandError> {
    let project_id = registered_project_id(app, &project_id)?;
    slots
        .create_external_cli(
            project_id,
            display_name,
            executable_path,
            launch_args,
            status_args,
        )
        .map(slot_to_dto)
        .map_err(map_slot_error)
}

pub(crate) fn get_external_cli_status_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    slot_id: String,
) -> Result<ExternalCliStatusDto, CommandError> {
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let ExternalCliStatus {
        exit_code,
        stdout,
        stderr,
        timed_out,
        truncated,
    } = slots
        .external_status(project_id_value, slot_id_value)
        .map_err(map_slot_error)?;
    Ok(ExternalCliStatusDto {
        slot_id,
        project_id,
        governance: "none".into(),
        exit_code,
        stdout,
        stderr,
        timed_out,
        truncated,
    })
}

pub(crate) fn get_runtime_slot_model_catalog_impl(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    project_id: String,
    slot_id: String,
) -> Result<RuntimeSlotModelCatalogDto, CommandError> {
    let project = app.get_registered_project(&project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let view = slots
        .list(project.id)
        .map_err(map_slot_error)?
        .into_iter()
        .find(|view| view.slot.id == slot_id_value)
        .ok_or_else(|| command_error("runtime_slot_not_found", "Runtime slot was not found."))?;
    let login_executable = if matches!(
        view.slot.kind,
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login { .. }
        }
    ) {
        let resolved = settings
            .resolve_runtime(Some(&project_id), &process_env_map())
            .map_err(|error| command_error(error.code(), &error.to_string()))?;
        Some(
            discover_grok_executable_with_override(resolved.grok_executable.value.as_deref())
                .map_err(|error| command_error(error.code, &error.message))?,
        )
    } else {
        None
    };
    let catalog = slots
        .model_catalog(
            project.id,
            slot_id_value,
            login_executable.as_deref(),
            Path::new(&project.root_path),
        )
        .map_err(map_slot_error)?;
    Ok(model_catalog_to_dto(catalog, slot_id, project_id))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_grok_api_key_slot_impl(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
    display_name: String,
    base_url: String,
    model_id: String,
    api_key: Option<String>,
) -> Result<RuntimeSlotDto, CommandError> {
    let _lifecycle = runtime
        .lock_slot_lifecycle()
        .map_err(|error| command_error(error.code, &error.message))?;
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let was_active = runtime
        .is_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;
    // None / empty / whitespace ⇒ keep stored secret. Non-empty is moved into
    // SecretBytes immediately so it never sits in a serializable DTO path.
    let secret = match api_key {
        None => None,
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(SecretBytes::new(value)),
    };
    let updated = slots
        .update_grok_api_key(
            project_id_value,
            slot_id_value,
            display_name,
            base_url,
            model_id,
            secret,
        )
        .map_err(map_slot_error)?;
    if was_active
        && let Err(error) = start_runtime_slot_impl_locked(
            slots,
            settings,
            app,
            runtime,
            project_id.clone(),
            slot_id.clone(),
        )
    {
        return Err(fail_closed_replacement_error(
            runtime,
            &project_id,
            &slot_id,
            error,
        ));
    }
    Ok(slot_to_dto(updated))
}

pub(crate) fn delete_runtime_slot_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
) -> Result<(), CommandError> {
    let _lifecycle = runtime
        .lock_slot_lifecycle()
        .map_err(|error| command_error(error.code, &error.message))?;
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    // If this slot is currently the active conversation runtime, stop it first
    // so delete cannot leave a live process bound to destroyed credentials.
    runtime
        .shutdown_if_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;
    slots
        .delete(project_id_value, slot_id_value)
        .map_err(map_slot_error)
}

pub(crate) fn start_runtime_slot_impl(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
) -> Result<RuntimeSlotStartDto, CommandError> {
    let _lifecycle = runtime
        .lock_slot_lifecycle()
        .map_err(|error| command_error(error.code, &error.message))?;
    start_runtime_slot_impl_locked(slots, settings, app, runtime, project_id, slot_id)
}

fn start_runtime_slot_impl_locked(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
) -> Result<RuntimeSlotStartDto, CommandError> {
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let view = slots
        .list(project_id_value)
        .map_err(map_slot_error)?
        .into_iter()
        .find(|view| view.slot.id == slot_id_value)
        .ok_or_else(|| command_error("runtime_slot_not_found", "Runtime slot was not found."))?;
    if matches!(view.slot.kind, RuntimeSlotKind::ExternalCli { .. }) {
        return Err(command_error(
            "external_cli_launch_unsupported",
            "External CLI slots are status-only and cannot be launched by Pillow.",
        ));
    }
    let resolved = settings
        .resolve_runtime(Some(&project_id), &process_env_map())
        .map_err(|error| command_error(error.code(), &error.to_string()))?;
    let executable =
        discover_grok_executable_with_override(resolved.grok_executable.value.as_deref())
            .map_err(|error| command_error(error.code, &error.message))?;
    let prepared = slots
        .prepare_grok(project_id_value, slot_id_value)
        .map_err(map_slot_error)?;
    let config = build_slot_grok_config(executable, prepared)
        .map_err(|error| command_error(error.code, &error.message))?;
    runtime
        .activate_grok(project_id.clone(), slot_id.clone(), config)
        .map_err(|error| command_error(error.code, &error.message))?;
    Ok(RuntimeSlotStartDto {
        slot_id,
        project_id,
        kind: "governed_grok".into(),
        governance: "full".into(),
        status: "active".into(),
    })
}

pub(crate) fn set_runtime_slot_model_impl(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
    model_id: String,
) -> Result<RuntimeSlotModelSwitchDto, CommandError> {
    let _lifecycle = runtime
        .lock_slot_lifecycle()
        .map_err(|error| command_error(error.code, &error.message))?;
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let was_active = runtime
        .is_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;
    slots
        .update_model(project_id_value, slot_id_value, model_id.clone())
        .map_err(map_slot_error)?;

    if was_active
        && let Err(error) = start_runtime_slot_impl_locked(
            slots,
            settings,
            app,
            runtime,
            project_id.clone(),
            slot_id.clone(),
        )
    {
        // Metadata now names the requested model. If replacement cannot be
        // activated, remove the prior active runtime so the command cannot
        // leave an old model running behind new metadata.
        return Err(fail_closed_replacement_error(
            runtime,
            &project_id,
            &slot_id,
            error,
        ));
    }

    Ok(RuntimeSlotModelSwitchDto {
        slot_id,
        project_id,
        model_id,
        runtime_restarted: was_active,
        session_restore: if was_active {
            "unavailable".into()
        } else {
            "not_applicable".into()
        },
    })
}

fn fail_closed_replacement_error(
    runtime: &RuntimeServiceState,
    project_id: &str,
    slot_id: &str,
    replacement_error: CommandError,
) -> CommandError {
    match runtime.shutdown_if_active_slot(project_id, slot_id) {
        Ok(_) => replacement_error,
        Err(error) => command_error(error.code, &error.message),
    }
}

fn slot_to_dto(view: RuntimeSlotView) -> RuntimeSlotDto {
    let slot = view.slot;
    let (
        kind,
        auth_mode,
        base_url,
        model_id,
        secret_ref,
        executable_path,
        launch_args,
        status_args,
    ) = match slot.kind {
        RuntimeSlotKind::GovernedGrok {
            auth: GrokSlotAuth::Login { model_id },
        } => (
            "governed_grok".into(),
            Some("login".into()),
            None,
            model_id,
            None,
            None,
            None,
            None,
        ),
        RuntimeSlotKind::GovernedGrok {
            auth:
                GrokSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref,
                },
        } => (
            "governed_grok".into(),
            Some("api_key".into()),
            Some(base_url),
            Some(model_id),
            Some(secret_ref),
            None,
            None,
            None,
        ),
        RuntimeSlotKind::ExternalCli {
            executable_path,
            launch_args,
            status_args,
        } => (
            "external_cli".into(),
            None,
            None,
            None,
            None,
            Some(executable_path),
            Some(launch_args),
            Some(status_args),
        ),
    };
    RuntimeSlotDto {
        slot_id: slot.id.0.to_string(),
        project_id: slot.project_id.0.to_string(),
        display_name: slot.display_name,
        kind,
        governance: governance_label(slot.governance).into(),
        auth_mode,
        base_url,
        model_id,
        secret_ref,
        secret_configured: view.secret_configured,
        executable_path,
        launch_args,
        status_args,
        created_at: slot.created_at.to_rfc3339(),
        updated_at: slot.updated_at.to_rfc3339(),
    }
}

fn model_catalog_to_dto(
    catalog: ModelCatalog,
    slot_id: String,
    project_id: String,
) -> RuntimeSlotModelCatalogDto {
    let source = match catalog.source {
        ModelCatalogSource::OpenAiModels => "openai_models",
        ModelCatalogSource::GrokCli => "grok_cli",
        ModelCatalogSource::ExternalCli => "external_cli",
    };
    let (status, reason_code) = match catalog.availability {
        ModelCatalogAvailability::Available => ("available", None),
        ModelCatalogAvailability::Unsupported { reason } => {
            let reason = match reason {
                ModelCatalogUnsupportedReason::Endpoint => "endpoint_not_supported",
                ModelCatalogUnsupportedReason::ExternalCli => "external_cli_not_supported",
            };
            ("unsupported", Some(reason.to_owned()))
        }
    };
    RuntimeSlotModelCatalogDto {
        slot_id,
        project_id,
        status: status.into(),
        source: source.into(),
        models: catalog.model_ids,
        reason_code,
    }
}

fn governance_label(value: RuntimeSlotGovernance) -> &'static str {
    match value {
        RuntimeSlotGovernance::Full => "full",
        RuntimeSlotGovernance::None => "none",
    }
}

fn registered_project_id(app: &AppState, project_id: &str) -> Result<ProjectId, CommandError> {
    app.get_registered_project(project_id)
        .map(|project| project.id)
}

fn parse_slot_id(value: &str) -> Result<RuntimeSlotId, CommandError> {
    Uuid::parse_str(value)
        .map(RuntimeSlotId)
        .map_err(|_| command_error("runtime_slot_id_invalid", "Runtime slot id is invalid."))
}

fn map_slot_error(error: RuntimeSlotError) -> CommandError {
    command_error(error.code(), &error.to_string())
}

fn command_error(code: &'static str, message: &str) -> CommandError {
    CommandError {
        code,
        message: message.into(),
    }
}
