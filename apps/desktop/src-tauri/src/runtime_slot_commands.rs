//! Host-side runtime-slot commands. Returned DTOs are closed and secret-free.
//!
//! One slot record shape: auth + role. `kind` / `governance` / `authMode` on the
//! wire are **derived** labels for existing UI contracts (not independent storage).

use std::path::Path;
use std::time::Duration;

use keeper_project::ProjectId;
use keeper_storage_sqlite::{
    RuntimeSlotGovernance, RuntimeSlotId, SlotAuth, SlotRole, SqliteSettingsRepository,
    process_env_map,
};
use pillow_cli_detect::{AuthState, CliDetection};
use pillow_runtime_slots::{
    AccountIdentifierStrategy, ClientIdSource, CreateSlotAuth, ExternalCliStatus, ModelCatalog,
    ModelCatalogAvailability, ModelCatalogSource, ModelCatalogUnsupportedReason, OauthLoginFailure,
    OauthLoginInput, OauthLoginProgress, OauthLoginProgressEvent, ProfileField,
    ProviderLoginMethod, ProviderLoginSupplement, RuntimeSlotError, RuntimeSlotService,
    RuntimeSlotView, SecretBytes, SystemBrowser, UpdateSlotAuthInput, WinHttpOauthTokenClient,
    fetch_openai_model_catalog, perform_oauth_login_production, provider_login_profile,
    provider_login_profiles,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::app_state::{AppState, CommandError};
use crate::runtime_composition::{build_slot_grok_config, discover_grok_executable_with_override};
use crate::runtime_service_state::RuntimeServiceState;

/// Tauri event name while OAuth login is in progress (public URL only).
pub const RUNTIME_SLOT_OAUTH_PROGRESS_EVENT: &str = "runtime-slot-oauth-progress";

/// Hard timeout for each CLI probe during `detect_local_clis` (per binary).
const DETECT_LOCAL_CLI_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSlotDto {
    pub slot_id: String,
    pub project_id: String,
    pub display_name: String,
    /// Derived wire label: `governed_grok` | `external_cli`.
    pub kind: String,
    /// Derived from auth: `full` | `none`.
    pub governance: String,
    /// Derived: `login` | `api_key` | `oauth` | null.
    pub auth_mode: Option<String>,
    pub base_url: Option<String>,
    pub model_id: Option<String>,
    pub secret_ref: Option<String>,
    pub secret_configured: bool,
    pub executable_path: Option<String>,
    pub launch_args: Option<Vec<String>>,
    pub status_args: Option<Vec<String>>,
    /// `controller` | `librarian` | `custom`
    pub role: String,
    /// Present only when `role == "custom"`.
    pub role_custom_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// OAuth non-secret account label; null for non-OAuth.
    pub account_label: Option<String>,
    /// OAuth access-token expiry (RFC3339); null for non-OAuth. Never token text.
    pub access_expires_at: Option<String>,
    /// True when this slot is the process-active conversation runtime for its
    /// project (`RuntimeServiceState.active_slot_id`). False for every row when
    /// none is live — never default the first slot.
    pub is_active: bool,
}

/// Public OAuth supplement from the renderer (never tokens/codes/secrets).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OauthLoginSupplementDto {
    pub client_id: Option<String>,
    pub issuer: Option<String>,
    pub authorize_endpoint: Option<String>,
    pub device_authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub account_label: Option<String>,
}

/// Progress payload for `runtime-slot-oauth-progress` (closed, secret-free).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OauthLoginProgressDto {
    pub authorize_url: String,
    pub browser_opened: bool,
    pub user_code: Option<String>,
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

/// One local CLI probe result. Closed, secret-free.
///
/// `auth_status` is three-state: `authenticated` | `not_authenticated` | `unknown`.
/// Probe failure / missing evidence is always `unknown` (+ `auth_reason`), never
/// collapsed into `not_authenticated`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalCliDetectionDto {
    /// `grok` | `claude` | `codex` | `opencode` | `gemini`
    pub kind: String,
    pub found: bool,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    /// `authenticated` | `not_authenticated` | `unknown`
    pub auth_status: String,
    /// Non-sensitive account label when authenticated; otherwise null.
    pub account_label: Option<String>,
    /// Present when `auth_status == "unknown"` (or optional detail); never secret.
    pub auth_reason: Option<String>,
}

