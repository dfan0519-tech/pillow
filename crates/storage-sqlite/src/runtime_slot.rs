//! Project-scoped, non-secret runtime-slot metadata.
//!
//! One slot record shape: `auth` + `role`. Governance is derived from `auth`
//! and is never stored independently (contradiction is unrepresentable).

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_project::ProjectId;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use thiserror::Error;
use uuid::Uuid;

const DISPLAY_NAME_MAX_CHARS: usize = 120;
const CONFIG_VALUE_MAX_CHARS: usize = 4_096;
const CUSTOM_ROLE_NAME_MAX_CHARS: usize = 64;
const ARG_COUNT_MAX: usize = 128;
const ARG_MAX_CHARS: usize = 8_192;

/// Default role applied to migrated rows and when callers omit a role.
pub const DEFAULT_CUSTOM_ROLE_NAME: &str = "未分配";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeSlotId(pub Uuid);

/// Derived governance — never an independent stored field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSlotGovernance {
    Full,
    None,
}

impl RuntimeSlotGovernance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::None => "none",
        }
    }
}

/// Authentication shape for a slot. Extensible: new variants may be added;
/// callers must not assume a closed three-variant set forever.
///
/// Tokens and API keys never appear here — only opaque DPAPI secret refs.
/// Consumer-subscription OAuth (ChatGPT Plus / Claude Pro proprietary backends)
/// is intentionally unsupported; only standard OIDC/OAuth providers that can
/// supply a bearer access token for an OpenAI-compatible `base_url` fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotAuth {
    /// Any OpenAI-compatible endpoint with a DPAPI-backed secret reference.
    ApiKey {
        base_url: String,
        model_id: String,
        secret_ref: String,
    },
    /// Grok session login (user profile; optional selected model).
    GrokLogin { model_id: Option<String> },
    /// Local executable registration only — ungoverned, not a conversation runtime.
    LocalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
    /// Standard OIDC/OAuth provider. Access + refresh tokens live in DPAPI
    /// under the two secret refs; metadata below is non-secret.
    Oauth {
        provider_id: String,
        issuer: String,
        client_id: String,
        scopes: Vec<String>,
        token_endpoint: String,
        /// Non-sensitive account label (e.g. email or display name).
        account_label: Option<String>,
        /// OpenAI-compatible base URL for the grok shell.
        base_url: String,
        model_id: String,
        /// DPAPI ref for the access token (injected as env_key at prepare).
        access_secret_ref: String,
        /// DPAPI ref for the refresh token (never given to the grok shell).
        refresh_secret_ref: String,
        /// UTC expiry of the current access token.
        access_expires_at: DateTime<Utc>,
    },
}

impl SlotAuth {
    /// Governance is derived solely from auth. Contradiction cannot be stored.
    pub fn governance(&self) -> RuntimeSlotGovernance {
        governance_from_auth(self)
    }

    pub fn is_governed(&self) -> bool {
        matches!(self.governance(), RuntimeSlotGovernance::Full)
    }

    /// Wire `kind` label retained for existing renderer closed contracts.
    pub fn wire_kind(&self) -> &'static str {
        match self {
            Self::ApiKey { .. } | Self::GrokLogin { .. } | Self::Oauth { .. } => "governed_grok",
            Self::LocalCli { .. } => "external_cli",
        }
    }

    /// Wire `auth_mode` label (null for LocalCli).
    pub fn wire_auth_mode(&self) -> Option<&'static str> {
        match self {
            Self::ApiKey { .. } => Some("api_key"),
            Self::GrokLogin { .. } => Some("login"),
            Self::Oauth { .. } => Some("oauth"),
            Self::LocalCli { .. } => None,
        }
    }
}

/// Derive governance from auth. Mappings are pinned by unit tests.
pub fn governance_from_auth(auth: &SlotAuth) -> RuntimeSlotGovernance {
    match auth {
        SlotAuth::ApiKey { .. } | SlotAuth::GrokLogin { .. } | SlotAuth::Oauth { .. } => {
            RuntimeSlotGovernance::Full
        }
        SlotAuth::LocalCli { .. } => RuntimeSlotGovernance::None,
    }
}

/// Role of the slot within a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotRole {
    /// Project conversation controller — at most one per project; must be governed.
    Controller,
    /// Knowledge / information administrator.
    Librarian,
    /// User-defined role name.
    Custom(String),
}

impl SlotRole {
    pub fn default_unassigned() -> Self {
        Self::Custom(DEFAULT_CUSTOM_ROLE_NAME.to_owned())
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Librarian => "librarian",
            Self::Custom(_) => "custom",
        }
    }

    pub fn custom_name(&self) -> Option<&str> {
        match self {
            Self::Custom(name) => Some(name.as_str()),
            _ => None,
        }
    }
}

/// Compatibility alias: historical Grok auth variants map 1:1 into `SlotAuth`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrokSlotAuth {
    Login {
        model_id: Option<String>,
    },
    ApiKey {
        base_url: String,
        model_id: String,
        secret_ref: String,
    },
}

