//! Pure runtime-event pipeline: validate → persist → commit → emit.

use std::sync::Arc;
use std::time::Duration;

use keeper_agent_protocol::{
    RuntimeEvent, RuntimeEventKind as ProtocolEventKind, validate_payload,
};
use keeper_runtime_host::SpawnedTurn;
use keeper_session::{
    ConversationId, ConversationRepository, NewRunEvent, RunId, RunStatus, RuntimeEventKind,
    SessionError,
};
use keeper_storage_sqlite::SqliteConversationRepository;

use crate::conversation_events::{RuntimeEventEmitter, live_runtime_event_dto, status_for_kind};

/// Result of draining one host turn into SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PumpOutcome {
    /// Run is durably terminal (`Completed` / `Failed` / `Cancelled` / `Interrupted`).
    DurableTerminal,
    /// Could not prove a durable terminal status; keep the residual active entry
    /// so shutdown/drain fail closed instead of reporting success.
    ResidualNonTerminal,
}

/// Drain the host delivery queue, persist each event, then join the worker.
///
/// Renderer emit failures are non-authoritative: they are logged and ingestion
/// continues so SQLite can still reach the real terminal event. Validation,
/// persistence, delivery, or worker-end-without-terminal failures attempt
/// `Interrupted` without inventing a duplicate terminal event.
pub(super) fn pump_turn(
    repository: Arc<SqliteConversationRepository>,
    emitter: Arc<dyn RuntimeEventEmitter>,
    conversation_id: ConversationId,
    run_id: RunId,
    spawned: SpawnedTurn,
) -> PumpOutcome {
    let delivery = Arc::clone(spawned.delivery());
    let mut authoritative_failure = false;

    loop {
        match delivery.pop_timeout(Duration::from_millis(50)) {
            Ok(Some(event)) => {
                let terminal = event.is_terminal();
                match process_runtime_event(&repository, emitter.as_ref(), conversation_id, &event)
                {
                    Ok(()) => {
                        if terminal {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("Keeper IDE conversation pump error: {error}");
                        authoritative_failure = true;
                        break;
                    }
                }
            }
            Ok(None) => {
                if delivery.is_closed() && delivery.is_empty() {
                    for event in delivery.drain() {
                        let terminal = event.is_terminal();
                        if let Err(error) = process_runtime_event(
                            &repository,
                            emitter.as_ref(),
                            conversation_id,
                            &event,
                        ) {
                            eprintln!("Keeper IDE conversation pump error: {error}");
                            authoritative_failure = true;
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                    break;
                }
                if spawned.is_finished() && delivery.is_empty() {
                    // Worker ended without a further queued event.
                    break;
                }
            }
            Err(error) => {
                eprintln!("Keeper IDE conversation delivery error: {error}");
                authoritative_failure = true;
                break;
            }
        }
    }

    // Join host worker; Drop of SpawnedTurn also joins if needed.
    let _ = spawned.join();

    if authoritative_failure {
        return ensure_interrupted(&repository, run_id);
    }

    // Happy path or quiet worker end: only terminalize if still inflight.
    ensure_durable_terminal(&repository, run_id)
}

/// Authoritative order for every runtime event:
/// 1. validate
/// 2. persist event + resulting run status in SQLite
/// 3. commit
/// 4. emit to the renderer (non-authoritative; failures do not roll back)
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

    // 4. Emit only after durable commit. Emit failure is non-authoritative:
    // durable truth already landed; log and continue so the pump can ingest
    // the real terminal event.
    let dto = live_runtime_event_dto(
        recorded.event_id.0.to_string(),
        domain_run_id,
        conversation_id,
        validated.sequence,
        kind,
        validated.payload.clone(),
        recorded.created_at,
    );
    if let Err(error) = emitter.emit_runtime_event(&dto) {
        eprintln!("Keeper IDE conversation emit error (non-authoritative): {error}");
    }
    Ok(())
}

/// After a pump ends, ensure the run is durably terminal without inventing events.
pub(crate) fn ensure_durable_terminal(
    repository: &SqliteConversationRepository,
    run_id: RunId,
) -> PumpOutcome {
    match repository.transition_run(run_id, RunStatus::Interrupted) {
        Ok(_) => {
            // Worker ended without a terminal runtime event; marked Interrupted.
            PumpOutcome::DurableTerminal
        }
        Err(SessionError::IllegalTransition { from, .. }) if from.is_terminal() => {
            // Already Completed / Failed / Cancelled / Interrupted.
            PumpOutcome::DurableTerminal
        }
        Err(error) => {
            eprintln!(
                "Keeper IDE conversation terminalization failed for run {}: {error}",
                run_id.0
            );
            PumpOutcome::ResidualNonTerminal
        }
    }
}

/// Authoritative failure path: attempt Interrupted; never invent a terminal event.
pub(crate) fn ensure_interrupted(
    repository: &SqliteConversationRepository,
    run_id: RunId,
) -> PumpOutcome {
    ensure_durable_terminal(repository, run_id)
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
