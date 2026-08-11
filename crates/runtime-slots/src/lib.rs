//! Host-side runtime-slot foundation.

mod grok_config;
mod secret;
mod status;

use std::path::{Path, PathBuf};

use keeper_project::ProjectId;
use keeper_storage_sqlite::{
    GrokSlotAuth, NewRuntimeSlot, RuntimeSlot, RuntimeSlotGovernance, RuntimeSlotId,
    RuntimeSlotKind, SqliteProjectRepository, SqliteRuntimeSlotRepository, UpdateGrokApiKeySlot,
};
use thiserror::Error;
use uuid::Uuid;

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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSlotView {
    pub slot: RuntimeSlot,
    pub secret_configured: bool,
}

pub enum PreparedGrokSlot {
    /// Uses the current user's default Grok profile. No HOME redirect is supplied.
    Login,
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

    pub fn create_grok_login(
        &self,
        project_id: ProjectId,
        display_name: String,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        let slot = self
            .slots
            .create(NewRuntimeSlot {
                project_id,
                display_name,
                governance: RuntimeSlotGovernance::Full,
                kind: RuntimeSlotKind::GovernedGrok {
                    auth: GrokSlotAuth::Login,
                },
            })
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?;
        Ok(RuntimeSlotView {
            slot,
            secret_configured: false,
        })
    }

    pub fn create_grok_api_key(
        &self,
        project_id: ProjectId,
        display_name: String,
        base_url: String,
        model_id: String,
        api_key: SecretBytes,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
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
            governance: RuntimeSlotGovernance::Full,
            kind: RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref: secret_ref.clone(),
                },
            },
        });
        let slot = match created {
            Ok(slot) => slot,
            Err(_) => {
                self.secrets.delete(&secret_ref)?;
                return Err(RuntimeSlotError::MetadataUnavailable);
            }
        };
        Ok(RuntimeSlotView {
            slot,
            secret_configured: true,
        })
    }

    pub fn create_external_cli(
        &self,
        project_id: ProjectId,
        display_name: String,
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    ) -> Result<RuntimeSlotView, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        let executable = Path::new(&executable_path);
        if !executable.is_absolute() || !executable.is_file() || status_args.is_empty() {
            return Err(RuntimeSlotError::InvalidInput);
        }
        let slot = self
            .slots
            .create(NewRuntimeSlot {
                project_id,
                display_name,
                governance: RuntimeSlotGovernance::None,
                kind: RuntimeSlotKind::ExternalCli {
                    executable_path,
                    launch_args,
                    status_args,
                },
            })
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?;
        Ok(RuntimeSlotView {
            slot,
            secret_configured: false,
        })
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
        let previous_secret_ref = match &existing.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey { secret_ref, .. },
            } => secret_ref.clone(),
            // Update command is API-key only; login/external slots must not share this path.
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
                return Err(match error {
                    keeper_storage_sqlite::RuntimeSlotStoreError::NotFound => {
                        RuntimeSlotError::SlotNotFound
                    }
                    keeper_storage_sqlite::RuntimeSlotStoreError::InvalidShape => {
                        RuntimeSlotError::InvalidInput
                    }
                    _ => RuntimeSlotError::MetadataUnavailable,
                });
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
        let secret_ref = match &existing.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::ApiKey { secret_ref, .. },
            } => Some(secret_ref.clone()),
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

    pub fn list(&self, project_id: ProjectId) -> Result<Vec<RuntimeSlotView>, RuntimeSlotError> {
        self.ensure_project(project_id)?;
        self.slots
            .list(project_id)
            .map_err(|_| RuntimeSlotError::MetadataUnavailable)?
            .into_iter()
            .map(|slot| {
                let secret_configured = match &slot.kind {
                    RuntimeSlotKind::GovernedGrok {
                        auth: GrokSlotAuth::ApiKey { secret_ref, .. },
                    } => self.secrets.exists(secret_ref)?,
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
        match slot.kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login,
            } => Ok(PreparedGrokSlot::Login),
            RuntimeSlotKind::GovernedGrok {
                auth:
                    GrokSlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref,
                    },
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
            RuntimeSlotKind::ExternalCli { .. } => Err(RuntimeSlotError::NotGovernedGrok),
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
        match slot.kind {
            RuntimeSlotKind::ExternalCli {
                executable_path,
                status_args,
                ..
            } => status::run_status(
                &executable_path,
                &status_args,
                Path::new(&project.root_path),
            ),
            RuntimeSlotKind::GovernedGrok { .. } => Err(RuntimeSlotError::InvalidInput),
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