/// Closed host DTO for one provider login catalog row.
///
/// Public OIDC/OAuth metadata only — never tokens, codes, or client secrets.
/// `known_client_id` is only set when the host has a PublicDocumented id.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLoginCatalogEntryDto {
    pub provider_id: String,
    pub display_name: String,
    /// `pkce` | `device_code` | `api_key_only`
    pub login_method: String,
    /// `known` | `user_supplied` | `unknown` | `not_applicable`
    pub issuer: String,
    pub authorize_endpoint: String,
    pub device_authorization_endpoint: String,
    pub token_endpoint: String,
    pub scopes: String,
    pub client_id: String,
    pub known_issuer: Option<String>,
    pub known_authorize_endpoint: Option<String>,
    pub known_device_authorization_endpoint: Option<String>,
    pub known_token_endpoint: Option<String>,
    pub known_scopes: Option<Vec<String>>,
    pub known_client_id: Option<String>,
    /// `token_claims` | `user_supplied` | `unknown` | `not_applicable`
    pub account_identifier: String,
}

/// Draft-state model catalog probe (no persisted slot). Closed, secret-free.
///
/// Same status/source/reason vocabulary as `RuntimeSlotModelCatalogDto`, without
/// slot/project ids — the key never appears on this wire shape.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeModelCatalogDto {
    pub status: String,
    pub source: String,
    pub models: Vec<String>,
    pub reason_code: Option<String>,
}

#[tauri::command]
pub fn list_runtime_slots(
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
) -> Result<Vec<RuntimeSlotDto>, CommandError> {
    list_runtime_slots_impl(&slots, &app, &runtime, project_id)
}