impl From<GrokSlotAuth> for SlotAuth {
    fn from(value: GrokSlotAuth) -> Self {
        match value {
            GrokSlotAuth::Login { model_id } => SlotAuth::GrokLogin { model_id },
            GrokSlotAuth::ApiKey {
                base_url,
                model_id,
                secret_ref,
            } => SlotAuth::ApiKey {
                base_url,
                model_id,
                secret_ref,
            },
        }
    }
}

impl TryFrom<SlotAuth> for GrokSlotAuth {
    type Error = ();

    fn try_from(value: SlotAuth) -> Result<Self, Self::Error> {
        match value {
            SlotAuth::GrokLogin { model_id } => Ok(Self::Login { model_id }),
            SlotAuth::ApiKey {
                base_url,
                model_id,
                secret_ref,
            } => Ok(Self::ApiKey {
                base_url,
                model_id,
                secret_ref,
            }),
            SlotAuth::LocalCli { .. } | SlotAuth::Oauth { .. } => Err(()),
        }
    }
}

/// Compatibility view of historical `kind` discrimination.
/// Prefer matching on [`SlotAuth`] for new code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSlotKind {
    GovernedGrok {
        auth: GrokSlotAuth,
    },
    ExternalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
}

impl From<SlotAuth> for RuntimeSlotKind {
    fn from(auth: SlotAuth) -> Self {
        match auth {
            SlotAuth::GrokLogin { model_id } => Self::GovernedGrok {
                auth: GrokSlotAuth::Login { model_id },
            },
            SlotAuth::ApiKey {
                base_url,
                model_id,
                secret_ref,
            } => Self::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref,
                },
            },
            // Historical kind view is lossy for OAuth: only access secret_ref is
            // retained. Prefer matching on SlotAuth for OAuth slots.
            SlotAuth::Oauth {
                base_url,
                model_id,
                access_secret_ref,
                ..
            } => Self::GovernedGrok {
                auth: GrokSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref: access_secret_ref,
                },
            },
            SlotAuth::LocalCli {
                executable_path,
                launch_args,
                status_args,
            } => Self::ExternalCli {
                executable_path,
                launch_args,
                status_args,
            },
        }
    }
}

impl From<RuntimeSlotKind> for SlotAuth {
    fn from(kind: RuntimeSlotKind) -> Self {
        match kind {
            RuntimeSlotKind::GovernedGrok {
                auth: GrokSlotAuth::Login { model_id },
            } => Self::GrokLogin { model_id },
            RuntimeSlotKind::GovernedGrok {
                auth:
                    GrokSlotAuth::ApiKey {
                        base_url,
                        model_id,
                        secret_ref,
                    },
            } => Self::ApiKey {
                base_url,
                model_id,
                secret_ref,
            },
            RuntimeSlotKind::ExternalCli {
                executable_path,
                launch_args,
                status_args,
            } => Self::LocalCli {
                executable_path,
                launch_args,
                status_args,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSlot {
    pub id: RuntimeSlotId,
    pub project_id: ProjectId,
    pub display_name: String,
    pub auth: SlotAuth,
    pub role: SlotRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RuntimeSlot {
    /// Derived governance (not stored).
    pub fn governance(&self) -> RuntimeSlotGovernance {
        self.auth.governance()
    }

    /// Compatibility: historical `kind` view of `auth`.
    pub fn kind(&self) -> RuntimeSlotKind {
        self.auth.clone().into()
    }
}

/// Create input for the uniform slot model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRuntimeSlot {
    pub project_id: ProjectId,
    pub display_name: String,
    pub auth: SlotAuth,
    pub role: SlotRole,
}

impl NewRuntimeSlot {
    /// Build from historical create shape (kind + governance). Governance is
    /// ignored when it matches derivation; mismatches are rejected in validate.
    pub fn from_legacy(
        project_id: ProjectId,
        display_name: String,
        governance: RuntimeSlotGovernance,
        kind: RuntimeSlotKind,
    ) -> Result<Self, RuntimeSlotStoreError> {
        let auth = SlotAuth::from(kind);
        if auth.governance() != governance {
            return Err(RuntimeSlotStoreError::InvalidShape);
        }
        Ok(Self {
            project_id,
            display_name,
            auth,
            role: SlotRole::default_unassigned(),
        })
    }
}

/// Metadata update for an existing API-key slot.
///
/// `secret_ref` is always the authoritative reference after the update: callers
/// that keep the prior key pass the existing opaque ref; callers that rotate
/// pass a newly minted ref after writing the new ciphertext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateGrokApiKeySlot {
    pub project_id: ProjectId,
    pub slot_id: RuntimeSlotId,
    pub display_name: String,
    pub base_url: String,
    pub model_id: String,
    pub secret_ref: String,
}

/// Model-only update for a governed Grok/ApiKey slot. Login and API-key slots
/// share this path so switching never rotates or exposes secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRuntimeSlotModel {
    pub project_id: ProjectId,
    pub slot_id: RuntimeSlotId,
    pub model_id: String,
}

/// Uniform update: display name, role, and auth-specific fields (branch on auth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRuntimeSlot {
    pub project_id: ProjectId,
    pub slot_id: RuntimeSlotId,
    pub display_name: String,
    pub role: SlotRole,
    /// When `Some`, replaces auth fields for the matching variant.
    /// Auth kind cannot change (ApiKey stays ApiKey, etc.).
    pub auth: UpdateSlotAuth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateSlotAuth {
    /// Keep existing auth payload; only display_name / role change.
    Keep,
    ApiKey {
        base_url: String,
        model_id: String,
        secret_ref: String,
    },
    GrokLogin {
        model_id: Option<String>,
    },
    LocalCli {
        executable_path: String,
        launch_args: Vec<String>,
        status_args: Vec<String>,
    },
    /// Replace OAuth non-secret fields and/or token refs. Auth kind stays Oauth.
    Oauth {
        provider_id: String,
        issuer: String,
        client_id: String,
        scopes: Vec<String>,
        token_endpoint: String,
        account_label: Option<String>,
        base_url: String,
        model_id: String,
        access_secret_ref: String,
        refresh_secret_ref: String,
        access_expires_at: DateTime<Utc>,
    },
}

#[derive(Debug, Error)]
pub enum RuntimeSlotStoreError {
    #[error("runtime slot shape is invalid")]
    InvalidShape,
    #[error("runtime slot was not found in this project")]
    NotFound,
    #[error("stored runtime slot is invalid")]
    InvalidStoredSlot,
    #[error("project already has a controller slot")]
    ControllerConflict,
    #[error("controller role requires governed auth")]
    ControllerRequiresGovernedAuth,
    #[error("SQLite runtime slot store error")]
    Sqlite(#[source] rusqlite::Error),
    #[error("runtime slot store lock was poisoned")]
    LockPoisoned,
}

impl RuntimeSlotStoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidShape => "runtime_slot_shape_invalid",
            Self::NotFound => "runtime_slot_not_found",
            Self::InvalidStoredSlot => "runtime_slot_stored_invalid",
            Self::ControllerConflict => "runtime_slot_controller_already_exists",
            Self::ControllerRequiresGovernedAuth => {
                "runtime_slot_controller_requires_governed_auth"
            }
            Self::Sqlite(_) => "runtime_slot_store_unavailable",
            Self::LockPoisoned => "runtime_slot_store_unavailable",
        }
    }
}

