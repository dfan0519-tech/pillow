//! Pure approval policy for Pillow write (C3) and process (C4) proposals.
//!
//! This crate owns only the approval lifecycle vocabulary and transition rules.
//! It has no filesystem, SQLite, renderer, adapter, or OS process dependencies.
//! Write and process state machines use separate enums and error types.

mod permission_rules;
mod process;
mod write;

pub use permission_rules::{
    MatchedPermissionRule, MatchedPermissionRuleSegment, PermissionRuleAction,
    PermissionRuleConjunctionEvaluation, PermissionRuleEvaluation, PermissionRulePrefix,
    PermissionRuleScope, PermissionRuleSpec, canonical_process_subject,
    conservative_shell_command_segments, evaluate_permission_rule_conjunction,
    evaluate_permission_rules, permission_rule_matches,
};

pub use process::{
    EXEC_PROPOSAL_TTL_SECS, ExecDecision, ExecPolicyError, ExecProposalState, apply_exec_decision,
    is_exec_expired, is_exec_terminal, validate_exec_transition,
};
pub use write::{
    PolicyError, WRITE_PROPOSAL_TTL_SECS, WriteDecision, WriteProposalState, apply_write_decision,
    is_expired, is_terminal, validate_write_transition,
};
