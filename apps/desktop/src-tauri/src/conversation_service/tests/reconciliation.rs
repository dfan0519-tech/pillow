use std::path::Path;

use keeper_agent_protocol::{
    RunId as ProtocolRunId, ToolActiveContext, ToolCallCompletedPayload, ToolCallOutputPayload,
    ToolCallStartedPayload, ToolCallStatus, ToolExecutionState, ToolProposal, ToolResult,
    canonical_arguments_digest, validate_payload,
};
use keeper_session::{
    ConversationRepository, NewRun, NewRunEvent, RunId, RunStatus, RuntimeEventKind,
};
use keeper_storage_sqlite::SqliteToolCallRepository;
use serde::Serialize;
use serde_json::{Value, json};

use super::support::{open_service, seed_project, temp_db};

struct ReceiptFixture {
    proposal: keeper_agent_protocol::ValidatedToolProposal,
    result_json: String,
}

fn create_run(
    path: &Path,
) -> (
    super::super::service::ConversationService,
    String,
    String,
    RunId,
) {
    let project_id = seed_project(path, "receipt-project");
    let service = open_service(path);
    let conversation = service
        .create_conversation(project_id.0.to_string(), Some("Receipts".into()))
        .unwrap();
    let run = service
        .repository()
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(
                uuid::Uuid::parse_str(&conversation.id).unwrap(),
            ),
            user_message: "read README".into(),
        })
        .unwrap();
    (service, project_id.0.to_string(), conversation.id, run.id)
}

fn persist_succeeded_receipt(
    path: &Path,
    project_id: &str,
    conversation_id: &str,
    run_id: RunId,
    call_id: &str,
) -> ReceiptFixture {
    let context = ToolActiveContext::new(
        project_id.to_owned(),
        conversation_id.to_owned(),
        ProtocolRunId(run_id.0),
    )
    .unwrap();
    let arguments = json!({"path": "README.md", "offset_bytes": 0, "max_bytes": 128});
    let proposal = ToolProposal {
        project_id: context.project_id().into(),
        conversation_id: context.conversation_id().into(),
        run_id: context.run_id(),
        call_id: call_id.into(),
        tool_id: "fs.read".into(),
        tool_version: 1,
        arguments_digest: canonical_arguments_digest(&arguments).unwrap(),
        arguments,
    }
    .validate_for_context(&context)
    .unwrap();
    let result = ToolResult {
        project_id: proposal.project_id().into(),
        conversation_id: proposal.conversation_id().into(),
        run_id: proposal.run_id(),
        call_id: proposal.call_id().into(),
        tool_id: proposal.tool_id().into(),
        tool_version: proposal.tool_version(),
        proposal_fingerprint: proposal.fingerprint().clone(),
        arguments_digest: proposal.normalized_arguments_digest().clone(),
        state: ToolExecutionState::Succeeded,
        output: Some(json!({
            "path": "README.md",
            "content_kind": "utf8",
            "content": "durable content",
            "bytes_returned": 15,
            "truncated": false
        })),
        error: None,
    }
    .validate_for_proposal(&context, &proposal)
    .unwrap();
    let store = SqliteToolCallRepository::open(path).unwrap();
    store.reserve(&proposal).unwrap();
    store
        .transition(&proposal, ToolExecutionState::Validated, None)
        .unwrap();
    store
        .transition(&proposal, ToolExecutionState::Executing, None)
        .unwrap();
    let terminal = store
        .transition(&proposal, ToolExecutionState::Succeeded, Some(&result))
        .unwrap();
    ReceiptFixture {
        proposal,
        result_json: terminal.result_json.unwrap(),
    }
}

fn payload_json<T: Serialize>(kind: keeper_agent_protocol::RuntimeEventKind, payload: T) -> String {
    let value = serde_json::to_value(payload).unwrap();
    validate_payload(kind, &value).unwrap();
    serde_json::to_string(&value).unwrap()
}

fn expected_payloads(call_id: &str, result_json: &str) -> [(RuntimeEventKind, String); 3] {
    [
        (
            RuntimeEventKind::ToolCallStarted,
            payload_json(
                keeper_agent_protocol::RuntimeEventKind::ToolCallStarted,
                ToolCallStartedPayload {
                    tool_call_id: call_id.into(),
                    name: "fs.read@1".into(),
                    summary: Some("Governed project read".into()),
                },
            ),
        ),
        (
            RuntimeEventKind::ToolCallOutput,
            payload_json(
                keeper_agent_protocol::RuntimeEventKind::ToolCallOutput,
                ToolCallOutputPayload {
                    tool_call_id: call_id.into(),
                    chunk: result_json.into(),
                },
            ),
        ),
        (
            RuntimeEventKind::ToolCallCompleted,
            payload_json(
                keeper_agent_protocol::RuntimeEventKind::ToolCallCompleted,
                ToolCallCompletedPayload {
                    tool_call_id: call_id.into(),
                    status: ToolCallStatus::Completed,
                },
            ),
        ),
    ]
}