impl From<rusqlite::Error> for RuntimeSlotStoreError {
    fn from(value: rusqlite::Error) -> Self {
        // Unique index on controller → explicit conflict when possible.
        if let rusqlite::Error::SqliteFailure(error, _) = &value
            && error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        {
            return Self::ControllerConflict;
        }
        Self::Sqlite(value)
    }
}

pub struct SqliteRuntimeSlotRepository {
    connection: Mutex<Connection>,
}

impl SqliteRuntimeSlotRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RuntimeSlotStoreError> {
        let connection = Connection::open(path).map_err(RuntimeSlotStoreError::Sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(RuntimeSlotStoreError::Sqlite)?;
        crate::migrations::apply(&connection).map_err(RuntimeSlotStoreError::Sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create(&self, input: NewRuntimeSlot) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_new_slot(&input)?;
        let now = Utc::now();
        let slot = RuntimeSlot {
            id: RuntimeSlotId(Uuid::new_v4()),
            project_id: input.project_id,
            display_name: input.display_name,
            auth: input.auth,
            role: input.role,
            created_at: now,
            updated_at: now,
        };
        let encoded = EncodedSlot::from_slot(&slot)?;
        self.connection()?.execute(
            "INSERT INTO runtime_slots (
                slot_id, project_id, display_name, auth_kind, role_kind, role_custom_name,
                base_url, model_id, secret_ref, executable_path,
                launch_args_json, status_args_json,
                refresh_secret_ref, access_expires_at, oauth_meta_json,
                created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
             )",
            params![
                slot.id.0.to_string(),
                slot.project_id.0.to_string(),
                slot.display_name,
                encoded.auth_kind,
                encoded.role_kind,
                encoded.role_custom_name,
                encoded.base_url,
                encoded.model_id,
                encoded.secret_ref,
                encoded.executable_path,
                encoded.launch_args_json,
                encoded.status_args_json,
                encoded.refresh_secret_ref,
                encoded.access_expires_at,
                encoded.oauth_meta_json,
                slot.created_at.to_rfc3339(),
                slot.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(slot)
    }

    pub fn get(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<Option<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let slot = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![project_id.0.to_string(), slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        Ok(slot)
    }

    pub fn list(&self, project_id: ProjectId) -> Result<Vec<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id = ?1 ORDER BY created_at ASC, rowid ASC",
            SELECT_SLOT
        ))?;
        let slots = statement
            .query_map([project_id.0.to_string()], runtime_slot_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(slots)
    }

    /// Update display name, endpoint, model, and opaque secret reference for an
    /// API-key slot. Login and LocalCli slots are rejected.
    pub fn update_grok_api_key(
        &self,
        input: UpdateGrokApiKeySlot,
    ) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_text(&input.display_name, DISPLAY_NAME_MAX_CHARS)?;
        validate_text(&input.base_url, CONFIG_VALUE_MAX_CHARS)?;
        validate_text(&input.model_id, CONFIG_VALUE_MAX_CHARS)?;
        Uuid::parse_str(&input.secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;

        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![input.project_id.0.to_string(), input.slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        let existing = existing.ok_or(RuntimeSlotStoreError::NotFound)?;
        match &existing.auth {
            SlotAuth::ApiKey { .. } => {}
            _ => return Err(RuntimeSlotStoreError::InvalidShape),
        }

        let now = Utc::now();
        let updated = RuntimeSlot {
            id: existing.id,
            project_id: existing.project_id,
            display_name: input.display_name,
            auth: SlotAuth::ApiKey {
                base_url: input.base_url,
                model_id: input.model_id,
                secret_ref: input.secret_ref,
            },
            role: existing.role,
            created_at: existing.created_at,
            updated_at: now,
        };
        // Controller + governed is still fine (ApiKey is governed).
        validate_role_auth_pair(&updated.role, &updated.auth)?;
        let encoded = EncodedSlot::from_slot(&updated)?;
        let changed = connection.execute(
            "UPDATE runtime_slots SET
                display_name = ?1,
                auth_kind = ?2,
                role_kind = ?3,
                role_custom_name = ?4,
                base_url = ?5,
                model_id = ?6,
                secret_ref = ?7,
                executable_path = NULL,
                launch_args_json = NULL,
                status_args_json = NULL,
                refresh_secret_ref = NULL,
                access_expires_at = NULL,
                oauth_meta_json = NULL,
                updated_at = ?8
             WHERE project_id = ?9 AND slot_id = ?10",
            params![
                updated.display_name,
                encoded.auth_kind,
                encoded.role_kind,
                encoded.role_custom_name,
                encoded.base_url,
                encoded.model_id,
                encoded.secret_ref,
                updated.updated_at.to_rfc3339(),
                updated.project_id.0.to_string(),
                updated.id.0.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(updated)
    }

    /// Uniform update: display name, role, and auth fields (kind cannot change).
    pub fn update(&self, input: UpdateRuntimeSlot) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_text(&input.display_name, DISPLAY_NAME_MAX_CHARS)?;
        validate_role(&input.role)?;

        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![input.project_id.0.to_string(), input.slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?
            .ok_or(RuntimeSlotStoreError::NotFound)?;

        let auth = match (&existing.auth, &input.auth) {
            (_, UpdateSlotAuth::Keep) => existing.auth.clone(),
            (
                SlotAuth::ApiKey { .. },
                UpdateSlotAuth::ApiKey {
                    base_url,
                    model_id,
                    secret_ref,
                },
            ) => {
                validate_text(base_url, CONFIG_VALUE_MAX_CHARS)?;
                validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
                Uuid::parse_str(secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
                SlotAuth::ApiKey {
                    base_url: base_url.clone(),
                    model_id: model_id.clone(),
                    secret_ref: secret_ref.clone(),
                }
            }
            (SlotAuth::GrokLogin { .. }, UpdateSlotAuth::GrokLogin { model_id }) => {
                if let Some(model_id) = model_id {
                    validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
                }
                SlotAuth::GrokLogin {
                    model_id: model_id.clone(),
                }
            }
            (
                SlotAuth::LocalCli { .. },
                UpdateSlotAuth::LocalCli {
                    executable_path,
                    launch_args,
                    status_args,
                },
            ) => {
                validate_text(executable_path, CONFIG_VALUE_MAX_CHARS)?;
                validate_args(launch_args)?;
                validate_args(status_args)?;
                if status_args.is_empty() {
                    return Err(RuntimeSlotStoreError::InvalidShape);
                }
                SlotAuth::LocalCli {
                    executable_path: executable_path.clone(),
                    launch_args: launch_args.clone(),
                    status_args: status_args.clone(),
                }
            }
            (
                SlotAuth::Oauth { .. },
                UpdateSlotAuth::Oauth {
                    provider_id,
                    issuer,
                    client_id,
                    scopes,
                    token_endpoint,
                    account_label,
                    base_url,
                    model_id,
                    access_secret_ref,
                    refresh_secret_ref,
                    access_expires_at,
                },
            ) => {
                validate_oauth_fields(
                    provider_id,
                    issuer,
                    client_id,
                    scopes,
                    token_endpoint,
                    account_label.as_deref(),
                    base_url,
                    model_id,
                    access_secret_ref,
                    refresh_secret_ref,
                )?;
                SlotAuth::Oauth {
                    provider_id: provider_id.clone(),
                    issuer: issuer.clone(),
                    client_id: client_id.clone(),
                    scopes: scopes.clone(),
                    token_endpoint: token_endpoint.clone(),
                    account_label: account_label.clone(),
                    base_url: base_url.clone(),
                    model_id: model_id.clone(),
                    access_secret_ref: access_secret_ref.clone(),
                    refresh_secret_ref: refresh_secret_ref.clone(),
                    access_expires_at: *access_expires_at,
                }
            }
            _ => return Err(RuntimeSlotStoreError::InvalidShape),
        };

        validate_role_auth_pair(&input.role, &auth)?;

        let updated = RuntimeSlot {
            id: existing.id,
            project_id: existing.project_id,
            display_name: input.display_name,
            auth,
            role: input.role,
            created_at: existing.created_at,
            updated_at: Utc::now(),
        };
        let encoded = EncodedSlot::from_slot(&updated)?;
        let changed = connection.execute(
            "UPDATE runtime_slots SET
                display_name = ?1,
                auth_kind = ?2,
                role_kind = ?3,
                role_custom_name = ?4,
                base_url = ?5,
                model_id = ?6,
                secret_ref = ?7,
                executable_path = ?8,
                launch_args_json = ?9,
                status_args_json = ?10,
                refresh_secret_ref = ?11,
                access_expires_at = ?12,
                oauth_meta_json = ?13,
                updated_at = ?14
             WHERE project_id = ?15 AND slot_id = ?16",
            params![
                updated.display_name,
                encoded.auth_kind,
                encoded.role_kind,
                encoded.role_custom_name,
                encoded.base_url,
                encoded.model_id,
                encoded.secret_ref,
                encoded.executable_path,
                encoded.launch_args_json,
                encoded.status_args_json,
                encoded.refresh_secret_ref,
                encoded.access_expires_at,
                encoded.oauth_meta_json,
                updated.updated_at.to_rfc3339(),
                updated.project_id.0.to_string(),
                updated.id.0.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(updated)
    }

    /// Update only the selected model for a governed Grok/ApiKey/Oauth slot.
    pub fn update_model(
        &self,
        input: UpdateRuntimeSlotModel,
    ) -> Result<RuntimeSlot, RuntimeSlotStoreError> {
        validate_text(&input.model_id, CONFIG_VALUE_MAX_CHARS)?;
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![input.project_id.0.to_string(), input.slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?
            .ok_or(RuntimeSlotStoreError::NotFound)?;
        let auth = match existing.auth {
            SlotAuth::GrokLogin { .. } => SlotAuth::GrokLogin {
                model_id: Some(input.model_id),
            },
            SlotAuth::ApiKey {
                base_url,
                secret_ref,
                ..
            } => SlotAuth::ApiKey {
                base_url,
                model_id: input.model_id,
                secret_ref,
            },
            SlotAuth::Oauth {
                provider_id,
                issuer,
                client_id,
                scopes,
                token_endpoint,
                account_label,
                base_url,
                access_secret_ref,
                refresh_secret_ref,
                access_expires_at,
                ..
            } => SlotAuth::Oauth {
                provider_id,
                issuer,
                client_id,
                scopes,
                token_endpoint,
                account_label,
                base_url,
                model_id: input.model_id,
                access_secret_ref,
                refresh_secret_ref,
                access_expires_at,
            },
            SlotAuth::LocalCli { .. } => {
                return Err(RuntimeSlotStoreError::InvalidShape);
            }
        };
        let updated = RuntimeSlot {
            id: existing.id,
            project_id: existing.project_id,
            display_name: existing.display_name,
            auth,
            role: existing.role,
            created_at: existing.created_at,
            updated_at: Utc::now(),
        };
        let encoded = EncodedSlot::from_slot(&updated)?;
        let changed = connection.execute(
            "UPDATE runtime_slots SET model_id = ?1, updated_at = ?2
             WHERE project_id = ?3 AND slot_id = ?4",
            params![
                encoded.model_id,
                updated.updated_at.to_rfc3339(),
                updated.project_id.0.to_string(),
                updated.id.0.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(updated)
    }

    /// Delete a project-scoped slot row. Returns the removed slot so callers can
    /// destroy any associated secret material. Missing rows yield `Ok(None)`.
    pub fn delete(
        &self,
        project_id: ProjectId,
        slot_id: RuntimeSlotId,
    ) -> Result<Option<RuntimeSlot>, RuntimeSlotStoreError> {
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                &format!("{} WHERE project_id = ?1 AND slot_id = ?2", SELECT_SLOT),
                params![project_id.0.to_string(), slot_id.0.to_string()],
                runtime_slot_from_row,
            )
            .optional()?;
        let Some(slot) = existing else {
            return Ok(None);
        };
        let changed = connection.execute(
            "DELETE FROM runtime_slots WHERE project_id = ?1 AND slot_id = ?2",
            params![project_id.0.to_string(), slot_id.0.to_string()],
        )?;
        if changed != 1 {
            return Err(RuntimeSlotStoreError::NotFound);
        }
        Ok(Some(slot))
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, RuntimeSlotStoreError> {
        self.connection
            .lock()
            .map_err(|_| RuntimeSlotStoreError::LockPoisoned)
    }
}

const SELECT_SLOT: &str = "SELECT slot_id, project_id, display_name, auth_kind, role_kind,
            role_custom_name, base_url, model_id, secret_ref, executable_path,
            launch_args_json, status_args_json, created_at, updated_at,
            refresh_secret_ref, access_expires_at, oauth_meta_json
     FROM runtime_slots";

struct EncodedSlot {
    auth_kind: &'static str,
    role_kind: &'static str,
    role_custom_name: Option<String>,
    base_url: Option<String>,
    model_id: Option<String>,
    secret_ref: Option<String>,
    executable_path: Option<String>,
    launch_args_json: Option<String>,
    status_args_json: Option<String>,
    refresh_secret_ref: Option<String>,
    access_expires_at: Option<String>,
    oauth_meta_json: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct OauthMetaStored {
    provider_id: String,
    issuer: String,
    client_id: String,
    scopes: Vec<String>,
    token_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_label: Option<String>,
}

impl EncodedSlot {
    fn from_slot(slot: &RuntimeSlot) -> Result<Self, RuntimeSlotStoreError> {
        let (
            auth_kind,
            base_url,
            model_id,
            secret_ref,
            executable_path,
            launch_args_json,
            status_args_json,
            refresh_secret_ref,
            access_expires_at,
            oauth_meta_json,
        ) = match &slot.auth {
            SlotAuth::GrokLogin { model_id } => (
                "grok_login",
                None,
                model_id.clone(),
                None,
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
                "api_key",
                Some(base_url.clone()),
                Some(model_id.clone()),
                Some(secret_ref.clone()),
                None,
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
                "local_cli",
                None,
                None,
                None,
                Some(executable_path.clone()),
                Some(
                    serde_json::to_string(launch_args)
                        .map_err(|_| RuntimeSlotStoreError::InvalidShape)?,
                ),
                Some(
                    serde_json::to_string(status_args)
                        .map_err(|_| RuntimeSlotStoreError::InvalidShape)?,
                ),
                None,
                None,
                None,
            ),
            SlotAuth::Oauth {
                provider_id,
                issuer,
                client_id,
                scopes,
                token_endpoint,
                account_label,
                base_url,
                model_id,
                access_secret_ref,
                refresh_secret_ref,
                access_expires_at,
            } => {
                let meta = OauthMetaStored {
                    provider_id: provider_id.clone(),
                    issuer: issuer.clone(),
                    client_id: client_id.clone(),
                    scopes: scopes.clone(),
                    token_endpoint: token_endpoint.clone(),
                    account_label: account_label.clone(),
                };
                let meta_json = serde_json::to_string(&meta)
                    .map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
                (
                    "oauth",
                    Some(base_url.clone()),
                    Some(model_id.clone()),
                    Some(access_secret_ref.clone()),
                    None,
                    None,
                    None,
                    Some(refresh_secret_ref.clone()),
                    Some(access_expires_at.to_rfc3339()),
                    Some(meta_json),
                )
            }
        };
        let (role_kind, role_custom_name) = match &slot.role {
            SlotRole::Controller => ("controller", None),
            SlotRole::Librarian => ("librarian", None),
            SlotRole::Custom(name) => ("custom", Some(name.clone())),
        };
        Ok(Self {
            auth_kind,
            role_kind,
            role_custom_name,
            base_url,
            model_id,
            secret_ref,
            executable_path,
            launch_args_json,
            status_args_json,
            refresh_secret_ref,
            access_expires_at,
            oauth_meta_json,
        })
    }
}

fn validate_new_slot(input: &NewRuntimeSlot) -> Result<(), RuntimeSlotStoreError> {
    validate_text(&input.display_name, DISPLAY_NAME_MAX_CHARS)?;
    validate_role(&input.role)?;
    validate_role_auth_pair(&input.role, &input.auth)?;
    match &input.auth {
        SlotAuth::GrokLogin { model_id } => {
            if let Some(model_id) = model_id {
                validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
            }
            Ok(())
        }
        SlotAuth::ApiKey {
            base_url,
            model_id,
            secret_ref,
        } => {
            validate_text(base_url, CONFIG_VALUE_MAX_CHARS)?;
            validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
            Uuid::parse_str(secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
            Ok(())
        }
        SlotAuth::LocalCli {
            executable_path,
            launch_args,
            status_args,
        } => {
            validate_text(executable_path, CONFIG_VALUE_MAX_CHARS)?;
            validate_args(launch_args)?;
            validate_args(status_args)?;
            if status_args.is_empty() {
                return Err(RuntimeSlotStoreError::InvalidShape);
            }
            Ok(())
        }
        SlotAuth::Oauth {
            provider_id,
            issuer,
            client_id,
            scopes,
            token_endpoint,
            account_label,
            base_url,
            model_id,
            access_secret_ref,
            refresh_secret_ref,
            access_expires_at: _,
        } => validate_oauth_fields(
            provider_id,
            issuer,
            client_id,
            scopes,
            token_endpoint,
            account_label.as_deref(),
            base_url,
            model_id,
            access_secret_ref,
            refresh_secret_ref,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_oauth_fields(
    provider_id: &str,
    issuer: &str,
    client_id: &str,
    scopes: &[String],
    token_endpoint: &str,
    account_label: Option<&str>,
    base_url: &str,
    model_id: &str,
    access_secret_ref: &str,
    refresh_secret_ref: &str,
) -> Result<(), RuntimeSlotStoreError> {
    validate_text(provider_id, CONFIG_VALUE_MAX_CHARS)?;
    validate_text(issuer, CONFIG_VALUE_MAX_CHARS)?;
    validate_text(client_id, CONFIG_VALUE_MAX_CHARS)?;
    validate_text(token_endpoint, CONFIG_VALUE_MAX_CHARS)?;
    validate_text(base_url, CONFIG_VALUE_MAX_CHARS)?;
    validate_text(model_id, CONFIG_VALUE_MAX_CHARS)?;
    if scopes.is_empty() || scopes.len() > ARG_COUNT_MAX {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    for scope in scopes {
        validate_text(scope, ARG_MAX_CHARS)?;
    }
    if let Some(label) = account_label {
        validate_text(label, CONFIG_VALUE_MAX_CHARS)?;
    }
    Uuid::parse_str(access_secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
    Uuid::parse_str(refresh_secret_ref).map_err(|_| RuntimeSlotStoreError::InvalidShape)?;
    if access_secret_ref == refresh_secret_ref {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    Ok(())
}

/// Controller must be governed auth. LocalCli cannot be Controller.
fn validate_role_auth_pair(role: &SlotRole, auth: &SlotAuth) -> Result<(), RuntimeSlotStoreError> {
    if matches!(role, SlotRole::Controller) && !auth.is_governed() {
        return Err(RuntimeSlotStoreError::ControllerRequiresGovernedAuth);
    }
    Ok(())
}

fn validate_role(role: &SlotRole) -> Result<(), RuntimeSlotStoreError> {
    match role {
        SlotRole::Controller | SlotRole::Librarian => Ok(()),
        SlotRole::Custom(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty()
                || name.chars().count() > CUSTOM_ROLE_NAME_MAX_CHARS
                || name.chars().any(char::is_control)
                || trimmed != name
            {
                // Reject blank, over-long, control chars, and untrimmed names.
                return Err(RuntimeSlotStoreError::InvalidShape);
            }
            Ok(())
        }
    }
}

fn validate_text(value: &str, max_chars: usize) -> Result<(), RuntimeSlotStoreError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    Ok(())
}

fn validate_args(args: &[String]) -> Result<(), RuntimeSlotStoreError> {
    if args.len() > ARG_COUNT_MAX
        || args
            .iter()
            .any(|arg| arg.chars().count() > ARG_MAX_CHARS || arg.chars().any(|c| c == '\0'))
    {
        return Err(RuntimeSlotStoreError::InvalidShape);
    }
    Ok(())
}

fn runtime_slot_from_row(row: &Row<'_>) -> rusqlite::Result<RuntimeSlot> {
    let slot_id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let auth_kind: String = row.get(3)?;
    let role_kind: String = row.get(4)?;
    let role_custom_name: Option<String> = row.get(5)?;
    let base_url: Option<String> = row.get(6)?;
    let model_id: Option<String> = row.get(7)?;
    let secret_ref: Option<String> = row.get(8)?;
    let executable_path: Option<String> = row.get(9)?;
    let launch_args_json: Option<String> = row.get(10)?;
    let status_args_json: Option<String> = row.get(11)?;
    let created_at: String = row.get(12)?;
    let updated_at: String = row.get(13)?;
    let refresh_secret_ref: Option<String> = row.get(14)?;
    let access_expires_at: Option<String> = row.get(15)?;
    let oauth_meta_json: Option<String> = row.get(16)?;

    let auth = match auth_kind.as_str() {
        "grok_login"
            if base_url.is_none()
                && secret_ref.is_none()
                && executable_path.is_none()
                && launch_args_json.is_none()
                && status_args_json.is_none()
                && refresh_secret_ref.is_none()
                && access_expires_at.is_none()
                && oauth_meta_json.is_none() =>
        {
            SlotAuth::GrokLogin { model_id }
        }
        "api_key"
            if executable_path.is_none()
                && launch_args_json.is_none()
                && status_args_json.is_none()
                && refresh_secret_ref.is_none()
                && access_expires_at.is_none()
                && oauth_meta_json.is_none() =>
        {
            SlotAuth::ApiKey {
                base_url: base_url.ok_or_else(|| invalid_stored(6))?,
                model_id: model_id.ok_or_else(|| invalid_stored(7))?,
                secret_ref: secret_ref.ok_or_else(|| invalid_stored(8))?,
            }
        }
        "local_cli"
            if base_url.is_none()
                && model_id.is_none()
                && secret_ref.is_none()
                && refresh_secret_ref.is_none()
                && access_expires_at.is_none()
                && oauth_meta_json.is_none() =>
        {
            SlotAuth::LocalCli {
                executable_path: executable_path.ok_or_else(|| invalid_stored(9))?,
                launch_args: parse_args(launch_args_json, 10)?,
                status_args: parse_args(status_args_json, 11)?,
            }
        }
        "oauth"
            if executable_path.is_none()
                && launch_args_json.is_none()
                && status_args_json.is_none() =>
        {
            let meta_raw = oauth_meta_json.ok_or_else(|| invalid_stored(16))?;
            let meta: OauthMetaStored =
                serde_json::from_str(&meta_raw).map_err(|_| invalid_stored(16))?;
            let expires_raw = access_expires_at.ok_or_else(|| invalid_stored(15))?;
            SlotAuth::Oauth {
                provider_id: meta.provider_id,
                issuer: meta.issuer,
                client_id: meta.client_id,
                scopes: meta.scopes,
                token_endpoint: meta.token_endpoint,
                account_label: meta.account_label,
                base_url: base_url.ok_or_else(|| invalid_stored(6))?,
                model_id: model_id.ok_or_else(|| invalid_stored(7))?,
                access_secret_ref: secret_ref.ok_or_else(|| invalid_stored(8))?,
                refresh_secret_ref: refresh_secret_ref.ok_or_else(|| invalid_stored(14))?,
                access_expires_at: parse_timestamp(&expires_raw, 15)?,
            }
        }
        _ => return Err(invalid_stored(3)),
    };

    let role = match (role_kind.as_str(), role_custom_name) {
        ("controller", None) => SlotRole::Controller,
        ("librarian", None) => SlotRole::Librarian,
        ("custom", Some(name)) if !name.trim().is_empty() => SlotRole::Custom(name),
        _ => return Err(invalid_stored(4)),
    };

    Ok(RuntimeSlot {
        id: RuntimeSlotId(parse_uuid(&slot_id, 0)?),
        project_id: ProjectId(parse_uuid(&project_id, 1)?),
        display_name,
        auth,
        role,
        created_at: parse_timestamp(&created_at, 12)?,
        updated_at: parse_timestamp(&updated_at, 13)?,
    })
}

fn parse_args(value: Option<String>, index: usize) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(&value.ok_or_else(|| invalid_stored(index))?)
        .map_err(|_| invalid_stored(index))
}

fn parse_uuid(value: &str, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn parse_timestamp(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

fn invalid_stored(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(RuntimeSlotStoreError::InvalidStoredSlot),
    )
}

#[cfg(test)]
mod governance_derivation_tests {
    use super::{RuntimeSlotGovernance, SlotAuth, governance_from_auth};

    #[test]
    fn api_key_maps_to_full() {
        let auth = SlotAuth::ApiKey {
            base_url: "https://example.invalid/v1".into(),
            model_id: "m".into(),
            secret_ref: uuid::Uuid::new_v4().to_string(),
        };
        assert_eq!(governance_from_auth(&auth), RuntimeSlotGovernance::Full);
        assert_eq!(auth.governance(), RuntimeSlotGovernance::Full);
    }

    #[test]
    fn grok_login_maps_to_full() {
        let auth = SlotAuth::GrokLogin { model_id: None };
        assert_eq!(governance_from_auth(&auth), RuntimeSlotGovernance::Full);
    }

    #[test]
    fn local_cli_maps_to_none() {
        let auth = SlotAuth::LocalCli {
            executable_path: "C:\\tools\\x.exe".into(),
            launch_args: vec![],
            status_args: vec!["status".into()],
        };
        assert_eq!(governance_from_auth(&auth), RuntimeSlotGovernance::None);
    }

    #[test]
    fn oauth_maps_to_full() {
        let auth = SlotAuth::Oauth {
            provider_id: "example".into(),
            issuer: "https://issuer.example.invalid".into(),
            client_id: "client".into(),
            scopes: vec!["openid".into()],
            token_endpoint: "https://issuer.example.invalid/token".into(),
            account_label: Some("label".into()),
            base_url: "https://api.example.invalid/v1".into(),
            model_id: "m".into(),
            access_secret_ref: uuid::Uuid::new_v4().to_string(),
            refresh_secret_ref: uuid::Uuid::new_v4().to_string(),
            access_expires_at: chrono::Utc::now(),
        };
        assert_eq!(governance_from_auth(&auth), RuntimeSlotGovernance::Full);
        assert_eq!(auth.wire_auth_mode(), Some("oauth"));
        assert_eq!(auth.wire_kind(), "governed_grok");
    }
}
