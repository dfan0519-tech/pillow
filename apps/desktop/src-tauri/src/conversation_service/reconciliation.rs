use chrono::Utc;
use keeper_agent_protocol::{
    RunId as ProtocolRunId, RuntimeEvent, RuntimeEventKind as ProtocolEventKind, ToolActiveContext,
    ToolCallCompletedPayload, ToolCallOutputPayload, ToolCallStartedPayload, ToolCallStatus,
    ToolExecutionState, ToolProposal, ToolResult, validate_payload,
};
use keeper_session::{
    ConversationId, ConversationRepository, NewRunEvent, RunId, RuntimeEventKind,
};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteToolCallRepository, StoredToolCall,
};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

struct ExpectedEvent {
    protocol_kind: ProtocolEventKind,
    domain_kind: RuntimeEventKind,
    payload: Value,
}

pub(super) fn reconcile_terminal_tool_receipts(
    conversations: &SqliteConversationRepository,
    tool_calls: &SqliteToolCallRepository,
) -> Result<usize, String> {
    let receipts = tool_calls
        .list_terminal_calls()
        .map_err(|_| "terminal tool receipt query failed".to_owned())?;
    let mut appended = 0;
    for receipt in receipts {
        appended += reconcile_receipt(conversations, &receipt)?;
    }
    Ok(appended)
}

fn reconcile_receipt(
    conversations: &SqliteConversationRepository,
    receipt: &StoredToolCall,
) -> Result<usize, String> {
    let protocol_run_id = ProtocolRunId(
        Uuid::parse_str(&receipt.run_id)
            .map_err(|_| "terminal tool receipt run identity is invalid".to_owned())?,
    );
    let domain_run_id = RunId(protocol_run_id.0);
    let conversation_id = ConversationId(
        Uuid::parse_str(&receipt.conversation_id)
            .map_err(|_| "terminal tool receipt conversation identity is invalid".to_owned())?,
    );
    validate_receipt(receipt, protocol_run_id)?;
    let result_json = receipt
        .result_json
        .as_deref()
        .ok_or_else(|| "terminal tool receipt is missing its result".to_owned())?;
    let expected = expected_events(receipt, protocol_run_id, result_json)?;
    let timeline = conversations
        .load_timeline(conversation_id)
        .map_err(|_| "terminal tool receipt timeline load failed".to_owned())?;
    if timeline.conversation.project_id.0.to_string() != receipt.project_id
        || !timeline.runs.iter().any(|run| run.id == domain_run_id)
    {
        return Err("terminal tool receipt context does not match its timeline".to_owned());
    }

    let run_events = timeline
        .events
        .iter()
        .filter(|event| event.run_id == domain_run_id)
        .collect::<Vec<_>>();
    let mut projected = Vec::new();
    for event in &run_events {
        let Some(protocol_kind) = protocol_tool_kind(event.kind) else {
            continue;
        };
        let payload: Value = serde_json::from_str(&event.payload_json)
            .map_err(|_| "tool lifecycle payload is not valid JSON".to_owned())?;
        validate_payload(protocol_kind, &payload)
            .map_err(|_| "tool lifecycle payload failed protocol validation".to_owned())?;
        if payload.get("tool_call_id").and_then(Value::as_str) == Some(receipt.call_id.as_str()) {
            projected.push((*event, payload));
        }
    }

    if projected.len() > expected.len() {
        return Err("tool lifecycle projection is not a valid prefix".to_owned());
    }
    for ((event, payload), expected_event) in projected.iter().zip(expected.iter()) {
        if event.kind != expected_event.domain_kind || payload != &expected_event.payload {
            return Err("tool lifecycle projection conflicts with its terminal receipt".to_owned());
        }
    }
    if projected
        .windows(2)
        .any(|events| events[1].0.sequence != events[0].0.sequence + 1)
    {
        return Err("tool lifecycle projection is not contiguous".to_owned());
    }
    if projected.len() < expected.len()
        && let Some((last, _)) = projected.last()
        && run_events.last().map(|event| event.sequence) != Some(last.sequence)
    {
        return Err("runtime events crossed an incomplete tool lifecycle".to_owned());
    }

    let mut next_sequence = run_events
        .last()
        .map_or(0, |event| event.sequence.saturating_add(1));
    let mut appended = 0;
    for missing in expected.iter().skip(projected.len()) {
        let protocol_sequence = next_sequence
            .checked_add(1)
            .ok_or_else(|| "tool lifecycle sequence overflow".to_owned())?;
        let event = RuntimeEvent::new(
            protocol_run_id,
            protocol_sequence,
            missing.protocol_kind,
            missing.payload.clone(),
            Utc::now(),
        )
        .and_then(|event| event.validate())
        .map_err(|_| "reconciled tool lifecycle event failed protocol validation".to_owned())?;
        validate_payload(event.kind, &event.payload)
            .map_err(|_| "reconciled tool lifecycle payload failed validation".to_owned())?;
        let payload_json = serde_json::to_string(&event.payload)
            .map_err(|_| "reconciled tool lifecycle payload serialization failed".to_owned())?;
        conversations
            .append_event_with_status(
                NewRunEvent {
                    run_id: domain_run_id,
                    sequence: next_sequence,
                    kind: missing.domain_kind,
                    payload_json,
                },
                None,
            )
            .map_err(|_| "reconciled tool lifecycle event persistence failed".to_owned())?;
        next_sequence = next_sequence
            .checked_add(1)
            .ok_or_else(|| "tool lifecycle sequence overflow".to_owned())?;
        appended += 1;
    }
    Ok(appended)
}

