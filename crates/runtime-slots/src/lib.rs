//! Host-side runtime-slot foundation.

mod catalog;
mod grok_config;
mod oauth;
mod secret;
mod status;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use keeper_project::ProjectId;
use keeper_storage_sqlite::{
    NewRuntimeSlot, RuntimeSlot, RuntimeSlotId, SlotAuth, SlotRole, SqliteProjectRepository,
    SqliteRuntimeSlotRepository, UpdateGrokApiKeySlot, UpdateRuntimeSlot, UpdateRuntimeSlotModel,
    UpdateSlotAuth,
};
use thiserror::Error;
use uuid::Uuid;

pub use catalog::{
    ModelCatalog, ModelCatalogAvailability, ModelCatalogSource, ModelCatalogUnsupportedReason,
    fetch_openai_model_catalog,
};
pub use oauth::{
    ACCESS_TOKEN_REFRESH_SKEW, PkceCodes, TokenResponse, access_token_needs_refresh,
    access_token_needs_refresh_now, build_authorize_url, build_code_exchange_form,
    build_refresh_token_form, expiry_from_expires_in, generate_pkce, parse_token_response,
    s256_challenge, verify_s256_challenge,
};
pub use secret::{DpapiFileSecretStore, SecretBytes, SecretStore};

pub const GROK_API_KEY_ENV: &str = "PILLOW_GROK_SLOT_API_KEY";

/// Injectable token-endpoint transport. Production network is a later ticket;
/// tests inject canned JSON bodies. Never logs form bodies (may hold tokens).
pub trait OauthTokenClient: Send + Sync {
    fn post_token_form(
        &self,
        token_endpoint: &str,
        form_body: &str,
    ) -> Result<String, RuntimeSlotError>;
}

#[derive(Debug, Error)]
pub enum RuntimeSlotError {
    #[error("runtime slot input is invalid")]
    InvalidInput,
    #[error("runtime slot was not found in this project")]
    SlotNotFound,
    #[error("runtime slot is not a governed Grok slot")]
    NotGovernedGrok,
    #[error("runtime slot metadata is unavailable")]
    MetadataUnavailable,
    #[error("runtime slot project is unavailable")]
    ProjectUnavailable,
    #[error("runtime slot secret already exists")]
    SecretAlreadyExists,
    #[error("runtime slot secret is missing")]
    SecretMissing,
    #[error("runtime slot secret storage is unavailable")]
    SecretStoreUnavailable,
    #[error("runtime slot secret storage is unsupported on this platform")]
    SecretStoreUnsupported,
    #[error("runtime slot private home contains a forbidden auth file")]
    PrivateHomeContaminated,
    #[error("runtime slot private configuration is unavailable")]
    ConfigUnavailable,
    #[error("external CLI account status is unavailable")]
    ExternalStatusUnavailable,
    #[error("runtime slot model catalog is unavailable")]
    ModelCatalogUnavailable,
    #[error("runtime slot model catalog response is invalid")]
    ModelCatalogInvalid,
    #[error("project already has a controller slot")]
    ControllerAlreadyExists,
    #[error("controller role requires governed auth")]
    ControllerRequiresGovernedAuth,
    #[error("oauth access token refresh failed")]
    OauthRefreshFailed,
    #[error("oauth token response is invalid")]
    OauthTokenResponseInvalid,
}

impl RuntimeSlotError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "runtime_slot_input_invalid",
            Self::SlotNotFound => "runtime_slot_not_found",
            Self::NotGovernedGrok => "runtime_slot_not_governed_grok",
            Self::MetadataUnavailable => "runtime_slot_store_unavailable",
            Self::ProjectUnavailable => "runtime_slot_project_unavailable",
            Self::SecretAlreadyExists => "runtime_slot_secret_conflict",
            Self::SecretMissing => "runtime_slot_secret_missing",
            Self::SecretStoreUnavailable => "runtime_slot_secret_unavailable",
            Self::SecretStoreUnsupported => "runtime_slot_secret_unsupported",
            Self::PrivateHomeContaminated => "runtime_slot_private_home_contaminated",
            Self::ConfigUnavailable => "runtime_slot_config_unavailable",
            Self::ExternalStatusUnavailable => "external_cli_status_unavailable",
            Self::ModelCatalogUnavailable => "runtime_slot_model_catalog_unavailable",
            Self::ModelCatalogInvalid => "runtime_slot_model_catalog_invalid",
            Self::ControllerAlreadyExists => "runtime_slot_controller_already_exists",
            Self::ControllerRequiresGovernedAuth => {
                "runtime_slot_controller_requires_governed_auth"
            }
            Self::OauthRefreshFailed => "runtime_slot_oauth_refresh_failed",
            Self::OauthTokenResponseInvalid => "runtime_slot_oauth_token_response_invalid",
        }
    }
}

