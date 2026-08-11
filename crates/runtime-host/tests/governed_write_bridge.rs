//! Focused runtime tests for the governed write coordinator.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(feature = "test-support")]
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId as ProtocolRunId, RuntimeError, RuntimeEvent,
    RuntimeEventKind, RuntimeProbe, RuntimeSession, SendTurn, StartSession, WriteProposalState,
    WritePublicErrorCode,
};
use keeper_governance_port::testkit::{FixedCancellationProbe, RecordingWriteObserver};
use keeper_governance_port::{
    CheckedGovernedOutcome, GovernedExecutionPort, GovernedWriteExecutionError,
    LifecycleObserverError,
};
use keeper_policy::WriteDecision;
use keeper_project::CreateProject;
use keeper_runtime_host::{
    BoundedDelivery, BoundedDeliverySink, GlobalGovernedExecutionPort, GovernedWriteBridgeError,
    GovernedWriteDelivery, HostConfig, ProtocolGuardSink, ProviderWriteProposal, RuntimeHost,
    execute_governed_write, is_canonical_grant, recover_write_proposals_with_roots,
};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    AddPermissionRule, PermissionMode, PermissionRuleAction, PermissionRulePrefix,
    PermissionRuleScope, SettingsUpdatedBy, SqliteConversationRepository,
    SqlitePermissionRuleRepository, SqliteProjectRepository, SqliteSettingsRepository,
    SqliteWriteProposalRepository, WriteApplyReconcileObservation,
};
use keeper_tool_executor::{GovernedWriteToolGateway, ProjectCapability, content_sha256_digest};
use serde_json::json;

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

struct Fixture {
    _temp: tempfile::TempDir,
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    project_id: String,
    conversation_id: String,
    run_id: ProtocolRunId,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "Project".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Governed write".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "write note".into(),
        })
        .unwrap();
    Fixture {
        _temp: temp,
        db,
        root,
        project_id: project.id.0.to_string(),
        conversation_id: conversation.id.0.to_string(),
        run_id: ProtocolRunId(run.id.0),
    }
}

fn provider_create(call_id: &str, path: &str, content: &str) -> ProviderWriteProposal {
    ProviderWriteProposal {
        call_id: call_id.into(),
        tool_id: "fs.write".into(),
        tool_version: 1,
        arguments: json!({
            "path": path,
            "mode": "create",
            "content": content
        }),
    }
}

fn provider_modify(call_id: &str, path: &str, content: &str) -> ProviderWriteProposal {
    ProviderWriteProposal {
        call_id: call_id.into(),
        tool_id: "fs.write".into(),
        tool_version: 1,
        arguments: json!({
            "path": path,
            "mode": "modify",
            "content": content
        }),
    }
}

fn host_for(fx: &Fixture) -> RuntimeHost {
    let host =
        RuntimeHost::new_with_governed_tools(Arc::new(IdleAdapter), HostConfig::default(), &fx.db)
            .unwrap();
    let root = fx.root.clone();
    let _ = host.recover_write_proposals(chrono::Utc::now(), move |_| Some(root.clone()));
    host
}