fn validate_receipt(receipt: &StoredToolCall, run_id: ProtocolRunId) -> Result<(), String> {
    let context = ToolActiveContext::new(
        receipt.project_id.clone(),
        receipt.conversation_id.clone(),
        run_id,
    )
    .map_err(|_| "terminal tool receipt context is invalid".to_owned())?;
    let proposal = ToolProposal::parse_str(&receipt.proposal_json)
        .and_then(|proposal| proposal.validate_for_context(&context))
        .map_err(|_| "terminal tool receipt proposal is invalid".to_owned())?;
    if proposal.fingerprint().as_str() != receipt.proposal_fingerprint
        || proposal.call_id() != receipt.call_id
        || proposal.tool_id() != receipt.tool_id
        || proposal.tool_version() != receipt.tool_version
        || proposal.normalized_arguments_digest().as_str() != receipt.arguments_digest
    {
        return Err("terminal tool receipt proposal identity conflicts".to_owned());
    }
    let result_json = receipt
        .result_json
        .as_deref()
        .ok_or_else(|| "terminal tool receipt is missing its result".to_owned())?;
    let result = ToolResult::parse_str(result_json)
        .and_then(|result| result.validate_for_proposal(&context, &proposal))
        .map_err(|_| "terminal tool receipt result is invalid".to_owned())?;
    if result.state() != receipt.state
        || serde_json::to_string(result.result())
            .map_err(|_| "terminal tool receipt result serialization failed".to_owned())?
            != result_json
    {
        return Err("terminal tool receipt result is not canonical".to_owned());
    }
    Ok(())
}

fn expected_events(
    receipt: &StoredToolCall,
    run_id: ProtocolRunId,
    result_json: &str,
) -> Result<[ExpectedEvent; 3], String> {
    let completed_status = match receipt.state {
        ToolExecutionState::Succeeded => ToolCallStatus::Completed,
        ToolExecutionState::Failed => ToolCallStatus::Failed,
        ToolExecutionState::Cancelled => ToolCallStatus::Cancelled,
        _ => return Err("non-terminal tool call reached reconciliation".to_owned()),
    };
    Ok([
        expected_event(
            run_id,
            ProtocolEventKind::ToolCallStarted,
            RuntimeEventKind::ToolCallStarted,
            ToolCallStartedPayload {
                tool_call_id: receipt.call_id.clone(),
                name: format!("{}@{}", receipt.tool_id, receipt.tool_version),
                summary: Some("Governed project read".into()),
            },
        )?,
        expected_event(
            run_id,
            ProtocolEventKind::ToolCallOutput,
            RuntimeEventKind::ToolCallOutput,
            ToolCallOutputPayload {
                tool_call_id: receipt.call_id.clone(),
                chunk: result_json.to_owned(),
            },
        )?,
        expected_event(
            run_id,
            ProtocolEventKind::ToolCallCompleted,
            RuntimeEventKind::ToolCallCompleted,
            ToolCallCompletedPayload {
                tool_call_id: receipt.call_id.clone(),
                status: completed_status,
            },
        )?,
    ])
}

fn expected_event(
    run_id: ProtocolRunId,
    protocol_kind: ProtocolEventKind,
    domain_kind: RuntimeEventKind,
    payload: impl Serialize,
) -> Result<ExpectedEvent, String> {
    let event = RuntimeEvent::new(run_id, 1, protocol_kind, payload, Utc::now())
        .and_then(|event| event.validate())
        .map_err(|_| "terminal tool receipt produced an invalid lifecycle payload".to_owned())?;
    validate_payload(event.kind, &event.payload)
        .map_err(|_| "terminal tool receipt lifecycle payload failed validation".to_owned())?;
    Ok(ExpectedEvent {
        protocol_kind,
        domain_kind,
        payload: event.payload,
    })
}

fn protocol_tool_kind(kind: RuntimeEventKind) -> Option<ProtocolEventKind> {
    match kind {
        RuntimeEventKind::ToolCallStarted => Some(ProtocolEventKind::ToolCallStarted),
        RuntimeEventKind::ToolCallOutput => Some(ProtocolEventKind::ToolCallOutput),
        RuntimeEventKind::ToolCallCompleted => Some(ProtocolEventKind::ToolCallCompleted),
        _ => None,
    }
}
