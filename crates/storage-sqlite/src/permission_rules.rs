//! SQLite-only permission-rule authority for the minimal rule slice.
//!
//! The repository accepts typed user-originated CRUD inputs and never accepts
//! a project root or an externally resolved effective policy. Global rules may
//! allow or deny; project rules are deny-only and effective loading filters
//! that invariant again so corrupt rows cannot widen authority.
//!
//! ## Unreadable rows are quarantined, not fatal
//!
//! Loading fails closed **per row**. A row this module cannot read back as a
//! rule is skipped by [`SqlitePermissionRuleRepository::list`] and
//! [`SqlitePermissionRuleRepository::list_effective`] — it is never evaluated,
//! so a planted match-all pattern cannot grant anything — and it is reported by
//! [`SqlitePermissionRuleRepository::list_unreadable`] so Settings can show and
//! remove it.
//!
//! Rejecting the whole load instead would take out the only surface that can
//! delete the bad row: one row would disable every rule, hide the rest of the
//! list, and leave a database editor as the only recovery. That is the shape of
//! the 2026-08-08 settings self-brick, and the same rule applies here.
//!
//! Together the two list calls cover the table: every row is either readable in
//! its scope or reported as unreadable.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Utc};
use keeper_agent_protocol::canonical_arguments_digest;
use rusqlite::types::{Type, ValueRef};
use rusqlite::{Connection, OptionalExtension, Row, params};
use thiserror::Error;
use uuid::Uuid;

pub const PERMISSION_RULE_PATTERN_BYTE_CAP: usize = 4096;
pub const PERMISSION_RULE_SUBJECT_BYTE_CAP: usize = 8192;

/// Bound on an id accepted by the quarantine repair path.
///
/// A readable rule id is always a 36-character UUID; this larger bound exists
/// only so an unreadable row's id can be echoed back for deletion. It matches
/// the renderer's own id cap.
const PERMISSION_RULE_ID_BYTE_CAP: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleScope {
    Global,
    Project,
}

