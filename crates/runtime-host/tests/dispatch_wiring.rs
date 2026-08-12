//! Host dispatch wiring: role-dispatch → governed process chain.
//!
//! Critical red line: unapproved dispatch must never start a process.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId as ProtocolRunId, RuntimeError, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};
use keeper_policy::{ExecDecision, ExecProposalState};
use keeper_project::CreateProject;
use keeper_runtime_host::{
    HostConfig, RuntimeHost, attribute_dispatch_receipt, claimed_governed_execution_for,
    clear_dispatch_ledger_for_tests, complete_dispatch_on_receipt, execute_prepared_dispatch,
    prepare_dispatch, register_dispatch_on_requested,
};
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    PermissionMode, SettingsUpdatedBy, SqliteConversationRepository, SqliteExecProposalRepository,
    SqliteProjectRepository, SqliteSettingsRepository,
};
use pillow_role_dispatch::{
    BuildAssignmentInput, GovernanceCapability, SlotAuthShape, SlotId, SlotRole, TargetSlotRecord,
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
            name: "DispatchProject".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let conversations = SqliteConversationRepository::open(&db).unwrap();
    let conversation = conversations
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("Dispatch".into()),
        })
        .unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "dispatch".into(),
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

fn host_for(fx: &Fixture) -> RuntimeHost {
    RuntimeHost::new_with_governed_tools(
        Arc::new(IdleAdapter),
        HostConfig::new(4, false, 64),
        &fx.db,
    )
    .unwrap()
}

fn register(host: &RuntimeHost, fx: &Fixture) {
    host.register_governed_process_run(fx.run_id, &fx.project_id, &fx.conversation_id, &fx.root)
        .unwrap();
}

fn set_default_mode(fx: &Fixture) {
    let settings = SqliteSettingsRepository::open(&fx.db).unwrap();
    settings
        .upsert_permission_mode(PermissionMode::Default, SettingsUpdatedBy::User)
        .unwrap();
}

fn local_cli_node(target: SlotId) -> TargetSlotRecord {
    TargetSlotRecord {
        slot_id: target,
        role: SlotRole::Custom("worker".into()),
        auth: SlotAuthShape::LocalCli {
            // Catalog label so seal succeeds; real LocalCli paths are residual.
            executable_path: "node".into(),
            launch_args: vec!["--version".into()],
            status_args: vec![],
        },
    }
}