fn wait_live_pending(host: &RuntimeHost, conversation_id: &str) -> (String, String) {
    for _ in 0..400 {
        if let Ok(list) = host.list_write_proposals(conversation_id)
            && let Some(p) = list.into_iter().find(|p| {
                p.state == WriteProposalState::PendingApproval && p.approval_grant.is_some()
            })
        {
            return (p.proposal_id, p.approval_grant.unwrap());
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for live pending proposal with grant");
}

fn register(host: &RuntimeHost, fx: &Fixture) {
    host.register_governed_write_run(fx.run_id, &fx.project_id, &fx.conversation_id, &fx.root)
        .unwrap();
}

fn add_rule(
    fx: &Fixture,
    scope: PermissionRuleScope,
    prefix: PermissionRulePrefix,
    pattern: &str,
    action: PermissionRuleAction,
) -> keeper_storage_sqlite::PermissionRule {
    SqlitePermissionRuleRepository::open(&fx.db)
        .unwrap()
        .add(AddPermissionRule {
            scope,
            project_id: (scope == PermissionRuleScope::Project).then(|| fx.project_id.clone()),
            prefix,
            pattern: pattern.to_owned(),
            action,
        })
        .unwrap()
}

fn set_permission_mode(fx: &Fixture, mode: PermissionMode) {
    SqliteSettingsRepository::open(&fx.db)
        .unwrap()
        .upsert_permission_mode(mode, SettingsUpdatedBy::User)
        .unwrap();
}

#[test]
fn accept_edits_and_bypass_auto_apply_writes_with_durable_mode_receipts() {
    for (index, mode) in [PermissionMode::AcceptEdits, PermissionMode::Bypass]
        .into_iter()
        .enumerate()
    {
        let fx = fixture();
        fs::create_dir_all(fx.root.join("src")).unwrap();
        set_permission_mode(&fx, mode);
        let host = host_for(&fx);
        register(&host, &fx);
        let cancel = AtomicBool::new(false);
        let mut requested_count = 0;

        let receipt = execute_governed_write(
            fx.run_id,
            provider_create(
                &format!("mode-write-{index}"),
                "src/mode.rs",
                "pub const MODE: bool = true;\n",
            ),
            &cancel,
            |requested| {
                requested_count += 1;
                let live = host
                    .list_write_proposals(&fx.conversation_id)
                    .unwrap()
                    .into_iter()
                    .find(|proposal| proposal.proposal_id == requested.proposal_id)
                    .expect("automatic write remains observable while its decision is pending");
                assert!(
                    live.approval_grant.is_none(),
                    "mode-owned decision must never expose a clickable human grant"
                );
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(receipt.resolved.state, WriteProposalState::Succeeded);
        assert_eq!(requested_count, 1, "proposal lifecycle must not be skipped");
        assert_eq!(
            fs::read_to_string(fx.root.join("src/mode.rs")).unwrap(),
            "pub const MODE: bool = true;\n"
        );
        let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
        let mode_receipt = store
            .get_permission_mode_receipt(&receipt.resolved.proposal_id)
            .unwrap()
            .expect("mode approval must have a durable receipt");
        assert_eq!(mode_receipt.permission_mode, mode);
        assert!(
            store
                .get_rule_receipt(&receipt.resolved.proposal_id)
                .unwrap()
                .is_none(),
            "mode approval must not masquerade as a rule approval"
        );
        host.shutdown().unwrap();
    }
}

#[test]
fn bypass_write_still_obeys_project_deny_and_never_creates_mode_receipt() {
    let fx = fixture();
    fs::create_dir_all(fx.root.join("blocked")).unwrap();
    set_permission_mode(&fx, PermissionMode::Bypass);
    let deny = add_rule(
        &fx,
        PermissionRuleScope::Project,
        PermissionRulePrefix::Write,
        "blocked/**",
        PermissionRuleAction::Deny,
    );
    let host = host_for(&fx);
    register(&host, &fx);
    let cancel = AtomicBool::new(false);

    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("bypass-denied-write", "blocked/no.txt", "must not exist"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(receipt.resolved.state, WriteProposalState::Rejected);
    assert!(!fx.root.join("blocked/no.txt").exists());
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    assert!(
        store
            .get_permission_mode_receipt(&receipt.resolved.proposal_id)
            .unwrap()
            .is_none()
    );
    let rule_receipt = store
        .get_rule_receipt(&receipt.resolved.proposal_id)
        .unwrap()
        .expect("deny must retain rule attribution");
    assert_eq!(rule_receipt.rule_id, deny.rule_id);
    assert_eq!(rule_receipt.rule_action, PermissionRuleAction::Deny);
    host.shutdown().unwrap();
}

#[test]
fn global_allow_auto_applies_write_and_exposes_rule_subject_and_attribution() {
    let fx = fixture();
    fs::create_dir_all(fx.root.join("src")).unwrap();
    let rule = add_rule(
        &fx,
        PermissionRuleScope::Global,
        PermissionRulePrefix::Write,
        "src/**",
        PermissionRuleAction::Allow,
    );
    let host = host_for(&fx);
    register(&host, &fx);
    let cancel = AtomicBool::new(false);

    let receipt = execute_governed_write(
        fx.run_id,
        provider_create(
            "auto-write",
            "src/auto.rs",
            "pub const AUTO: bool = true;\n",
        ),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(receipt.resolved.state, WriteProposalState::Succeeded);
    assert_eq!(
        fs::read_to_string(fx.root.join("src/auto.rs")).unwrap(),
        "pub const AUTO: bool = true;\n"
    );
    let durable = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let stored = durable
        .get_rule_receipt(&receipt.resolved.proposal_id)
        .unwrap()
        .expect("rule-attributed write receipt");
    assert_eq!(stored.rule_id, rule.rule_id);
    assert_eq!(stored.rule_pattern, "src/**");
    assert_eq!(stored.match_subject, "src/auto.rs");

    let view = host
        .list_write_proposals(&fx.conversation_id)
        .unwrap()
        .into_iter()
        .find(|view| view.proposal_id == receipt.resolved.proposal_id)
        .unwrap();
    assert_eq!(view.rule_subject, "src/auto.rs");
    assert!(view.approval_grant.is_none());
    let attribution = view.auto_approved_by_rule.expect("rule attribution view");
    assert_eq!(attribution.prefix, "write");
    assert_eq!(attribution.pattern, "src/**");
    assert_eq!(attribution.action, "allow");
    host.shutdown().unwrap();
}

#[test]
fn matching_project_deny_beats_global_allow_before_any_write() {
    let fx = fixture();
    fs::create_dir_all(fx.root.join("blocked")).unwrap();
    add_rule(
        &fx,
        PermissionRuleScope::Global,
        PermissionRulePrefix::Write,
        "**",
        PermissionRuleAction::Allow,
    );
    let deny = add_rule(
        &fx,
        PermissionRuleScope::Project,
        PermissionRulePrefix::Write,
        "blocked/**",
        PermissionRuleAction::Deny,
    );
    let host = host_for(&fx);
    register(&host, &fx);
    let cancel = AtomicBool::new(false);

    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("auto-deny", "blocked/no.txt", "must not exist"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(receipt.resolved.state, WriteProposalState::Rejected);
    assert!(!fx.root.join("blocked/no.txt").exists());
    let stored = SqliteWriteProposalRepository::open(&fx.db)
        .unwrap()
        .get_rule_receipt(&receipt.resolved.proposal_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.rule_id, deny.rule_id);
    assert_eq!(stored.rule_action, PermissionRuleAction::Deny);
    host.shutdown().unwrap();
}

#[test]
fn durable_pending_before_requested_callback() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);

    let events = Arc::new(AtomicUsize::new(0));
    let store_seen = Arc::new(AtomicBool::new(false));
    let events_cb = Arc::clone(&events);
    let store_seen_cb = Arc::clone(&store_seen);
    let db = fx.db.clone();
    let conversation_id = fx.conversation_id.clone();
    let cancel = AtomicBool::new(false);
    let after_callback = Arc::new(Barrier::new(2));

    let host2 = Arc::clone(&host);
    let after_callback_decide = Arc::clone(&after_callback);
    let decide_thread = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        assert!(is_canonical_grant(&grant));
        after_callback_decide.wait();
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });

    let after_callback_cb = Arc::clone(&after_callback);
    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("call-dur", "note.txt", "hello\n"),
        &cancel,
        |requested| {
            let store = SqliteWriteProposalRepository::open(&db).unwrap();
            let row = store.get(&requested.proposal_id).unwrap().unwrap();
            assert_eq!(row.state, WriteProposalState::PendingApproval);
            store_seen_cb.store(true, Ordering::SeqCst);
            events_cb.fetch_add(1, Ordering::SeqCst);
            assert!(!format!("{requested:?}").contains("hello"));
            after_callback_cb.wait();
            Ok(())
        },
    )
    .unwrap();

    decide_thread.join().unwrap();
    assert!(store_seen.load(Ordering::SeqCst));
    assert_eq!(events.load(Ordering::SeqCst), 1);
    assert_eq!(receipt.resolved.state, WriteProposalState::Rejected);
    assert_eq!(
        receipt.provider_error,
        Some(WritePublicErrorCode::ApprovalRejected)
    );
    assert!(!fx.root.join("note.txt").exists());
    host.shutdown().unwrap();
}

#[test]
fn approve_applies_once_and_replay_skips_second_write() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);

    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::ApproveOnce,
            )
            .unwrap();
    });

    fs::create_dir_all(fx.root.join("src")).unwrap();
    let first = execute_governed_write(
        fx.run_id,
        provider_create("call-ok", "src/a.ts", "export const n = 1;\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    decide.join().unwrap();
    assert_eq!(first.delivery, GovernedWriteDelivery::Executed);
    assert_eq!(first.resolved.state, WriteProposalState::Succeeded);
    assert_eq!(
        fs::read_to_string(fx.root.join("src/a.ts")).unwrap(),
        "export const n = 1;\n"
    );

    fs::write(fx.root.join("src/a.ts"), "mutated\n").unwrap();
    let replay = execute_governed_write(
        fx.run_id,
        provider_create("call-ok", "src/a.ts", "export const n = 1;\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(replay.delivery, GovernedWriteDelivery::Replayed);
    assert_eq!(replay.resolved.state, WriteProposalState::Succeeded);
    assert_eq!(
        fs::read_to_string(fx.root.join("src/a.ts")).unwrap(),
        "mutated\n"
    );
    host.shutdown().unwrap();
}

#[test]
fn modify_terminal_replay_preserves_external_mutation() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let path = "src/mod.ts";
    fs::create_dir_all(fx.root.join("src")).unwrap();
    fs::write(fx.root.join(path), "base-v1\n").unwrap();

    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::ApproveOnce,
            )
            .unwrap();
    });

    let first = execute_governed_write(
        fx.run_id,
        provider_modify("call-mod", path, "modified-v2\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    decide.join().unwrap();
    assert_eq!(first.delivery, GovernedWriteDelivery::Executed);
    assert_eq!(first.resolved.state, WriteProposalState::Succeeded);
    assert_eq!(
        fs::read_to_string(fx.root.join(path)).unwrap(),
        "modified-v2\n"
    );

    fs::write(fx.root.join(path), "external-mutation\n").unwrap();
    let replay = execute_governed_write(
        fx.run_id,
        provider_modify("call-mod", path, "modified-v2\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(replay.delivery, GovernedWriteDelivery::Replayed);
    assert_eq!(replay.resolved.state, WriteProposalState::Succeeded);
    assert_eq!(
        fs::read_to_string(fx.root.join(path)).unwrap(),
        "external-mutation\n"
    );

    let conflict = execute_governed_write(
        fx.run_id,
        provider_modify("call-mod", path, "different-content\n"),
        &cancel,
        |_| Ok(()),
    );
    assert!(matches!(
        conflict,
        Err(GovernedWriteBridgeError::ReservationConflict)
    ));

    let conflict_path = execute_governed_write(
        fx.run_id,
        provider_modify("call-mod", "src/other.ts", "modified-v2\n"),
        &cancel,
        |_| Ok(()),
    );
    assert!(matches!(
        conflict_path,
        Err(GovernedWriteBridgeError::ReservationConflict)
    ));

    let conflict_mode = execute_governed_write(
        fx.run_id,
        provider_create("call-mod", path, "modified-v2\n"),
        &cancel,
        |_| Ok(()),
    );
    assert!(matches!(
        conflict_mode,
        Err(GovernedWriteBridgeError::ReservationConflict)
    ));

    host.shutdown().unwrap();
}

#[test]
fn requested_event_failure_cleans_live_and_allows_later_call() {
    let fx = fixture();
    let host = host_for(&fx);
    register(&host, &fx);
    let cancel = AtomicBool::new(false);

    let err = execute_governed_write(
        fx.run_id,
        provider_create("call-fail-emit", "f.txt", "body\n"),
        &cancel,
        |_| Err(GovernedWriteBridgeError::Gateway),
    );
    assert!(err.is_err());

    let list = host.list_write_proposals(&fx.conversation_id).unwrap();
    assert!(list.iter().all(|p| p.approval_grant.is_none()));
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let rows = store.list_for_conversation(&fx.conversation_id).unwrap();
    assert!(
        rows.iter().any(|r| {
            r.call_id == "call-fail-emit" && r.state == WriteProposalState::Interrupted
        })
    );

    let host = Arc::new(host);
    let host2 = Arc::clone(&host);
    let conversation_id = fx.conversation_id.clone();
    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });
    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("call-after-fail", "g.txt", "next\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    decide.join().unwrap();
    assert_eq!(receipt.resolved.state, WriteProposalState::Rejected);
    host.shutdown().unwrap();
}

#[test]
#[cfg(feature = "test-support")]
fn listing_past_expiry_never_returns_grant_and_wakes_waiter() {
    use keeper_runtime_host::force_live_expiry_for_tests;

    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let woke = Arc::new(AtomicBool::new(false));
    let woke2 = Arc::clone(&woke);

    let waiter = {
        let conversation_id = conversation_id.clone();
        thread::spawn(move || {
            let receipt = execute_governed_write(
                fx.run_id,
                provider_create("call-exp", "e.txt", "e\n"),
                &cancel,
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(receipt.resolved.state, WriteProposalState::Expired);
            assert_eq!(
                receipt.provider_error,
                Some(WritePublicErrorCode::ApprovalExpired)
            );
            woke2.store(true, Ordering::SeqCst);
            let _ = conversation_id;
        })
    };

    let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
    assert!(is_canonical_grant(&grant));
    assert!(host2.force_live_expiry_for_tests(&proposal_id));
    let _ = force_live_expiry_for_tests; // host method is preferred

    // List must durably expire and never return the grant.
    let list = host2.list_write_proposals(&conversation_id).unwrap();
    let row = list.iter().find(|p| p.proposal_id == proposal_id).unwrap();
    assert!(row.approval_grant.is_none());
    assert_eq!(row.state, WriteProposalState::Expired);

    for _ in 0..200 {
        if woke.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    waiter.join().unwrap();
    assert!(woke.load(Ordering::SeqCst));

    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let durable = store.get(&proposal_id).unwrap().unwrap();
    assert_eq!(durable.state, WriteProposalState::Expired);
    host.shutdown().unwrap();
}

#[test]
fn drop_host_interrupts_waiter_other_host_unaffected() {
    let fx_a = fixture();
    let fx_b = fixture();
    // Deliberately collide durable ids on B by using same string shapes after
    // separate DBs — each host has its own owner, so cross-select is denied.
    let host_a = Arc::new(host_for(&fx_a));
    let host_b = Arc::new(host_for(&fx_b));
    register(&host_a, &fx_a);
    register(&host_b, &fx_b);

    let conversation_a = fx_a.conversation_id.clone();
    let host_a_list = Arc::clone(&host_a);
    let pending = Arc::new(Barrier::new(2));
    let pending2 = Arc::clone(&pending);
    let run_id_a = fx_a.run_id;

    let waiter = thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let receipt = execute_governed_write(
            run_id_a,
            provider_create("call-drop", "drop.txt", "x\n"),
            &cancel,
            |_| {
                pending2.wait();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(receipt.resolved.state, WriteProposalState::Interrupted);
        assert_eq!(
            receipt.provider_error,
            Some(WritePublicErrorCode::Interrupted)
        );
    });

    // Wait until live, then drop host A without shutdown.
    pending.wait();
    let (pid, grant) = wait_live_pending(&host_a_list, &conversation_a);
    assert!(is_canonical_grant(&grant));
    drop(host_a_list);
    drop(host_a); // Drop must release write owner and interrupt waiter.

    waiter.join().unwrap();

    // Host B is independent: can still list empty for its conversation and
    // cannot see A's proposal.
    let list_b = host_b.list_write_proposals(&fx_b.conversation_id).unwrap();
    assert!(!list_b.iter().any(|p| p.proposal_id == pid));
    assert!(
        host_b
            .open_write_proposal_diff(&fx_b.conversation_id, &pid)
            .is_err()
    );
    assert!(
        host_b
            .decide_write_proposal(&fx_b.conversation_id, &pid, &grant, WriteDecision::Reject)
            .is_err()
    );
    host_b.shutdown().unwrap();
}

#[test]
fn two_hosts_colliding_ids_cannot_cross_select() {
    let fx_a = fixture();
    let fx_b = fixture();
    let host_a = Arc::new(host_for(&fx_a));
    let host_b = Arc::new(host_for(&fx_b));
    register(&host_a, &fx_a);
    register(&host_b, &fx_b);

    let conversation_a = fx_a.conversation_id.clone();
    let host_a2 = Arc::clone(&host_a);
    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host_a2, &conversation_a);
        // Host B with A's conversation/proposal/grant must fail closed.
        assert!(
            host_b
                .list_write_proposals(&conversation_a)
                .unwrap()
                .iter()
                .all(|p| p.approval_grant.is_none())
        );
        assert!(
            host_b
                .open_write_proposal_diff(&conversation_a, &proposal_id)
                .is_err()
        );
        assert!(
            host_b
                .decide_write_proposal(
                    &conversation_a,
                    &proposal_id,
                    &grant,
                    WriteDecision::ApproveOnce
                )
                .is_err()
        );
        host_a2
            .decide_write_proposal(&conversation_a, &proposal_id, &grant, WriteDecision::Reject)
            .unwrap();
    });

    let cancel = AtomicBool::new(false);
    let _ = execute_governed_write(
        fx_a.run_id,
        provider_create("call-cross", "x.txt", "x\n"),
        &cancel,
        |_| Ok(()),
    )
    .unwrap();
    decide.join().unwrap();
    host_a.shutdown().unwrap();
}

#[test]
fn wrong_grant_and_foreign_conversation_denied() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let barrier = Arc::new(Barrier::new(2));
    let host2 = Arc::clone(&host);
    let barrier2 = Arc::clone(&barrier);

    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        assert!(matches!(
            host2.decide_write_proposal(
                &conversation_id,
                &proposal_id,
                "short",
                WriteDecision::ApproveOnce
            ),
            Err(GovernedWriteBridgeError::InvalidGrant)
        ));
        let wrong = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(is_canonical_grant(wrong));
        assert!(matches!(
            host2.decide_write_proposal(
                &conversation_id,
                &proposal_id,
                wrong,
                WriteDecision::ApproveOnce
            ),
            Err(GovernedWriteBridgeError::InvalidGrant)
        ));
        // Foreign conversation is not-live / ownership fail-closed.
        assert!(
            host2
                .decide_write_proposal(
                    "00000000-0000-0000-0000-000000000099",
                    &proposal_id,
                    &grant,
                    WriteDecision::ApproveOnce
                )
                .is_err()
        );
        barrier2.wait();
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });

    let _ = execute_governed_write(
        fx.run_id,
        provider_create("call-deny", "x.txt", "body\n"),
        &cancel,
        |_| {
            barrier.wait();
            Ok(())
        },
    );
    decide.join().unwrap();
    assert!(!fx.root.join("x.txt").exists());
    host.shutdown().unwrap();
}

