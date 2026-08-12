//! Host-side runtime-slot foundation.

mod catalog;
mod grok_config;
mod secret;
mod status;

use std::path::{Path, PathBuf};

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
pub use secret::{DpapiFileSecretStore, SecretBytes, SecretStore};

pub const GROK_API_KEY_ENV: &str = "PILLOW_GROK_SLOT_API_KEY";

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
}

impl RuntimeSlotService {
    pub fn open(
        database_path: impl AsRef<Path>,
        app_data_dir: impl AsRef<Path>,
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
        let secret_ref = match &existing.auth {
            SlotAuth::ApiKey { secret_ref, .. } => Some(secret_ref.clone()),
            _ => None,
        };

        if let Some(secret_ref) = &secret_ref {
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
        let secret_configured = match &slot.auth {
            SlotAuth::ApiKey { secret_ref, .. } => self.secrets.exists(secret_ref)?,
            _ => false,
        };
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
                let secret_configured = match &slot.auth {
                    SlotAuth::ApiKey { secret_ref, .. } => self.secrets.exists(secret_ref)?,
                    _ => false,
                };
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
            SlotAuth::ApiKey { .. } | SlotAuth::GrokLogin { .. } => {
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
}

/// Service-level auth update (ApiKey may rotate secret material).
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