fn map_store_error(error: keeper_storage_sqlite::RuntimeSlotStoreError) -> RuntimeSlotError {
    match error {
        keeper_storage_sqlite::RuntimeSlotStoreError::NotFound => RuntimeSlotError::SlotNotFound,
        keeper_storage_sqlite::RuntimeSlotStoreError::InvalidShape => {
            RuntimeSlotError::InvalidInput
        }
        keeper_storage_sqlite::RuntimeSlotStoreError::ControllerConflict => {
            RuntimeSlotError::ControllerAlreadyExists
        }
        keeper_storage_sqlite::RuntimeSlotStoreError::ControllerRequiresGovernedAuth => {
            RuntimeSlotError::ControllerRequiresGovernedAuth
        }
        _ => RuntimeSlotError::MetadataUnavailable,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSlotView {
    pub slot: RuntimeSlot,
    pub secret_configured: bool,
}

/// Auth payload for create (secrets as host SecretBytes, not yet referenced).
/// No Debug: may hold SecretBytes.
pub enum CreateSlotAuth {
    ApiKey {
        base_url: String,
        model_id: String,
        api_key: SecretBytes,
    },
    GrokLogin {
        model_id: Option<String>,
    },
    LocalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
    /// Standard OIDC/OAuth only — not consumer-subscription backends.
    Oauth {
        provider_id: String,
        issuer: String,
        client_id: String,
        scopes: Vec<String>,
        token_endpoint: String,
        account_label: Option<String>,
        base_url: String,
        model_id: String,
        access_token: SecretBytes,
        refresh_token: SecretBytes,
        access_expires_at: DateTime<Utc>,
    },
}

pub enum PreparedGrokSlot {
    /// Uses the current user's default Grok profile. No HOME redirect is supplied.
    Login { model_id: Option<String> },
    /// Uses a slot-private home and a secret injected only into the child environment.
    ApiKey {
        private_home: PathBuf,
        model_id: String,
        api_key: SecretBytes,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCliStatus {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
}

pub struct RuntimeSlotService {
    slots: SqliteRuntimeSlotRepository,
    projects: SqliteProjectRepository,
    secrets: DpapiFileSecretStore,
    private_homes: PathBuf,
    /// Optional token-endpoint client for OAuth refresh-on-prepare.
    oauth_client: Option<Arc<dyn OauthTokenClient>>,
}

impl RuntimeSlotService {
    pub fn open(
        database_path: impl AsRef<Path>,
        app_data_dir: impl AsRef<Path>,
    ) -> Result<Self, RuntimeSlotError> {
        Self::open_with_oauth_client(database_path, app_data_dir, None)
    }

    pub fn open_with_oauth_client(
        database_path: impl AsRef<Path>,
        app_data_dir: impl AsRef<Path>,
        oauth_client: Option<Arc<dyn OauthTokenClient>>,
    ) -> Result<Self, RuntimeSlotError> {
        let database_path = database_path.as_ref();
        let app_data_dir = app_data_dir.as_ref();
        Ok(Self {
            slots: SqliteRuntimeSlotRepository::open(database_path)
                .map_err(|_| RuntimeSlotError::MetadataUnavailable)?,
            projects: SqliteProjectRepository::open(database_path)
                .map_err(|_| RuntimeSlotError::ProjectUnavailable)?,
            secrets: DpapiFileSecretStore::new(app_data_dir.join("secrets")),
            private_homes: app_data_dir.join("runtime-slots"),
            oauth_client,
        })
    }

    /// Unified create: one record shape, auth is a field, role is a field.
    pub fn create_slot(
        &self,
        project_id: ProjectId,
        display_name: String,
        auth: CreateSlotAuth,
        role: SlotRole,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        match auth {
            CreateSlotAuth::GrokLogin { model_id } => {
                if let Some(ref model_id) = model_id {
                    validate_model(model_id)?;
                }
                let slot = self
                    .slots
                    .create(NewRuntimeSlot {
                        project_id,
                        display_name,
                        auth: SlotAuth::GrokLogin { model_id },
                        role,
                    })
                    .map_err(map_store_error)?;
                Ok(RuntimeSlotView {
                    slot,
                    secret_configured: false,
                })
            }
            CreateSlotAuth::ApiKey {
                base_url,
                model_id,
                api_key,
            } => {
                validate_endpoint(&base_url)?;
                validate_model(&model_id)?;
                if api_key.is_empty() || api_key.as_bytes().contains(&0) {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                let secret_ref = Uuid::new_v4().to_string();
                self.secrets.put(&secret_ref, &api_key)?;
                let created = self.slots.create(NewRuntimeSlot {
                    project_id,
                    display_name,
                    auth: SlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref: secret_ref.clone(),
                    },
                    role,
                });
                let slot = match created {
                    Ok(slot) => slot,
                    Err(error) => {
                        self.secrets.delete(&secret_ref)?;
                        return Err(map_store_error(error));
                    }
                };
                Ok(RuntimeSlotView {
                    slot,
                    secret_configured: true,
                })
            }
            CreateSlotAuth::LocalCli {
                executable_path,
                launch_args,
                status_args,
            } => {
                let executable = Path::new(&executable_path);
                if !executable.is_absolute() || !executable.is_file() || status_args.is_empty() {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                let slot = self
                    .slots
                    .create(NewRuntimeSlot {
                        project_id,
                        display_name,
                        auth: SlotAuth::LocalCli {
                            executable_path,
                            launch_args,
                            status_args,
                        },
                        role,
                    })
                    .map_err(map_store_error)?;
                Ok(RuntimeSlotView {
                    slot,
                    secret_configured: false,
                })
            }
            CreateSlotAuth::Oauth {
                provider_id,
                issuer,
                client_id,
                scopes,
                token_endpoint,
                account_label,
                base_url,
                model_id,
                access_token,
                refresh_token,
                access_expires_at,
            } => {
                validate_endpoint(&base_url)?;
                validate_endpoint(&token_endpoint)?;
                validate_endpoint(&issuer)?;
                validate_model(&model_id)?;
                if provider_id.trim().is_empty()
                    || client_id.trim().is_empty()
                    || scopes.is_empty()
                    || access_token.is_empty()
                    || refresh_token.is_empty()
                    || access_token.as_bytes().contains(&0)
                    || refresh_token.as_bytes().contains(&0)
                {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                let access_secret_ref = Uuid::new_v4().to_string();
                let refresh_secret_ref = Uuid::new_v4().to_string();
                self.secrets.put(&access_secret_ref, &access_token)?;
                if let Err(error) = self.secrets.put(&refresh_secret_ref, &refresh_token) {
                    let _ = self.secrets.delete(&access_secret_ref);
                    return Err(error);
                }
                let created = self.slots.create(NewRuntimeSlot {
                    project_id,
                    display_name,
                    auth: SlotAuth::Oauth {
                        provider_id,
                        issuer,
                        client_id,
                        scopes,
                        token_endpoint,
                        account_label,
                        base_url,
                        model_id,
                        access_secret_ref: access_secret_ref.clone(),
                        refresh_secret_ref: refresh_secret_ref.clone(),
                        access_expires_at,
                    },
                    role,
                });
                let slot = match created {
                    Ok(slot) => slot,
                    Err(error) => {
                        let _ = self.secrets.delete(&access_secret_ref);
                        let _ = self.secrets.delete(&refresh_secret_ref);
                        return Err(map_store_error(error));
                    }
                };
                Ok(RuntimeSlotView {
                    slot,
                    secret_configured: true,
                })
            }
        }
    }

    /// Thin wrapper — default role is Custom("未分配").
    pub fn create_grok_login(
        &self,
        project_id: ProjectId,
        display_name: String,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.create_slot(
            project_id,
            display_name,
            CreateSlotAuth::GrokLogin { model_id: None },
            SlotRole::default_unassigned(),
        )
    }

    /// Thin wrapper — default role is Custom("未分配").
    pub fn create_grok_api_key(
        &self,
        project_id: ProjectId,
        display_name: String,
        base_url: String,
        model_id: String,
        api_key: SecretBytes,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.create_slot(
            project_id,
            display_name,
            CreateSlotAuth::ApiKey {
                base_url,
                model_id,
                api_key,
            },
            SlotRole::default_unassigned(),
        )
    }

    /// Thin wrapper — default role is Custom("未分配").
    pub fn create_external_cli(
        &self,
        project_id: ProjectId,
        display_name: String,
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.create_slot(
            project_id,
            display_name,
            CreateSlotAuth::LocalCli {
                executable_path,
                launch_args,
                status_args,
            },
            SlotRole::default_unassigned(),
        )
    }

    /// Update a governed Grok API-key slot.
    ///
    /// `api_key == None` keeps the existing DPAPI ciphertext and opaque ref
    /// (omit-means-keep). `Some` rotates: a new opaque ref is written first,
    /// metadata is updated to point at it, then the prior ciphertext is deleted
    /// so the old material cannot be recovered through this store.
    pub fn update_grok_api_key(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        display_name: String,
        base_url: String,
        model_id: String,
        api_key: Option<SecretBytes>,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        validate_endpoint(&base_url)?;
        validate_model(&model_id)?;
        let existing = self.get(project_id, slot_id)?;
        let previous_secret_ref = match &existing.auth {
            SlotAuth::ApiKey { secret_ref, .. } => secret_ref.clone(),
            // Update command is API-key only; login/local slots must not share this path.
            _ => return Err(RuntimeSlotError::InvalidInput),
        };

        let (secret_ref, rotated_from) = match api_key {
            None => (previous_secret_ref, None),
            Some(secret) => {
                if secret.is_empty() || secret.as_bytes().contains(&0) {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                let new_ref = Uuid::new_v4().to_string();
                self.secrets.put(&new_ref, &secret)?;
                (new_ref, Some(previous_secret_ref))
            }
        };

        let updated = self.slots.update_grok_api_key(UpdateGrokApiKeySlot {
            project_id,
            slot_id,
            display_name,
            base_url,
            model_id,
            secret_ref: secret_ref.clone(),
        });
        let slot = match updated {
            Ok(slot) => slot,
            Err(error) => {
                if rotated_from.is_some() {
                    // Roll back the newly written ciphertext so a failed metadata
                    // write does not leave an unreachable secret.
                    let _ = self.secrets.delete(&secret_ref);
                }
                return Err(map_store_error(error));
            }
        };

        if let Some(old_ref) = rotated_from {
            // Destroy prior ciphertext after the new ref is durable in metadata.
            // Failure is surfaced so operators can retry cleanup; the slot already
            // points at the new ref and prepare uses only that ref.
            self.secrets.delete(&old_ref)?;
        }

        Ok(RuntimeSlotView {
            slot,
            secret_configured: self.secrets.exists(&secret_ref)?,
        })
    }

    /// Uniform update: display name, role, and auth fields (auth kind cannot change).
    ///
    /// For ApiKey, `api_key == None` means keep stored secret; `Some` rotates.
    pub fn update_slot(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        display_name: String,
        role: SlotRole,
        auth_update: UpdateSlotAuthInput,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        let existing = self.get(project_id, slot_id)?;

        let (store_auth, secret_configured_path) = match (existing.auth, auth_update) {
            (auth, UpdateSlotAuthInput::Keep) => {
                let secret_configured = match &auth {
                    SlotAuth::ApiKey { secret_ref, .. } => self.secrets.exists(secret_ref)?,
                    SlotAuth::Oauth {
                        access_secret_ref,
                        refresh_secret_ref,
                        ..
                    } => {
                        self.secrets.exists(access_secret_ref)?
                            && self.secrets.exists(refresh_secret_ref)?
                    }
                    _ => false,
                };
                (UpdateSlotAuth::Keep, secret_configured)
            }
            (
                SlotAuth::ApiKey {
                    secret_ref: previous_secret_ref,
                    ..
                },
                UpdateSlotAuthInput::ApiKey {
                    base_url,
                    model_id,
                    api_key,
                },
            ) => {
                validate_endpoint(&base_url)?;
                validate_model(&model_id)?;
                let (secret_ref, rotated_from) = match api_key {
                    None => (previous_secret_ref, None),
                    Some(secret) => {
                        if secret.is_empty() || secret.as_bytes().contains(&0) {
                            return Err(RuntimeSlotError::InvalidInput);
                        }
                        let new_ref = Uuid::new_v4().to_string();
                        self.secrets.put(&new_ref, &secret)?;
                        (new_ref, Some(previous_secret_ref))
                    }
                };
                let updated = self.slots.update(UpdateRuntimeSlot {
                    project_id,
                    slot_id,
                    display_name: display_name.clone(),
                    role: role.clone(),
                    auth: UpdateSlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref: secret_ref.clone(),
                    },
                });
                let slot = match updated {
                    Ok(slot) => slot,
                    Err(error) => {
                        if rotated_from.is_some() {
                            let _ = self.secrets.delete(&secret_ref);
                        }
                        return Err(map_store_error(error));
                    }
                };
                if let Some(old_ref) = rotated_from {
                    self.secrets.delete(&old_ref)?;
                }
                return Ok(RuntimeSlotView {
                    slot,
                    secret_configured: self.secrets.exists(&secret_ref)?,
                });
            }
            (SlotAuth::GrokLogin { .. }, UpdateSlotAuthInput::GrokLogin { model_id }) => {
                if let Some(ref model_id) = model_id {
                    validate_model(model_id)?;
                }
                (UpdateSlotAuth::GrokLogin { model_id }, false)
            }
            (
                SlotAuth::LocalCli { .. },
                UpdateSlotAuthInput::LocalCli {
                    executable_path,
                    launch_args,
                    status_args,
                },
            ) => {
                let executable = Path::new(&executable_path);
                if !executable.is_absolute() || !executable.is_file() || status_args.is_empty() {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                (
                    UpdateSlotAuth::LocalCli {
                        executable_path,
                        launch_args,
                        status_args,
                    },
                    false,
                )
            }
            (
                SlotAuth::Oauth {
                    access_secret_ref: previous_access,
                    refresh_secret_ref: previous_refresh,
                    ..
                },
                UpdateSlotAuthInput::Oauth {
                    provider_id,
                    issuer,
                    client_id,
                    scopes,
                    token_endpoint,
                    account_label,
                    base_url,
                    model_id,
                    access_token,
                    refresh_token,
                    access_expires_at,
                },
            ) => {
                validate_endpoint(&base_url)?;
                validate_endpoint(&token_endpoint)?;
                validate_endpoint(&issuer)?;
                validate_model(&model_id)?;
                if provider_id.trim().is_empty() || client_id.trim().is_empty() || scopes.is_empty()
                {
                    return Err(RuntimeSlotError::InvalidInput);
                }
                let (access_secret_ref, access_rotated) = match access_token {
                    None => (previous_access, None),
                    Some(secret) => {
                        if secret.is_empty() || secret.as_bytes().contains(&0) {
                            return Err(RuntimeSlotError::InvalidInput);
                        }
                        let new_ref = Uuid::new_v4().to_string();
                        self.secrets.put(&new_ref, &secret)?;
                        (new_ref, Some(previous_access))
                    }
                };
                let (refresh_secret_ref, refresh_rotated) = match refresh_token {
                    None => (previous_refresh, None),
                    Some(secret) => {
                        if secret.is_empty() || secret.as_bytes().contains(&0) {
                            if access_rotated.is_some() {
                                let _ = self.secrets.delete(&access_secret_ref);
                            }
                            return Err(RuntimeSlotError::InvalidInput);
                        }
                        let new_ref = Uuid::new_v4().to_string();
                        if let Err(error) = self.secrets.put(&new_ref, &secret) {
                            if access_rotated.is_some() {
                                let _ = self.secrets.delete(&access_secret_ref);
                            }
                            return Err(error);
                        }
                        (new_ref, Some(previous_refresh))
                    }
                };
                let updated = self.slots.update(UpdateRuntimeSlot {
                    project_id,
                    slot_id,
                    display_name: display_name.clone(),
                    role: role.clone(),
                    auth: UpdateSlotAuth::Oauth {
                        provider_id,
                        issuer,
                        client_id,
                        scopes,
                        token_endpoint,
                        account_label,
                        base_url,
                        model_id,
                        access_secret_ref: access_secret_ref.clone(),
                        refresh_secret_ref: refresh_secret_ref.clone(),
                        access_expires_at,
                    },
                });
                let slot = match updated {
                    Ok(slot) => slot,
                    Err(error) => {
                        if access_rotated.is_some() {
                            let _ = self.secrets.delete(&access_secret_ref);
                        }
                        if refresh_rotated.is_some() {
                            let _ = self.secrets.delete(&refresh_secret_ref);
                        }
                        return Err(map_store_error(error));
                    }
                };
                if let Some(old) = access_rotated {
                    self.secrets.delete(&old)?;
                }
                if let Some(old) = refresh_rotated {
                    self.secrets.delete(&old)?;
                }
                return Ok(RuntimeSlotView {
                    slot,
                    secret_configured: self.secrets.exists(&access_secret_ref)?
                        && self.secrets.exists(&refresh_secret_ref)?,
                });
            }
            _ => return Err(RuntimeSlotError::InvalidInput),
        };

        // Keep path or non-ApiKey updates go through store.update once.
        if matches!(store_auth, UpdateSlotAuth::Keep)
            || matches!(store_auth, UpdateSlotAuth::GrokLogin { .. })
            || matches!(store_auth, UpdateSlotAuth::LocalCli { .. })
        {
            let slot = self
                .slots
                .update(UpdateRuntimeSlot {
                    project_id,
                    slot_id,
                    display_name,
                    role,
                    auth: store_auth,
                })
                .map_err(map_store_error)?;
            return Ok(RuntimeSlotView {
                slot,
                secret_configured: secret_configured_path,
            });
        }
        Err(RuntimeSlotError::InvalidInput)
    }

    /// Delete a project-scoped slot and destroy any associated DPAPI secret.
    ///
    /// Order prioritizes no orphan ciphertext: secret is removed before the
    /// metadata row when present. Private per-slot home directories are removed
    /// best-effort after the row is gone.
    pub fn delete(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<(), RuntimeSlotError> {
        self.ensure_project(project_id)?;
        let existing = self.get(project_id, slot_id)?;
        let secret_refs: Vec<String> = match &existing.auth {
            SlotAuth::ApiKey { secret_ref, .. } => vec![secret_ref.clone()],
            SlotAuth::Oauth {
                access_secret_ref,
                refresh_secret_ref,
                ..
            } => vec![access_secret_ref.clone(), refresh_secret_ref.clone()],
            _ => vec![],
        };

        for secret_ref in &secret_refs {
            self.secrets.delete(secret_ref)?;
        }

        let removed = self
            .slots
            .delete(project_id, slot_id)
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?;
        if removed.is_none() {
            return Err(RuntimeSlotError::SlotNotFound);
        }

        let private_home = self
            .private_homes
            .join(project_id.0.to_string())
            .join(slot_id.0.to_string());
        if private_home.exists() {
            let _ = std::fs::remove_dir_all(&private_home);
        }
        Ok(())
    }

    pub fn update_model(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        model_id: String,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        validate_model(&model_id)?;
        let slot = self
            .slots
            .update_model(UpdateRuntimeSlotModel {
                project_id,
                slot_id,
                model_id,
            })
            .map_err(map_store_error)?;
        let secret_configured = self.secret_configured_for(&slot.auth)?;
        Ok(RuntimeSlotView {
            slot,
            secret_configured,
        })
    }

    pub fn list(&self, project_id: ProjectId) -> Result<Vec<RuntimeSlotView>, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        self.slots
            .list(project_id)
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?
            .into_iter()
            .map(|slot| {
                let secret_configured = self.secret_configured_for(&slot.auth)?;
                Ok(RuntimeSlotView {
                    slot,
                    secret_configured,
                })
            })
            .collect()
    }

    pub fn prepare_grok(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<PreparedGrokSlot, RuntimeSlotError> {
        let slot = self.get(project_id, slot_id)?;
        match slot.auth {
            SlotAuth::GrokLogin { model_id } => Ok(PreparedGrokSlot::Login { model_id }),
            SlotAuth::ApiKey {
                base_url,
                model_id,
                secret_ref,
            } => {
                let api_key = self.secrets.get(&secret_ref)?;
                let private_home = grok_config::write_private_grok_config(
                    &self.private_homes,
                    project_id,
                    slot_id,
                    &slot.display_name,
                    &base_url,
                    &model_id,
                )?;
                Ok(PreparedGrokSlot::ApiKey {
                    private_home,
                    model_id,
                    api_key,
                })
            }
            SlotAuth::Oauth {
                client_id,
                token_endpoint,
                base_url,
                model_id,
                access_secret_ref,
                refresh_secret_ref,
                access_expires_at,
                provider_id: _,
                issuer: _,
                scopes: _,
                account_label: _,
            } => self.prepare_oauth_slot(
                project_id,
                slot_id,
                &slot.display_name,
                &client_id,
                &token_endpoint,
                &base_url,
                &model_id,
                &access_secret_ref,
                &refresh_secret_ref,
                access_expires_at,
            ),
            SlotAuth::LocalCli { .. } => Err(RuntimeSlotError::NotGovernedGrok),
        }
    }

    pub fn external_status(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<ExternalCliStatus, RuntimeSlotError> {
        let slot = self.get(project_id, slot_id)?;
        let project = self
            .projects
            .get(project_id)
            .map_err(|_| RuntimeSlotError::ProjectUnavailable)?
            .ok_or(RuntimeSlotError::ProjectUnavailable)?;
        match slot.auth {
            SlotAuth::LocalCli {
                executable_path,
                status_args,
                ..
            } => status::run_status(
                &executable_path,
                &status_args,
                Path::new(&project.root_path),
            ),
            SlotAuth::ApiKey { .. } | SlotAuth::GrokLogin { .. } | SlotAuth::Oauth { .. } => {
                Err(RuntimeSlotError::InvalidInput)
            }
        }
    }

    /// Fetch a bounded, secret-free model catalog for a project-scoped slot.
    ///
    /// API-key material is decrypted only inside this host service and is
    /// consumed directly by the HTTP transport. Login slots use the configured
    /// Grok executable; LocalCli slots return an explicit unsupported
    /// outcome so the renderer can retain manual input.
    pub fn model_catalog(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        login_executable: Option<&Path>,
        project_root: &Path,
    ) -> Result<ModelCatalog, RuntimeSlotError> {
        let slot = self.get(project_id, slot_id)?;
        match slot.auth {
            SlotAuth::GrokLogin { .. } => {
                let executable = login_executable.ok_or(RuntimeSlotError::InvalidInput)?;
                catalog::fetch_grok_model_catalog(executable, project_root)
            }
            SlotAuth::ApiKey {
                base_url,
                secret_ref,
                ..
            } => {
                let api_key = self.secrets.get(&secret_ref)?;
                catalog::fetch_openai_model_catalog(&base_url, &api_key)
            }
            SlotAuth::Oauth {
                base_url,
                access_secret_ref,
                access_expires_at,
                refresh_secret_ref,
                client_id,
                token_endpoint,
                ..
            } => {
                // Catalog uses a valid access token only (never refresh).
                let access = self.resolve_oauth_access_token(
                    project_id,
                    slot_id,
                    &client_id,
                    &token_endpoint,
                    &access_secret_ref,
                    &refresh_secret_ref,
                    access_expires_at,
                )?;
                catalog::fetch_openai_model_catalog(&base_url, &access)
            }
            SlotAuth::LocalCli { .. } => Ok(ModelCatalog::unsupported(
                ModelCatalogSource::ExternalCli,
                ModelCatalogUnsupportedReason::ExternalCli,
            )),
        }
    }

    fn get(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<RuntimeSlot, RuntimeSlotError> {
        self.slots
            .get(project_id, slot_id)
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?
            .ok_or(RuntimeSlotError::SlotNotFound)
    }

    fn ensure_project(&self, project_id: ProjectId) -> Result<(), RuntimeSlotError> {
        self.projects
            .get(project_id)
            .map_err(|_| RuntimeSlotError::ProjectUnavailable)?
            .map(|_| ())
            .ok_or(RuntimeSlotError::ProjectUnavailable)
    }

    fn secret_configured_for(&self, auth: &SlotAuth) -> Result<bool, RuntimeSlotError> {
        match auth {
            SlotAuth::ApiKey { secret_ref, .. } => self.secrets.exists(secret_ref),
            SlotAuth::Oauth {
                access_secret_ref,
                refresh_secret_ref,
                ..
            } => Ok(self.secrets.exists(access_secret_ref)?
                && self.secrets.exists(refresh_secret_ref)?),
            _ => Ok(false),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_oauth_slot(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        display_name: &str,
        client_id: &str,
        token_endpoint: &str,
        base_url: &str,
        model_id: &str,
        access_secret_ref: &str,
        refresh_secret_ref: &str,
        access_expires_at: DateTime<Utc>,
    ) -> Result<PreparedGrokSlot, RuntimeSlotError> {
        let access = self.resolve_oauth_access_token(
            project_id,
            slot_id,
            client_id,
            token_endpoint,
            access_secret_ref,
            refresh_secret_ref,
            access_expires_at,
        )?;
        let private_home = grok_config::write_private_grok_config(
            &self.private_homes,
            project_id,
            slot_id,
            display_name,
            base_url,
            model_id,
        )?;
        Ok(PreparedGrokSlot::ApiKey {
            private_home,
            model_id: model_id.to_owned(),
            api_key: access,
        })
    }

    /// Return a usable access token, refreshing first when expired.
    /// Refresh failure is hard-fail (no silent use of expired access).
    /// Only the access token is returned — never the refresh token.
    #[allow(clippy::too_many_arguments)]
    fn resolve_oauth_access_token(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
        client_id: &str,
        token_endpoint: &str,
        access_secret_ref: &str,
        refresh_secret_ref: &str,
        access_expires_at: DateTime<Utc>,
    ) -> Result<SecretBytes, RuntimeSlotError> {
        let expires_at = datetime_to_system_time(access_expires_at);
        if !access_token_needs_refresh(expires_at, SystemTime::now(), ACCESS_TOKEN_REFRESH_SKEW) {
            return self.secrets.get(access_secret_ref);
        }

        let client = self
            .oauth_client
            .as_ref()
            .ok_or(RuntimeSlotError::OauthRefreshFailed)?;

        let refresh = self.secrets.get(refresh_secret_ref)?;
        let refresh_str = std::str::from_utf8(refresh.as_bytes())
            .map_err(|_| RuntimeSlotError::OauthRefreshFailed)?;
        let form = build_refresh_token_form(client_id, refresh_str)?;
        // Drop refresh SecretBytes before network-shaped call returns.
        drop(refresh);

        let body = client
            .post_token_form(token_endpoint, &form)
            .map_err(|_| RuntimeSlotError::OauthRefreshFailed)?;
        // Best-effort zero of form (contains refresh token).
        drop(form);

        let tokens = parse_token_response(&body)?;
        let new_access_ref = Uuid::new_v4().to_string();
        let new_refresh_ref = match &tokens.refresh_token {
            Some(_) => Uuid::new_v4().to_string(),
            None => refresh_secret_ref.to_owned(),
        };

        self.secrets.put(&new_access_ref, &tokens.access_token)?;
        if let Some(new_refresh) = &tokens.refresh_token
            && let Err(error) = self.secrets.put(&new_refresh_ref, new_refresh)
        {
            let _ = self.secrets.delete(&new_access_ref);
            return Err(error);
        }

        let expires_in = tokens.expires_in.unwrap_or(3600);
        let new_expires = Utc::now() + chrono::Duration::seconds(expires_in as i64);

        // Reload full auth metadata to preserve non-token fields.
        let existing = self.get(project_id, slot_id)?;
        let (provider_id, issuer, scopes, account_label, base_url, model_id) = match existing.auth {
            SlotAuth::Oauth {
                provider_id,
                issuer,
                scopes,
                account_label,
                base_url,
                model_id,
                ..
            } => (
                provider_id,
                issuer,
                scopes,
                account_label,
                base_url,
                model_id,
            ),
            _ => return Err(RuntimeSlotError::InvalidInput),
        };

        let updated = self.slots.update(UpdateRuntimeSlot {
            project_id,
            slot_id,
            display_name: existing.display_name,
            role: existing.role,
            auth: UpdateSlotAuth::Oauth {
                provider_id,
                issuer,
                client_id: client_id.to_owned(),
                scopes,
                token_endpoint: token_endpoint.to_owned(),
                account_label,
                base_url,
                model_id,
                access_secret_ref: new_access_ref.clone(),
                refresh_secret_ref: new_refresh_ref.clone(),
                access_expires_at: new_expires,
            },
        });
        if let Err(error) = updated {
            let _ = self.secrets.delete(&new_access_ref);
            if tokens.refresh_token.is_some() {
                let _ = self.secrets.delete(&new_refresh_ref);
            }
            return Err(map_store_error(error));
        }

        // Destroy prior ciphertext after durable metadata update.
        let _ = self.secrets.delete(access_secret_ref);
        if tokens.refresh_token.is_some() && new_refresh_ref != refresh_secret_ref {
            let _ = self.secrets.delete(refresh_secret_ref);
        }

        // Return only the access token for the grok shell.
        Ok(tokens.access_token)
    }
}

fn datetime_to_system_time(value: DateTime<Utc>) -> SystemTime {
    let millis = value.timestamp_millis();
    if millis <= 0 {
        return SystemTime::UNIX_EPOCH;
    }
    SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64)
}

/// Service-level auth update (ApiKey/Oauth may rotate secret material).
/// No Debug: may hold SecretBytes.
pub enum UpdateSlotAuthInput {
    Keep,
    ApiKey {
        base_url: String,
        model_id: String,
        api_key: Option<SecretBytes>,
    },
    GrokLogin {
        model_id: Option<String>,
    },
    LocalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
    Oauth {
        provider_id: String,
        issuer: String,
        client_id: String,
        scopes: Vec<String>,
        token_endpoint: String,
        account_label: Option<String>,
        base_url: String,
        model_id: String,
        /// `None` keeps the stored access token.
        access_token: Option<SecretBytes>,
        /// `None` keeps the stored refresh token.
        refresh_token: Option<SecretBytes>,
        access_expires_at: DateTime<Utc>,
    },
}

fn validate_endpoint(value: &str) -> Result<(), RuntimeSlotError> {
    if value.len() > 4_096
        || value.chars().any(char::is_control)
        || !(value.starts_with("https://") || value.starts_with("http://"))
    {
        return Err(RuntimeSlotError::InvalidInput);
    }
    Ok(())
}

fn validate_model(value: &str) -> Result<(), RuntimeSlotError> {
    if value.trim().is_empty() || value.len() > 4_096 || value.chars().any(char::is_control) {
        return Err(RuntimeSlotError::InvalidInput);
    }
    Ok(())
}
