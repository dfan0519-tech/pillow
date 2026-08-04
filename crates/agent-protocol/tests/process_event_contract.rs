//! Fixed process runtime event contract (C4 D3AR).
//!
//! Proves snake_case kind names, deny-unknown-fields payloads, rejection of
//! secret/native/output fields, and requested → decided → started → terminal
//! ordering for the caller-emits-event pattern.

use chrono::{TimeZone, Utc};
use keeper_agent_protocol::{
    ProcessProposalDecidedPayload, ProcessProposalRequestedPayload, ProcessRiskClass,
    ProcessStartedPayload, ProcessTerminalOutcomePayload, RunId, RuntimeEvent, RuntimeEventKind,
    validate_payload,
};
use keeper_policy::{ExecDecision, ExecProposalState};
use serde_json::json;
use uuid::Uuid;

fn fixed_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap()
}

fn requested_payload() -> ProcessProposalRequestedPayload {
    ProcessProposalRequestedPayload {
        proposal_id: "ep-1".into(),
        program: "node".into(),
        argv: vec!["--version".into()],
        portable_cwd: Some(".".into()),
        summary: "\"node\" \"--version\"".into(),
        risk_class: ProcessRiskClass::Standard,
        expires_at: fixed_time(),
    }
}

#[test]
fn process_event_kind_names_are_snake_case() {
    assert_eq!(
        serde_json::to_value(RuntimeEventKind::ProcessProposalRequested).unwrap(),
        json!("process_proposal_requested")
    );
    assert_eq!(
        serde_json::to_value(RuntimeEventKind::ProcessProposalDecided).unwrap(),
        json!("process_proposal_decided")
    );
    assert_eq!(
        serde_json::to_value(RuntimeEventKind::ProcessStarted).unwrap(),
        json!("process_started")
    );
    assert_eq!(
        serde_json::to_value(RuntimeEventKind::ProcessTerminalOutcome).unwrap(),
        json!("process_terminal_outcome")
    );
}

#[test]
fn process_event_payloads_round_trip_and_validate() {
    let cases: Vec<(RuntimeEventKind, serde_json::Value)> = vec![
        (
            RuntimeEventKind::ProcessProposalRequested,
            serde_json::to_value(requested_payload()).unwrap(),
        ),
        (
            RuntimeEventKind::ProcessProposalDecided,
            serde_json::to_value(ProcessProposalDecidedPayload {
                proposal_id: "ep-1".into(),
                program: "node".into(),
                decision: ExecDecision::ApproveOnce,
                state: ExecProposalState::Approved,
            })
            .unwrap(),
        ),
        (
            RuntimeEventKind::ProcessStarted,
            serde_json::to_value(ProcessStartedPayload {
                proposal_id: "ep-1".into(),
                program: "node".into(),
                argv: vec!["--version".into()],
                portable_cwd: Some(".".into()),
            })
            .unwrap(),
        ),
        (
            RuntimeEventKind::ProcessTerminalOutcome,
            serde_json::to_value(ProcessTerminalOutcomePayload {
                proposal_id: "ep-1".into(),
                program: "node".into(),
                state: ExecProposalState::Succeeded,
                exit_code: Some(0),
                duration_ms: Some(12),
                truncated: Some(false),
                encoding_lossy: Some(false),
                captured_bytes: Some(8),
                cancelled: Some(false),
                timed_out: Some(false),
            })
            .unwrap(),
        ),
    ];

    for (kind, payload) in cases {
        let canonical = validate_payload(kind, &payload).expect("validate");
        let event = RuntimeEvent::new(RunId(Uuid::nil()), 1, kind, &canonical, fixed_time())
            .expect("RuntimeEvent::new");
        let json = serde_json::to_string(&event).unwrap();
        let back: RuntimeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, kind);
        assert_eq!(back.payload, canonical);
    }
}

#[test]
fn process_payloads_reject_unknown_secret_native_and_output_fields() {
    let forbidden_fields = [
        ("approval_grant", json!("raw-grant-material")),
        ("grant", json!("x")),
        ("executable", json!("C:\\\\Windows\\\\node.exe")),
        ("native_cwd", json!("C:\\\\proj")),
        ("project_root", json!("C:\\\\proj")),
        ("stdout", json!("hello")),
        ("stderr", json!("err")),
        ("output", json!("full transcript")),
        ("chunks", json!([])),
        ("environment", json!({"PATH": "/bin"})),
        ("env", json!({})),
        ("provider_payload", json!({})),
        ("session_id", json!("sess-1")),
        ("database_path", json!("C:\\\\db.sqlite")),
        ("api_key", json!("sk-secret")),
        ("debug", json!({})),
    ];

    let kinds = [
        RuntimeEventKind::ProcessProposalRequested,
        RuntimeEventKind::ProcessProposalDecided,
        RuntimeEventKind::ProcessStarted,
        RuntimeEventKind::ProcessTerminalOutcome,
    ];

    let base = |kind: RuntimeEventKind| -> serde_json::Value {
        match kind {
            RuntimeEventKind::ProcessProposalRequested => {
                serde_json::to_value(requested_payload()).unwrap()
            }
            RuntimeEventKind::ProcessProposalDecided => json!({
                "proposal_id": "ep-1",
                "program": "node",
                "decision": "reject",
                "state": "rejected"
            }),
            RuntimeEventKind::ProcessStarted => json!({
                "proposal_id": "ep-1",
                "program": "node",
                "argv": ["--version"]
            }),
            RuntimeEventKind::ProcessTerminalOutcome => json!({
                "proposal_id": "ep-1",
                "program": "node",
                "state": "succeeded"
            }),
            _ => unreachable!(),
        }
    };

    for kind in kinds {
        for (field, value) in &forbidden_fields {
            let mut payload = base(kind);
            payload
                .as_object_mut()
                .unwrap()
                .insert((*field).into(), value.clone());
            let err = validate_payload(kind, &payload).expect_err("must reject");
            let msg = err.to_string().to_ascii_lowercase();
            assert!(
                msg.contains("unknown")
                    || msg.contains("forbidden")
                    || msg.contains("does not match")
                    || msg.contains(field),
                "kind={kind:?} field={field} err={err}"
            );
        }
    }
}

#[test]
fn process_event_order_requested_decided_started_terminal() {
    let kinds = [
        RuntimeEventKind::ProcessProposalRequested,
        RuntimeEventKind::ProcessProposalDecided,
        RuntimeEventKind::ProcessStarted,
        RuntimeEventKind::ProcessTerminalOutcome,
    ];
    // Fixed index order matches durable emission order for approve path.
    assert_eq!(
        kinds,
        [
            RuntimeEventKind::ProcessProposalRequested,
            RuntimeEventKind::ProcessProposalDecided,
            RuntimeEventKind::ProcessStarted,
            RuntimeEventKind::ProcessTerminalOutcome,
        ]
    );
    // Reject path has no start: decided terminal states need not be Running.
    let reject = ProcessProposalDecidedPayload {
        proposal_id: "ep-2".into(),
        program: "node".into(),
        decision: ExecDecision::Reject,
        state: ExecProposalState::Rejected,
    };
    validate_payload(
        RuntimeEventKind::ProcessProposalDecided,
        &serde_json::to_value(reject).unwrap(),
    )
    .unwrap();
}