#[test]
fn two_conversations_wake_only_own_waiter() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    fs::create_dir(&root_a).unwrap();
    fs::create_dir(&root_b).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "P".into(),
            root_path: root_a.to_string_lossy().into_owned(),
        })
        .unwrap();
    let project_b = projects
        .create(CreateProject {
            name: "Q".into(),
            root_path: root_b.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conv_a = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("A".into()),
        })
        .unwrap();
    let conv_b = conversations
        .create_conversation(NewConversation {
            project_id: project_b.id,
            title: Some("B".into()),
        })
        .unwrap();
    let run_a = conversations
        .create_run(NewRun {
            conversation_id: conv_a.id,
            user_message: "a".into(),
        })
        .unwrap();
    let run_b = conversations
        .create_run(NewRun {
            conversation_id: conv_b.id,
            user_message: "b".into(),
        })
        .unwrap();

    let host = Arc::new(
        RuntimeHost::new_with_governed_tools(Arc::new(IdleAdapter), HostConfig::default(), &db)
            .unwrap(),
    );
    let run_id_a = ProtocolRunId(run_a.id.0);
    let run_id_b = ProtocolRunId(run_b.id.0);
    host.register_governed_write_run(
        run_id_a,
        project.id.0.to_string(),
        conv_a.id.0.to_string(),
        &root_a,
    )
    .unwrap();
    host.register_governed_write_run(
        run_id_b,
        project_b.id.0.to_string(),
        conv_b.id.0.to_string(),
        &root_b,
    )
    .unwrap();

    let woke_a = Arc::new(AtomicBool::new(false));
    let woke_b = Arc::new(AtomicBool::new(false));
    let conv_a_id = conv_a.id.0.to_string();
    let conv_b_id = conv_b.id.0.to_string();

    let t_a = {
        let woke_a = Arc::clone(&woke_a);
        thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let receipt = execute_governed_write(
                run_id_a,
                provider_create("ca", "a.txt", "a\n"),
                &cancel,
                |_| Ok(()),
            )
            .unwrap();
            woke_a.store(true, Ordering::SeqCst);
            assert_eq!(receipt.resolved.state, WriteProposalState::Rejected);
        })
    };
    let t_b = {
        let woke_b = Arc::clone(&woke_b);
        thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let receipt = execute_governed_write(
                run_id_b,
                provider_create("cb", "b.txt", "b\n"),
                &cancel,
                |_| Ok(()),
            )
            .unwrap();
            woke_b.store(true, Ordering::SeqCst);
            assert_eq!(receipt.resolved.state, WriteProposalState::Succeeded);
        })
    };

    let (id_a, grant_a) = wait_live_pending(&host, &conv_a_id);
    host.decide_write_proposal(&conv_a_id, &id_a, &grant_a, WriteDecision::Reject)
        .unwrap();
    for _ in 0..100 {
        if woke_a.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(woke_a.load(Ordering::SeqCst));
    assert!(!woke_b.load(Ordering::SeqCst));

    let (id_b, grant_b) = wait_live_pending(&host, &conv_b_id);
    host.decide_write_proposal(&conv_b_id, &id_b, &grant_b, WriteDecision::ApproveOnce)
        .unwrap();
    t_a.join().unwrap();
    t_b.join().unwrap();
    assert!(woke_b.load(Ordering::SeqCst));
    assert!(!root_a.join("a.txt").exists());
    assert_eq!(fs::read_to_string(root_b.join("b.txt")).unwrap(), "b\n");
    host.shutdown().unwrap();
}

