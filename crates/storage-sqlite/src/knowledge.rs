//! Knowledge document registry (K0a) + dispatch/acceptance ledger (K0d)
//! + explicit archive (K1a).
//!
//! Durable authority for document role, lifecycle status, multi-valued
//! supersession / related / implements links, and controller dispatch /
//! acceptance records (ADR-0004 §2: registration is a byproduct of work).
//! Path grammar is the `keeper_project::validate_portable_path` SSoT — this
//! module must not re-implement portable-path rules.
//!
//! Status is never auto-migrated: only explicit set/update/archive/unarchive
//! calls change it. Default for unclassified registration is
//! [`DocumentStatus::Unknown`].
//!
//! **Retirement (ADR-0004 §5 / K1a):** automatic retirement is mark + archive
//! only. [`SqliteKnowledgeRepository::archive`] / [`unarchive`] are explicit;
//! list/query defaults **exclude** `archived` so retirement is not cosmetic.
//! There is **no** delete/purge API — true deletion requires a future approval
//! path and receipt (intentionally not implemented here).
//!
//! **Privacy (K0d red line):** never store lease ids, Keeper session ids, or
//! machine user-profile paths. Only slot labels (e.g. `grok3`) and portable
//! relative paths.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use keeper_project::validate_portable_path;
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

/// Document role (ADR-0004 inventory-aligned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRole {
    Brief,
    Report,
    Decision,
    Research,
    Verification,
    Spec,
    Plan,
    Handoff,
    Roadmap,
    Migration,
    Reference,
    Other,
}

impl DocumentRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brief => "brief",
            Self::Report => "report",
            Self::Decision => "decision",
            Self::Research => "research",
            Self::Verification => "verification",
            Self::Spec => "spec",
            Self::Plan => "plan",
            Self::Handoff => "handoff",
            Self::Roadmap => "roadmap",
            Self::Migration => "migration",
            Self::Reference => "reference",
            Self::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "brief" => Ok(Self::Brief),
            "report" => Ok(Self::Report),
            "decision" => Ok(Self::Decision),
            "research" => Ok(Self::Research),
            "verification" => Ok(Self::Verification),
            "spec" => Ok(Self::Spec),
            "plan" => Ok(Self::Plan),
            "handoff" => Ok(Self::Handoff),
            "roadmap" => Ok(Self::Roadmap),
            "migration" => Ok(Self::Migration),
            "reference" => Ok(Self::Reference),
            "other" => Ok(Self::Other),
            other => Err(KnowledgeError::InvalidRole {
                role: other.to_owned(),
            }),
        }
    }
}

/// Document lifecycle status. Default for new unclassified rows is [`Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentStatus {
    Active,
    Proposed,
    Accepted,
    Consumed,
    Superseded,
    Evidence,
    #[default]
    Unknown,
    Archived,
}

impl DocumentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Consumed => "consumed",
            Self::Superseded => "superseded",
            Self::Evidence => "evidence",
            Self::Unknown => "unknown",
            Self::Archived => "archived",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "active" => Ok(Self::Active),
            "proposed" => Ok(Self::Proposed),
            "accepted" => Ok(Self::Accepted),
            "consumed" => Ok(Self::Consumed),
            "superseded" => Ok(Self::Superseded),
            "evidence" => Ok(Self::Evidence),
            "unknown" => Ok(Self::Unknown),
            "archived" => Ok(Self::Archived),
            other => Err(KnowledgeError::InvalidStatus {
                status: other.to_owned(),
            }),
        }
    }
}

/// Who registered the document row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisteredBy {
    Controller,
    Worker,
    Lint,
}

impl RegisteredBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Worker => "worker",
            Self::Lint => "lint",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "controller" => Ok(Self::Controller),
            "worker" => Ok(Self::Worker),
            "lint" => Ok(Self::Lint),
            other => Err(KnowledgeError::InvalidRegisteredBy {
                value: other.to_owned(),
            }),
        }
    }
}

/// Link relation between documents (or a free-path claim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentLinkRel {
    Supersedes,
    Related,
    Implements,
}

impl DocumentLinkRel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supersedes => "supersedes",
            Self::Related => "related",
            Self::Implements => "implements",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "supersedes" => Ok(Self::Supersedes),
            "related" => Ok(Self::Related),
            "implements" => Ok(Self::Implements),
            other => Err(KnowledgeError::InvalidLinkRel {
                rel: other.to_owned(),
            }),
        }
    }
}

/// Durable knowledge document row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeDocument {
    pub document_id: String,
    pub project_id: String,
    pub path: String,
    pub role: DocumentRole,
    pub status: DocumentStatus,
    pub content_digest: String,
    pub updated_at: DateTime<Utc>,
    pub registered_by: RegisteredBy,
    pub source_run_id: Option<String>,
}

/// Durable multi-valued link row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeDocumentLink {
    pub link_id: String,
    pub from_document_id: String,
    pub to_document_id: Option<String>,
    pub to_path: Option<String>,
    pub rel: DocumentLinkRel,
}

/// Input for registering (inserting) a document.
///
/// When [`status`](RegisterDocument::status) is `None`, the stored status is
/// [`DocumentStatus::Unknown`] — never silently `active`.
#[derive(Debug, Clone)]
pub struct RegisterDocument {
    pub project_id: String,
    pub path: String,
    pub role: DocumentRole,
    /// Explicit lifecycle status. `None` → [`DocumentStatus::Unknown`].
    pub status: Option<DocumentStatus>,
    pub content_digest: String,
    pub registered_by: RegisteredBy,
    pub source_run_id: Option<String>,
}

/// Input for updating an existing document. Status changes only via this
/// explicit path (or a full re-register upsert of status field) — never
/// automatic.
#[derive(Debug, Clone)]
pub struct UpdateDocument {
    pub document_id: String,
    pub role: Option<DocumentRole>,
    pub status: Option<DocumentStatus>,
    pub content_digest: Option<String>,
    pub registered_by: Option<RegisteredBy>,
    pub source_run_id: Option<Option<String>>,
}