impl PermissionRuleScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, PermissionRuleError> {
        match raw {
            "global" => Ok(Self::Global),
            "project" => Ok(Self::Project),
            _ => Err(PermissionRuleError::InvalidScope),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRulePrefix {
    Process,
    Write,
    Edit,
}

impl PermissionRulePrefix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::Write => "write",
            Self::Edit => "edit",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, PermissionRuleError> {
        match raw {
            "process" => Ok(Self::Process),
            "write" => Ok(Self::Write),
            "edit" => Ok(Self::Edit),
            _ => Err(PermissionRuleError::InvalidPrefix),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleAction {
    Allow,
    Deny,
}

impl PermissionRuleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, PermissionRuleError> {
        match raw {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            _ => Err(PermissionRuleError::InvalidAction),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    pub rule_id: String,
    pub scope: PermissionRuleScope,
    pub project_id: Option<String>,
    pub prefix: PermissionRulePrefix,
    pub pattern: String,
    pub action: PermissionRuleAction,
    pub content_digest: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPermissionRule {
    pub scope: PermissionRuleScope,
    pub project_id: Option<String>,
    pub prefix: PermissionRulePrefix,
    pub pattern: String,
    pub action: PermissionRuleAction,
}

/// Why a stored row could not be read back as a rule.
///
/// Reported in the order [`RawPermissionRuleRow::classify`] checks: the first
/// failing check wins, so the same row always reports the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadablePermissionRuleReason {
    /// A required column is absent or not stored as text.
    Column,
    /// `scope` is not `global` / `project`.
    Scope,
    /// `prefix` is not `process` / `write` / `edit`.
    Prefix,
    /// `action` is not `allow` / `deny`.
    Action,
    /// Global row carries a project id, or a project row carries none.
    ScopeShape,
    /// Project-scoped allow: project rules may only deny.
    ProjectAllow,
    /// Empty, whitespace-only, oversize, or control-character pattern.
    Pattern,
    /// `rule_id` is not a UUID.
    RuleId,
    /// `created_at` is not an RFC 3339 timestamp.
    CreatedAt,
    /// `content_digest` does not match the row's own content.
    ContentDigest,
}

impl UnreadablePermissionRuleReason {
    /// Stable wire code. The renderer maps these to guidance text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Scope => "scope",
            Self::Prefix => "prefix",
            Self::Action => "action",
            Self::ScopeShape => "scope_shape",
            Self::ProjectAllow => "project_allow",
            Self::Pattern => "pattern",
            Self::RuleId => "rule_id",
            Self::CreatedAt => "created_at",
            Self::ContentDigest => "content_digest",
        }
    }
}

/// A stored row that is not a readable rule, reported so the user can remove it.
///
/// The stored `pattern` and `content_digest` are deliberately **not** carried
/// here. An unreadable row either was never written by this product or has a
/// digest the product cannot vouch for, so its pattern is not something to
/// re-display as a rule the user wrote or offer back for re-adding. The
/// remaining fields are raw column text, present only so the user can recognise
/// the row before deleting it, and absent when the column itself is unreadable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadablePermissionRule {
    pub rule_id: String,
    pub scope: Option<String>,
    pub project_id: Option<String>,
    pub prefix: Option<String>,
    pub action: Option<String>,
    pub created_at: Option<String>,
    pub reason: UnreadablePermissionRuleReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleMatchSnapshot {
    pub rule: PermissionRule,
    pub match_subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleSegmentMatchSnapshot {
    pub segment_index: u32,
    pub matched: PermissionRuleMatchSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleReceiptDecision {
    ApproveByRule,
    RejectByRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleReceiptActor {
    RuleEngine,
}

impl PermissionRuleReceiptActor {
    pub fn as_str(self) -> &'static str {
        "rule_engine"
    }

    fn parse(raw: &str) -> Option<Self> {
        (raw == "rule_engine").then_some(Self::RuleEngine)
    }
}

impl PermissionRuleReceiptDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApproveByRule => "approve_by_rule",
            Self::RejectByRule => "reject_by_rule",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "approve_by_rule" => Some(Self::ApproveByRule),
            "reject_by_rule" => Some(Self::RejectByRule),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPermissionRuleReceipt {
    pub proposal_id: String,
    pub decision: PermissionRuleReceiptDecision,
    pub actor: PermissionRuleReceiptActor,
    pub rule_id: String,
    pub rule_scope: PermissionRuleScope,
    pub rule_project_id: Option<String>,
    pub rule_prefix: PermissionRulePrefix,
    pub rule_pattern: String,
    pub rule_action: PermissionRuleAction,
    pub rule_content_digest: String,
    pub match_subject: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPermissionRuleSegmentReceipt {
    pub segment_index: u32,
    pub receipt: StoredPermissionRuleReceipt,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PermissionRuleError {
    #[error("invalid permission rule scope")]
    InvalidScope,
    #[error("invalid permission rule prefix")]
    InvalidPrefix,
    #[error("invalid permission rule action")]
    InvalidAction,
    #[error("global scope requires no project_id; project scope requires one")]
    ScopeShape,
    #[error("project-scoped permission rules may only deny")]
    ProjectAllowForbidden,
    #[error("permission rule pattern is invalid or exceeds its bound")]
    InvalidPattern,
    #[error("permission rule id is invalid")]
    InvalidRuleId,
    #[error("stored permission rule is invalid")]
    InvalidStoredRule,
    #[error("SQLite permission rule store error")]
    Sqlite,
    #[error("permission rule store lock was poisoned")]
    LockPoisoned,
}

impl PermissionRuleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidScope => "permission_rule_invalid_scope",
            Self::InvalidPrefix => "permission_rule_invalid_prefix",
            Self::InvalidAction => "permission_rule_invalid_action",
            Self::ScopeShape => "permission_rule_scope_shape",
            Self::ProjectAllowForbidden => "permission_rule_project_allow_forbidden",
            Self::InvalidPattern => "permission_rule_invalid_pattern",
            Self::InvalidRuleId => "permission_rule_invalid_rule_id",
            Self::InvalidStoredRule => "permission_rule_invalid_stored_row",
            Self::Sqlite => "permission_rule_sqlite",
            Self::LockPoisoned => "permission_rule_lock_poisoned",
        }
    }
}

pub struct SqlitePermissionRuleRepository {
    connection: Mutex<Connection>,
    database_path: PathBuf,
}

impl SqlitePermissionRuleRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PermissionRuleError> {
        let database_path = path.as_ref().to_path_buf();
        let connection = Connection::open(&database_path).map_err(map_sqlite)?;
        // Command and runtime-host repositories intentionally use separate
        // connections. Wait through brief cross-connection lock contention
        // instead of relying on rusqlite's changeable default busy timeout.
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(map_sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
            database_path,
        })
    }

    pub fn add(&self, input: AddPermissionRule) -> Result<PermissionRule, PermissionRuleError> {
        validate_scope_shape(input.scope, input.project_id.as_deref())?;
        if input.scope == PermissionRuleScope::Project
            && input.action == PermissionRuleAction::Allow
        {
            return Err(PermissionRuleError::ProjectAllowForbidden);
        }
        // Trim then reject empty: empty patterns are match-all in kernel-policy.
        // This layer must not rely on renderer-only validation.
        let pattern = input.pattern.trim().to_owned();
        validate_pattern(&pattern)?;

        let rule = PermissionRule {
            rule_id: Uuid::new_v4().to_string(),
            scope: input.scope,
            project_id: input.project_id,
            prefix: input.prefix,
            pattern,
            action: input.action,
            content_digest: String::new(),
            created_at: Utc::now(),
        };
        let rule = PermissionRule {
            content_digest: content_digest(&rule),
            ..rule
        };
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO permission_rules (
                    rule_id, scope, project_id, prefix, pattern, action,
                    content_digest, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    rule.rule_id,
                    rule.scope.as_str(),
                    rule.project_id,
                    rule.prefix.as_str(),
                    rule.pattern,
                    rule.action.as_str(),
                    rule.content_digest,
                    rule.created_at.to_rfc3339(),
                ],
            )
            .map_err(map_sqlite)?;
        Ok(rule)
    }

    pub fn list(
        &self,
        scope: PermissionRuleScope,
        project_id: Option<&str>,
    ) -> Result<Vec<PermissionRule>, PermissionRuleError> {
        validate_scope_shape(scope, project_id)?;
        let connection = self.connection()?;
        let (sql, bind_project) = match scope {
            PermissionRuleScope::Global => (
                "SELECT rule_id, scope, project_id, prefix, pattern, action,
                        content_digest, created_at
                 FROM permission_rules
                 WHERE scope = 'global'
                 ORDER BY created_at ASC, rule_id ASC",
                None,
            ),
            PermissionRuleScope::Project => (
                "SELECT rule_id, scope, project_id, prefix, pattern, action,
                        content_digest, created_at
                 FROM permission_rules
                 WHERE scope = 'project' AND project_id = ?1
                 ORDER BY created_at ASC, rule_id ASC",
                project_id,
            ),
        };
        let mut statement = connection.prepare(sql).map_err(map_sqlite)?;
        let rows = if let Some(project_id) = bind_project {
            statement
                .query_map([project_id], classify_permission_rule_row)
                .map_err(map_sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlite)?
        } else {
            statement
                .query_map([], classify_permission_rule_row)
                .map_err(map_sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlite)?
        };
        Ok(readable_rules(rows))
    }

    /// Every stored row that is not a readable rule, across all scopes.
    ///
    /// Not scope-filtered on purpose. A row whose `scope` column is itself
    /// unreadable belongs to no scope, and the recovery surface must be able to
    /// show every row [`Self::list`] skipped — including rows no scoped query
    /// would return.
    pub fn list_unreadable(&self) -> Result<Vec<UnreadablePermissionRule>, PermissionRuleError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT rule_id, scope, project_id, prefix, pattern, action,
                        content_digest, created_at
                 FROM permission_rules
                 ORDER BY created_at ASC, rule_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map([], classify_permission_rule_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| match row {
                ClassifiedPermissionRule::Readable(_) => None,
                ClassifiedPermissionRule::Unreadable(row) => Some(row),
            })
            .collect())
    }

    /// Delete one row reported by [`Self::list_unreadable`].
    ///
    /// Readable rules are never deleted through this path — [`Self::delete`]
    /// owns those and keeps its UUID-shaped id requirement. This path exists
    /// because an unreadable row's id may not be a UUID at all, which is exactly
    /// the case `delete` refuses; the row is re-classified inside the deleting
    /// transaction so relaxing the id shape cannot reach a readable rule.
    ///
    /// Returns `false` when no unreadable row carries that id.
    pub fn delete_unreadable(&self, rule_id: &str) -> Result<bool, PermissionRuleError> {
        if rule_id.is_empty() || rule_id.len() > PERMISSION_RULE_ID_BYTE_CAP {
            return Err(PermissionRuleError::InvalidRuleId);
        }
        let connection = self.connection()?;
        connection
            .execute("BEGIN IMMEDIATE", [])
            .map_err(map_sqlite)?;
        match delete_unreadable_rows(&connection, rule_id) {
            Ok(removed) => {
                connection.execute("COMMIT", []).map_err(map_sqlite)?;
                Ok(removed > 0)
            }
            Err(error) => {
                let _ = connection.execute("ROLLBACK", []);
                Err(error)
            }
        }
    }

    pub fn list_effective(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<PermissionRule>, PermissionRuleError> {
        if project_id.is_some_and(|id| id.trim().is_empty()) {
            return Err(PermissionRuleError::ScopeShape);
        }
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT rule_id, scope, project_id, prefix, pattern, action,
                        content_digest, created_at
                 FROM permission_rules
                 WHERE scope = 'global'
                    OR (scope = 'project' AND project_id = ?1 AND action = 'deny')
                 ORDER BY CASE scope WHEN 'global' THEN 0 ELSE 1 END,
                          created_at ASC, rule_id ASC",
            )
            .map_err(map_sqlite)?;
        let rows = statement
            .query_map([project_id], classify_permission_rule_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;
        Ok(readable_rules(rows))
    }

    pub fn delete(&self, rule_id: &str) -> Result<bool, PermissionRuleError> {
        Uuid::parse_str(rule_id).map_err(|_| PermissionRuleError::InvalidRuleId)?;
        let connection = self.connection()?;
        let changed = connection
            .execute("DELETE FROM permission_rules WHERE rule_id = ?1", [rule_id])
            .map_err(map_sqlite)?;
        Ok(changed == 1)
    }

    pub fn decide_exec_by_rule(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        matched: &PermissionRuleMatchSnapshot,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<crate::ExecDecisionOutcome, crate::ExecProposalStoreError> {
        crate::SqliteExecProposalRepository::open(&self.database_path)?.decide_by_rule(
            proposal_id,
            conversation_id,
            matched,
            approval_grant_digest,
            proposal_fingerprint,
            now,
        )
    }

    pub fn decide_exec_by_rule_segments(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        matched: &[PermissionRuleSegmentMatchSnapshot],
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<crate::ExecDecisionOutcome, crate::ExecProposalStoreError> {
        crate::SqliteExecProposalRepository::open(&self.database_path)?.decide_by_rule_segments(
            proposal_id,
            conversation_id,
            matched,
            approval_grant_digest,
            proposal_fingerprint,
            now,
        )
    }

    pub fn get_write_rule_receipt(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredPermissionRuleReceipt>, PermissionRuleError> {
        self.get_rule_receipt("write_permission_rule_receipts", proposal_id)
    }

    pub fn get_exec_rule_receipt(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredPermissionRuleReceipt>, PermissionRuleError> {
        self.get_rule_receipt("exec_permission_rule_receipts", proposal_id)
    }

    pub fn get_exec_rule_receipts(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<StoredPermissionRuleSegmentReceipt>, PermissionRuleError> {
        crate::SqliteExecProposalRepository::open(&self.database_path)
            .map_err(|_| PermissionRuleError::Sqlite)?
            .get_rule_receipts(proposal_id)
            .map_err(|_| PermissionRuleError::Sqlite)
    }

    fn get_rule_receipt(
        &self,
        table: &'static str,
        proposal_id: &str,
    ) -> Result<Option<StoredPermissionRuleReceipt>, PermissionRuleError> {
        let sql = match table {
            "write_permission_rule_receipts" => {
                "SELECT proposal_id, decision, actor, rule_id, rule_scope, rule_project_id,
                        rule_prefix, rule_pattern, rule_action, rule_content_digest,
                        match_subject, decided_at
                 FROM write_permission_rule_receipts WHERE proposal_id = ?1"
            }
            "exec_permission_rule_receipts" => {
                "SELECT proposal_id, decision, actor, rule_id, rule_scope, rule_project_id,
                        rule_prefix, rule_pattern, rule_action, rule_content_digest,
                        match_subject, decided_at
                 FROM exec_permission_rule_receipts WHERE proposal_id = ?1"
            }
            _ => return Err(PermissionRuleError::Sqlite),
        };
        let connection = self.connection()?;
        connection
            .query_row(sql, [proposal_id], permission_rule_receipt_from_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, PermissionRuleError> {
        self.connection
            .lock()
            .map_err(|_| PermissionRuleError::LockPoisoned)
    }
}

fn validate_scope_shape(
    scope: PermissionRuleScope,
    project_id: Option<&str>,
) -> Result<(), PermissionRuleError> {
    match (scope, project_id) {
        (PermissionRuleScope::Global, None) => Ok(()),
        (PermissionRuleScope::Project, Some(id)) if !id.trim().is_empty() => Ok(()),
        _ => Err(PermissionRuleError::ScopeShape),
    }
}

/// Reject patterns that cannot mean what a user wrote.
///
/// Empty and whitespace-only patterns are rejected because kernel policy reads
/// an empty pattern as match-all: a blank `global process allow` row would
/// silently authorise every command (external-audit EA-03 sequence B). Rejecting
/// them here covers both directions — nothing can be inserted with such a
/// pattern, and a row that already carries one is quarantined on load rather
/// than evaluated.
fn validate_pattern(pattern: &str) -> Result<(), PermissionRuleError> {
    if pattern.trim().is_empty()
        || pattern.len() > PERMISSION_RULE_PATTERN_BYTE_CAP
        || pattern.chars().any(|ch| matches!(ch, '\0' | '\r' | '\n'))
    {
        return Err(PermissionRuleError::InvalidPattern);
    }
    Ok(())
}

pub(crate) fn validate_rule_match_snapshot(
    matched: &PermissionRuleMatchSnapshot,
) -> Result<(), PermissionRuleError> {
    validate_scope_shape(matched.rule.scope, matched.rule.project_id.as_deref())?;
    if matched.rule.scope == PermissionRuleScope::Project
        && matched.rule.action == PermissionRuleAction::Allow
    {
        return Err(PermissionRuleError::ProjectAllowForbidden);
    }
    validate_pattern(&matched.rule.pattern)?;
    if matched.match_subject.is_empty()
        || matched.match_subject.len() > PERMISSION_RULE_SUBJECT_BYTE_CAP
        || Uuid::parse_str(&matched.rule.rule_id).is_err()
        || matched.rule.content_digest != content_digest(&matched.rule)
    {
        return Err(PermissionRuleError::InvalidStoredRule);
    }
    Ok(())
}

pub(crate) fn receipt_matches_snapshot(
    receipt: &StoredPermissionRuleReceipt,
    matched: &PermissionRuleMatchSnapshot,
) -> bool {
    receipt.rule_id == matched.rule.rule_id
        && receipt.rule_scope == matched.rule.scope
        && receipt.rule_project_id == matched.rule.project_id
        && receipt.rule_prefix == matched.rule.prefix
        && receipt.rule_pattern == matched.rule.pattern
        && receipt.rule_action == matched.rule.action
        && receipt.rule_content_digest == matched.rule.content_digest
        && receipt.match_subject == matched.match_subject
}

pub(crate) fn rule_snapshot_exists(
    connection: &Connection,
    matched: &PermissionRuleMatchSnapshot,
) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM permission_rules
            WHERE rule_id = ?1
              AND scope = ?2
              AND project_id IS ?3
              AND prefix = ?4
              AND pattern = ?5
              AND action = ?6
              AND content_digest = ?7
         )",
        params![
            matched.rule.rule_id,
            matched.rule.scope.as_str(),
            matched.rule.project_id,
            matched.rule.prefix.as_str(),
            matched.rule.pattern,
            matched.rule.action.as_str(),
            matched.rule.content_digest,
        ],
        |row| row.get(0),
    )
}

pub(crate) fn permission_rule_receipt_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<StoredPermissionRuleReceipt> {
    let decision: String = row.get(1)?;
    let actor: String = row.get(2)?;
    let scope: String = row.get(4)?;
    let prefix: String = row.get(6)?;
    let action: String = row.get(8)?;
    let decided_at: String = row.get(11)?;
    Ok(StoredPermissionRuleReceipt {
        proposal_id: row.get(0)?,
        decision: PermissionRuleReceiptDecision::parse(&decision)
            .ok_or_else(|| conversion_error(1, PermissionRuleError::InvalidStoredRule))?,
        actor: PermissionRuleReceiptActor::parse(&actor)
            .ok_or_else(|| conversion_error(2, PermissionRuleError::InvalidStoredRule))?,
        rule_id: row.get(3)?,
        rule_scope: PermissionRuleScope::parse(&scope)
            .map_err(|error| conversion_error(4, error))?,
        rule_project_id: row.get(5)?,
        rule_prefix: PermissionRulePrefix::parse(&prefix)
            .map_err(|error| conversion_error(6, error))?,
        rule_pattern: row.get(7)?,
        rule_action: PermissionRuleAction::parse(&action)
            .map_err(|error| conversion_error(8, error))?,
        rule_content_digest: row.get(9)?,
        match_subject: row.get(10)?,
        decided_at: DateTime::parse_from_rfc3339(&decided_at)
            .map(|time| time.with_timezone(&Utc))
            .map_err(|error| conversion_error(11, error))?,
    })
}

pub(crate) fn permission_rule_segment_receipt_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<StoredPermissionRuleSegmentReceipt> {
    let raw_segment_index: i64 = row.get(12)?;
    let segment_index = u32::try_from(raw_segment_index)
        .map_err(|_| conversion_error(12, PermissionRuleError::InvalidStoredRule))?;
    Ok(StoredPermissionRuleSegmentReceipt {
        segment_index,
        receipt: permission_rule_receipt_from_row(row)?,
    })
}

pub(crate) fn content_digest(rule: &PermissionRule) -> String {
    let body = serde_json::json!({
        "schema": "permission-rule-v1",
        "scope": rule.scope.as_str(),
        "project_id": rule.project_id.as_deref(),
        "prefix": rule.prefix.as_str(),
        "action": rule.action.as_str(),
        "pattern": rule.pattern,
    });
    canonical_arguments_digest(&body)
        .expect("validated permission-rule bodies fit the canonical digest bound")
        .as_str()
        .to_owned()
}

/// One stored row, after classification.
enum ClassifiedPermissionRule {
    Readable(PermissionRule),
    Unreadable(UnreadablePermissionRule),
}

/// The eight `permission_rules` columns as raw text.
///
/// Read as raw values rather than typed ones so a row that cannot convert is
/// classified instead of failing the load: `permission_rule_from_row`'s typed
/// reads used to surface a single bad `created_at` as a store-wide error, which
/// took the whole rule set (and the Settings list that deletes it) down with it.
struct RawPermissionRuleRow {
    rule_id: Option<String>,
    /// The id column as addressable text, including a lossy rendering of a
    /// non-text id. Never used to classify a row — a blob id is not a readable
    /// id even when its bytes spell a UUID — only to report a key the repair
    /// path can match, so such a row is still removable in-product.
    id_text: Option<String>,
    scope: Option<String>,
    project_id: Option<String>,
    prefix: Option<String>,
    pattern: Option<String>,
    action: Option<String>,
    content_digest: Option<String>,
    created_at: Option<String>,
}

impl RawPermissionRuleRow {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            rule_id: raw_column_text(row, 0)?,
            id_text: raw_addressable_text(row, 0)?,
            scope: raw_column_text(row, 1)?,
            project_id: raw_column_text(row, 2)?,
            prefix: raw_column_text(row, 3)?,
            pattern: raw_column_text(row, 4)?,
            action: raw_column_text(row, 5)?,
            content_digest: raw_column_text(row, 6)?,
            created_at: raw_column_text(row, 7)?,
        })
    }

    fn unreadable(&self, reason: UnreadablePermissionRuleReason) -> ClassifiedPermissionRule {
        ClassifiedPermissionRule::Unreadable(UnreadablePermissionRule {
            rule_id: self.id_text.clone().unwrap_or_default(),
            scope: self.scope.clone(),
            project_id: self.project_id.clone(),
            prefix: self.prefix.clone(),
            action: self.action.clone(),
            created_at: self.created_at.clone(),
            reason,
        })
    }

    /// Classify the row. Checks run in a fixed order so one row always reports
    /// one reason; the pattern bound is checked before the digest so an oversize
    /// pattern can never reach the digest helper's own bound.
    fn classify(self) -> ClassifiedPermissionRule {
        use UnreadablePermissionRuleReason as Reason;

        let (Some(rule_id), Some(raw_scope), Some(raw_prefix), Some(pattern), Some(raw_action)) = (
            self.rule_id.as_deref(),
            self.scope.as_deref(),
            self.prefix.as_deref(),
            self.pattern.as_deref(),
            self.action.as_deref(),
        ) else {
            return self.unreadable(Reason::Column);
        };
        let (Some(raw_content_digest), Some(raw_created_at)) =
            (self.content_digest.as_deref(), self.created_at.as_deref())
        else {
            return self.unreadable(Reason::Column);
        };

        let Ok(scope) = PermissionRuleScope::parse(raw_scope) else {
            return self.unreadable(Reason::Scope);
        };
        let Ok(prefix) = PermissionRulePrefix::parse(raw_prefix) else {
            return self.unreadable(Reason::Prefix);
        };
        let Ok(action) = PermissionRuleAction::parse(raw_action) else {
            return self.unreadable(Reason::Action);
        };
        if validate_scope_shape(scope, self.project_id.as_deref()).is_err() {
            return self.unreadable(Reason::ScopeShape);
        }
        if scope == PermissionRuleScope::Project && action != PermissionRuleAction::Deny {
            return self.unreadable(Reason::ProjectAllow);
        }
        if validate_pattern(pattern).is_err() {
            return self.unreadable(Reason::Pattern);
        }
        if Uuid::parse_str(rule_id).is_err() {
            return self.unreadable(Reason::RuleId);
        }
        let Ok(created_at) = DateTime::parse_from_rfc3339(raw_created_at) else {
            return self.unreadable(Reason::CreatedAt);
        };

        let rule = PermissionRule {
            rule_id: rule_id.to_owned(),
            scope,
            project_id: self.project_id.clone(),
            prefix,
            pattern: pattern.to_owned(),
            action,
            content_digest: raw_content_digest.to_owned(),
            created_at: created_at.with_timezone(&Utc),
        };
        if rule.content_digest != content_digest(&rule) {
            return self.unreadable(Reason::ContentDigest);
        }
        ClassifiedPermissionRule::Readable(rule)
    }
}

/// Read one column as text, treating a null or non-text value as absent.
///
/// SQLite is dynamically typed, so a hand-edited row can hold a blob where the
/// schema declares text. Such a column is unreadable, not a load failure.
fn raw_column_text(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Text(bytes) => Ok(Some(String::from_utf8_lossy(bytes).into_owned())),
        _ => Ok(None),
    }
}