#[test]
fn diff_only_when_live_in_memory() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let proposal_holder: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let host2 = Arc::clone(&host);
    let holder = Arc::clone(&proposal_holder);

    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        *holder.lock().unwrap() = Some(proposal_id.clone());
        let diff = host2
            .open_write_proposal_diff(&conversation_id, &proposal_id)
            .unwrap();
        assert!(diff.unified.contains("live-body") || diff.byte_count > 0);
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });

    let _ = execute_governed_write(
        fx.run_id,
        provider_create("call-diff", "d.txt", "live-body\n"),
        &cancel,
        |_| Ok(()),
    );
    decide.join().unwrap();
    let proposal_id = proposal_holder.lock().unwrap().clone().unwrap();
    assert!(matches!(
        host.open_write_proposal_diff(&fx.conversation_id, &proposal_id),
        Err(GovernedWriteBridgeError::NotLive)
    ));
    host.shutdown().unwrap();
}

#[test]
fn cancel_returns_opaque_cancelled() {
    let fx = fixture();
    let host = host_for(&fx);
    register(&host, &fx);
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let killer = {
        let cancel_flag = Arc::clone(&cancel_flag);
        let conversation_id = fx.conversation_id.clone();
        let host = host_for(&fx);
        // Poll list on same owner won't work — use sleep after execute starts.
        thread::spawn(move || {
            // Give execute time to become live via store polling on a second
            // open is owner-empty; just delay then signal cancel.
            let _ = (host, conversation_id);
            thread::sleep(Duration::from_millis(50));
            cancel_flag.store(true, Ordering::SeqCst);
        })
    };

    let shared = Arc::new(AtomicBool::new(false));
    let shared2 = Arc::clone(&shared);
    let cancel_flag2 = Arc::clone(&cancel_flag);
    let set_cancel = thread::spawn(move || {
        while !cancel_flag2.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        shared2.store(true, Ordering::SeqCst);
    });

    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("call-cancel", "c.txt", "c\n"),
        &shared,
        |_| Ok(()),
    )
    .unwrap();
    killer.join().unwrap();
    set_cancel.join().unwrap();
    assert_eq!(receipt.resolved.state, WriteProposalState::Cancelled);
    assert_eq!(
        receipt.provider_error,
        Some(WritePublicErrorCode::Cancelled)
    );
    host.shutdown().unwrap();
}

#[test]
fn restart_recovery_reconciles_applying_rows() {
    let fx = fixture();
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();

    // Create applying row that matches proposed content on disk.
    let path = "rec.txt";
    let content = "proposed-body\n";
    let store = gateway.store();
    let proposed_digest = content_sha256_digest(content);
    let proposal_id = uuid::Uuid::new_v4().to_string();
    let run_id = fx.run_id;
    let input = keeper_storage_sqlite::WriteProposalReserveInput {
        proposal_id: proposal_id.clone(),
        reservation_key: format!(
            "wp:{}:{}:{}:call-rec",
            fx.project_id, fx.conversation_id, run_id
        ),
        proposal_fingerprint: content_sha256_digest("fp-rec"),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id,
        call_id: "call-rec".into(),
        path: path.into(),
        mode: keeper_agent_protocol::WriteMode::Create,
        proposed_digest: proposed_digest.clone(),
        proposed_bytes: content.len() as u64,
        base_digest: None,
        base_bytes: 0,
    };
    store.reserve(&input).unwrap();
    store
        .decide(
            &proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &content_sha256_digest("grant-rec-unique-bytes-aaaaaaa"),
            &input.proposal_fingerprint,
            chrono::Utc::now(),
        )
        .unwrap();
    store
        .claim_apply(&proposal_id, "owner-rec", 30_000, chrono::Utc::now())
        .unwrap();
    fs::write(fx.root.join(path), content).unwrap();

    let applying = store.list_applying().unwrap();
    assert_eq!(applying.len(), 1);
    let obs = gateway.observe_applying_target(&capability, &applying[0]);
    assert_eq!(obs, WriteApplyReconcileObservation::MatchesProposed);

    let root = fx.root.clone();
    let recovered = recover_write_proposals_with_roots(&gateway, chrono::Utc::now(), move |_| {
        Some(root.clone())
    })
    .unwrap();
    assert!(
        recovered
            .iter()
            .any(|r| r.proposal_id == proposal_id && r.state == WriteProposalState::Succeeded)
    );
    assert!(store.list_applying().unwrap().is_empty());
    // Observation path must not rewrite content.
    assert_eq!(fs::read_to_string(fx.root.join(path)).unwrap(), content);
}