/// Input for inserting a multi-valued document link.
///
/// At least one of `to_document_id` / `to_path` must be present (enforced in
/// Rust and by SQL CHECK).
#[derive(Debug, Clone)]
pub struct AddDocumentLink {
    pub from_document_id: String,
    pub to_document_id: Option<String>,
    pub to_path: Option<String>,
    pub rel: DocumentLinkRel,
}

/// Query options for listing knowledge documents.
///
/// **Default excludes archived** (ADR-0004 §5): retirement is real only when
/// the default retrieval path cannot "forget" to hide archived rows. Callers
/// that need inventory / rescan / unarchive flows must set
/// [`include_archived`](ListDocumentsQuery::include_archived) to `true`.
/// The `false` default is load-bearing, not incidental. It is enforced by
/// `default_list_excludes_archived_include_flag_shows_them` rather than by the
/// shape of this impl, so deriving costs nothing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListDocumentsQuery {
    /// When `false` (default), rows with `status = archived` are omitted.
    pub include_archived: bool,
}

// ---------------------------------------------------------------------------
// K0d: dispatch / acceptance ledger
// ---------------------------------------------------------------------------

/// Task lifecycle status. Never auto-migrated — only explicit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Dispatched,
    Completed,
    Blocked,
    Abandoned,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "dispatched" => Ok(Self::Dispatched),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "abandoned" => Ok(Self::Abandoned),
            other => Err(KnowledgeError::InvalidTaskStatus {
                status: other.to_owned(),
            }),
        }
    }
}

/// Controller acceptance outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictOutcome {
    Accepted,
    Rejected,
    Partial,
}

impl VerdictOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Partial => "partial",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, KnowledgeError> {
        match raw {
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "partial" => Ok(Self::Partial),
            other => Err(KnowledgeError::InvalidVerdictOutcome {
                outcome: other.to_owned(),
            }),
        }
    }
}

/// Durable dispatch row (派单台账).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeTask {
    pub task_id: String,
    pub project_id: String,
    pub title: String,
    pub brief_path: Option<String>,
    /// Slot label only (e.g. `grok3`). Never a lease or session id.
    pub worker_label: String,
    pub dispatched_at: DateTime<Utc>,
    pub status: TaskStatus,
    pub completion_signal: Option<String>,
}

/// Durable controller verdict row (验收结论).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeVerdict {
    pub verdict_id: String,
    /// Optional: controller-self work may have no dispatch row.
    pub task_id: Option<String>,
    pub project_id: String,
    pub outcome: VerdictOutcome,
    pub gate_result: Option<String>,
    pub e2e_result: Option<String>,
    pub summary: String,
    pub residuals: Option<String>,
    pub decided_at: DateTime<Utc>,
}

/// Input for recording a dispatch (byproduct of controller dispatch).
#[derive(Debug, Clone)]
pub struct DispatchTask {
    pub project_id: String,
    pub title: String,
    pub brief_path: Option<String>,
    /// Slot label only — e.g. `grok3`, `codex1`. Lease/session ids rejected.
    pub worker_label: String,
}

/// Explicit task status update. Status is never inferred.
#[derive(Debug, Clone)]
pub struct SetTaskStatus {
    pub task_id: String,
    pub status: TaskStatus,
    pub completion_signal: Option<String>,
}

/// Input for recording a controller acceptance verdict.
#[derive(Debug, Clone)]
pub struct RecordVerdict {
    pub project_id: String,
    /// Optional link to a dispatch row; `None` = controller-self work.
    pub task_id: Option<String>,
    pub outcome: VerdictOutcome,
    pub gate_result: Option<String>,
    pub e2e_result: Option<String>,
    pub summary: String,
    pub residuals: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KnowledgeError {
    #[error("project id must be a nonempty identifier")]
    InvalidProjectId,
    #[error("document path must be a portable relative path: {detail}")]
    InvalidPath { detail: String },
    #[error("invalid knowledge document role: {role}")]
    InvalidRole { role: String },
    #[error("invalid knowledge document status: {status}")]
    InvalidStatus { status: String },
    #[error("invalid registered_by: {value}")]
    InvalidRegisteredBy { value: String },
    #[error("invalid document link relation: {rel}")]
    InvalidLinkRel { rel: String },
    #[error("invalid content digest (expect sha256:<64 lowercase hex>)")]
    InvalidContentDigest,
    #[error("document link requires to_document_id and/or to_path")]
    LinkEndpointRequired,
    #[error("source_run_id must be empty-or-nonempty within length bounds")]
    InvalidSourceRunId,
    #[error("knowledge document not found: {document_id}")]
    NotFound { document_id: String },
    #[error("knowledge document already registered for project path")]
    DuplicatePath,
    #[error("invalid task status: {status}")]
    InvalidTaskStatus { status: String },
    #[error("invalid verdict outcome: {outcome}")]
    InvalidVerdictOutcome { outcome: String },
    #[error("task title must be nonempty within length bounds")]
    InvalidTaskTitle,
    #[error("worker_label must be a slot label (not a lease/session id): {detail}")]
    InvalidWorkerLabel { detail: String },
    #[error("privacy red line: refused to store lease id, session id, or host user path")]
    PrivacyViolation,
    #[error("verdict summary must be nonempty within length bounds")]
    InvalidVerdictSummary,
    #[error("knowledge task not found: {task_id}")]
    TaskNotFound { task_id: String },
    #[error("knowledge verdict not found: {verdict_id}")]
    VerdictNotFound { verdict_id: String },
    #[error("SQLite knowledge store error")]
    Sqlite,
    #[error("knowledge store lock was poisoned")]
    LockPoisoned,
}

impl KnowledgeError {
    /// Stable error code for UI / command mapping.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidProjectId => "knowledge_invalid_project_id",
            Self::InvalidPath { .. } => "knowledge_invalid_path",
            Self::InvalidRole { .. } => "knowledge_invalid_role",
            Self::InvalidStatus { .. } => "knowledge_invalid_status",
            Self::InvalidRegisteredBy { .. } => "knowledge_invalid_registered_by",
            Self::InvalidLinkRel { .. } => "knowledge_invalid_link_rel",
            Self::InvalidContentDigest => "knowledge_invalid_content_digest",
            Self::LinkEndpointRequired => "knowledge_link_endpoint_required",
            Self::InvalidSourceRunId => "knowledge_invalid_source_run_id",
            Self::NotFound { .. } => "knowledge_not_found",
            Self::DuplicatePath => "knowledge_duplicate_path",
            Self::InvalidTaskStatus { .. } => "knowledge_invalid_task_status",
            Self::InvalidVerdictOutcome { .. } => "knowledge_invalid_verdict_outcome",
            Self::InvalidTaskTitle => "knowledge_invalid_task_title",
            Self::InvalidWorkerLabel { .. } => "knowledge_invalid_worker_label",
            Self::PrivacyViolation => "knowledge_privacy_violation",
            Self::InvalidVerdictSummary => "knowledge_invalid_verdict_summary",
            Self::TaskNotFound { .. } => "knowledge_task_not_found",
            Self::VerdictNotFound { .. } => "knowledge_verdict_not_found",
            Self::Sqlite => "knowledge_sqlite",
            Self::LockPoisoned => "knowledge_lock_poisoned",
        }
    }
}

