//! Host-owned permission-rule CRUD commands.
//!
//! The renderer supplies one requested mutation, never an effective rule set.
//! Project ids are resolved through the registered-project authority before
//! accessing project scope.

use keeper_storage_sqlite::{
    AddPermissionRule, PermissionRule, PermissionRuleAction, PermissionRuleError,
    PermissionRulePrefix, PermissionRuleScope, SqlitePermissionRuleRepository,
};
use serde::Serialize;
use tauri::State;

use crate::app_state::{AppState, CommandError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionRuleView {
    pub rule_id: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub prefix: String,
    pub pattern: String,
    pub action: String,
    pub created_at: String,
}

impl From<PermissionRule> for PermissionRuleView {
    fn from(rule: PermissionRule) -> Self {
        Self {
            rule_id: rule.rule_id,
            scope: rule.scope.as_str().to_owned(),
            project_id: rule.project_id,
            prefix: rule.prefix.as_str().to_owned(),
            pattern: rule.pattern,
            action: rule.action.as_str().to_owned(),
            created_at: rule.created_at.to_rfc3339(),
        }
    }
}

#[tauri::command]
pub fn list_permission_rules(
    rules: State<'_, SqlitePermissionRuleRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
) -> Result<Vec<PermissionRuleView>, CommandError> {
    list_permission_rules_impl(&rules, &app, scope, project_id)
}

#[tauri::command]
pub fn add_permission_rule(
    rules: State<'_, SqlitePermissionRuleRepository>,
    app: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    prefix: String,
    pattern: String,
    action: String,
) -> Result<PermissionRuleView, CommandError> {
    add_permission_rule_impl(&rules, &app, scope, project_id, prefix, pattern, action)
}

#[tauri::command]
pub fn delete_permission_rule(
    rules: State<'_, SqlitePermissionRuleRepository>,
    rule_id: String,
) -> Result<(), CommandError> {
    delete_permission_rule_impl(&rules, rule_id)
}

pub(crate) fn list_permission_rules_impl(
    rules: &SqlitePermissionRuleRepository,
    app: &AppState,
    scope: String,
    project_id: Option<String>,
) -> Result<Vec<PermissionRuleView>, CommandError> {
    let (scope, project_id) = validated_scope(&scope, project_id.as_deref(), app)?;
    rules
        .list(scope, project_id.as_deref())
        .map(|rows| rows.into_iter().map(PermissionRuleView::from).collect())
        .map_err(map_rule_error)
}

pub(crate) fn add_permission_rule_impl(
    rules: &SqlitePermissionRuleRepository,
    app: &AppState,
    scope: String,
    project_id: Option<String>,
    prefix: String,
    pattern: String,
    action: String,
) -> Result<PermissionRuleView, CommandError> {
    let (scope, project_id) = validated_scope(&scope, project_id.as_deref(), app)?;
    let prefix = PermissionRulePrefix::parse(&prefix).map_err(map_rule_error)?;
    let action = PermissionRuleAction::parse(&action).map_err(map_rule_error)?;
    if scope == PermissionRuleScope::Project && action == PermissionRuleAction::Allow {
        return Err(permission_error(
            "permission_rules_project_allow_forbidden",
            "Project scope may only deny; allow rules must be global.",
        ));
    }
    rules
        .add(AddPermissionRule {
            scope,
            project_id,
            prefix,
            pattern,
            action,
        })
        .map(PermissionRuleView::from)
        .map_err(map_rule_error)
}

pub(crate) fn delete_permission_rule_impl(
    rules: &SqlitePermissionRuleRepository,
    rule_id: String,
) -> Result<(), CommandError> {
    match rules.delete(&rule_id).map_err(map_rule_error)? {
        true => Ok(()),
        false => Err(permission_error(
            "permission_rules_not_found",
            "Permission rule not found.",
        )),
    }
}

fn validated_scope(
    scope: &str,
    project_id: Option<&str>,
    app: &AppState,
) -> Result<(PermissionRuleScope, Option<String>), CommandError> {
    let scope = PermissionRuleScope::parse(scope).map_err(map_rule_error)?;
    match scope {
        PermissionRuleScope::Global => Ok((scope, None)),
        PermissionRuleScope::Project => {
            let project_id = project_id
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| {
                    permission_error(
                        "permission_rules_scope_shape",
                        "Project scope requires a project id.",
                    )
                })?;
            let _ = app.get_registered_project(project_id)?;
            Ok((scope, Some(project_id.to_owned())))
        }
    }
}

fn map_rule_error(error: PermissionRuleError) -> CommandError {
    let code = match error {
        PermissionRuleError::ProjectAllowForbidden => "permission_rules_project_allow_forbidden",
        PermissionRuleError::InvalidScope => "permission_rules_invalid_scope",
        PermissionRuleError::InvalidPrefix => "permission_rules_invalid_prefix",
        PermissionRuleError::InvalidAction => "permission_rules_invalid_action",
        PermissionRuleError::ScopeShape => "permission_rules_scope_shape",
        PermissionRuleError::InvalidPattern => "permission_rules_invalid_pattern",
        PermissionRuleError::InvalidRuleId => "permission_rules_invalid_rule_id",
        PermissionRuleError::InvalidStoredRule => "permission_rules_invalid_stored_rule",
        PermissionRuleError::Sqlite | PermissionRuleError::LockPoisoned => {
            "permission_rules_unavailable"
        }
    };
    eprintln!("Pillow permission rule store error ({code}): {error}");
    CommandError {
        code,
        message: error.to_string(),
    }
}

fn permission_error(code: &'static str, message: &str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}