#[test]
fn secrets_absent_from_debug_and_store_metadata() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let secret_content = "TOP_SECRET_PROPOSED_CONTENT_XYZ";
    let grant_holder = Arc::new(Mutex::new(None::<String>));
    let grant_holder2 = Arc::clone(&grant_holder);

    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        *grant_holder2.lock().unwrap() = Some(grant.clone());
        let list_dbg = format!(
            "{:?}",
            host2.list_write_proposals(&conversation_id).unwrap()
        );
        assert!(!list_dbg.contains(&grant));
        assert!(!list_dbg.contains(secret_content));
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });

    let cancel = AtomicBool::new(false);
    let receipt = execute_governed_write(
        fx.run_id,
        provider_create("call-secret", "s.txt", &format!("{secret_content}\n")),
        &cancel,
        |requested| {
            assert!(!format!("{requested:?}").contains(secret_content));
            Ok(())
        },
    )
    .unwrap();
    decide.join().unwrap();
    let grant = grant_holder.lock().unwrap().clone().unwrap();
    assert!(!format!("{receipt:?}").contains(&grant));
    assert!(!format!("{receipt:?}").contains(secret_content));
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    for row in store.list_for_conversation(&fx.conversation_id).unwrap() {
        let dbg = format!("{row:?}");
        assert!(!dbg.contains(&grant));
        assert!(!dbg.contains(secret_content));
        assert!(!row.proposal_fingerprint.contains(&grant));
    }
    host.shutdown().unwrap();
}

#[test]
#[cfg(feature = "test-support")]
fn storage_failure_after_live_never_fabricates_terminal() {
    let barrier = run_storage_failure_barrier_case();
    let control = run_storage_failure_waiter_paused_control();

    assert_eq!(barrier.barrier_trace.len(), 2);
    assert!(
        barrier
            .barrier_trace
            .iter()
            .all(|(proposal_id, _)| proposal_id == &barrier.proposal_id)
    );
    let mut barrier_sources = barrier
        .barrier_trace
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>();
    barrier_sources.sort_unstable();
    assert_eq!(barrier_sources, ["main-list", "waiter-expiry"]);

    assert_eq!(
        barrier.transition_trace.len(),
        1,
        "the two expiry initiators must single-flight one SQLite transition"
    );
    assert_eq!(barrier.transition_trace[0].0, barrier.proposal_id);
    assert!(matches!(
        barrier.transition_trace[0].1.as_str(),
        "main-list" | "waiter-expiry"
    ));
    assert_eq!(
        barrier.attempts_after,
        barrier.attempts_before + 1,
        "exactly one failed transition attempt; no second write/retry"
    );

    assert_eq!(
        control.barrier_trace,
        vec![(control.proposal_id.clone(), "main-list".to_owned())]
    );
    assert_eq!(
        control.transition_trace,
        vec![(control.proposal_id.clone(), "main-list".to_owned())]
    );
    assert_eq!(
        control.attempts_after,
        control.attempts_before + 1,
        "pausing the waiter must isolate exactly one main-list transition"
    );
}

#[cfg(feature = "test-support")]
struct StorageFailureObservation {
    proposal_id: String,
    attempts_before: usize,
    attempts_after: usize,
    barrier_trace: Vec<(String, String)>,
    transition_trace: Vec<(String, String)>,
}

#[cfg(feature = "test-support")]
fn run_storage_failure_barrier_case() -> StorageFailureObservation {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let woke = Arc::new(AtomicBool::new(false));
    let woke2 = Arc::clone(&woke);
    let waiter_err = Arc::new(Mutex::new(None::<GovernedWriteBridgeError>));
    let waiter_err2 = Arc::clone(&waiter_err);

    let waiter = {
        let run_id = fx.run_id;
        thread::spawn(move || {
            let result = execute_governed_write(
                run_id,
                provider_create("call-store-fail", "fail.txt", "secret-body\n"),
                &cancel,
                |_| Ok(()),
            );
            match result {
                Ok(receipt) => {
                    // Must never invent a terminal success/expired/cancelled receipt.
                    panic!("unexpected terminal receipt: {:?}", receipt.resolved.state);
                }
                Err(error) => {
                    *waiter_err2.lock().unwrap() = Some(error);
                    woke2.store(true, Ordering::SeqCst);
                }
            }
        })
    };

    let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
    assert!(is_canonical_grant(&grant));

    let gateway = host2.write_gateway().expect("write gateway");
    let attempts_before = gateway.store().write_attempt_count_for_tests();
    gateway.store().clear_transition_traces_for_tests();
    gateway.store().set_transition_barrier_for_tests(
        proposal_id.clone(),
        2,
        None,
        Duration::from_secs(5),
    );
    gateway.store().set_fail_writes_for_tests(true);

    // Force both expiry initiators past their coordinator-locked eligibility
    // checks before either may enter SQLite transition.
    assert!(host2.force_live_expiry_for_tests(&proposal_id));
    let list_err = host2
        .list_write_proposals(&conversation_id)
        .expect_err("list must fail closed on storage fault");
    assert!(matches!(list_err, GovernedWriteBridgeError::Gateway));

    for _ in 0..200 {
        if woke.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    waiter.join().unwrap();
    assert!(woke.load(Ordering::SeqCst));
    let err = waiter_err.lock().unwrap().clone().expect("waiter error");
    assert!(
        matches!(err, GovernedWriteBridgeError::Gateway),
        "provider must get opaque unavailable, got {err:?}"
    );

    let attempts_after = gateway.store().write_attempt_count_for_tests();
    let barrier_trace = gateway.store().transition_barrier_trace_for_tests();
    let transition_trace = gateway.store().transition_attempt_trace_for_tests();
    gateway.store().clear_transition_barrier_for_tests();
    eprintln!(
        "STORAGE_BARRIER_RESULT proposal_id={proposal_id} barrier_trace={barrier_trace:?} transition_trace={transition_trace:?} attempts_before={attempts_before} attempts_after={attempts_after} delta={} list_error=Gateway waiter_error=Gateway",
        attempts_after - attempts_before
    );

    // Grant/content/diff must not remain actionable after quarantine.
    assert!(
        host2
            .open_write_proposal_diff(&conversation_id, &proposal_id)
            .is_err()
    );
    assert!(
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::ApproveOnce
            )
            .is_err()
    );
    // Re-enable reads; durable row stays non-terminal (no fabricated expired).
    gateway.store().set_fail_writes_for_tests(false);
    let durable = gateway.store().get(&proposal_id).unwrap().unwrap();
    assert_eq!(durable.state, WriteProposalState::PendingApproval);
    assert!(!is_terminal_list_state(durable.state));

    // Listing must never expose grant after quarantine.
    let list = host2.list_write_proposals(&conversation_id).unwrap();
    for row in &list {
        if row.proposal_id == proposal_id {
            assert!(row.approval_grant.is_none());
            assert!(row.diff.is_none());
        }
    }
    host.shutdown().unwrap();

    StorageFailureObservation {
        proposal_id,
        attempts_before,
        attempts_after,
        barrier_trace,
        transition_trace,
    }
}

