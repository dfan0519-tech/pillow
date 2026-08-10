//! SQLite-only permission-rule authority for the minimal rule slice.
//!
//! The repository accepts typed user-originated CRUD inputs and never accepts
//! a project root or an externally resolved effective policy. Global rules may
//! allow or deny; project rules are deny-only and effective loading filters
//! that invariant again so corrupt rows cannot widen authority.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Utc};
use keeper_agent_protocol::canonical_arguments_digest;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use thiserror::Error;
use uuid::Uuid;

pub const PERMISSION_RULE_PATTERN_BYTE_CAP: usize = 4096;
pub const PERMISSION_RULE_SUBJECT_BYTE_CAP: usize = 8192;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleMatchSnapshot {
    pub rule: PermissionRule,
    pub match_subject: String,
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
        validate_pattern(&input.pattern)?;

        let rule = PermissionRule {
            rule_id: Uuid::new_v4().to_string(),
            scope: input.scope,
            project_id: input.project_id,
            prefix: input.prefix,
            pattern: input.pattern,
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
                .query_map([project_id], permission_rule_from_row)
                .map_err(map_sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlite)?
        } else {
            statement
                .query_map([], permission_rule_from_row)
                .map_err(map_sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlite)?
        };
        validate_loaded_rules(rows)
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
            .query_map([project_id], permission_rule_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;
        validate_loaded_rules(rows)
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

fn validate_pattern(pattern: &str) -> Result<(), PermissionRuleError> {
    if pattern.len() > PERMISSION_RULE_PATTERN_BYTE_CAP
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

fn permission_rule_from_row(row: &Row<'_>) -> rusqlite::Result<PermissionRule> {
    let scope: String = row.get(1)?;
    let prefix: String = row.get(3)?;
    let action: String = row.get(5)?;
    let created_at: String = row.get(7)?;
    Ok(PermissionRule {
        rule_id: row.get(0)?,
        scope: PermissionRuleScope::parse(&scope).map_err(|error| conversion_error(1, error))?,
        project_id: row.get(2)?,
        prefix: PermissionRulePrefix::parse(&prefix).map_err(|error| conversion_error(3, error))?,
        pattern: row.get(4)?,
        action: PermissionRuleAction::parse(&action).map_err(|error| conversion_error(5, error))?,
        content_digest: row.get(6)?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map(|time| time.with_timezone(&Utc))
            .map_err(|error| conversion_error(7, error))?,
    })
}

fn validate_loaded_rules(
    rules: Vec<PermissionRule>,
) -> Result<Vec<PermissionRule>, PermissionRuleError> {
    for rule in &rules {
        validate_scope_shape(rule.scope, rule.project_id.as_deref())?;
        if rule.scope == PermissionRuleScope::Project && rule.action != PermissionRuleAction::Deny {
            return Err(PermissionRuleError::InvalidStoredRule);
        }
        validate_pattern(&rule.pattern)?;
        if Uuid::parse_str(&rule.rule_id).is_err() || rule.content_digest != content_digest(rule) {
            return Err(PermissionRuleError::InvalidStoredRule);
        }
    }
    Ok(rules)
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
