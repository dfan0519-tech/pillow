//! Pure vocabulary for role dispatch (no storage or host coupling).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Catalog program label used when the target auth is a governed Grok/API-key
/// shape that does not embed an executable path. The host maps this label to a
/// real binary; this crate never resolves paths.
pub const GOVERNED_CATALOG_PROGRAM: &str = "grok";

/// Structured argv flag that precedes the task text as its own element.
/// Task text is never shell-joined and never placed in `program` or `cwd`.
pub const TASK_ARGV_FLAG: &str = "--pillow-task";

/// Opaque runtime-slot identity (UUID only; no native paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotId(pub Uuid);

impl SlotId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SlotId {
    fn default() -> Self {
        Self::new()
    }
}

/// Role of a slot within a project. Mirrors storage vocabulary without
/// depending on the storage crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    Controller,
    Librarian,
    Custom(String),
}

impl SlotRole {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Librarian => "librarian",
            Self::Custom(_) => "custom",
        }
    }
}

/// Authentication shape relevant to dispatch. Secrets are never carried here
/// (`secret_ref` is omitted; governed shapes only need model selection for
/// argv construction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotAuthShape {
    /// OpenAI-compatible endpoint (governed).
    ApiKey { model_id: String },
    /// Grok session login (governed).
    GrokLogin { model_id: Option<String> },
    /// Local executable registration only — ungoverned.
    LocalCli {
        executable_path: String,
        launch_args: Vec<String>,
        #[serde(default)]
        status_args: Vec<String>,
    },
}

impl SlotAuthShape {
    /// Governance is derived solely from auth. Contradiction is unrepresentable.
    pub fn governance(&self) -> GovernanceCapability {
        match self {
            Self::ApiKey { .. } | Self::GrokLogin { .. } => GovernanceCapability::Full,
            Self::LocalCli { .. } => GovernanceCapability::None,
        }
    }

    pub fn is_governed(&self) -> bool {
        matches!(self.governance(), GovernanceCapability::Full)
    }
}

/// Capability tag frozen onto an assignment / proposed command.
///
/// [`None`] means the target is ungoverned (`LocalCli`). Outcomes for such
/// targets may only be **recorded**; they must never claim governed execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceCapability {
    Full,
    None,
}

impl GovernanceCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::None => "none",
        }
    }

    pub fn is_governed(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Assignment lifecycle. Terminal: succeeded | failed | rejected | cancelled.
///
/// Graph (see [`crate::transition`]):
/// - `proposed` → `approved` | `rejected` | `cancelled`
/// - `approved` → `running` | `rejected` | `cancelled`
/// - `running`  → `succeeded` | `failed` | `cancelled`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentState {
    Proposed,
    Approved,
    Running,
    Succeeded,
    Failed,
    Rejected,
    Cancelled,
}

impl AssignmentState {
    pub const ALL: &'static [Self] = &[
        Self::Proposed,
        Self::Approved,
        Self::Running,
        Self::Succeeded,
        Self::Failed,
        Self::Rejected,
        Self::Cancelled,
    ];

    pub const TERMINAL: &'static [Self] = &[
        Self::Succeeded,
        Self::Failed,
        Self::Rejected,
        Self::Cancelled,
    ];

    pub const NON_TERMINAL: &'static [Self] = &[Self::Proposed, Self::Approved, Self::Running];
}