#[cfg(feature = "test-support")]
fn run_storage_failure_waiter_paused_control() -> StorageFailureObservation {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let (requested_tx, requested_rx) = mpsc::sync_channel::<String>(1);
    let (release_tx, release_rx) = mpsc::channel::<()>();

    let waiter = {
        let run_id = fx.run_id;
        thread::spawn(move || {
            execute_governed_write(
                run_id,
                provider_create(
                    "call-store-fail-control",
                    "fail-control.txt",
                    "secret-control-body\n",
                ),
                &cancel,
                |requested| {
                    requested_tx
                        .send(requested.proposal_id.clone())
                        .map_err(|_| GovernedWriteBridgeError::Gateway)?;
                    release_rx
                        .recv()
                        .map_err(|_| GovernedWriteBridgeError::Gateway)?;
                    Ok(())
                },
            )
        })
    };

    let proposal_id = requested_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("waiter must pause after publishing the requested proposal");
    let (listed_proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
    let gateway = host2.write_gateway().expect("write gateway");
    let attempts_before = gateway.store().write_attempt_count_for_tests();
    gateway.store().clear_transition_traces_for_tests();
    gateway.store().set_transition_barrier_for_tests(
        proposal_id.clone(),
        1,
        None,
        Duration::from_secs(5),
    );
    gateway.store().set_fail_writes_for_tests(true);

    let expiry_forced = host2.force_live_expiry_for_tests(&proposal_id);
    let list_result = host2.list_write_proposals(&conversation_id);

    let attempts_before_release = gateway.store().write_attempt_count_for_tests();
    let diff_result = host2.open_write_proposal_diff(&conversation_id, &proposal_id);
    let decide_result = host2.decide_write_proposal(
        &conversation_id,
        &proposal_id,
        &grant,
        WriteDecision::ApproveOnce,
    );
    let durable_before_release = gateway.store().get(&proposal_id);
    let list_after_quarantine_before_release = host2.list_write_proposals(&conversation_id);
    let barrier_trace_before_release = gateway.store().transition_barrier_trace_for_tests();
    let transition_trace_before_release = gateway.store().transition_attempt_trace_for_tests();

    release_tx
        .send(())
        .expect("release paused waiter after quarantine assertions");
    let waiter_result = waiter.join().expect("paused waiter thread");

    assert_eq!(listed_proposal_id, proposal_id);
    assert!(is_canonical_grant(&grant));
    assert!(expiry_forced);
    let list_err = list_result.expect_err("list must fail closed on storage fault");
    assert!(matches!(list_err, GovernedWriteBridgeError::Gateway));
    assert_eq!(attempts_before_release, attempts_before + 1);
    assert!(
        matches!(diff_result, Err(GovernedWriteBridgeError::NotLive)),
        "quarantine must remove the actionable diff before waiter release"
    );
    assert!(
        matches!(decide_result, Err(GovernedWriteBridgeError::NotLive)),
        "quarantine must remove approval authority before waiter release"
    );
    let durable_before_release = durable_before_release
        .expect("durable read before waiter release")
        .expect("durable proposal before waiter release");
    assert_eq!(
        durable_before_release.state,
        WriteProposalState::PendingApproval
    );
    assert!(!is_terminal_list_state(durable_before_release.state));
    let list_after_quarantine_before_release =
        list_after_quarantine_before_release.expect("durable list after quarantine");
    for row in &list_after_quarantine_before_release {
        if row.proposal_id == proposal_id {
            assert!(row.approval_grant.is_none());
            assert!(row.diff.is_none());
        }
    }
    let waiter_err = waiter_result.expect_err("waiter must receive opaque storage failure");
    assert!(
        matches!(waiter_err, GovernedWriteBridgeError::Gateway),
        "provider must get opaque unavailable, got {waiter_err:?}"
    );

    let attempts_after = gateway.store().write_attempt_count_for_tests();
    assert_eq!(attempts_after, attempts_before_release);
    let barrier_trace = gateway.store().transition_barrier_trace_for_tests();
    let transition_trace = gateway.store().transition_attempt_trace_for_tests();
    assert_eq!(barrier_trace, barrier_trace_before_release);
    assert_eq!(transition_trace, transition_trace_before_release);
    gateway.store().clear_transition_barrier_for_tests();
    eprintln!(
        "STORAGE_CONTROL_RESULT proposal_id={proposal_id} barrier_trace={barrier_trace:?} transition_trace={transition_trace:?} attempts_before={attempts_before} attempts_after={attempts_after} delta={} list_error=Gateway waiter_error=Gateway quarantine_before_release=true durable_before_release=pending_approval",
        attempts_after - attempts_before
    );

    gateway.store().set_fail_writes_for_tests(false);
    let durable = gateway.store().get(&proposal_id).unwrap().unwrap();
    assert_eq!(durable.state, WriteProposalState::PendingApproval);
    assert!(!is_terminal_list_state(durable.state));
    let list = host2.list_write_proposals(&conversation_id).unwrap();
    for row in &list {
        if row.proposal_id == proposal_id {
            assert!(row.approval_grant.is_none());
            assert!(row.diff.is_none());
        }
    }
    host.shutdown().unwrap();

    StorageFailureObservation {
        proposal_id,
        attempts_before,
        attempts_after,
        barrier_trace,
        transition_trace,
    }
}

#[test]
#[cfg(feature = "test-support")]
fn expiry_single_flight_waiter_winner_survives_live_removal() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();

    let waiter = {
        let run_id = fx.run_id;
        thread::spawn(move || {
            execute_governed_write(
                run_id,
                provider_create(
                    "call-expiry-single-flight",
                    "expiry-single-flight.txt",
                    "body\n",
                ),
                &cancel,
                |_| Ok(()),
            )
        })
    };

    let (proposal_id, grant) = wait_live_pending(&host, &conversation_id);
    assert!(is_canonical_grant(&grant));
    let gateway = host.write_gateway().expect("write gateway");
    let attempts_before = gateway.store().write_attempt_count_for_tests();
    gateway.store().clear_transition_traces_for_tests();
    gateway.store().set_transition_barrier_for_tests(
        proposal_id.clone(),
        2,
        Some("waiter-expiry"),
        Duration::from_secs(5),
    );

    assert!(host.force_live_expiry_for_tests(&proposal_id));
    let list = host.list_write_proposals(&conversation_id).unwrap();
    let receipt = waiter.join().unwrap().unwrap();
    let attempts_after = gateway.store().write_attempt_count_for_tests();
    let barrier_trace = gateway.store().transition_barrier_trace_for_tests();
    let transition_trace = gateway.store().transition_attempt_trace_for_tests();
    gateway.store().clear_transition_barrier_for_tests();

    assert_eq!(barrier_trace.len(), 2);
    assert!(
        barrier_trace
            .iter()
            .all(|(traced_proposal, _)| traced_proposal == &proposal_id)
    );
    let mut sources = barrier_trace
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>();
    sources.sort_unstable();
    assert_eq!(sources, ["main-list", "waiter-expiry"]);
    assert_eq!(
        transition_trace,
        vec![(proposal_id.clone(), "waiter-expiry".to_owned())]
    );
    assert_eq!(attempts_after, attempts_before + 1);

    assert_eq!(receipt.resolved.state, WriteProposalState::Expired);
    assert_eq!(
        receipt.provider_error,
        Some(WritePublicErrorCode::ApprovalExpired)
    );
    let row = list
        .iter()
        .find(|row| row.proposal_id == proposal_id)
        .expect("durable expired row after waiter removes live entry");
    assert_eq!(row.state, WriteProposalState::Expired);
    assert!(row.approval_grant.is_none());
    assert!(row.diff.is_none());
    let durable = gateway.store().get(&proposal_id).unwrap().unwrap();
    assert_eq!(durable.state, WriteProposalState::Expired);
    assert_eq!(
        gateway.store().history(&proposal_id).unwrap(),
        vec![
            WriteProposalState::PendingApproval,
            WriteProposalState::Expired
        ]
    );
    eprintln!(
        "STORAGE_SUCCESS_SINGLE_FLIGHT proposal_id={proposal_id} barrier_trace={barrier_trace:?} transition_trace={transition_trace:?} attempts_before={attempts_before} attempts_after={attempts_after} delta={} waiter_removed_live=true list_state=expired",
        attempts_after - attempts_before
    );
    host.shutdown().unwrap();
}

#[cfg(feature = "test-support")]
fn is_terminal_list_state(state: WriteProposalState) -> bool {
    matches!(
        state,
        WriteProposalState::Succeeded
            | WriteProposalState::Rejected
            | WriteProposalState::Expired
            | WriteProposalState::Cancelled
            | WriteProposalState::Interrupted
            | WriteProposalState::Failed
    )
}