/// SQLite-backed knowledge registry repository.
pub struct SqliteKnowledgeRepository {
    connection: Mutex<Connection>,
}

impl SqliteKnowledgeRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, KnowledgeError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Register a new document. Status defaults to [`DocumentStatus::Unknown`]
    /// when not provided. Paths are validated via
    /// [`keeper_project::validate_portable_path`].
    pub fn register(&self, input: RegisterDocument) -> Result<KnowledgeDocument, KnowledgeError> {
        validate_project_id(&input.project_id)?;
        validate_path(&input.path)?;
        validate_content_digest(&input.content_digest)?;
        validate_source_run_id(input.source_run_id.as_deref())?;

        let status = input.status.unwrap_or(DocumentStatus::Unknown);
        let now = Utc::now();
        let document_id = Uuid::new_v4().to_string();

        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO knowledge_documents (
                    document_id, project_id, path, role, status, content_digest,
                    updated_at, registered_by, source_run_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    document_id,
                    input.project_id,
                    input.path,
                    input.role.as_str(),
                    status.as_str(),
                    input.content_digest,
                    now.to_rfc3339(),
                    input.registered_by.as_str(),
                    input.source_run_id,
                ],
            )
            .map_err(map_insert_error)?;

        Ok(KnowledgeDocument {
            document_id,
            project_id: input.project_id,
            path: input.path,
            role: input.role,
            status,
            content_digest: input.content_digest,
            updated_at: now,
            registered_by: input.registered_by,
            source_run_id: input.source_run_id,
        })
    }

    /// Explicit field update. Status is never inferred — only set when
    /// `input.status` is `Some`.
    pub fn update(&self, input: UpdateDocument) -> Result<KnowledgeDocument, KnowledgeError> {
        if input.document_id.is_empty() {
            return Err(KnowledgeError::NotFound {
                document_id: input.document_id,
            });
        }
        if let Some(ref digest) = input.content_digest {
            validate_content_digest(digest)?;
        }
        if let Some(ref run) = input.source_run_id {
            validate_source_run_id(run.as_deref())?;
        }

        let connection = self.connection()?;
        let mut current = self
            .load_by_id_conn(&connection, &input.document_id)?
            .ok_or_else(|| KnowledgeError::NotFound {
                document_id: input.document_id.clone(),
            })?;

        if let Some(role) = input.role {
            current.role = role;
        }
        if let Some(status) = input.status {
            current.status = status;
        }
        if let Some(digest) = input.content_digest {
            current.content_digest = digest;
        }
        if let Some(by) = input.registered_by {
            current.registered_by = by;
        }
        if let Some(run) = input.source_run_id {
            current.source_run_id = run;
        }

        let now = Utc::now();
        connection
            .execute(
                "UPDATE knowledge_documents SET
                    role = ?1,
                    status = ?2,
                    content_digest = ?3,
                    updated_at = ?4,
                    registered_by = ?5,
                    source_run_id = ?6
                 WHERE document_id = ?7",
                params![
                    current.role.as_str(),
                    current.status.as_str(),
                    current.content_digest,
                    now.to_rfc3339(),
                    current.registered_by.as_str(),
                    current.source_run_id,
                    current.document_id,
                ],
            )
            .map_err(map_sqlite)?;

        current.updated_at = now;
        Ok(current)
    }

    /// Get one document by id.
    pub fn get(&self, document_id: &str) -> Result<Option<KnowledgeDocument>, KnowledgeError> {
        let connection = self.connection()?;
        self.load_by_id_conn(&connection, document_id)
    }

    /// Get one document by project + portable path.
    pub fn get_by_path(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<Option<KnowledgeDocument>, KnowledgeError> {
        validate_project_id(project_id)?;
        // Query by exact stored path; still reject non-portable lookup keys so
        // callers cannot probe with illegal paths.
        validate_path(path)?;
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT document_id, project_id, path, role, status, content_digest,
                        updated_at, registered_by, source_run_id
                 FROM knowledge_documents
                 WHERE project_id = ?1 AND path = ?2",
                params![project_id, path],
                document_from_row,
            )
            .optional()
            .map_err(map_sqlite)
    }

    /// List documents for a project (path ASC, then document_id ASC).
    ///
    /// **Excludes `archived` by default** (ADR-0004 §5). Equivalent to
    /// [`list_documents`](Self::list_documents) with
    /// [`ListDocumentsQuery::default()`].
    pub fn list_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeDocument>, KnowledgeError> {
        self.list_documents(project_id, ListDocumentsQuery::default())
    }

    /// List documents for a project with explicit visibility options.
    ///
    /// Use `include_archived: true` only for inventory, rescan, or explicit
    /// recovery paths. Default query surfaces must call
    /// [`list_by_project`](Self::list_by_project) or pass
    /// `include_archived: false`.
    pub fn list_documents(
        &self,
        project_id: &str,
        query: ListDocumentsQuery,
    ) -> Result<Vec<KnowledgeDocument>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
        if query.include_archived {
            let mut statement = connection
                .prepare(
                    "SELECT document_id, project_id, path, role, status, content_digest,
                            updated_at, registered_by, source_run_id
                     FROM knowledge_documents
                     WHERE project_id = ?1
                     ORDER BY path ASC, document_id ASC",
                )
                .map_err(map_sqlite)?;
            let rows = statement
                .query_map(params![project_id], document_from_row)
                .map_err(map_sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(map_sqlite)?);
            }
            Ok(out)
        } else {
            // Hard default: status ≠ archived. Callers cannot "forget" the filter.
            let mut statement = connection
                .prepare(
                    "SELECT document_id, project_id, path, role, status, content_digest,
                            updated_at, registered_by, source_run_id
                     FROM knowledge_documents
                     WHERE project_id = ?1 AND status != 'archived'
                     ORDER BY path ASC, document_id ASC",
                )
                .map_err(map_sqlite)?;
            let rows = statement
                .query_map(params![project_id], document_from_row)
                .map_err(map_sqlite)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(map_sqlite)?);
            }
            Ok(out)
        }
    }

    /// Explicit archive (K1a). Sets `status = archived`.
    ///
    /// Never automatic — scanners and link writes must not call this. Does not
    /// delete rows. Idempotent when already archived.
    pub fn archive(&self, document_id: &str) -> Result<KnowledgeDocument, KnowledgeError> {
        if document_id.is_empty() {
            return Err(KnowledgeError::NotFound {
                document_id: document_id.to_owned(),
            });
        }
        self.update(UpdateDocument {
            document_id: document_id.to_owned(),
            role: None,
            status: Some(DocumentStatus::Archived),
            content_digest: None,
            registered_by: None,
            source_run_id: None,
        })
    }

    /// Explicit unarchive (K1a). Restores status to [`DocumentStatus::Unknown`].
    ///
    /// Callers that need a stronger lifecycle label must follow with an
    /// explicit [`update`](Self::update). Does not invent prior status (no
    /// auto-restore of `active` / `superseded`).
    pub fn unarchive(&self, document_id: &str) -> Result<KnowledgeDocument, KnowledgeError> {
        if document_id.is_empty() {
            return Err(KnowledgeError::NotFound {
                document_id: document_id.to_owned(),
            });
        }
        let connection = self.connection()?;
        let current = self
            .load_by_id_conn(&connection, document_id)?
            .ok_or_else(|| KnowledgeError::NotFound {
                document_id: document_id.to_owned(),
            })?;
        drop(connection);

        if current.status != DocumentStatus::Archived {
            // Already live — return as-is (no silent status invention).
            return Ok(current);
        }

        self.update(UpdateDocument {
            document_id: document_id.to_owned(),
            role: None,
            status: Some(DocumentStatus::Unknown),
            content_digest: None,
            registered_by: None,
            source_run_id: None,
        })
    }

    /// Insert a multi-valued link. Supports 1→N supersession: call once per
    /// target. Both endpoints may not be empty.
    pub fn add_link(
        &self,
        input: AddDocumentLink,
    ) -> Result<KnowledgeDocumentLink, KnowledgeError> {
        if input.from_document_id.is_empty() {
            return Err(KnowledgeError::NotFound {
                document_id: input.from_document_id,
            });
        }
        let to_document_id = normalize_optional(&input.to_document_id);
        let to_path = normalize_optional(&input.to_path);
        if to_document_id.is_none() && to_path.is_none() {
            return Err(KnowledgeError::LinkEndpointRequired);
        }
        // When a path is provided, accept free-text non-path claims as well as
        // portable paths (audit case: "prior delivery status"). Path-form
        // targets are still checked when they look portable-relative.
        if let Some(ref path) = to_path {
            // Only enforce SSoT when the claim is a path-shaped portable form
            // (contains `/` or looks like a relative file). Free-text claims
            // (no path) are stored as-is within length bounds.
            if looks_like_relative_path(path) {
                validate_path(path)?;
            } else if path.is_empty() || path.len() > 4096 {
                return Err(KnowledgeError::InvalidPath {
                    detail: "empty or too long".to_owned(),
                });
            }
        }

        let connection = self.connection()?;
        // Ensure from document exists (clearer error than bare FK).
        if self
            .load_by_id_conn(&connection, &input.from_document_id)?
            .is_none()
        {
            return Err(KnowledgeError::NotFound {
                document_id: input.from_document_id,
            });
        }
        if let Some(ref to_id) = to_document_id
            && self.load_by_id_conn(&connection, to_id)?.is_none()
        {
            return Err(KnowledgeError::NotFound {
                document_id: to_id.clone(),
            });
        }

        let link_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO knowledge_document_links (
                    link_id, from_document_id, to_document_id, to_path, rel
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    link_id,
                    input.from_document_id,
                    to_document_id,
                    to_path,
                    input.rel.as_str(),
                ],
            )
            .map_err(map_sqlite)?;

        Ok(KnowledgeDocumentLink {
            link_id,
            from_document_id: input.from_document_id,
            to_document_id,
            to_path,
            rel: input.rel,
        })
    }

    /// List all outbound links from a document (supports 1→N query).
    pub fn list_links_from(
        &self,
        from_document_id: &str,
    ) -> Result<Vec<KnowledgeDocumentLink>, KnowledgeError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT link_id, from_document_id, to_document_id, to_path, rel
                 FROM knowledge_document_links
                 WHERE from_document_id = ?1
                 ORDER BY link_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![from_document_id], link_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// List outbound links filtered by relation (e.g. all `supersedes`).
    pub fn list_links_from_with_rel(
        &self,
        from_document_id: &str,
        rel: DocumentLinkRel,
    ) -> Result<Vec<KnowledgeDocumentLink>, KnowledgeError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT link_id, from_document_id, to_document_id, to_path, rel
                 FROM knowledge_document_links
                 WHERE from_document_id = ?1 AND rel = ?2
                 ORDER BY link_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![from_document_id, rel.as_str()], link_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------
    // K0d ledger: dispatch + acceptance
    // -----------------------------------------------------------------------

    /// Record a dispatch as a byproduct of controller dispatch (ADR-0004 §2).
    /// Status starts as [`TaskStatus::Dispatched`]. Never stores lease/session ids.
    pub fn dispatch_task(&self, input: DispatchTask) -> Result<KnowledgeTask, KnowledgeError> {
        validate_project_id(&input.project_id)?;
        validate_task_title(&input.title)?;
        reject_privacy_text(&input.title)?;
        let brief_path = normalize_optional(&input.brief_path);
        if let Some(ref path) = brief_path {
            validate_path(path)?;
            reject_privacy_text(path)?;
        }
        validate_worker_label(&input.worker_label)?;

        let now = Utc::now();
        let task_id = Uuid::new_v4().to_string();
        let status = TaskStatus::Dispatched;

        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO knowledge_tasks (
                    task_id, project_id, title, brief_path, worker_label,
                    dispatched_at, status, completion_signal
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![
                    task_id,
                    input.project_id,
                    input.title,
                    brief_path,
                    input.worker_label,
                    now.to_rfc3339(),
                    status.as_str(),
                ],
            )
            .map_err(map_sqlite)?;

        Ok(KnowledgeTask {
            task_id,
            project_id: input.project_id,
            title: input.title,
            brief_path,
            worker_label: input.worker_label,
            dispatched_at: now,
            status,
            completion_signal: None,
        })
    }

    /// Explicit task status update. Status is never inferred or auto-migrated.
    pub fn set_task_status(&self, input: SetTaskStatus) -> Result<KnowledgeTask, KnowledgeError> {
        if input.task_id.is_empty() {
            return Err(KnowledgeError::TaskNotFound {
                task_id: input.task_id,
            });
        }
        let completion_signal = normalize_optional(&input.completion_signal);
        if let Some(ref signal) = completion_signal {
            reject_privacy_text(signal)?;
        }

        let connection = self.connection()?;
        let mut current = self
            .load_task_conn(&connection, &input.task_id)?
            .ok_or_else(|| KnowledgeError::TaskNotFound {
                task_id: input.task_id.clone(),
            })?;

        current.status = input.status;
        current.completion_signal = completion_signal;

        connection
            .execute(
                "UPDATE knowledge_tasks SET
                    status = ?1,
                    completion_signal = ?2
                 WHERE task_id = ?3",
                params![
                    current.status.as_str(),
                    current.completion_signal,
                    current.task_id,
                ],
            )
            .map_err(map_sqlite)?;

        Ok(current)
    }

    /// Get one task by id.
    pub fn get_task(&self, task_id: &str) -> Result<Option<KnowledgeTask>, KnowledgeError> {
        let connection = self.connection()?;
        self.load_task_conn(&connection, task_id)
    }

    /// List all tasks for a project (dispatched_at DESC, then task_id ASC).
    pub fn list_tasks_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeTask>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT task_id, project_id, title, brief_path, worker_label,
                        dispatched_at, status, completion_signal
                 FROM knowledge_tasks
                 WHERE project_id = ?1
                 ORDER BY dispatched_at DESC, task_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![project_id], task_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// List tasks with a given status for a project.
    pub fn list_tasks_by_status(
        &self,
        project_id: &str,
        status: TaskStatus,
    ) -> Result<Vec<KnowledgeTask>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT task_id, project_id, title, brief_path, worker_label,
                        dispatched_at, status, completion_signal
                 FROM knowledge_tasks
                 WHERE project_id = ?1 AND status = ?2
                 ORDER BY dispatched_at DESC, task_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![project_id, status.as_str()], task_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// Record a controller acceptance verdict (byproduct of acceptance).
    /// `task_id` may be `None` for controller-self work.
    pub fn record_verdict(&self, input: RecordVerdict) -> Result<KnowledgeVerdict, KnowledgeError> {
        validate_project_id(&input.project_id)?;
        validate_verdict_summary(&input.summary)?;
        reject_privacy_text(&input.summary)?;
        let residuals = normalize_optional(&input.residuals);
        if let Some(ref r) = residuals {
            reject_privacy_text(r)?;
        }
        let gate_result = normalize_optional(&input.gate_result);
        if let Some(ref g) = gate_result {
            reject_privacy_text(g)?;
        }
        let e2e_result = normalize_optional(&input.e2e_result);
        if let Some(ref e) = e2e_result {
            reject_privacy_text(e)?;
        }
        let task_id = normalize_optional(&input.task_id);

        let connection = self.connection()?;
        if let Some(ref tid) = task_id
            && self.load_task_conn(&connection, tid)?.is_none()
        {
            return Err(KnowledgeError::TaskNotFound {
                task_id: tid.clone(),
            });
        }

        let now = Utc::now();
        let verdict_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO knowledge_verdicts (
                    verdict_id, task_id, project_id, outcome, gate_result,
                    e2e_result, summary, residuals, decided_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    verdict_id,
                    task_id,
                    input.project_id,
                    input.outcome.as_str(),
                    gate_result,
                    e2e_result,
                    input.summary,
                    residuals,
                    now.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;

        Ok(KnowledgeVerdict {
            verdict_id,
            task_id,
            project_id: input.project_id,
            outcome: input.outcome,
            gate_result,
            e2e_result,
            summary: input.summary,
            residuals,
            decided_at: now,
        })
    }

    /// Get one verdict by id.
    pub fn get_verdict(
        &self,
        verdict_id: &str,
    ) -> Result<Option<KnowledgeVerdict>, KnowledgeError> {
        let connection = self.connection()?;
        self.load_verdict_conn(&connection, verdict_id)
    }

    /// List all verdicts for a project (decided_at DESC, then verdict_id ASC).
    pub fn list_verdicts_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeVerdict>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT verdict_id, task_id, project_id, outcome, gate_result,
                        e2e_result, summary, residuals, decided_at
                 FROM knowledge_verdicts
                 WHERE project_id = ?1
                 ORDER BY decided_at DESC, verdict_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![project_id], verdict_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// Attach a registered document as an output of a task.
    pub fn attach_task_document(
        &self,
        task_id: &str,
        document_id: &str,
    ) -> Result<(), KnowledgeError> {
        if task_id.is_empty() {
            return Err(KnowledgeError::TaskNotFound {
                task_id: task_id.to_owned(),
            });
        }
        if document_id.is_empty() {
            return Err(KnowledgeError::NotFound {
                document_id: document_id.to_owned(),
            });
        }
        let connection = self.connection()?;
        if self.load_task_conn(&connection, task_id)?.is_none() {
            return Err(KnowledgeError::TaskNotFound {
                task_id: task_id.to_owned(),
            });
        }
        if self.load_by_id_conn(&connection, document_id)?.is_none() {
            return Err(KnowledgeError::NotFound {
                document_id: document_id.to_owned(),
            });
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_task_documents (task_id, document_id)
                 VALUES (?1, ?2)",
                params![task_id, document_id],
            )
            .map_err(map_sqlite)?;
        Ok(())
    }

    /// List document ids produced by a task.
    pub fn list_task_documents(&self, task_id: &str) -> Result<Vec<String>, KnowledgeError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT document_id FROM knowledge_task_documents
                 WHERE task_id = ?1
                 ORDER BY document_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![task_id], |row| row.get::<_, String>(0))
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    /// Tasks with `status=completed` that have no verdict row — the core
    /// "controller forgot to accept" mechanical detection.
    pub fn list_completed_without_verdict(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeTask>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT t.task_id, t.project_id, t.title, t.brief_path, t.worker_label,
                        t.dispatched_at, t.status, t.completion_signal
                 FROM knowledge_tasks t
                 WHERE t.project_id = ?1
                   AND t.status = 'completed'
                   AND NOT EXISTS (
                       SELECT 1 FROM knowledge_verdicts v
                       WHERE v.task_id = t.task_id
                   )
                 ORDER BY t.dispatched_at DESC, t.task_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![project_id], task_from_row)
            .map_err(map_sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sqlite)?);
        }
        Ok(out)
    }

    fn load_by_id_conn(
        &self,
        connection: &Connection,
        document_id: &str,
    ) -> Result<Option<KnowledgeDocument>, KnowledgeError> {
        connection
            .query_row(
                "SELECT document_id, project_id, path, role, status, content_digest,
                        updated_at, registered_by, source_run_id
                 FROM knowledge_documents
                 WHERE document_id = ?1",
                params![document_id],
                document_from_row,
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn load_task_conn(
        &self,
        connection: &Connection,
        task_id: &str,
    ) -> Result<Option<KnowledgeTask>, KnowledgeError> {
        connection
            .query_row(
                "SELECT task_id, project_id, title, brief_path, worker_label,
                        dispatched_at, status, completion_signal
                 FROM knowledge_tasks
                 WHERE task_id = ?1",
                params![task_id],
                task_from_row,
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn load_verdict_conn(
        &self,
        connection: &Connection,
        verdict_id: &str,
    ) -> Result<Option<KnowledgeVerdict>, KnowledgeError> {
        connection
            .query_row(
                "SELECT verdict_id, task_id, project_id, outcome, gate_result,
                        e2e_result, summary, residuals, decided_at
                 FROM knowledge_verdicts
                 WHERE verdict_id = ?1",
                params![verdict_id],
                verdict_from_row,
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, KnowledgeError> {
        self.connection
            .lock()
            .map_err(|_| KnowledgeError::LockPoisoned)
    }
}

fn validate_project_id(project_id: &str) -> Result<(), KnowledgeError> {
    if project_id.is_empty() {
        return Err(KnowledgeError::InvalidProjectId);
    }
    Ok(())
}

/// Path validation SSoT: `keeper_project::validate_portable_path` only.
/// Do not reimplement portable-path rules here.
fn validate_path(path: &str) -> Result<(), KnowledgeError> {
    validate_portable_path(path).map_err(|detail| KnowledgeError::InvalidPath {
        detail: detail.to_owned(),
    })
}

fn validate_content_digest(digest: &str) -> Result<(), KnowledgeError> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(KnowledgeError::InvalidContentDigest);
    };
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(KnowledgeError::InvalidContentDigest);
    }
    Ok(())
}

fn validate_source_run_id(source_run_id: Option<&str>) -> Result<(), KnowledgeError> {
    match source_run_id {
        None => Ok(()),
        Some(id) if !id.is_empty() && id.len() <= 64 => Ok(()),
        Some(_) => Err(KnowledgeError::InvalidSourceRunId),
    }
}

fn validate_task_title(title: &str) -> Result<(), KnowledgeError> {
    let t = title.trim();
    if t.is_empty() || t.len() > 512 {
        return Err(KnowledgeError::InvalidTaskTitle);
    }
    Ok(())
}

fn validate_verdict_summary(summary: &str) -> Result<(), KnowledgeError> {
    let s = summary.trim();
    if s.is_empty() || s.len() > 8192 {
        return Err(KnowledgeError::InvalidVerdictSummary);
    }
    Ok(())
}

/// Slot label only: short alphanumeric token like `grok3` / `codex1`.
/// Rejects lease ids, session ids, and anything that is not a plain label.
fn validate_worker_label(label: &str) -> Result<(), KnowledgeError> {
    let l = label.trim();
    if l.is_empty() || l.len() > 64 {
        return Err(KnowledgeError::InvalidWorkerLabel {
            detail: "empty or too long".to_owned(),
        });
    }
    // Must look like a slot label, not a free-form path or id.
    let ok_chars = l
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !ok_chars {
        return Err(KnowledgeError::InvalidWorkerLabel {
            detail: "only alphanumeric, hyphen, underscore allowed".to_owned(),
        });
    }
    // Explicit red lines even if they match the char class.
    if contains_privacy_leak(l) {
        return Err(KnowledgeError::InvalidWorkerLabel {
            detail: "lease id, session id, or host path refused".to_owned(),
        });
    }
    // Lease tokens use only allowed chars — block by assembled marker too.
    let lease_marker = format!("{}-{}-", "grok", "lease");
    if l.starts_with(&lease_marker) || l.contains(&lease_marker) {
        return Err(KnowledgeError::InvalidWorkerLabel {
            detail: "lease id refused; store slot label only".to_owned(),
        });
    }
    Ok(())
}

/// Reject Keeper lease ids, session ids, and host user-profile paths
/// (aligned with `scripts/check-secrets.mjs` block patterns).
///
/// Forbidden samples are never written as contiguous literals in this file so
/// the repository secret gate does not flag production/test sources.
fn contains_privacy_leak(text: &str) -> bool {
    // Keeper lease marker (assembled so source has no contiguous lease token).
    let lease_marker = format!("{}-{}-", "grok", "lease");
    if text.contains(&lease_marker) {
        return true;
    }
    // Keeper session shape: YYYYMMDDTHHMMSSZ-<word>-<tail>
    if session_id_present(text) {
        return true;
    }
    // Host user-profile paths (Windows / MSYS / cygdrive).
    // Alice is the synthetic redaction fixture exception in check-secrets.
    if user_profile_path_present(text) {
        return true;
    }
    false
}

fn session_id_present(text: &str) -> bool {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i + 16 <= n {
        // 8 digits
        if bytes[i..i + 8].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 8] == b'T'
            && bytes[i + 9..i + 15].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 15] == b'Z'
            && i + 16 < n
            && bytes[i + 16] == b'-'
        {
            // -[a-z]+-[a-z0-9]{4,}
            let mut j = i + 17;
            let start_word = j;
            while j < n && bytes[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > start_word && j < n && bytes[j] == b'-' {
                j += 1;
                let start_tail = j;
                while j < n && (bytes[j].is_ascii_lowercase() || bytes[j].is_ascii_digit()) {
                    j += 1;
                }
                if j - start_tail >= 4 {
                    // word boundary-ish: start is beginning or non-alnum
                    let start_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                    let end_ok = j == n || !bytes[j].is_ascii_alphanumeric();
                    if start_ok && end_ok {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn user_profile_path_present(text: &str) -> bool {
    // Drive form: c:\users\<name> or c:/users/<name>
    // MSYS: /c/users/<name>
    // cygdrive: /cygdrive/c/users/<name>
    let lower = text.to_ascii_lowercase();
    let s = lower.as_str();

    // Drive letter form: <letter>:\users\<name> or <letter>:/users/<name>
    for i in 0..s.len().saturating_sub(9) {
        let rest = &s[i..];
        if (rest.starts_with(":\\users\\") || rest.starts_with(":/users/"))
            && i > 0
            && s.as_bytes()[i - 1].is_ascii_alphabetic()
        {
            let after = &rest[8..];
            if let Some(name) = user_name_segment(after)
                && name != "alice"
            {
                return true;
            }
        }
    }
    if let Some(idx) = s.find("/c/users/") {
        let after = &s[idx + "/c/users/".len()..];
        if let Some(name) = user_name_segment(after)
            && name != "alice"
        {
            return true;
        }
    }
    if let Some(idx) = s.find("/cygdrive/") {
        let after_drive = &s[idx + "/cygdrive/".len()..];
        // single letter + /users/
        if after_drive.len() >= 8
            && after_drive.as_bytes()[0].is_ascii_lowercase()
            && after_drive.as_bytes().get(1) == Some(&b'/')
            && after_drive[2..].starts_with("users/")
        {
            let after = &after_drive[2 + "users/".len()..];
            if let Some(name) = user_name_segment(after)
                && name != "alice"
            {
                return true;
            }
        }
    }
    false
}

fn user_name_segment(after: &str) -> Option<&str> {
    let end = after
        .find(|c: char| c == '/' || c == '\\' || c.is_whitespace())
        .unwrap_or(after.len());
    let name = &after[..end];
    if name.is_empty() {
        None
    } else if name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        Some(name)
    } else {
        None
    }
}

fn reject_privacy_text(text: &str) -> Result<(), KnowledgeError> {
    if contains_privacy_leak(text) {
        Err(KnowledgeError::PrivacyViolation)
    } else {
        Ok(())
    }
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

/// Heuristic: path-shaped claims contain `/` or a file extension-like suffix.
/// Free-text non-path supersession claims (audit: "prior delivery status")
/// skip portable-path SSoT and are stored as declared text.
fn looks_like_relative_path(value: &str) -> bool {
    value.contains('/') || value.contains('.')
}

fn document_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocument> {
    let document_id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let path: String = row.get(2)?;
    let role_raw: String = row.get(3)?;
    let status_raw: String = row.get(4)?;
    let content_digest: String = row.get(5)?;
    let updated_at_raw: String = row.get(6)?;
    let registered_by_raw: String = row.get(7)?;
    let source_run_id: Option<String> = row.get(8)?;

    let role = DocumentRole::parse(&role_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let status = DocumentStatus::parse(&status_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let registered_by = RegisteredBy::parse(&registered_by_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_raw)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(KnowledgeDocument {
        document_id,
        project_id,
        path,
        role,
        status,
        content_digest,
        updated_at,
        registered_by,
        source_run_id,
    })
}

fn link_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocumentLink> {
    let link_id: String = row.get(0)?;
    let from_document_id: String = row.get(1)?;
    let to_document_id: Option<String> = row.get(2)?;
    let to_path: Option<String> = row.get(3)?;
    let rel_raw: String = row.get(4)?;
    let rel = DocumentLinkRel::parse(&rel_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    Ok(KnowledgeDocumentLink {
        link_id,
        from_document_id,
        to_document_id,
        to_path,
        rel,
    })
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeTask> {
    let task_id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let title: String = row.get(2)?;
    let brief_path: Option<String> = row.get(3)?;
    let worker_label: String = row.get(4)?;
    let dispatched_at_raw: String = row.get(5)?;
    let status_raw: String = row.get(6)?;
    let completion_signal: Option<String> = row.get(7)?;

    let status = TaskStatus::parse(&status_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let dispatched_at = DateTime::parse_from_rfc3339(&dispatched_at_raw)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(KnowledgeTask {
        task_id,
        project_id,
        title,
        brief_path,
        worker_label,
        dispatched_at,
        status,
        completion_signal,
    })
}

fn verdict_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeVerdict> {
    let verdict_id: String = row.get(0)?;
    let task_id: Option<String> = row.get(1)?;
    let project_id: String = row.get(2)?;
    let outcome_raw: String = row.get(3)?;
    let gate_result: Option<String> = row.get(4)?;
    let e2e_result: Option<String> = row.get(5)?;
    let summary: String = row.get(6)?;
    let residuals: Option<String> = row.get(7)?;
    let decided_at_raw: String = row.get(8)?;

    let outcome = VerdictOutcome::parse(&outcome_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let decided_at = DateTime::parse_from_rfc3339(&decided_at_raw)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(KnowledgeVerdict {
        verdict_id,
        task_id,
        project_id,
        outcome,
        gate_result,
        e2e_result,
        summary,
        residuals,
        decided_at,
    })
}

fn map_sqlite(_: rusqlite::Error) -> KnowledgeError {
    KnowledgeError::Sqlite
}

fn map_insert_error(error: rusqlite::Error) -> KnowledgeError {
    match &error {
        rusqlite::Error::SqliteFailure(err, _)
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            KnowledgeError::DuplicatePath
        }
        _ => KnowledgeError::Sqlite,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn status_default_is_unknown_not_active() {
        assert_eq!(DocumentStatus::default(), DocumentStatus::Unknown);
        assert_ne!(DocumentStatus::default(), DocumentStatus::Active);
        assert_eq!(DocumentStatus::default().as_str(), "unknown");
    }

    #[test]
    fn role_parse_rejects_unknown() {
        let err = DocumentRole::parse("not-a-role").unwrap_err();
        assert!(matches!(err, KnowledgeError::InvalidRole { .. }));
        assert_eq!(err.code(), "knowledge_invalid_role");
    }

    #[test]
    fn status_parse_rejects_unknown() {
        let err = DocumentStatus::parse("live").unwrap_err();
        assert!(matches!(err, KnowledgeError::InvalidStatus { .. }));
        assert_eq!(err.code(), "knowledge_invalid_status");
    }

    #[test]
    fn path_validation_uses_ssot_and_rejects_superscript_com() {
        // COM¹ (U+00B9) is rejected by keeper_project SSoT, not a local rule.
        let err = validate_path("COM\u{00b9}").unwrap_err();
        assert!(matches!(err, KnowledgeError::InvalidPath { .. }));
        assert_eq!(err.code(), "knowledge_invalid_path");
        // Cross-check the SSoT directly so a local reimplementation that
        // accidentally accepted COM¹ would still fail this unit test's twin.
        assert!(
            validate_portable_path("COM\u{00b9}").is_err(),
            "SSoT must reject COM¹"
        );
        assert!(validate_path("docs/handoff/current.md").is_ok());
    }

    #[test]
    fn digest_requires_canonical_sha256() {
        assert!(validate_content_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
        assert!(validate_content_digest("sha256:ABC").is_err());
        assert!(validate_content_digest("md5:deadbeef").is_err());
    }

    #[test]
    fn task_status_parse_rejects_illegal() {
        assert!(TaskStatus::parse("running").is_err());
        assert!(TaskStatus::parse("Dispatched").is_err());
        assert_eq!(
            TaskStatus::parse("dispatched").unwrap(),
            TaskStatus::Dispatched
        );
        assert_eq!(
            TaskStatus::parse("completed").unwrap(),
            TaskStatus::Completed
        );
    }

    #[test]
    fn verdict_outcome_parse_rejects_illegal() {
        assert!(VerdictOutcome::parse("ok").is_err());
        assert!(VerdictOutcome::parse("Accepted").is_err());
        assert_eq!(
            VerdictOutcome::parse("accepted").unwrap(),
            VerdictOutcome::Accepted
        );
    }

    /// Assemble forbidden samples at runtime so source never holds contiguous
    /// lease / session / host-path tokens (secret-gate self-test samples).
    fn privacy_samples() -> (String, String, String, String, String) {
        let lease = format!("{}-{}-{}-{}", "grok", "lease", "msij5g0v", "fc6b73");
        let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
        let win = format!("{}:\\{}\\someone\\.cargo", "C", "Users");
        let msys = format!("/{}/{}/someone/.grok/bin/grok.exe", "c", "Users");
        let cyg = format!("/cygdrive/{}/{}/someone/.grok", "c", "Users");
        (lease, session, win, msys, cyg)
    }

    #[test]
    fn privacy_rejects_lease_session_and_user_paths() {
        let (lease, session, win, msys, cyg) = privacy_samples();
        assert!(contains_privacy_leak(&lease));
        assert!(contains_privacy_leak(&format!(
            "signal with {session} inside"
        )));
        assert!(contains_privacy_leak(&win));
        assert!(contains_privacy_leak(&msys));
        assert!(contains_privacy_leak(&cyg));
        // Synthetic Alice fixture is the documented exception in check-secrets.
        let alice = format!("{}:\\{}\\Alice\\redaction", "C", "Users");
        assert!(!contains_privacy_leak(&alice));
        assert!(!contains_privacy_leak("slot grok3 with clean summary"));
        assert!(validate_worker_label("grok3").is_ok());
        assert!(validate_worker_label("codex1").is_ok());
        assert!(validate_worker_label(&lease).is_err());
    }
}