/// Unified create command. Auth kind is selected by `auth_kind`.
///
/// `auth_kind`: `api_key` | `grok_login` | `local_cli` | `oauth`
/// `role`: `controller` | `librarian` | `custom` (default custom / 未分配 when omitted)
///
/// For `oauth`, the host performs browser / device-code login inside this
/// invoke and emits [`RUNTIME_SLOT_OAUTH_PROGRESS_EVENT`] with a copyable
/// authorize URL (especially when the system browser cannot be opened).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_slot<R: tauri::Runtime>(
    app_handle: AppHandle<R>,
    slots: State<'_, RuntimeSlotService>,
    app: State<'_, AppState>,
    project_id: String,
    display_name: String,
    auth_kind: String,
    role: Option<String>,
    role_custom_name: Option<String>,
    base_url: Option<String>,
    model_id: Option<String>,
    api_key: Option<String>,
    executable_path: Option<String>,
    launch_args: Option<Vec<String>>,
    status_args: Option<Vec<String>>,
    provider_id: Option<String>,
    oauth_supplement: Option<OauthLoginSupplementDto>,
) -> Result<RuntimeSlotDto, CommandError> {
    create_slot_impl(
        &app_handle,
        &slots,
        &app,
        project_id,
        display_name,
        auth_kind,
        role,
        role_custom_name,
        base_url,
        model_id,
        api_key,
        executable_path,
        launch_args,
        status_args,
        provider_id,
        oauth_supplement,
    )
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

/// Discover installed agent CLIs and probe login status (read-only).
///
/// Does not start conversations, does not write CLI config, and never returns
/// token/key material — only sanitized account labels when authenticated.
#[tauri::command]
pub fn detect_local_clis() -> Result<Vec<LocalCliDetectionDto>, CommandError> {
    detect_local_clis_impl()
}

/// Draft-state OpenAI-compatible model list: `base_url` + one-shot `api_key`.
///
/// Reuses `pillow_runtime_slots::fetch_openai_model_catalog` (same path as
/// persisted-slot catalog). The key is moved into `SecretBytes` immediately and
/// never appears on the success DTO, error code, or error message.
#[tauri::command]
pub fn probe_model_catalog(
    base_url: String,
    api_key: String,
) -> Result<ProbeModelCatalogDto, CommandError> {
    probe_model_catalog_impl(base_url, api_key)
}

/// Public provider-login catalog (OIDC/OAuth metadata only).
///
/// Never returns tokens, client secrets, or guessed Unknown client ids.
#[tauri::command]
pub fn list_provider_login_catalog() -> Result<Vec<ProviderLoginCatalogEntryDto>, CommandError> {
    list_provider_login_catalog_impl()
}

pub(crate) fn list_provider_login_catalog_impl()
-> Result<Vec<ProviderLoginCatalogEntryDto>, CommandError> {
    Ok(provider_login_profiles()
        .iter()
        .map(provider_login_profile_to_dto)
        .collect())
}

fn provider_login_profile_to_dto(
    profile: &pillow_runtime_slots::ProviderLoginProfile,
) -> ProviderLoginCatalogEntryDto {
    let (client_id_field, known_client_id) = match profile.client_id.source {
        ClientIdSource::PublicDocumented => match profile.client_id.value {
            ProfileField::Known(id) => (String::from("known"), Some(id.to_owned())),
            ProfileField::UserSupplied => (String::from("user_supplied"), None),
            ProfileField::Unknown => (String::from("unknown"), None),
            ProfileField::NotApplicable => (String::from("not_applicable"), None),
        },
        // Unknown provenance must never smuggle a knownClientId guess.
        ClientIdSource::Unknown => (profile_field_wire(&profile.client_id.value), None),
        ClientIdSource::UserSupplied => (String::from("user_supplied"), None),
    };

    ProviderLoginCatalogEntryDto {
        provider_id: profile.provider_id.to_owned(),
        display_name: profile.display_name.to_owned(),
        login_method: match profile.login_method {
            ProviderLoginMethod::Pkce => "pkce",
            ProviderLoginMethod::DeviceCode => "device_code",
            ProviderLoginMethod::ApiKeyOnly => "api_key_only",
        }
        .to_owned(),
        issuer: profile_field_wire(&profile.issuer),
        authorize_endpoint: profile_field_wire(&profile.authorize_endpoint),
        device_authorization_endpoint: profile_field_wire(&profile.device_authorization_endpoint),
        token_endpoint: profile_field_wire(&profile.token_endpoint),
        scopes: profile_field_wire(&profile.scopes),
        client_id: client_id_field,
        known_issuer: known_string(&profile.issuer),
        known_authorize_endpoint: known_string(&profile.authorize_endpoint),
        known_device_authorization_endpoint: known_string(&profile.device_authorization_endpoint),
        known_token_endpoint: known_string(&profile.token_endpoint),
        known_scopes: known_scopes(&profile.scopes),
        known_client_id,
        account_identifier: match profile.account_identifier {
            AccountIdentifierStrategy::TokenClaims { .. } => "token_claims",
            AccountIdentifierStrategy::UserSupplied => "user_supplied",
            AccountIdentifierStrategy::Unknown => "unknown",
            AccountIdentifierStrategy::NotApplicable => "not_applicable",
        }
        .to_owned(),
    }
}

fn profile_field_wire<T>(field: &ProfileField<T>) -> String {
    match field {
        ProfileField::Known(_) => "known",
        ProfileField::UserSupplied => "user_supplied",
        ProfileField::Unknown => "unknown",
        ProfileField::NotApplicable => "not_applicable",
    }
    .to_owned()
}

fn known_string(field: &ProfileField<&'static str>) -> Option<String> {
    match field {
        ProfileField::Known(value) => Some((*value).to_owned()),
        _ => None,
    }
}

fn known_scopes(field: &ProfileField<&'static [&'static str]>) -> Option<Vec<String>> {
    match field {
        ProfileField::Known(scopes) => Some(scopes.iter().map(|s| (*s).to_owned()).collect()),
        _ => None,
    }
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

/// Unified update: display name, role, and auth-branch fields.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn update_slot(
    slots: State<'_, RuntimeSlotService>,
    settings: State<'_, SqliteSettingsRepository>,
    app: State<'_, AppState>,
    runtime: State<'_, RuntimeServiceState>,
    project_id: String,
    slot_id: String,
    display_name: String,
    role: Option<String>,
    role_custom_name: Option<String>,
    base_url: Option<String>,
    model_id: Option<String>,
    api_key: Option<String>,
    executable_path: Option<String>,
    launch_args: Option<Vec<String>>,
    status_args: Option<Vec<String>>,
    keep_auth: Option<bool>,
) -> Result<RuntimeSlotDto, CommandError> {
    update_slot_impl(
        &slots,
        &settings,
        &app,
        &runtime,
        project_id,
        slot_id,
        display_name,
        role,
        role_custom_name,
        base_url,
        model_id,
        api_key,
        executable_path,
        launch_args,
        status_args,
        keep_auth,
    )
}