#[test]
fn unresolvable_applying_root_leaves_row_and_errors() {
    let fx = fixture();
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let path = "orphan-applying.txt";
    let content = "on-disk-proposed\n";
    let proposed_digest = content_sha256_digest(content);
    let proposal_id = uuid::Uuid::new_v4().to_string();
    let input = keeper_storage_sqlite::WriteProposalReserveInput {
        proposal_id: proposal_id.clone(),
        reservation_key: format!(
            "wp:{}:{}:{}:call-orphan",
            fx.project_id, fx.conversation_id, fx.run_id
        ),
        proposal_fingerprint: content_sha256_digest("fp-orphan"),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: fx.run_id,
        call_id: "call-orphan".into(),
        path: path.into(),
        mode: keeper_agent_protocol::WriteMode::Create,
        proposed_digest: proposed_digest.clone(),
        proposed_bytes: content.len() as u64,
        base_digest: None,
        base_bytes: 0,
    };
    gateway.store().reserve(&input).unwrap();
    gateway
        .store()
        .decide(
            &proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &content_sha256_digest("grant-orphan-unique-bytes-bbbbbb"),
            &input.proposal_fingerprint,
            chrono::Utc::now(),
        )
        .unwrap();
    gateway
        .store()
        .claim_apply(&proposal_id, "owner-orphan", 30_000, chrono::Utc::now())
        .unwrap();
    fs::write(fx.root.join(path), content).unwrap();

    // No resolver → must not synthesize Neither / failed.
    let err = recover_write_proposals_with_roots(&gateway, chrono::Utc::now(), |_| None)
        .expect_err("missing root must fail closed");
    assert!(matches!(err, GovernedWriteBridgeError::Gateway));
    let still = gateway.store().get(&proposal_id).unwrap().unwrap();
    assert_eq!(still.state, WriteProposalState::Applying);

    // Valid root still reconciles exactly to success for MatchesProposed.
    let root = fx.root.clone();
    let recovered = recover_write_proposals_with_roots(&gateway, chrono::Utc::now(), move |_| {
        Some(root.clone())
    })
    .unwrap();
    assert!(
        recovered
            .iter()
            .any(|r| r.proposal_id == proposal_id && r.state == WriteProposalState::Succeeded)
    );
    assert!(gateway.store().list_applying().unwrap().is_empty());
}

#[test]
fn durable_list_after_restart_without_bound_run() {
    let fx = fixture();
    let conversation_id = fx.conversation_id.clone();
    let proposal_id = {
        let host = Arc::new(host_for(&fx));
        register(&host, &fx);
        let cancel = AtomicBool::new(false);
        let run_id = fx.run_id;
        let waiter = thread::spawn(move || {
            execute_governed_write(
                run_id,
                provider_create("call-restart-list", "restart.txt", "r\n"),
                &cancel,
                |_| Ok(()),
            )
        });
        let (pid, _grant) = wait_live_pending(&host, &conversation_id);
        // Drop host while still pending: interrupts waiter; durable may become
        // interrupted in-process. Restart host binds no new run.
        drop(host);
        let _ = waiter.join().unwrap();
        pid
    };

    // Reopen host on same DB, recover, bind no run.
    let host2 =
        RuntimeHost::new_with_governed_tools(Arc::new(IdleAdapter), HostConfig::default(), &fx.db)
            .unwrap();
    let root = fx.root.clone();
    let _ = host2
        .recover_write_proposals(chrono::Utc::now(), move |_| Some(root.clone()))
        .unwrap();

    // No register_governed_write_run — durable list must still work via host gateway.
    let list = host2.list_write_proposals(&conversation_id).unwrap();
    let row = list
        .iter()
        .find(|p| p.proposal_id == proposal_id)
        .expect("durable interrupted proposal listed without bound run");
    assert_eq!(row.state, WriteProposalState::Interrupted);
    assert!(row.approval_grant.is_none());
    assert!(row.diff.is_none());
    host2.shutdown().unwrap();
}

#[test]
#[cfg(feature = "test-support")]
fn direct_diff_at_expiry_returns_not_live_and_wakes_waiter() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    let cancel = AtomicBool::new(false);
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let woke = Arc::new(AtomicBool::new(false));
    let woke2 = Arc::clone(&woke);

    let waiter = {
        let run_id = fx.run_id;
        thread::spawn(move || {
            let receipt = execute_governed_write(
                run_id,
                provider_create("call-diff-exp", "diff-e.txt", "body\n"),
                &cancel,
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(receipt.resolved.state, WriteProposalState::Expired);
            woke2.store(true, Ordering::SeqCst);
        })
    };

    let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
    assert!(is_canonical_grant(&grant));
    // Diff is available before expiry.
    let before = host2
        .open_write_proposal_diff(&conversation_id, &proposal_id)
        .unwrap();
    assert!(before.unified.contains("body") || before.byte_count > 0);

    assert!(host2.force_live_expiry_for_tests(&proposal_id));
    let diff_err = host2
        .open_write_proposal_diff(&conversation_id, &proposal_id)
        .expect_err("diff at/past expiry must fail");
    assert!(
        matches!(
            diff_err,
            GovernedWriteBridgeError::NotLive | GovernedWriteBridgeError::Expired
        ),
        "got {diff_err:?}"
    );

    for _ in 0..200 {
        if woke.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    waiter.join().unwrap();
    assert!(woke.load(Ordering::SeqCst));

    // No grant/diff after durable expiry.
    let list = host2.list_write_proposals(&conversation_id).unwrap();
    let row = list.iter().find(|p| p.proposal_id == proposal_id).unwrap();
    assert_eq!(row.state, WriteProposalState::Expired);
    assert!(row.approval_grant.is_none());
    assert!(row.diff.is_none());
    assert!(
        host2
            .open_write_proposal_diff(&conversation_id, &proposal_id)
            .is_err()
    );
    let durable = SqliteWriteProposalRepository::open(&fx.db)
        .unwrap()
        .get(&proposal_id)
        .unwrap()
        .unwrap();
    assert_eq!(durable.state, WriteProposalState::Expired);
    host.shutdown().unwrap();
}

// ── C3-F cancel resolution arbitration ─────────────────────────────────────

fn cancel_arb_event(
    run: ProtocolRunId,
    seq: u64,
    kind: RuntimeEventKind,
    payload: serde_json::Value,
) -> RuntimeEvent {
    RuntimeEvent {
        run_id: run,
        sequence: seq,
        kind,
        payload,
        created_at: Utc::now(),
    }
}

fn safe_cancelled_resolved() -> serde_json::Value {
    json!({
        "proposal_id": "wp-cancel-arb",
        "path": "c.txt",
        "mode": "create",
        "state": "cancelled",
        "error_code": "cancelled"
    })
}

/// Reproduce C3-F defect: host cancel must still deliver the conversation-owned
/// write terminal (`write_proposal_resolved` cancelled/cancelled) before the
/// single host-forced `run_cancelled`.
#[test]
fn cancel_arbitration_emits_safe_write_resolution_before_run_cancelled() {
    let delivery = BoundedDelivery::new(16);
    let mut sink = BoundedDeliverySink::new(Arc::clone(&delivery));
    let run = ProtocolRunId::new();
    let cancel = AtomicBool::new(false);
    {
        let mut guard = ProtocolGuardSink::with_cancel(run, &mut sink, &cancel);
        guard
            .emit(cancel_arb_event(
                run,
                1,
                RuntimeEventKind::RunStarted,
                json!({}),
            ))
            .unwrap();
        guard
            .emit(cancel_arb_event(
                run,
                2,
                RuntimeEventKind::WriteProposalRequested,
                json!({
                    "proposal_id": "wp-cancel-arb",
                    "path": "c.txt",
                    "mode": "create",
                    "summary": "Create c.txt",
                    "expires_at": "2026-08-01T12:10:00Z"
                }),
            ))
            .unwrap();

        // Host Stop: delivery + protocol cancel land before adapter resolution.
        delivery.request_cancel();
        cancel.store(true, Ordering::SeqCst);

        let err = guard
            .emit(cancel_arb_event(
                run,
                3,
                RuntimeEventKind::WriteProposalResolved,
                safe_cancelled_resolved(),
            ))
            .unwrap_err();
        assert!(
            matches!(err, RuntimeError::Cancelled),
            "post-cancel emit still reports Cancelled to the adapter"
        );
        guard.finish().unwrap();
    }

    let events = delivery.drain();
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RuntimeEventKind::RunStarted,
            RuntimeEventKind::WriteProposalRequested,
            RuntimeEventKind::WriteProposalResolved,
            RuntimeEventKind::RunCancelled,
        ],
        "exact C3-F event order; got {kinds:?}"
    );
    let resolved = events
        .iter()
        .find(|e| e.kind == RuntimeEventKind::WriteProposalResolved)
        .expect("safe write_proposal_resolved must be present");
    assert_eq!(
        resolved.payload.get("state").and_then(|v| v.as_str()),
        Some("cancelled")
    );
    assert_eq!(
        resolved.payload.get("error_code").and_then(|v| v.as_str()),
        Some("cancelled")
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == RuntimeEventKind::RunCancelled)
            .count(),
        1
    );
}