fn wait_live_pending(host: &RuntimeHost, conversation_id: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let views = host.list_process_proposals(conversation_id).unwrap();
        if let Some(view) = views
            .into_iter()
            .find(|v| v.state == ExecProposalState::PendingApproval && v.decision_available)
        {
            return view.proposal_id;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for pending dispatch proposal"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// 🔴 Red line: proposing a dispatch must not start a process until approved.
#[test]
fn unapproved_dispatch_never_starts_a_process() {
    clear_dispatch_ledger_for_tests();
    let fx = fixture();
    set_default_mode(&fx);
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);

    let initiator = SlotId::new();
    let target = SlotId::new();
    let prepared = prepare_dispatch(BuildAssignmentInput {
        initiator_slot_id: initiator,
        target: Some(local_cli_node(target)),
        requested_role: None,
        requested_slot_id: Some(target),
        task_text: "do not run yet".into(),
        cwd: Some(".".into()),
    })
    .unwrap();
    assert_eq!(prepared.assignment.governance, GovernanceCapability::None);
    assert!(!claimed_governed_execution_for(
        prepared.assignment.governance
    ));

    let started_count = Arc::new(AtomicUsize::new(0));
    let requested_count = Arc::new(AtomicUsize::new(0));
    let proposal_id_slot = Arc::new(Mutex::new(None::<String>));
    let cancel = AtomicBool::new(false);

    let deciding_host = Arc::clone(&host);
    let conversation_id = fx.conversation_id.clone();
    let rejector = thread::spawn(move || {
        let proposal_id = wait_live_pending(&deciding_host, &conversation_id);
        // Reject without ever approving — process must not start.
        deciding_host
            .decide_process_proposal(&conversation_id, &proposal_id, ExecDecision::Reject)
            .unwrap();
    });

    let started_for_cb = Arc::clone(&started_count);
    let requested_for_cb = Arc::clone(&requested_count);
    let proposal_for_cb = Arc::clone(&proposal_id_slot);
    let prepared_for_ledger = prepared.clone();
    let conversation_for_ledger = fx.conversation_id.clone();
    let run_id = fx.run_id;

    let receipt = execute_prepared_dispatch(run_id, &prepared, &cancel, |event| {
        match event {
            keeper_runtime_host::GovernedProcessLifecycle::Requested(requested) => {
                requested_for_cb.fetch_add(1, Ordering::SeqCst);
                *proposal_for_cb.lock().unwrap() = Some(requested.proposal_id.clone());
                register_dispatch_on_requested(
                    &prepared_for_ledger,
                    conversation_for_ledger.clone(),
                    run_id,
                    requested.proposal_id.clone(),
                )
                .unwrap();
            }
            keeper_runtime_host::GovernedProcessLifecycle::Decided(_) => {}
            keeper_runtime_host::GovernedProcessLifecycle::Started(_) => {
                started_for_cb.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(())
    })
    .unwrap();

    rejector.join().unwrap();

    assert_eq!(
        requested_count.load(Ordering::SeqCst),
        1,
        "dispatch must still register a pending proposal"
    );
    assert_eq!(
        started_count.load(Ordering::SeqCst),
        0,
        "unapproved dispatch must never emit Started"
    );
    assert!(
        receipt.started.is_none(),
        "receipt must not carry a started payload when rejected"
    );
    assert_eq!(receipt.state, ExecProposalState::Rejected);

    // Durable row also never reached Running/Succeeded without approval.
    let store = SqliteExecProposalRepository::open(&fx.db).unwrap();
    let row = store.get(&receipt.proposal_id).unwrap().unwrap();
    assert_eq!(row.state, ExecProposalState::Rejected);
    assert!(row.exit_code.is_none());

    // Attribution still records LocalCli without claiming governed execution.
    let mut assignment = prepared.assignment.clone();
    let outcome =
        complete_dispatch_on_receipt(&receipt.proposal_id, &mut assignment, &receipt).unwrap();
    assert!(!outcome.claimed_governed_execution);
    assert_eq!(outcome.governance, GovernanceCapability::None);

    let record = keeper_runtime_host::get_dispatch_record(&receipt.proposal_id)
        .unwrap()
        .expect("ledger entry after request");
    assert_eq!(record.initiator_slot_id, initiator);
    assert_eq!(record.target_slot_id, target);
    assert!(
        record
            .outcome
            .as_ref()
            .is_some_and(|o| !o.claimed_governed_execution),
        "ledger outcome must not mark LocalCli as governed"
    );

    host.shutdown().unwrap();
}

/// Approve path still uses the ordinary decision chain (smoke).
#[test]
fn approved_dispatch_uses_same_process_chain_and_attributes_local_cli_as_record_only() {
    clear_dispatch_ledger_for_tests();
    let fx = fixture();
    set_default_mode(&fx);
    let host = Arc::new(host_for(&fx));
    register(&host, &fx);

    let prepared = prepare_dispatch(BuildAssignmentInput {
        initiator_slot_id: SlotId::new(),
        target: Some(local_cli_node(SlotId::new())),
        requested_role: None,
        requested_slot_id: None,
        task_text: "print version".into(),
        cwd: Some(".".into()),
    })
    .unwrap();

    let cancel = AtomicBool::new(false);
    let deciding_host = Arc::clone(&host);
    let conversation_id = fx.conversation_id.clone();
    let approver = thread::spawn(move || {
        let proposal_id = wait_live_pending(&deciding_host, &conversation_id);
        deciding_host
            .decide_process_proposal(&conversation_id, &proposal_id, ExecDecision::ApproveOnce)
            .unwrap();
    });

    let prepared_for_ledger = prepared.clone();
    let conversation_for_ledger = fx.conversation_id.clone();
    let receipt = execute_prepared_dispatch(fx.run_id, &prepared, &cancel, |event| {
        if let keeper_runtime_host::GovernedProcessLifecycle::Requested(requested) = event {
            register_dispatch_on_requested(
                &prepared_for_ledger,
                conversation_for_ledger.clone(),
                fx.run_id,
                requested.proposal_id.clone(),
            )
            .unwrap();
        }
        Ok(())
    })
    .unwrap();
    approver.join().unwrap();

    assert!(receipt.started.is_some());
    assert!(matches!(
        receipt.state,
        ExecProposalState::Succeeded | ExecProposalState::Failed
    ));

    let outcome = attribute_dispatch_receipt(&prepared.assignment, &receipt).unwrap();
    assert!(
        !outcome.claimed_governed_execution,
        "LocalCli must remain record-only even after a real process run"
    );
    assert_eq!(outcome.governance, GovernanceCapability::None);

    host.shutdown().unwrap();
}

#[test]
fn prepare_failures_do_not_touch_process_coordinator() {
    clear_dispatch_ledger_for_tests();
    let fx = fixture();
    let host = host_for(&fx);
    register(&host, &fx);

    let id = SlotId::new();
    let err = prepare_dispatch(BuildAssignmentInput {
        initiator_slot_id: id,
        target: Some(local_cli_node(id)),
        requested_role: None,
        requested_slot_id: Some(id),
        task_text: "self".into(),
        cwd: None,
    })
    .unwrap_err();
    assert_eq!(err.code(), "role_dispatch_self_dispatch");

    // No live proposals created by a failed prepare.
    let views = host.list_process_proposals(&fx.conversation_id).unwrap();
    assert!(views.is_empty());
    host.shutdown().unwrap();
}
