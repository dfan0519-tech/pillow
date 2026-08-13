//! Cross-component permission-rule wiring tests.
//!
//! This is deliberately an integration test (under `tests/`), not a unit test:
//! it opens the desktop command-side repository and the runtime host as two
//! independent SQLite connections in one process.

use std::fs;
use std::sync::Arc;

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId as ProtocolRunId, RuntimeError, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};
use keeper_policy::{ExecDecision, ExecProposalState, canonical_process_subject};
use keeper_project::CreateProject;
use keeper_runtime_host::{
    GovernedProcessLifecycle, HostConfig, ProviderProcessProposal, RuntimeHost,
    execute_governed_process,
};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    PermissionRuleAction, PermissionRulePrefix, PermissionRuleReceiptActor,
    PermissionRuleReceiptDecision, PermissionRuleScope, SqliteConversationRepository,
    SqliteExecProposalRepository, SqlitePermissionRuleRepository, SqliteProjectRepository,
};
use serde_json::json;

// Compile the production command module into this integration-test crate so
// the test exercises the exact command implementation without widening the
// desktop library's public API solely for tests.
mod app_state {
    pub use pillow_desktop::app_state::{AppState, CommandError};
}

#[allow(dead_code)]
#[path = "../src/permission_rule_commands.rs"]
mod permission_rule_commands;

use app_state::AppState;
use permission_rule_commands::{
    add_permission_rule_impl, delete_permission_rule_impl, delete_unreadable_permission_rule_impl,
    list_permission_rules_impl, list_unreadable_permission_rules_impl,
};

struct IdleAdapter;

impl AgentRuntimeAdapter for IdleAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "idle".into(),
            version: "1".into(),
            detail: None,
        })
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        Ok(RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn disarm_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn send_turn(&self, _: SendTurn, _: &mut dyn EventSink) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn cancel_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

/// Plant the row EA-03 sequence B describes: a global process allow whose
/// pattern is blank. The digest is computed over the row's own content, so the
/// only thing wrong with it is the pattern — which kernel policy reads as
/// match-all, i.e. auto-approve every command.
fn plant_match_all_row(db: &std::path::Path, rule_id: &str) {
    let body = json!({
        "schema": "permission-rule-v1",
        "scope": "global",
        "project_id": Option::<&str>::None,
        "prefix": "process",
        "action": "allow",
        "pattern": "",
    });
    let digest = keeper_agent_protocol::canonical_arguments_digest(&body)
        .expect("digest body fits the canonical bound")
        .as_str()
        .to_owned();
    rusqlite::Connection::open(db)
        .unwrap()
        .execute(
            "INSERT INTO permission_rules (
                rule_id, scope, project_id, prefix, pattern, action,
                content_digest, created_at
             ) VALUES (?1, 'global', NULL, 'process', '', 'allow', ?2,
                       '2026-08-12T10:00:00Z')",
            rusqlite::params![rule_id, digest],
        )
        .expect("a caller that bypassed the gateway could write this row");
}

/// A quarantined row must neither grant authority nor take the rule path down.
///
/// Both halves matter. If the blank pattern loaded, it would auto-approve every
/// command. If one unreadable row failed the whole effective load — the previous
/// behaviour — `evaluate_host_permission_rule_conjunction` maps that failure to
/// a gateway error, so every governed process proposal would fail before the
/// user could decide anything, with no in-product way to delete the row.
#[test]
fn a_quarantined_row_neither_grants_authority_nor_breaks_rule_evaluation() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();

    let app = AppState::open(&db).unwrap();
    let project = SqliteProjectRepository::open(&db)
        .unwrap()
        .create(CreateProject {
            name: "Quarantine wiring".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Quarantine wiring".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "run node version".into(),
        })
        .unwrap();

    let planted = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    plant_match_all_row(&db, planted);

    let host =
        RuntimeHost::new_with_governed_tools(Arc::new(IdleAdapter), HostConfig::default(), &db)
            .unwrap();
    host.register_governed_process_run(
        ProtocolRunId(run.id.0),
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    let command_rules = SqlitePermissionRuleRepository::open(&db).unwrap();
    let added = add_permission_rule_impl(
        &command_rules,
        &app,
        "global".into(),
        None,
        "process".into(),
        "node --version".into(),
        "allow".into(),
    )
    .unwrap();

    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let mut lifecycle = Vec::new();
    let receipt = execute_governed_process(
        ProtocolRunId(run.id.0),
        ProviderProcessProposal {
            call_id: "quarantined-store".into(),
            tool_id: "process.run".into(),
            tool_version: 1,
            arguments: json!({
                "program": "node",
                "argv": ["--version"],
                "cwd": "."
            }),
        },
        &cancelled,
        |event| {
            lifecycle.push(match event {
                GovernedProcessLifecycle::Requested(_) => "requested",
                GovernedProcessLifecycle::Decided(payload) => {
                    assert_eq!(payload.decision, ExecDecision::ApproveOnce);
                    "decided"
                }
                GovernedProcessLifecycle::Started(_) => "started",
            });
            Ok(())
        },
    )
    .expect("an unreadable row must not fail the governed process path");

    assert_eq!(receipt.state, ExecProposalState::Succeeded);
    assert_eq!(lifecycle, ["requested", "decided", "started"]);

    // Attribution names the rule the user actually wrote, not the planted row.
    let durable = SqliteExecProposalRepository::open(&db)
        .unwrap()
        .get_rule_receipt(&receipt.proposal_id)
        .unwrap()
        .expect("auto-approval retains a durable rule receipt");
    assert_eq!(durable.rule_id, added.rule_id);
    assert_eq!(durable.rule_pattern, "node --version");
    assert_ne!(durable.rule_id, planted);

    // The Settings surface stays usable: the readable rule renders, and the
    // planted row is reported where the user can delete it.
    assert_eq!(
        list_permission_rules_impl(&command_rules, &app, "global".into(), None).unwrap(),
        vec![added.clone()]
    );
    let unreadable = list_unreadable_permission_rules_impl(&command_rules).unwrap();
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].rule_id, planted);
    assert_eq!(unreadable[0].reason, "pattern");

    delete_unreadable_permission_rule_impl(&command_rules, planted.into()).unwrap();
    assert!(
        list_unreadable_permission_rules_impl(&command_rules)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        list_permission_rules_impl(&command_rules, &app, "global".into(), None).unwrap(),
        vec![added]
    );

    host.shutdown().unwrap();
}

