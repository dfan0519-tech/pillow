//! Host-side permission-rule loading, pure-policy evaluation and view mapping.

use keeper_policy::{
    MatchedPermissionRuleSegment, PermissionRuleAction as PolicyAction,
    PermissionRuleConjunctionEvaluation as PolicyConjunctionEvaluation,
    PermissionRuleEvaluation as PolicyEvaluation, PermissionRulePrefix as PolicyPrefix,
    PermissionRuleScope as PolicyScope, PermissionRuleSpec, evaluate_permission_rule_conjunction,
    evaluate_permission_rules,
};
use keeper_storage_sqlite::{
    PermissionRule, PermissionRuleAction, PermissionRuleMatchSnapshot, PermissionRulePrefix,
    PermissionRuleScope, PermissionRuleSegmentMatchSnapshot, SqlitePermissionRuleRepository,
    StoredPermissionRuleReceipt,
};

/// Rule attribution for host-side views.
///
/// Aliased to the protocol type rather than redeclared: the same three fields
/// also travel on the decided/resolved event payloads, and this repository has
/// already been burned by one invariant living in several places.
pub type PermissionRuleAttributionView = keeper_agent_protocol::PermissionRuleAttribution;

pub(crate) enum HostPermissionRuleEvaluation {
    Deny(PermissionRuleMatchSnapshot),
    Allow(PermissionRuleMatchSnapshot),
    RequireConfirmation,
}

pub(crate) enum HostPermissionRuleConjunctionEvaluation {
    Deny(PermissionRuleSegmentMatchSnapshot),
    Allow(Vec<PermissionRuleSegmentMatchSnapshot>),
    RequireConfirmation,
}

pub(crate) fn evaluate_host_permission_rules(
    repository: Option<&SqlitePermissionRuleRepository>,
    project_id: &str,
    prefix: PermissionRulePrefix,
    subject: &str,
) -> Result<HostPermissionRuleEvaluation, ()> {
    let Some(repository) = repository else {
        return Ok(HostPermissionRuleEvaluation::RequireConfirmation);
    };
    let stored = repository
        .list_effective(Some(project_id))
        .map_err(|_| ())?;
    let policy_prefix = policy_prefix(prefix);
    let policy_rules = stored.iter().map(policy_rule).collect::<Vec<_>>();

    match evaluate_permission_rules(policy_prefix, subject, &policy_rules) {
        PolicyEvaluation::RequireConfirmation => {
            Ok(HostPermissionRuleEvaluation::RequireConfirmation)
        }
        PolicyEvaluation::Allow(matched) => find_match(&stored, &matched.rule.rule_id, subject)
            .map(HostPermissionRuleEvaluation::Allow),
        PolicyEvaluation::Deny(matched) => find_match(&stored, &matched.rule.rule_id, subject)
            .map(HostPermissionRuleEvaluation::Deny),
    }
}

pub(crate) fn evaluate_host_permission_rule_conjunction(
    repository: Option<&SqlitePermissionRuleRepository>,
    project_id: &str,
    prefix: PermissionRulePrefix,
    subjects: &[String],
) -> Result<HostPermissionRuleConjunctionEvaluation, ()> {
    let Some(repository) = repository else {
        return Ok(HostPermissionRuleConjunctionEvaluation::RequireConfirmation);
    };
    let stored = repository
        .list_effective(Some(project_id))
        .map_err(|_| ())?;
    let policy_prefix = policy_prefix(prefix);
    let policy_rules = stored.iter().map(policy_rule).collect::<Vec<_>>();

    match evaluate_permission_rule_conjunction(policy_prefix, subjects, &policy_rules) {
        PolicyConjunctionEvaluation::RequireConfirmation => {
            Ok(HostPermissionRuleConjunctionEvaluation::RequireConfirmation)
        }
        PolicyConjunctionEvaluation::Allow(matched) => matched
            .into_iter()
            .map(|matched| find_segment_match(&stored, matched))
            .collect::<Result<Vec<_>, _>>()
            .map(HostPermissionRuleConjunctionEvaluation::Allow),
        PolicyConjunctionEvaluation::Deny(matched) => {
            find_segment_match(&stored, matched).map(HostPermissionRuleConjunctionEvaluation::Deny)
        }
    }
}

pub(crate) fn attribution_from_receipt(
    receipt: &StoredPermissionRuleReceipt,
) -> PermissionRuleAttributionView {
    PermissionRuleAttributionView {
        prefix: receipt.rule_prefix.as_str().to_owned(),
        pattern: receipt.rule_pattern.clone(),
        action: receipt.rule_action.as_str().to_owned(),
    }
}

fn find_match(
    rules: &[PermissionRule],
    rule_id: &str,
    subject: &str,
) -> Result<PermissionRuleMatchSnapshot, ()> {
    let rule = rules
        .iter()
        .find(|rule| rule.rule_id == rule_id)
        .cloned()
        .ok_or(())?;
    Ok(PermissionRuleMatchSnapshot {
        rule,
        match_subject: subject.to_owned(),
    })
}

fn find_segment_match(
    rules: &[PermissionRule],
    matched: MatchedPermissionRuleSegment,
) -> Result<PermissionRuleSegmentMatchSnapshot, ()> {
    Ok(PermissionRuleSegmentMatchSnapshot {
        segment_index: u32::try_from(matched.segment_index).map_err(|_| ())?,
        matched: find_match(
            rules,
            &matched.matched.rule.rule_id,
            &matched.matched.match_subject,
        )?,
    })
}

fn policy_rule(rule: &PermissionRule) -> PermissionRuleSpec {
    PermissionRuleSpec {
        rule_id: rule.rule_id.clone(),
        scope: match rule.scope {
            PermissionRuleScope::Global => PolicyScope::Global,
            PermissionRuleScope::Project => PolicyScope::Project,
        },
        project_id: rule.project_id.clone(),
        prefix: policy_prefix(rule.prefix),
        pattern: rule.pattern.clone(),
        action: match rule.action {
            PermissionRuleAction::Allow => PolicyAction::Allow,
            PermissionRuleAction::Deny => PolicyAction::Deny,
        },
        content_digest: rule.content_digest.clone(),
    }
}

fn policy_prefix(prefix: PermissionRulePrefix) -> PolicyPrefix {
    match prefix {
        PermissionRulePrefix::Process => PolicyPrefix::Process,
        PermissionRulePrefix::Write => PolicyPrefix::Write,
        PermissionRulePrefix::Edit => PolicyPrefix::Edit,
    }
}
