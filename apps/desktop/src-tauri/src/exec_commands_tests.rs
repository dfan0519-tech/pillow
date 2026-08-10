//! Native governed-process command boundary tests.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use keeper_policy::ExecProposalState;
use keeper_project::CreateProject;
use keeper_runtime_host::{
    GovernedProcessBridgeError, LiveProcessProposalView, PermissionRuleAttributionView,
    ProcessOrigin, ProcessOutputView, ProcessRiskClass,
};
use keeper_storage_sqlite::SqliteProjectRepository;

use crate::conversation_events::NoopEmitter;
use crate::conversation_service::exec_service::{
    ProcessOperation, map_process_error, output_dto, proposal_dto,
};
use crate::conversation_service::{ConversationService, ExecOutputDto, ExecProposalDto};

fn registered_conversation(service: &ConversationService, db: &std::path::Path) -> String {
    let project_root = db.parent().unwrap();
    let projects = SqliteProjectRepository::open(db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "exec-command-test".into(),
            root_path: project_root.to_string_lossy().into_owned(),
        })
        .unwrap();
    service
        .create_conversation(project.id.0.to_string(), Some("Exec".into()))
        .unwrap()
        .id
}

#[test]
fn exec_dtos_serialize_renderer_contract_without_authority() {
    let proposal = ExecProposalDto {
        proposal_id: "proposal-1".into(),
        conversation_id: "conversation-1".into(),
        origin: "agent".into(),
        state: "pending_approval".into(),
        program_label: "node".into(),
        argv: vec!["--version".into()],
        portable_cwd: "src".into(),
        risk_class: "standard".into(),
        summary: "Check Node version".into(),
        expires_at: "2026-08-04T00:00:00Z".into(),
        decision_available: true,
        error_code: None,
        rule_subject: "node --version".into(),
        auto_approved_by_rule: None,
    };
    let output = ExecOutputDto {
        proposal_id: "proposal-1".into(),
        state: "succeeded".into(),
        chunks: vec![],
        live_retained: true,
        truncated: false,
        encoding_lossy: false,
        captured_bytes: 0,
        exit_code: Some(0),
        duration_ms: Some(12),
    };

    let proposal_json = serde_json::to_value(proposal).unwrap();
    assert_eq!(proposal_json["proposalId"], "proposal-1");
    assert_eq!(proposal_json["decisionAvailable"], true);
    assert!(proposal_json.get("approvalGrant").is_none());
    assert!(proposal_json.get("grant").is_none());
    assert!(proposal_json.get("errorCode").is_none());
    assert_eq!(proposal_json["ruleSubject"], "node --version");

    let output_json = serde_json::to_value(output).unwrap();
    assert_eq!(output_json["durationMs"], 12);
    assert_eq!(output_json["liveRetained"], true);
    assert!(output_json.get("program").is_none());
}

#[test]
fn output_projection_preserves_host_live_retained_truth() {
    let output = output_dto(ProcessOutputView {
        proposal_id: "proposal-1".into(),
        program: "node".into(),
        argv: vec!["--version".into()],
        state: ExecProposalState::Succeeded,
        chunks: vec![],
        truncated: false,
        encoding_lossy: false,
        captured_bytes: 0,
        exit_code: Some(0),
        duration_ms: Some(12),
        live_retained: false,
    });

    let output_json = serde_json::to_value(output).unwrap();
    assert_eq!(output_json["liveRetained"], false);
    assert!(output_json.get("program").is_none());
    assert!(output_json.get("argv").is_none());
}

#[test]
fn proposal_projection_serializes_portable_project_root_and_relative_cwd() {
    let view = |portable_cwd| LiveProcessProposalView {
        proposal_id: "proposal-1".into(),
        conversation_id: "conversation-1".into(),
        origin: ProcessOrigin::Agent,
        state: ExecProposalState::PendingApproval,
        program: "node".into(),
        argv: vec!["--version".into()],
        portable_cwd,
        risk_class: ProcessRiskClass::Standard,
        summary: "Check Node version".into(),
        expires_at: Utc.with_ymd_and_hms(2026, 8, 4, 0, 0, 0).unwrap(),
        decision_available: true,
        exit_code: None,
        duration_ms: None,
        cancelled: None,
        timed_out: None,
        truncated: None,
        encoding_lossy: None,
        captured_bytes: None,
        rule_subject: "node --version".into(),
        auto_approved_by_rule: Some(PermissionRuleAttributionView {
            prefix: "process".into(),
            pattern: "node --version".into(),
            action: "allow".into(),
        }),
    };

    let root_json = serde_json::to_value(proposal_dto(view(None))).unwrap();
    assert_eq!(root_json["portableCwd"], ".");
    assert_eq!(root_json["ruleSubject"], "node --version");
    assert_eq!(root_json["autoApprovedByRule"]["action"], "allow");

    let relative_json = serde_json::to_value(proposal_dto(view(Some("src/tools".into())))).unwrap();
    assert_eq!(relative_json["portableCwd"], "src/tools");
}

#[test]
fn ownership_rejection_stays_in_public_error_vocabulary() {
    let error = map_process_error(
        GovernedProcessBridgeError::OwnershipMismatch,
        ProcessOperation::Decision,
    );
    assert_eq!(error.code, "process_ownership_mismatch");
}

#[test]
fn decide_rejects_unknown_decision_before_host_access() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let service = ConversationService::open(&db, Arc::new(NoopEmitter), false).unwrap();
    let error = service
        .decide_exec_proposal(
            "00000000-0000-0000-0000-000000000001".into(),
            "proposal-1".into(),
            "approve_session".into(),
        )
        .unwrap_err();
    assert_eq!(error.code, "process_invalid_decision");
}

#[test]
fn output_open_is_unavailable_without_host_retained_output() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let service = ConversationService::open(&db, Arc::new(NoopEmitter), false).unwrap();
    let conversation_id = registered_conversation(&service, &db);
    let error = service
        .open_exec_output(conversation_id, "proposal-1".into())
        .unwrap_err();
    assert_eq!(error.code, "process_output_unavailable");
}

#[test]
fn desktop_invoke_handler_registers_exact_exec_contract_commands() {
    let source = include_str!("lib.rs");
    let handler = source
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("desktop invoke handler must exist");
    for command in [
        "list_exec_proposals",
        "decide_exec_proposal",
        "open_exec_output",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}
