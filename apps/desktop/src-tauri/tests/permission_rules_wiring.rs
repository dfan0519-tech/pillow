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
use keeper_policy::{ExecDecision, ExecProposalState};
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
use permission_rule_commands::{add_permission_rule_impl, delete_permission_rule_impl};

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
    assert_eq!(durable.match_subject, "node --version");

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