/// Delete a project-scoped runtime slot and destroy any associated secret material.
/// If this slot is the process-active runtime, the runtime is shut down first.
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
    runtime: &RuntimeServiceState,
    project_id: String,
) -> Result<Vec<RuntimeSlotDto>, CommandError> {
    let project_id_str = project_id.clone();
    let project_id = registered_project_id(app, &project_id)?;
    let views = slots.list(project_id).map_err(map_slot_error)?;
    let mut out = Vec::with_capacity(views.len());
    for view in views {
        let slot_id = view.slot.id.0.to_string();
        let is_active = runtime
            .is_active_slot(&project_id_str, &slot_id)
            .map_err(|error| command_error(error.code, &error.message))?;
        out.push(slot_to_dto(view, is_active));
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_slot_impl<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    display_name: String,
    auth_kind: String,
    role: Option<String>,
    role_custom_name: Option<String>,
    base_url: Option<String>,
    model_id: Option<String>,
    api_key: Option<String>,
    executable_path: Option<String>,
    launch_args: Option<Vec<String>>,
    status_args: Option<Vec<String>>,
    provider_id: Option<String>,
    oauth_supplement: Option<OauthLoginSupplementDto>,
) -> Result<RuntimeSlotDto, CommandError> {
    let project_id = registered_project_id(app, &project_id)?;
    let role = parse_role(role, role_custom_name)?;
    let auth = match auth_kind.as_str() {
        "grok_login" => CreateSlotAuth::GrokLogin {
            model_id: model_id.filter(|value| !value.trim().is_empty()),
        },
        "api_key" => {
            let secret = SecretBytes::new(api_key.ok_or_else(|| {
                command_error("runtime_slot_input_invalid", "API key is required.")
            })?);
            CreateSlotAuth::ApiKey {
                base_url: base_url.ok_or_else(|| {
                    command_error("runtime_slot_input_invalid", "base_url is required.")
                })?,
                model_id: model_id.ok_or_else(|| {
                    command_error("runtime_slot_input_invalid", "model_id is required.")
                })?,
                api_key: secret,
            }
        }
        "local_cli" => CreateSlotAuth::LocalCli {
            executable_path: executable_path.ok_or_else(|| {
                command_error("runtime_slot_input_invalid", "executable_path is required.")
            })?,
            launch_args: launch_args.unwrap_or_default(),
            status_args: status_args.ok_or_else(|| {
                command_error("runtime_slot_input_invalid", "status_args is required.")
            })?,
        },
        "oauth" => {
            return create_oauth_slot_impl(
                app_handle,
                slots,
                project_id,
                display_name,
                role,
                base_url,
                model_id,
                provider_id,
                oauth_supplement.unwrap_or_default(),
            );
        }
        _ => {
            return Err(command_error(
                "runtime_slot_input_invalid",
                "Unknown auth_kind.",
            ));
        }
    };
    slots
        .create_slot(project_id, display_name, auth, role)
        .map(|view| slot_to_dto(view, false))
        .map_err(map_slot_error)
}

/// Perform browser/device OAuth login then persist tokens host-side.
#[allow(clippy::too_many_arguments)]
fn create_oauth_slot_impl<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
    slots: &RuntimeSlotService,
    project_id: keeper_project::ProjectId,
    display_name: String,
    role: SlotRole,
    base_url: Option<String>,
    model_id: Option<String>,
    provider_id: Option<String>,
    oauth_supplement: OauthLoginSupplementDto,
) -> Result<RuntimeSlotDto, CommandError> {
    let provider_id = provider_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            command_error(
                "runtime_slot_input_invalid",
                "provider_id is required for oauth.",
            )
        })?;
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| command_error("runtime_slot_input_invalid", "base_url is required."))?;
    let model_id = model_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| command_error("runtime_slot_input_invalid", "model_id is required."))?;
    let profile = provider_login_profile(&provider_id)
        .ok_or_else(|| command_error("runtime_slot_input_invalid", "Unknown OAuth provider_id."))?;

    let account_label = oauth_supplement.account_label.clone();
    let input = OauthLoginInput {
        provider_id: provider_id.clone(),
        base_url,
        model_id,
        supplement: supplement_from_dto(&oauth_supplement),
        account_label,
    };
    let progress = HostOauthLoginProgress {
        app: app_handle.clone(),
    };
    let token_client = WinHttpOauthTokenClient::default();
    let tokens =
        perform_oauth_login_production(profile, &input, &SystemBrowser, &token_client, &progress)
            .map_err(map_oauth_login_failure)?;

    let auth = CreateSlotAuth::Oauth {
        provider_id: tokens.provider_id,
        issuer: tokens.issuer,
        client_id: tokens.client_id,
        scopes: tokens.scopes,
        token_endpoint: tokens.token_endpoint,
        account_label: tokens.account_label,
        base_url: tokens.base_url,
        model_id: tokens.model_id,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        access_expires_at: tokens.access_expires_at,
    };
    slots
        .create_slot(project_id, display_name, auth, role)
        .map(|view| slot_to_dto(view, false))
        .map_err(map_slot_error)
}

