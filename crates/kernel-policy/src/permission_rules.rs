//! Pure permission-rule matching and precedence.
//!
//! Pillow pins a small deterministic wildcard language rather than claiming
//! bit-for-bit Grok compatibility. Matching is case-sensitive on every host;
//! only `*` and `**` are operators. Process rules use literal-prefix OR
//! whole-subject wildcard matching. Write/edit rules use whole portable-path
//! wildcard matching only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "global" => Some(Self::Global),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "process" => Some(Self::Process),
            "write" => Some(Self::Write),
            "edit" => Some(Self::Edit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// Complete immutable rule body needed for matching and receipt attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleSpec {
    pub rule_id: String,
    pub scope: PermissionRuleScope,
    pub project_id: Option<String>,
    pub prefix: PermissionRulePrefix,
    pub pattern: String,
    pub action: PermissionRuleAction,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPermissionRule {
    pub rule: PermissionRuleSpec,
    pub match_subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRuleEvaluation {
    Deny(MatchedPermissionRule),
    Allow(MatchedPermissionRule),
    RequireConfirmation,
}

/// Canonical process subject: catalog label followed by literal argv tokens,
/// each separated by one ASCII space. No shell quoting or parsing occurs.
pub fn canonical_process_subject(program: &str, argv: &[String]) -> String {
    let suffix_bytes = argv.iter().map(|arg| arg.len() + 1).sum::<usize>();
    let mut subject = String::with_capacity(program.len() + suffix_bytes);
    subject.push_str(program);
    for argument in argv {
        subject.push(' ');
        subject.push_str(argument);
    }
    subject
}

/// Match one normalized subject against a pattern for its rule class.
pub fn permission_rule_matches(prefix: PermissionRulePrefix, pattern: &str, subject: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }

    match prefix {
        PermissionRulePrefix::Process => {
            subject.starts_with(pattern) || wildcard_match(pattern, subject, WildcardMode::Process)
        }
        PermissionRulePrefix::Write | PermissionRulePrefix::Edit => {
            wildcard_match(pattern, subject, WildcardMode::PortablePath)
        }
    }
}

/// Evaluate deny before allow. Input order breaks ties within the same action.
/// A project-scoped allow is ignored even if a corrupt caller supplies one.
pub fn evaluate_permission_rules(
    prefix: PermissionRulePrefix,
    subject: &str,
    rules: &[PermissionRuleSpec],
) -> PermissionRuleEvaluation {
    let mut first_deny = None;
    let mut first_allow = None;

    for rule in rules {
        if rule.prefix != prefix
            || (rule.scope == PermissionRuleScope::Project
                && rule.action == PermissionRuleAction::Allow)
            || !permission_rule_matches(prefix, &rule.pattern, subject)
        {
            continue;
        }

        let matched = MatchedPermissionRule {
            rule: rule.clone(),
            match_subject: subject.to_owned(),
        };
        match rule.action {
            PermissionRuleAction::Deny if first_deny.is_none() => first_deny = Some(matched),
            PermissionRuleAction::Allow if first_allow.is_none() => first_allow = Some(matched),
            PermissionRuleAction::Deny | PermissionRuleAction::Allow => {}
        }
    }

    if let Some(matched) = first_deny {
        PermissionRuleEvaluation::Deny(matched)
    } else if let Some(matched) = first_allow {
        PermissionRuleEvaluation::Allow(matched)
    } else {
        PermissionRuleEvaluation::RequireConfirmation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WildcardMode {
    Process,
    PortablePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Literal(char),
    AnySequence {
        crosses_slash: bool,
    },
    /// Path-only `**/`: zero or more complete directory segments.
    AnyDirectories,
}

fn wildcard_match(pattern: &str, subject: &str, mode: WildcardMode) -> bool {
    let tokens = tokenize(pattern, mode);
    let subject = subject.chars().collect::<Vec<_>>();
    let mut current = vec![false; subject.len() + 1];
    current[0] = true;

    for token in tokens {
        let mut next = vec![false; subject.len() + 1];
        match token {
            Token::Literal(expected) => {
                for index in 0..subject.len() {
                    if current[index] && subject[index] == expected {
                        next[index + 1] = true;
                    }
                }
            }
            Token::AnySequence { crosses_slash } => {
                next[0] = current[0];
                for end in 1..=subject.len() {
                    let can_extend = next[end - 1] && (crosses_slash || subject[end - 1] != '/');
                    next[end] = current[end] || can_extend;
                }
            }
            Token::AnyDirectories => {
                next.clone_from_slice(&current);
                let mut has_start = false;
                for end in 1..=subject.len() {
                    has_start |= current[end - 1];
                    if has_start && subject[end - 1] == '/' {
                        next[end] = true;
                    }
                }
            }
        }
        current = next;
    }

    current[subject.len()]
}

fn tokenize(pattern: &str, mode: WildcardMode) -> Vec<Token> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut tokens = Vec::with_capacity(chars.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '*' {
            tokens.push(Token::Literal(chars[index]));
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && chars[index] == '*' {
            index += 1;
        }
        let star_count = index - start;

        match mode {
            WildcardMode::Process => tokens.push(Token::AnySequence {
                crosses_slash: true,
            }),
            WildcardMode::PortablePath if star_count >= 2 && chars.get(index) == Some(&'/') => {
                tokens.push(Token::AnyDirectories);
                index += 1;
            }
            WildcardMode::PortablePath => tokens.push(Token::AnySequence {
                crosses_slash: star_count >= 2,
            }),
        }
    }

    tokens
}
