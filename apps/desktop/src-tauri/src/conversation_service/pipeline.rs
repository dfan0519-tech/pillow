//! Pure runtime-event pipeline: validate → persist → commit → emit.

use std::sync::Arc;
use std::time::Duration;

use keeper_agent_protocol::{
    RuntimeEvent, RuntimeEventKind as ProtocolEventKind, validate_payload,
};
use keeper_runtime_host::SpawnedTurn;
use keeper_session::{ConversationId, NewRunEvent, RunId, RuntimeEventKind};
use keeper_storage_sqlite::SqliteConversationRepository;

use crate::conversation_events::{RuntimeEventEmitter, live_runtime_event_dto, status_for_kind};

/// Drain the host delivery queue, persist each event, then join the worker.
pub(super) fn pump_turn(
    repository: Arc<SqliteConversationRepository>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    conversation_id: ConversationId,
    _run_id: RunId,
    spawned: SpawnedTurn,
) {
    let delivery = Arc::clone(spawned.delivery());
    loop {
        match delivery.pop_timeout(Duration::from_millis(50)) {
            Ok(Some(event)) => {
                if let Err(error) =
                    process_runtime_event(&repository, emitter.as_ref(), conversation_id, &event)
                {
                    eprintln!("Keeper IDE conversation pump error: {error}");
                    break;
                }
                if event.is_terminal() {
                    break;
                }
            }
            Ok(None) => {
                if delivery.is_closed() && delivery.is_empty() {
                    // Drain any late events.
                    for event in delivery.drain() {
                        let _ = process_runtime_event(
                            &repository,
                            emitter.as_ref(),
                            conversation_id,
                            &event,
                        );
                    }
                    break;
                }
                if spawned.is_finished() && delivery.is_empty() {
                    break;
                }
            }
            Err(error) => {
                eprintln!("Keeper IDE conversation delivery error: {error}");
                break;
            }
        }
    }

    // Join host worker; Drop of SpawnedTurn also joins if needed.
    let _ = spawned.join();
}

/// Authoritative order for every runtime event:
/// 1. validate
/// 2. persist event + resulting run status in SQLite
/// 3. commit
/// 4. emit to the renderer
///
/// Never emit an event that has not been durably committed.
pub(super) fn process_runtime_event(
    repository: &SqliteConversationRepository,
    emitter: &dyn RuntimeEventEmitter,
    conversation_id: ConversationId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    // 1. Validate
    let validated = event
        .validate()
        .map_err(|error| format!("validate: {error}"))?;
    validate_payload(validated.kind, &validated.payload)
        .map_err(|error| format!("payload: {error}"))?;

    let kind = map_kind(validated.kind)?;
    let domain_sequence = protocol_to_domain_sequence(validated.sequence)?;
    let payload_json = serde_json::to_string(&validated.payload)
        .map_err(|error| format!("payload serialize: {error}"))?;
    let domain_run_id = RunId(validated.run_id.0);
    let target_status = status_for_kind(kind);

    // 2–3. Persist event + resulting run status in one SQLite transaction, then commit.
    let recorded = repository
        .append_event_with_status(
            NewRunEvent {
                run_id: domain_run_id,
                sequence: domain_sequence,
                kind,
                payload_json,
            },
            target_status,
        )
        .map_err(|error| format!("append_event_with_status: {error}"))?;

    // 4. Emit only after durable commit of both event and resulting status.
    let dto = live_runtime_event_dto(
        recorded.event_id.0.to_string(),
        domain_run_id,
        conversation_id,
        validated.sequence,
        kind,
        validated.payload.clone(),
        recorded.created_at,
    );
    emitter
        .emit_runtime_event(&dto)
        .map_err(|error| format!("emit: {error}"))?;
    Ok(())
}

pub(super) fn protocol_to_domain_sequence(protocol_sequence: u64) -> Result<u64, String> {
    protocol_sequence
        .checked_sub(1)
        .ok_or_else(|| format!("protocol sequence {protocol_sequence} is not 1-based"))
}

pub(super) fn map_kind(kind: ProtocolEventKind) -> Result<RuntimeEventKind, String> {
    RuntimeEventKind::parse(kind_str(kind))
        .ok_or_else(|| format!("unknown runtime event kind: {kind:?}"))
}

fn kind_str(kind: ProtocolEventKind) -> &'static str {
    match kind {
        ProtocolEventKind::RunStarted => "run_started",
        ProtocolEventKind::AssistantDelta => "assistant_delta",
        ProtocolEventKind::ReasoningSummaryDelta => "reasoning_summary_delta",
        ProtocolEventKind::PlanUpdated => "plan_updated",
        ProtocolEventKind::ToolCallStarted => "tool_call_started",
        ProtocolEventKind::ToolCallOutput => "tool_call_output",
        ProtocolEventKind::ToolCallCompleted => "tool_call_completed",
        ProtocolEventKind::SubagentStarted => "subagent_started",
        ProtocolEventKind::SubagentProgress => "subagent_progress",
        ProtocolEventKind::SubagentCompleted => "subagent_completed",
        ProtocolEventKind::ArtifactProduced => "artifact_produced",
        ProtocolEventKind::TurnCompleted => "turn_completed",
        ProtocolEventKind::TurnFailed => "turn_failed",
        ProtocolEventKind::RunCancelled => "run_cancelled",
        ProtocolEventKind::Heartbeat => "heartbeat",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_agent_protocol::RuntimeEventKind as ProtocolEventKind;

    #[test]
    fn protocol_sequence_is_zero_based_in_domain() {
        assert_eq!(protocol_to_domain_sequence(1).unwrap(), 0);
        assert_eq!(protocol_to_domain_sequence(2).unwrap(), 1);
        assert!(protocol_to_domain_sequence(0).is_err());
    }

    #[test]
    fn map_kind_covers_terminal_and_stream_kinds() {
        assert_eq!(
            map_kind(ProtocolEventKind::RunStarted).unwrap(),
            RuntimeEventKind::RunStarted
        );
        assert_eq!(
            map_kind(ProtocolEventKind::TurnCompleted).unwrap(),
            RuntimeEventKind::TurnCompleted
        );
        assert_eq!(
            map_kind(ProtocolEventKind::RunCancelled).unwrap(),
            RuntimeEventKind::RunCancelled
        );
        assert_eq!(
            map_kind(ProtocolEventKind::AssistantDelta).unwrap(),
            RuntimeEventKind::AssistantDelta
        );
    }
}