fn supplement_from_dto(dto: &OauthLoginSupplementDto) -> ProviderLoginSupplement {
    ProviderLoginSupplement {
        issuer: dto.issuer.clone(),
        authorize_endpoint: dto.authorize_endpoint.clone(),
        device_authorization_endpoint: dto.device_authorization_endpoint.clone(),
        token_endpoint: dto.token_endpoint.clone(),
        scopes: dto.scopes.clone(),
        client_id: dto.client_id.clone(),
    }
}

struct HostOauthLoginProgress<R: tauri::Runtime> {
    app: AppHandle<R>,
}

impl<R: tauri::Runtime> OauthLoginProgress for HostOauthLoginProgress<R> {
    fn on_authorize_ready(&self, event: &OauthLoginProgressEvent) {
        let payload = OauthLoginProgressDto {
            authorize_url: event.authorize_url.clone(),
            browser_opened: event.browser_opened,
            user_code: event.user_code.clone(),
        };
        // Best-effort: UI may miss the event if it has not subscribed yet, but
        // terminal failures still carry authorize_url on the error path.
        let _ = self.app.emit(RUNTIME_SLOT_OAUTH_PROGRESS_EVENT, payload);
    }
}

fn map_oauth_login_failure(failure: OauthLoginFailure) -> CommandError {
    // For browser-open failure, put the public authorize URL in `message` so
    // the renderer can show a copyable fallback. Other codes keep a fixed
    // non-secret Display string (gateway never echoes host messages except
    // for this specific code).
    if failure.error.code() == "runtime_slot_oauth_browser_open_failed"
        && let Some(url) = failure.authorize_url
    {
        return CommandError {
            code: "runtime_slot_oauth_browser_open_failed",
            message: url,
        };
    }
    // Timeout / denied / etc.: include authorize URL only in message when
    // present and code is browser-related; otherwise standard Display.
    if let Some(url) = failure.authorize_url.as_ref()
        && failure.error.code() == "runtime_slot_oauth_loopback_timeout"
    {
        // Still surface URL so the user can diagnose a missed browser open.
        return CommandError {
            code: failure.error.code(),
            message: url.clone(),
        };
    }
    map_slot_error(failure.error)
}