/// Read one column as the text `CAST(column AS TEXT)` would compare against,
/// so a blob-typed value is still addressable by the repair path.
fn raw_addressable_text(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
        }
        _ => Ok(None),
    }
}

fn classify_permission_rule_row(row: &Row<'_>) -> rusqlite::Result<ClassifiedPermissionRule> {
    Ok(RawPermissionRuleRow::read(row)?.classify())
}

/// Keep the rules the product can read; quarantine the rest.
fn readable_rules(rows: Vec<ClassifiedPermissionRule>) -> Vec<PermissionRule> {
    rows.into_iter()
        .filter_map(|row| match row {
            ClassifiedPermissionRule::Readable(rule) => Some(rule),
            ClassifiedPermissionRule::Unreadable(_) => None,
        })
        .collect()
}

/// Delete the rows carrying `rule_id` that are still unreadable. Caller owns the
/// surrounding transaction.
///
/// Matches on `CAST(rule_id AS TEXT)` so a row whose id was stored with the
/// wrong column type is still addressable by the text the recovery list showed,
/// and deletes by `rowid` so a readable row that merely casts to the same text
/// is left alone.
fn delete_unreadable_rows(
    connection: &Connection,
    rule_id: &str,
) -> Result<usize, PermissionRuleError> {
    let candidates = {
        let mut statement = connection
            .prepare(
                "SELECT rule_id, scope, project_id, prefix, pattern, action,
                        content_digest, created_at, rowid
                 FROM permission_rules
                 WHERE CAST(rule_id AS TEXT) = ?1",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([rule_id], |row| {
                Ok((classify_permission_rule_row(row)?, row.get::<_, i64>(8)?))
            })
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?
    };

    let mut removed = 0;
    for (classified, rowid) in candidates {
        if matches!(classified, ClassifiedPermissionRule::Readable(_)) {
            continue;
        }
        removed += connection
            .execute("DELETE FROM permission_rules WHERE rowid = ?1", [rowid])
            .map_err(map_sqlite)?;
    }
    Ok(removed)
}

fn conversion_error(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}

fn map_sqlite(_: rusqlite::Error) -> PermissionRuleError {
    PermissionRuleError::Sqlite
}