fn append_prefix(
    service: &super::super::service::ConversationService,
    run_id: RunId,
    expected: &[(RuntimeEventKind, String); 3],
    length: usize,
) {
    for (sequence, (kind, payload_json)) in expected.iter().take(length).enumerate() {
        service
            .repository()
            .append_event(NewRunEvent {
                run_id,
                sequence: sequence as u64,
                kind: *kind,
                payload_json: payload_json.clone(),
            })
            .unwrap();
    }
}

fn assert_one_lifecycle_each(
    events: &[crate::conversation_events::RuntimeEventDto],
    call_id: &str,
) {
    for kind in [
        "tool_call_started",
        "tool_call_output",
        "tool_call_completed",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|event| {
                    event.kind == kind
                        && event.payload["tool_call_id"] == Value::String(call_id.into())
                })
                .count(),
            1,
            "{kind}"
        );
    }
}

#[test]
fn zero_event_receipt_is_repaired_once_across_reopen_without_changing_ledger_history() {
    let (_temp, db) = temp_db();
    let (service, project_id, conversation_id, run_id) = create_run(&db);
    let receipt =
        persist_succeeded_receipt(&db, &project_id, &conversation_id, run_id, "repair-zero");
    let store = SqliteToolCallRepository::open(&db).unwrap();
    let before_history = store.history(&receipt.proposal).unwrap();

    assert_eq!(service.recover_inflight().unwrap(), 1);
    let timeline = service.load_timeline(conversation_id.clone()).unwrap();
    assert_one_lifecycle_each(&timeline.events, "repair-zero");
    let output = timeline
        .events
        .iter()
        .find(|event| event.kind == "tool_call_output")
        .unwrap();
    assert_eq!(output.payload["chunk"], receipt.result_json);
    assert_eq!(timeline.runs[0].status, RunStatus::Interrupted.as_str());
    assert_eq!(store.history(&receipt.proposal).unwrap(), before_history);
    service.shutdown().unwrap();
    drop(service);

    let reopened = open_service(&db);
    assert_eq!(reopened.recover_inflight().unwrap(), 0);
    let timeline = reopened.load_timeline(conversation_id).unwrap();
    assert_eq!(timeline.events.len(), 3);
    assert_one_lifecycle_each(&timeline.events, "repair-zero");
    assert_eq!(store.history(&receipt.proposal).unwrap(), before_history);
    reopened.shutdown().unwrap();
}

#[test]
fn partial_started_and_started_output_prefixes_only_append_the_missing_suffix() {
    for prefix_length in [1, 2] {
        let (_temp, db) = temp_db();
        let (service, project_id, conversation_id, run_id) = create_run(&db);
        let call_id = format!("repair-prefix-{prefix_length}");
        let receipt =
            persist_succeeded_receipt(&db, &project_id, &conversation_id, run_id, &call_id);
        let expected = expected_payloads(&call_id, &receipt.result_json);
        append_prefix(&service, run_id, &expected, prefix_length);

        service.recover_inflight().unwrap();

        let timeline = service.load_timeline(conversation_id).unwrap();
        assert_eq!(timeline.events.len(), 3);
        assert_one_lifecycle_each(&timeline.events, &call_id);
        for (event, (kind, payload_json)) in timeline.events.iter().zip(expected.iter()) {
            assert_eq!(event.kind, kind.as_str());
            assert_eq!(
                event.payload,
                serde_json::from_str::<Value>(payload_json).unwrap()
            );
        }
        service.shutdown().unwrap();
    }
}

#[test]
fn non_prefix_tool_projection_fails_closed_without_appending_or_overwriting() {
    let (_temp, db) = temp_db();
    let (service, project_id, conversation_id, run_id) = create_run(&db);
    let receipt = persist_succeeded_receipt(
        &db,
        &project_id,
        &conversation_id,
        run_id,
        "repair-conflict",
    );
    let expected = expected_payloads("repair-conflict", &receipt.result_json);
    service
        .repository()
        .append_event(NewRunEvent {
            run_id,
            sequence: 0,
            kind: expected[1].0,
            payload_json: expected[1].1.clone(),
        })
        .unwrap();

    let error = service.recover_inflight().unwrap_err();

    assert_eq!(error.code, "tool_receipt_reconciliation_failed");
    let timeline = service.load_timeline(conversation_id).unwrap();
    assert_eq!(timeline.events.len(), 1);
    assert_eq!(timeline.events[0].kind, "tool_call_output");
    assert_eq!(
        timeline.events[0].payload,
        serde_json::from_str::<Value>(&expected[1].1).unwrap()
    );
    service.shutdown().unwrap();
}