pub(crate) fn create_grok_login_slot_impl(
    slots: &RuntimeSlotService,
    app: &AppState,
    project_id: String,
    display_name: String,
) -> Result<RuntimeSlotDto, CommandError> {
    // Legacy thin wrapper — no OAuth AppHandle path required.
    let project_id = registered_project_id(app, &project_id)?;
    slots
        .create_slot(
            project_id,
            display_name,
            CreateSlotAuth::GrokLogin { model_id: None },
            SlotRole::default_unassigned(),
        )
        .map(|view| slot_to_dto(view, false))
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
    let secret = SecretBytes::new(api_key);
    slots
        .create_slot(
            project_id,
            display_name,
            CreateSlotAuth::ApiKey {
                base_url,
                model_id,
                api_key: secret,
            },
            SlotRole::default_unassigned(),
        )
        .map(|view| slot_to_dto(view, false))
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
        .create_slot(
            project_id,
            display_name,
            CreateSlotAuth::LocalCli {
                executable_path,
                launch_args,
                status_args,
            },
            SlotRole::default_unassigned(),
        )
        .map(|view| slot_to_dto(view, false))
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
    let login_executable = if matches!(view.slot.auth, SlotAuth::GrokLogin { .. }) {
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

pub(crate) fn detect_local_clis_impl() -> Result<Vec<LocalCliDetectionDto>, CommandError> {
    let detections = pillow_cli_detect::detect_all_with_timeout(DETECT_LOCAL_CLI_TIMEOUT);
    Ok(detections
        .into_iter()
        .map(local_cli_detection_to_dto)
        .collect())
}

/// Map a crate-level detection into the closed host DTO.
///
/// Pure and side-effect free so unit tests can pin auth three-state mapping
/// without launching real CLIs.
pub(crate) fn local_cli_detection_to_dto(detection: CliDetection) -> LocalCliDetectionDto {
    let (auth_status, account_label, auth_reason) = match detection.auth {
        AuthState::Authenticated { account_label } => {
            ("authenticated".into(), Some(account_label), None)
        }
        AuthState::NotAuthenticated => ("not_authenticated".into(), None, None),
        AuthState::Unknown { reason } => ("unknown".into(), None, Some(reason)),
    };
    LocalCliDetectionDto {
        kind: detection.kind.as_str().into(),
        found: detection.found,
        executable_path: detection
            .executable
            .map(|path| path.to_string_lossy().into_owned()),
        version: detection.version,
        auth_status,
        account_label,
        auth_reason,
    }
}

pub(crate) fn probe_model_catalog_impl(
    base_url: String,
    api_key: String,
) -> Result<ProbeModelCatalogDto, CommandError> {
    // Move key material into SecretBytes immediately; do not keep the String for
    // any error-message or DTO path. Empty key is invalid input (fixed copy).
    if api_key.trim().is_empty() {
        return Err(command_error(
            "runtime_slot_input_invalid",
            "API key is required for model catalog probe.",
        ));
    }
    if base_url.trim().is_empty() {
        return Err(command_error(
            "runtime_slot_input_invalid",
            "base_url is required for model catalog probe.",
        ));
    }
    // Capture length only for the empty check above; drop the owned String into
    // SecretBytes so Drop zeroizes it. Never format the key into errors.
    let secret = SecretBytes::new(api_key);
    let catalog = fetch_openai_model_catalog(base_url.trim(), &secret).map_err(map_slot_error)?;
    // secret drops here (zeroized). Catalog never carried key material.
    Ok(probe_catalog_to_dto(catalog))
}

fn probe_catalog_to_dto(catalog: ModelCatalog) -> ProbeModelCatalogDto {
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
    ProbeModelCatalogDto {
        status: status.into(),
        source: source.into(),
        models: catalog.model_ids,
        reason_code,
    }
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
    let is_active = runtime
        .is_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;
    Ok(slot_to_dto(updated, is_active))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_slot_impl(
    slots: &RuntimeSlotService,
    settings: &SqliteSettingsRepository,
    app: &AppState,
    runtime: &RuntimeServiceState,
    project_id: String,
    slot_id: String,
    display_name: String,
    role: Option<String>,
    role_custom_name: Option<String>,
    base_url: Option<String>,
    model_id: Option<String>,
    api_key: Option<String>,
    executable_path: Option<String>,
    launch_args: Option<Vec<String>>,
    status_args: Option<Vec<String>>,
    keep_auth: Option<bool>,
) -> Result<RuntimeSlotDto, CommandError> {
    let _lifecycle = runtime
        .lock_slot_lifecycle()
        .map_err(|error| command_error(error.code, &error.message))?;
    let project_id_value = registered_project_id(app, &project_id)?;
    let slot_id_value = parse_slot_id(&slot_id)?;
    let was_active = runtime
        .is_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;

    let existing = slots
        .list(project_id_value)
        .map_err(map_slot_error)?
        .into_iter()
        .find(|view| view.slot.id == slot_id_value)
        .ok_or_else(|| command_error("runtime_slot_not_found", "Runtime slot was not found."))?;

    let role = match role {
        Some(value) => parse_role(Some(value), role_custom_name)?,
        None => existing.slot.role.clone(),
    };

    let auth_update = if keep_auth.unwrap_or(false) {
        UpdateSlotAuthInput::Keep
    } else {
        match &existing.slot.auth {
            SlotAuth::ApiKey { .. } => {
                let secret = match api_key {
                    None => None,
                    Some(value) if value.trim().is_empty() => None,
                    Some(value) => Some(SecretBytes::new(value)),
                };
                UpdateSlotAuthInput::ApiKey {
                    base_url: base_url.ok_or_else(|| {
                        command_error("runtime_slot_input_invalid", "base_url is required.")
                    })?,
                    model_id: model_id.ok_or_else(|| {
                        command_error("runtime_slot_input_invalid", "model_id is required.")
                    })?,
                    api_key: secret,
                }
            }
            SlotAuth::GrokLogin { .. } => UpdateSlotAuthInput::GrokLogin {
                model_id: model_id.filter(|value| !value.trim().is_empty()),
            },
            SlotAuth::LocalCli { .. } => UpdateSlotAuthInput::LocalCli {
                executable_path: executable_path.ok_or_else(|| {
                    command_error("runtime_slot_input_invalid", "executable_path is required.")
                })?,
                launch_args: launch_args.unwrap_or_default(),
                status_args: status_args.ok_or_else(|| {
                    command_error("runtime_slot_input_invalid", "status_args is required.")
                })?,
            },
            // OAuth / future auth shapes: unified update keeps existing auth
            // (use dedicated update paths). Fail closed rather than invent fields.
            _ => UpdateSlotAuthInput::Keep,
        }
    };

    let updated = slots
        .update_slot(
            project_id_value,
            slot_id_value,
            display_name,
            role,
            auth_update,
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
    let is_active = runtime
        .is_active_slot(&project_id, &slot_id)
        .map_err(|error| command_error(error.code, &error.message))?;
    Ok(slot_to_dto(updated, is_active))
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
    if matches!(view.slot.auth, SlotAuth::LocalCli { .. }) {
        return Err(command_error(
            "external_cli_launch_unsupported",
            "Local CLI slots are status-only and cannot be launched by Pillow.",
        ));
    }

    // Path is derived from auth + provider profile — not a user toggle.
    let path = slots
        .resolve_path(project_id_value, slot_id_value)
        .map_err(map_slot_error)?;
    match path.path {
        pillow_runtime_slots::SlotExecutionPath::NotRunnable => Err(command_error(
            "external_cli_launch_unsupported",
            "This slot is not a conversation runtime.",
        )),
        pillow_runtime_slots::SlotExecutionPath::NativeAdapter => {
            // NativeAdapter: Pillow is the harness (full governance). Refuse
            // stuffing into the Grok shell. Prepare validates access-only
            // credential injection + request shape via provider-adapters.
            if !path.governance.is_full() {
                return Err(command_error(
                    "runtime_slot_controller_requires_governed_auth",
                    "NativeAdapter path is marked incomplete governance and cannot start.",
                ));
            }
            let prepared = slots
                .prepare_slot(project_id_value, slot_id_value)
                .map_err(map_slot_error)?;
            match prepared {
                pillow_runtime_slots::PreparedSlot::NativeAdapter(native) => {
                    // Do not call prepare_grok / activate_grok — that would
                    // silently send a wrong request shape.
                    //
                    // Wire `kind` stays `governed_grok` (closed renderer allowlist
                    // + Oauth list DTO already use that label). Path is signalled
                    // via `status = native_adapter_prepared` and by never
                    // activating the Grok ACP child.
                    let _ = (native.provider_id.as_str(), native.governance_mechanism);
                    Ok(RuntimeSlotStartDto {
                        slot_id,
                        project_id,
                        kind: "governed_grok".into(),
                        governance: "full".into(),
                        status: "native_adapter_prepared".into(),
                    })
                }
                pillow_runtime_slots::PreparedSlot::GrokShell(_) => Err(command_error(
                    "runtime_slot_path_not_grok_shell_expressible",
                    "Path resolution selected NativeAdapter but prepare returned GrokShell.",
                )),
            }
        }
        pillow_runtime_slots::SlotExecutionPath::GrokShell => {
            let resolved = settings
                .resolve_runtime(Some(&project_id), &process_env_map())
                .map_err(|error| command_error(error.code(), &error.to_string()))?;
            let executable =
                discover_grok_executable_with_override(resolved.grok_executable.value.as_deref())
                    .map_err(|error| command_error(error.code, &error.message))?;
            // prepare_grok fails closed if the slot actually requires NativeAdapter.
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
    }
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

fn slot_to_dto(view: RuntimeSlotView, is_active: bool) -> RuntimeSlotDto {
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
        account_label,
        access_expires_at,
    ) = match &slot.auth {
        SlotAuth::GrokLogin { model_id } => (
            slot.auth.wire_kind().into(),
            slot.auth.wire_auth_mode().map(str::to_owned),
            None,
            model_id.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        SlotAuth::ApiKey {
            base_url,
            model_id,
            secret_ref,
        } => (
            slot.auth.wire_kind().into(),
            slot.auth.wire_auth_mode().map(str::to_owned),
            Some(base_url.clone()),
            Some(model_id.clone()),
            Some(secret_ref.clone()),
            None,
            None,
            None,
            None,
            None,
        ),
        SlotAuth::LocalCli {
            executable_path,
            launch_args,
            status_args,
        } => (
            slot.auth.wire_kind().into(),
            slot.auth.wire_auth_mode().map(str::to_owned),
            None,
            None,
            None,
            Some(executable_path.clone()),
            Some(launch_args.clone()),
            Some(status_args.clone()),
            None,
            None,
        ),
        // OAuth: closed wire labels; never surface DPAPI secret refs.
        SlotAuth::Oauth {
            base_url,
            model_id,
            account_label,
            access_expires_at,
            ..
        } => (
            slot.auth.wire_kind().into(),
            slot.auth.wire_auth_mode().map(str::to_owned),
            Some(base_url.clone()),
            Some(model_id.clone()),
            None,
            None,
            None,
            None,
            account_label.clone(),
            Some(access_expires_at.to_rfc3339()),
        ),
    };
    let (role, role_custom_name) = match &slot.role {
        SlotRole::Controller => ("controller".into(), None),
        SlotRole::Librarian => ("librarian".into(), None),
        SlotRole::Custom(name) => ("custom".into(), Some(name.clone())),
    };
    // Read the derived governance before moving any owned field out of `slot`.
    let governance = governance_label(slot.governance()).into();
    RuntimeSlotDto {
        slot_id: slot.id.0.to_string(),
        project_id: slot.project_id.0.to_string(),
        display_name: slot.display_name,
        kind,
        governance,
        auth_mode,
        base_url,
        model_id,
        secret_ref,
        secret_configured: view.secret_configured,
        executable_path,
        launch_args,
        status_args,
        role,
        role_custom_name,
        created_at: slot.created_at.to_rfc3339(),
        updated_at: slot.updated_at.to_rfc3339(),
        account_label,
        access_expires_at,
        is_active,
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

fn parse_role(
    role: Option<String>,
    role_custom_name: Option<String>,
) -> Result<SlotRole, CommandError> {
    match role.as_deref() {
        None | Some("") | Some("custom") => {
            let name = role_custom_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| keeper_storage_sqlite::DEFAULT_CUSTOM_ROLE_NAME.to_owned());
            Ok(SlotRole::Custom(name))
        }
        Some("controller") => Ok(SlotRole::Controller),
        Some("librarian") => Ok(SlotRole::Librarian),
        Some(_) => Err(command_error("runtime_slot_input_invalid", "Unknown role.")),
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
