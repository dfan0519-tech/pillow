//! Knowledge document registry (K0a storage + repository only).
//!
//! Durable authority for document role, lifecycle status, and multi-valued
//! supersession / related / implements links. Path grammar is the
//! `keeper_project::validate_portable_path` SSoT — this module must not
//! re-implement portable-path rules.
//!
//! Status is never auto-migrated: only explicit set/update calls change it.
//! Default for unclassified registration is [`DocumentStatus::Unknown`].

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

    /// List all documents for a project (path ASC, then document_id ASC).
    pub fn list_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeDocument>, KnowledgeError> {
        validate_project_id(project_id)?;
        let connection = self.connection()?;
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
}