#[test]
fn command_rule_added_after_host_open_auto_decides_next_matching_proposal() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();

    let app = AppState::open(&db).unwrap();
    let project = SqliteProjectRepository::open(&db)
        .unwrap()
        .create(CreateProject {
            name: "Permission wiring".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Permission wiring".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "run node version".into(),
        })
        .unwrap();

    // Important ordering for T1: the host opens and retains its rule-store
    // connection before the command-side connection writes the new rule.
    let host =
        RuntimeHost::new_with_governed_tools(Arc::new(IdleAdapter), HostConfig::default(), &db)
            .unwrap();
    host.register_governed_process_run(
        ProtocolRunId(run.id.0),
        project.id.0.to_string(),
        conversation.id.0.to_string(),
        &root,
    )
    .unwrap();

    let command_rules = SqlitePermissionRuleRepository::open(&db).unwrap();
    let added = add_permission_rule_impl(
        &command_rules,
        &app,
        "global".into(),
        Some(project.id.0.to_string()),
        "process".into(),
        "node --version".into(),
        "allow".into(),
    )
    .unwrap();
    assert_eq!(added.scope, "global");
    assert_eq!(added.pattern, "node --version");
    assert_eq!(added.action, "allow");

    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let mut lifecycle = Vec::new();
    let mut decided_attribution = None;
    let receipt = execute_governed_process(
        ProtocolRunId(run.id.0),
        ProviderProcessProposal {
            call_id: "hot-rule-visible".into(),
            tool_id: "process.run".into(),
            tool_version: 1,
            arguments: json!({
                "program": "node",
                "argv": ["--version"],
                "cwd": "."
            }),
        },
        &cancelled,
        |event| {
            lifecycle.push(match event {
                GovernedProcessLifecycle::Requested(_) => "requested",
                GovernedProcessLifecycle::Decided(payload) => {
                    assert_eq!(payload.decision, ExecDecision::ApproveOnce);
                    assert_eq!(payload.state, ExecProposalState::Approved);
                    decided_attribution = payload.auto_approved_by_rule.clone();
                    "decided"
                }
                GovernedProcessLifecycle::Started(_) => "started",
            });
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(receipt.state, ExecProposalState::Succeeded);
    assert_eq!(lifecycle, ["requested", "decided", "started"]);
    let event_attribution = decided_attribution.expect("decided event rule attribution");
    assert_eq!(event_attribution.prefix, "process");
    assert_eq!(event_attribution.pattern, "node --version");
    assert_eq!(event_attribution.action, "allow");

    let receipts = SqliteExecProposalRepository::open(&db).unwrap();
    let durable = receipts
        .get_rule_receipt(&receipt.proposal_id)
        .unwrap()
        .expect("auto-approval must retain a durable rule receipt");
    assert_eq!(
        durable.decision,
        PermissionRuleReceiptDecision::ApproveByRule
    );
    assert_eq!(durable.actor, PermissionRuleReceiptActor::RuleEngine);
    assert_eq!(durable.rule_id, added.rule_id);
    assert_eq!(durable.rule_scope, PermissionRuleScope::Global);
    assert_eq!(durable.rule_project_id, None);
    assert_eq!(durable.rule_prefix, PermissionRulePrefix::Process);
    assert_eq!(durable.rule_pattern, "node --version");
    assert_eq!(durable.rule_action, PermissionRuleAction::Allow);
    assert!(durable.rule_content_digest.starts_with("sha256:"));
    assert_eq!(durable.rule_content_digest.len(), 71);
    // The receipt records the canonical v1 subject, not a space-joined string.
    // That is the point of EA-02: a flat "node --version" cannot say whether
    // the agent ran one argument or two, so a receipt carrying it cannot be
    // audited after the fact. The rule *pattern* stays human-written
    // (`node --version` above still matched) — only the recorded subject is
    // canonical.
    assert_eq!(
        durable.match_subject,
        canonical_process_subject("node", &["--version".to_owned()])
    );

    let view = host
        .list_process_proposals(&conversation.id.0.to_string())
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.proposal_id == receipt.proposal_id)
        .expect("auto-decided proposal remains visible in the host timeline view");
    assert!(
        !view.decision_available,
        "auto-approval must not wait for UI"
    );
    assert_eq!(
        view.auto_approved_by_rule
            .expect("matching command-added rule attribution")
            .pattern,
        "node --version"
    );

    // The receipt is a decision-time snapshot, not a join to mutable live rule
    // content. Removing the rule must not erase the historical answer.
    delete_permission_rule_impl(&command_rules, added.rule_id).unwrap();
    assert_eq!(
        receipts
            .get_rule_receipt(&receipt.proposal_id)
            .unwrap()
            .expect("receipt survives live-rule deletion"),
        durable
    );

    host.shutdown().unwrap();
}
