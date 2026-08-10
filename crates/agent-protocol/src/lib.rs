//! Provider-neutral agent runtime protocol.
//!
//! Adapters normalize provider wire formats into this vocabulary before any
//! event crosses into the runtime host, domain store, or renderer. Provider
//! payloads must not appear in these types.
//!
//! Operational tool ingress must use the crate-owned
//! [`ToolProposal::parse_str`], [`ToolProposal::parse_bytes`],
//! [`ToolProposal::parse_value`], [`ToolResult::parse_str`],
//! [`ToolResult::parse_bytes`], or [`ToolResult::parse_value`] boundaries.
//! They intentionally replace raw serde diagnostics with fixed
//! [`ToolProtocolError`] values.

mod adapter;
mod error;
mod governed;
mod tool;
mod types;
mod validate;

pub use adapter::{AgentRuntimeAdapter, EventSink};
pub use error::RuntimeError;
pub use governed::{
    GovernedProcessDelivery, GovernedProcessReceipt, GovernedReadDelivery, GovernedReadReceipt,
    GovernedShellOutput, GovernedShellReceipt, GovernedWriteDelivery, GovernedWriteReceipt,
    ProviderProcessProposal, ProviderShellProposal, ProviderToolProposal, ProviderWriteProposal,
};
pub use keeper_policy::WriteProposalState;
pub use tool::{
    ArgumentsDigest, FsListArguments, FsListEntry, FsListOutput, FsReadArguments, FsReadOutput,
    FsSearchArguments, FsSearchMatch, FsSearchOutput, FsWriteArguments, FsWriteOutput,
    PROCESS_ARGV_BYTE_CAP, PROCESS_ARGV_COUNT_CAP, ProcessRunArguments, ProcessRunOutput,
    ToolActiveContext, ToolArgumentSpec, ToolArgumentType, ToolArguments, ToolCallReservation,
    ToolCallReservationDecision, ToolCallReservationKey, ToolContentKind, ToolEntryKind,
    ToolExecutionLimits, ToolExecutionState, ToolId, ToolManifest, ToolProposal,
    ToolProposalFingerprint, ToolProtocolError, ToolProtocolErrorCode, ToolPublicError,
    ToolPublicErrorCode, ToolResult, ValidatedToolProposal, ValidatedToolResult,
    WRITE_CONTENT_BYTE_CAP, WriteMode, WritePublicErrorCode, canonical_arguments_digest,
    fixed_tool_manifests,
};
pub use types::{
    ArtifactProducedPayload, AssistantDeltaPayload, HeartbeatPayload, PermissionRuleAttribution,
    PlanStep, PlanUpdatedPayload, ProcessProposalDecidedPayload, ProcessProposalRequestedPayload,
    ProcessRiskClass, ProcessStartedPayload, ProcessTerminalOutcomePayload,
    ReasoningSummaryDeltaPayload, RunCancelledPayload, RunId, RunStartedPayload, RunStatus,
    RuntimeEvent, RuntimeEventKind, RuntimeProbe, RuntimeSession, RuntimeSessionId, SendTurn,
    StartSession, SubagentCompletedPayload, SubagentProgressPayload, SubagentStartedPayload,
    SubagentStatus, ToolCallCompletedPayload, ToolCallOutputPayload, ToolCallStartedPayload,
    ToolCallStatus, TurnCompletedPayload, TurnFailedPayload, WriteProposalRequestedPayload,
    WriteProposalResolvedPayload,
};
pub use validate::validate_payload;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use keeper_policy::{ExecDecision, ExecProposalState};
    use uuid::Uuid;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 30, 12, 0, 0).unwrap()
    }

    fn sample_payload(kind: RuntimeEventKind) -> serde_json::Value {
        match kind {
            RuntimeEventKind::RunStarted => serde_json::json!({
                "prompt_preview": "hello"
            }),
            RuntimeEventKind::AssistantDelta => serde_json::json!({
                "text": "Hi"
            }),
            RuntimeEventKind::ReasoningSummaryDelta => serde_json::json!({
                "text": "thinking"
            }),
            RuntimeEventKind::PlanUpdated => serde_json::json!({
                "steps": [{"id": "1", "title": "Plan", "status": "active"}]
            }),
            RuntimeEventKind::ToolCallStarted => serde_json::json!({
                "tool_call_id": "tc-1",
                "name": "read_file",
                "summary": "read lib.rs"
            }),
            RuntimeEventKind::ToolCallOutput => serde_json::json!({
                "tool_call_id": "tc-1",
                "chunk": "fn main() {}"
            }),
            RuntimeEventKind::ToolCallCompleted => serde_json::json!({
                "tool_call_id": "tc-1",
                "status": "completed"
            }),
            RuntimeEventKind::SubagentStarted => serde_json::json!({
                "subagent_id": "sa-1",
                "role": "explore"
            }),
            RuntimeEventKind::SubagentProgress => serde_json::json!({
                "subagent_id": "sa-1",
                "summary": "scanning"
            }),
            RuntimeEventKind::SubagentCompleted => serde_json::json!({
                "subagent_id": "sa-1",
                "status": "completed"
            }),
            RuntimeEventKind::ArtifactProduced => serde_json::json!({
                "artifact_id": "art-1",
                "path": "out/report.md",
                "kind": "markdown"
            }),
            RuntimeEventKind::WriteProposalRequested => serde_json::json!({
                "proposal_id": "wp-1",
                "path": "src/example.ts",
                "mode": "create",
                "summary": "Create src/example.ts",
                "expires_at": "2026-08-01T12:10:00Z"
            }),
            RuntimeEventKind::WriteProposalResolved => serde_json::json!({
                "proposal_id": "wp-1",
                "path": "src/example.ts",
                "mode": "create",
                "state": "succeeded"
            }),
            RuntimeEventKind::ProcessProposalRequested => serde_json::json!({
                "proposal_id": "ep-1",
                "program": "node",
                "argv": ["--version"],
                "portable_cwd": ".",
                "summary": "\"node\" \"--version\"",
                "risk_class": "standard",
                "expires_at": "2026-08-01T12:10:00Z"
            }),
            RuntimeEventKind::ProcessProposalDecided => serde_json::json!({
                "proposal_id": "ep-1",
                "program": "node",
                "decision": "approve_once",
                "state": "approved"
            }),
            RuntimeEventKind::ProcessStarted => serde_json::json!({
                "proposal_id": "ep-1",
                "program": "node",
                "argv": ["--version"],
                "portable_cwd": "."
            }),
            RuntimeEventKind::ProcessTerminalOutcome => serde_json::json!({
                "proposal_id": "ep-1",
                "program": "node",
                "state": "succeeded",
                "exit_code": 0,
                "duration_ms": 12,
                "truncated": false,
                "encoding_lossy": false,
                "captured_bytes": 8,
                "cancelled": false,
                "timed_out": false
            }),
            RuntimeEventKind::TurnCompleted => serde_json::json!({
                "summary": "done"
            }),
            RuntimeEventKind::TurnFailed => serde_json::json!({
                "code": "adapter_error",
                "message": "boom"
            }),
            RuntimeEventKind::RunCancelled => serde_json::json!({
                "reason": "user"
            }),
            RuntimeEventKind::Heartbeat => serde_json::json!({
                "tick": 1
            }),
        }
    }

    #[test]
    fn every_event_kind_round_trips_through_serde() {
        let run_id = RunId(Uuid::nil());
        for kind in RuntimeEventKind::all() {
            let event = RuntimeEvent {
                run_id,
                sequence: 1,
                kind: *kind,
                payload: sample_payload(*kind),
                created_at: fixed_time(),
            };
            let json = serde_json::to_string(&event).expect("serialize");
            let back: RuntimeEvent = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, event, "round-trip failed for {kind:?}");
            assert_eq!(back.kind, *kind);
        }
    }

    #[test]
    fn run_status_and_kind_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&RunStatus::Cancelling).unwrap(),
            "\"cancelling\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeEventKind::AssistantDelta).unwrap(),
            "\"assistant_delta\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeEventKind::TurnCompleted).unwrap(),
            "\"turn_completed\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeEventKind::RunCancelled).unwrap(),
            "\"run_cancelled\""
        );
    }

    #[test]
    fn terminal_kinds_are_exactly_completion_failure_and_cancel() {
        let terminals: Vec<_> = RuntimeEventKind::all()
            .iter()
            .copied()
            .filter(|k| k.is_terminal())
            .collect();
        assert_eq!(
            terminals,
            vec![
                RuntimeEventKind::TurnCompleted,
                RuntimeEventKind::TurnFailed,
                RuntimeEventKind::RunCancelled,
            ]
        );
    }

    #[test]
    fn command_types_round_trip() {
        let start = StartSession {
            conversation_key: "conv-1".into(),
            working_directory: Some("/tmp/proj".into()),
        };
        let session = RuntimeSession {
            session_id: RuntimeSessionId(Uuid::nil()),
            conversation_key: "conv-1".into(),
        };
        let turn = SendTurn {
            session_id: session.session_id,
            run_id: RunId(Uuid::nil()),
            content: "do work".into(),
        };
        let probe = RuntimeProbe {
            healthy: true,
            adapter_name: "fake".into(),
            version: "0.1.0".into(),
            detail: Some("deterministic".into()),
        };

        for value in [
            serde_json::to_value(&start).unwrap(),
            serde_json::to_value(&session).unwrap(),
            serde_json::to_value(&turn).unwrap(),
            serde_json::to_value(&probe).unwrap(),
        ] {
            let text = value.to_string();
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed, value);
        }
    }

    #[test]
    fn payload_structs_round_trip_without_provider_fields() {
        let samples: Vec<serde_json::Value> = vec![
            serde_json::to_value(RunStartedPayload {
                prompt_preview: Some("hi".into()),
            })
            .unwrap(),
            serde_json::to_value(AssistantDeltaPayload { text: "a".into() }).unwrap(),
            serde_json::to_value(ReasoningSummaryDeltaPayload { text: "r".into() }).unwrap(),
            serde_json::to_value(PlanUpdatedPayload {
                steps: vec![PlanStep {
                    id: "1".into(),
                    title: "t".into(),
                    status: "pending".into(),
                }],
            })
            .unwrap(),
            serde_json::to_value(ToolCallStartedPayload {
                tool_call_id: "tc".into(),
                name: "read_file".into(),
                summary: None,
            })
            .unwrap(),
            serde_json::to_value(ToolCallOutputPayload {
                tool_call_id: "tc".into(),
                chunk: "out".into(),
            })
            .unwrap(),
            serde_json::to_value(ToolCallCompletedPayload {
                tool_call_id: "tc".into(),
                status: ToolCallStatus::Completed,
            })
            .unwrap(),
            serde_json::to_value(SubagentStartedPayload {
                subagent_id: "s".into(),
                role: "explore".into(),
            })
            .unwrap(),
            serde_json::to_value(SubagentProgressPayload {
                subagent_id: "s".into(),
                summary: "go".into(),
            })
            .unwrap(),
            serde_json::to_value(SubagentCompletedPayload {
                subagent_id: "s".into(),
                status: SubagentStatus::Completed,
            })
            .unwrap(),
            serde_json::to_value(ArtifactProducedPayload {
                artifact_id: "a".into(),
                path: "p".into(),
                kind: "markdown".into(),
            })
            .unwrap(),
            serde_json::to_value(TurnCompletedPayload {
                summary: Some("ok".into()),
            })
            .unwrap(),
            serde_json::to_value(TurnFailedPayload {
                code: "x".into(),
                message: "y".into(),
            })
            .unwrap(),
            serde_json::to_value(RunCancelledPayload {
                reason: Some("user".into()),
            })
            .unwrap(),
            serde_json::to_value(HeartbeatPayload { tick: 3 }).unwrap(),
            serde_json::to_value(WriteProposalRequestedPayload {
                proposal_id: "wp-1".into(),
                path: "src/a.ts".into(),
                mode: WriteMode::Create,
                summary: "Create src/a.ts".into(),
                expires_at: fixed_time(),
            })
            .unwrap(),
            serde_json::to_value(WriteProposalResolvedPayload {
                proposal_id: "wp-1".into(),
                path: "src/a.ts".into(),
                mode: WriteMode::Create,
                state: WriteProposalState::Succeeded,
                error_code: None,
                auto_approved_by_rule: None,
            })
            .unwrap(),
            // Rule attribution is optional and skipped when absent, so a
            // `None`-only sample would never exercise its serialized form. The
            // populated variants below are what keep the timeline's
            // human-vs-rule distinction inside this guard.
            serde_json::to_value(WriteProposalResolvedPayload {
                proposal_id: "wp-2".into(),
                path: "src/b.ts".into(),
                mode: WriteMode::Modify,
                state: WriteProposalState::Succeeded,
                error_code: None,
                auto_approved_by_rule: Some(PermissionRuleAttribution {
                    prefix: "Write".into(),
                    pattern: "src/**".into(),
                    action: "allow".into(),
                }),
            })
            .unwrap(),
            serde_json::to_value(ProcessProposalDecidedPayload {
                proposal_id: "pp-1".into(),
                program: "cargo".into(),
                decision: ExecDecision::ApproveOnce,
                state: ExecProposalState::Approved,
                auto_approved_by_rule: Some(PermissionRuleAttribution {
                    prefix: "Bash".into(),
                    pattern: "cargo*".into(),
                    action: "allow".into(),
                }),
            })
            .unwrap(),
        ];

        for sample in samples {
            let text = serde_json::to_string(&sample).unwrap();
            let back: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(back, sample);
            let dump = text.to_ascii_lowercase();
            assert!(
                !dump.contains("grok")
                    && !dump.contains("acp")
                    && !dump.contains("openai")
                    && !dump.contains("anthropic")
                    && !dump.contains("api_key")
                    && !dump.contains("authorization"),
                "provider-specific field leaked: {text}"
            );
        }
    }

    #[test]
    fn vocabulary_covers_twenty_one_fixed_kinds() {
        // Base 13 + write pair (2) + process quartet (4) + terminals/heartbeat (2) = 21.
        assert_eq!(RuntimeEventKind::all().len(), 21);
    }

    #[test]
    fn write_proposal_event_payloads_reject_secret_fields() {
        let err = validate_payload(
            RuntimeEventKind::WriteProposalRequested,
            &serde_json::json!({
                "proposal_id": "wp-1",
                "path": "src/a.ts",
                "mode": "create",
                "summary": "Create src/a.ts",
                "expires_at": "2026-08-01T12:10:00Z",
                "content": "secret body"
            }),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");

        let err = validate_payload(
            RuntimeEventKind::WriteProposalResolved,
            &serde_json::json!({
                "proposal_id": "wp-1",
                "path": "src/a.ts",
                "mode": "create",
                "state": "succeeded",
                "approval_grant": "not-allowed"
            }),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");
    }
}