/// Negatives stay blocked after cancel: write success, ordinary output, and a
/// second safe resolution must not commit.
#[test]
fn cancel_arbitration_blocks_success_ordinary_and_duplicate_safe_resolution() {
    // Success resolution after cancel → only run_cancelled.
    {
        let delivery = BoundedDelivery::new(8);
        let mut sink = BoundedDeliverySink::new(Arc::clone(&delivery));
        let run = ProtocolRunId::new();
        let cancel = AtomicBool::new(true);
        delivery.request_cancel();
        let mut guard = ProtocolGuardSink::with_cancel(run, &mut sink, &cancel);
        let _ = guard.emit(cancel_arb_event(
            run,
            1,
            RuntimeEventKind::WriteProposalResolved,
            json!({
                "proposal_id": "wp-ok",
                "path": "ok.txt",
                "mode": "create",
                "state": "succeeded"
            }),
        ));
        let events = delivery.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RuntimeEventKind::RunCancelled);
    }

    // Ordinary assistant delta after cancel → only run_cancelled.
    {
        let delivery = BoundedDelivery::new(8);
        let mut sink = BoundedDeliverySink::new(Arc::clone(&delivery));
        let run = ProtocolRunId::new();
        let cancel = AtomicBool::new(true);
        delivery.request_cancel();
        let mut guard = ProtocolGuardSink::with_cancel(run, &mut sink, &cancel);
        let _ = guard.emit(cancel_arb_event(
            run,
            1,
            RuntimeEventKind::AssistantDelta,
            json!({"text": "nope"}),
        ));
        let events = delivery.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RuntimeEventKind::RunCancelled);
    }

    // Duplicate safe resolution after the first already forced the terminal.
    {
        let delivery = BoundedDelivery::new(8);
        let mut sink = BoundedDeliverySink::new(Arc::clone(&delivery));
        let run = ProtocolRunId::new();
        let cancel = AtomicBool::new(false);
        let mut guard = ProtocolGuardSink::with_cancel(run, &mut sink, &cancel);
        guard
            .emit(cancel_arb_event(
                run,
                1,
                RuntimeEventKind::RunStarted,
                json!({}),
            ))
            .unwrap();
        delivery.request_cancel();
        cancel.store(true, Ordering::SeqCst);
        let _ = guard.emit(cancel_arb_event(
            run,
            2,
            RuntimeEventKind::WriteProposalResolved,
            safe_cancelled_resolved(),
        ));
        let _ = guard.emit(cancel_arb_event(
            run,
            3,
            RuntimeEventKind::WriteProposalResolved,
            safe_cancelled_resolved(),
        ));
        let events = delivery.drain();
        let resolved = events
            .iter()
            .filter(|e| e.kind == RuntimeEventKind::WriteProposalResolved)
            .count();
        let cancelled = events
            .iter()
            .filter(|e| e.kind == RuntimeEventKind::RunCancelled)
            .count();
        assert_eq!(resolved, 1, "duplicate safe resolution must fail closed");
        assert_eq!(cancelled, 1, "exactly one run_cancelled terminal");
    }
}

// ── Step 3: level-triggered CancellationProbe (deterministic barrier) ───────

/// Proves the write waiter re-probes cancel after execute entry.
///
/// Barrier order (deterministic, no sleep race):
/// 1. cancel starts false (an entry snapshot would capture false);
/// 2. `on_requested` fires only after durable reserve — execute has entered;
/// 3. only then do we flip the AtomicBool with SeqCst;
/// 4. waiter must observe the live true and return Cancelled.
///
/// If the host snapshotted once at entry, this test hangs until expiry or
/// returns a non-Cancelled terminal (mutation probe fails closed).
#[test]
fn cancel_probe_level_triggered_after_execute_entry_barrier() {
    let fx = fixture();
    let host = host_for(&fx);
    register(&host, &fx);

    let cancel = Arc::new(AtomicBool::new(false));
    // 2 parties: waiter thread (on_requested) + main (flip after entry).
    let entered = Arc::new(Barrier::new(2));
    let cancel_for_cb = Arc::clone(&cancel);
    let entered_for_cb = Arc::clone(&entered);

    let cancel_token = Arc::clone(&cancel);
    let waiter = thread::spawn(move || {
        execute_governed_write(
            fx.run_id,
            provider_create("call-level-cancel", "level.txt", "x\n"),
            cancel_token.as_ref(),
            |_| {
                // Execute has reserved and is past any entry-time cancel check.
                assert!(
                    !cancel_for_cb.load(Ordering::SeqCst),
                    "cancel must still be false at entry/on_requested"
                );
                entered_for_cb.wait();
                // Main flips cancel only after this barrier.
                Ok(())
            },
        )
    });

    // Wait until execute has entered (on_requested), then flip live cancel.
    entered.wait();
    cancel.store(true, Ordering::SeqCst);

    let receipt = waiter
        .join()
        .expect("waiter thread")
        .expect("level-triggered cancel must yield a receipt");
    assert_eq!(
        receipt.resolved.state,
        WriteProposalState::Cancelled,
        "waiter must re-probe cancel after entry; got {:?}",
        receipt.resolved.state
    );
    assert_eq!(
        receipt.provider_error,
        Some(WritePublicErrorCode::Cancelled)
    );
    host.shutdown().unwrap();
}

#[test]
fn governance_port_write_preserves_lifecycle_replay_and_observer_failure() {
    let fx = fixture();
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);
    fs::create_dir_all(fx.root.join("src")).unwrap();

    let port = GlobalGovernedExecutionPort::new();
    let cancel = FixedCancellationProbe::new();
    let conversation_id = fx.conversation_id.clone();
    let host2 = Arc::clone(&host);
    let decide = thread::spawn(move || {
        let (proposal_id, grant) = wait_live_pending(&host2, &conversation_id);
        host2
            .decide_write_proposal(
                &conversation_id,
                &proposal_id,
                &grant,
                WriteDecision::Reject,
            )
            .unwrap();
    });

    let proposal = provider_create("port-write", "src/port.ts", "export {}\n");
    let mut observer = RecordingWriteObserver::new();
    let first = port
        .execute_write(fx.run_id, proposal.clone(), &cancel, &mut observer)
        .unwrap();
    decide.join().unwrap();
    assert_eq!(first.receipt().delivery, GovernedWriteDelivery::Executed);
    assert_eq!(first.receipt().resolved.state, WriteProposalState::Rejected);
    assert_eq!(observer.requested_count, 1);

    let replay = port
        .execute_write(fx.run_id, proposal, &cancel, &mut observer)
        .unwrap();
    assert_eq!(replay.receipt().delivery, GovernedWriteDelivery::Replayed);
    assert_eq!(
        observer.requested_count, 1,
        "replay must not invoke the port observer"
    );

    for call_id in [
        "port-write-observer-failure-1",
        "port-write-observer-failure-2",
    ] {
        let mut failing = RecordingWriteObserver::new();
        failing.fail_next(LifecycleObserverError::DeliveryFailed);
        let error = port
            .execute_write(
                fx.run_id,
                provider_create(call_id, &format!("src/{call_id}.ts"), "x\n"),
                &cancel,
                &mut failing,
            )
            .unwrap_err();
        assert_eq!(
            error,
            GovernedWriteExecutionError::LifecycleDeliveryFailed,
            "observer failure must stay distinct from ordinary Unavailable"
        );
    }

    host.shutdown().unwrap();
}
