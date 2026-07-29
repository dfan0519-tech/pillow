//! Protocol-guard regressions: run id, sequence, terminal, payload boundary.

use std::sync::Arc;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};
use keeper_runtime_host::{CollectingSink, HostConfig, ProtocolGuardSink, RuntimeHost};

/// Malicious adapter that emits wrong run ids, bad sequences, missing terminal,
/// and forbidden payload fields.
struct MaliciousAdapter {
    mode: MaliciousMode,
}

#[derive(Clone, Copy)]
enum MaliciousMode {
    WrongRunId,
    SkipSequence,
    DuplicateSequence,
    MissingTerminal,
    ForbiddenPayload,
}

impl AgentRuntimeAdapter for MaliciousAdapter {
    fn probe(&self) -> Result<keeper_agent_protocol::RuntimeProbe, RuntimeError> {
        Ok(keeper_agent_protocol::RuntimeProbe {
            healthy: true,
            adapter_name: "malicious".into(),
            version: "0".into(),
            detail: None,
        })
    }

    fn start_session(
        &self,
        request: StartSession,
    ) -> Result<keeper_agent_protocol::RuntimeSession, RuntimeError> {
        Ok(keeper_agent_protocol::RuntimeSession {
            session_id: keeper_agent_protocol::RuntimeSessionId::new(),
            conversation_key: request.conversation_key,
        })
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        let run = match self.mode {
            MaliciousMode::WrongRunId => RunId::new(),
            _ => request.run_id,
        };
        match self.mode {
            MaliciousMode::WrongRunId => {
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                )?;
                emit(
                    sink,
                    run,
                    2,
                    RuntimeEventKind::TurnCompleted,
                    serde_json::json!({}),
                )?;
            }
            MaliciousMode::SkipSequence => {
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                )?;
                emit(
                    sink,
                    run,
                    3,
                    RuntimeEventKind::TurnCompleted,
                    serde_json::json!({}),
                )?;
            }
            MaliciousMode::DuplicateSequence => {
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                )?;
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::TurnCompleted,
                    serde_json::json!({}),
                )?;
            }
            MaliciousMode::MissingTerminal => {
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::RunStarted,
                    serde_json::json!({}),
                )?;
            }
            MaliciousMode::ForbiddenPayload => {
                emit(
                    sink,
                    run,
                    1,
                    RuntimeEventKind::AssistantDelta,
                    serde_json::json!({
                        "text": "leaked",
                        "authorization": "Bearer secret-token",
                        "api_key": "sk-test"
                    }),
                )?;
                emit(
                    sink,
                    run,
                    2,
                    RuntimeEventKind::TurnCompleted,
                    serde_json::json!({}),
                )?;
            }
        }
        Ok(())
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        Err(RuntimeError::RunNotFound { run_id })
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

fn emit(
    sink: &mut dyn EventSink,
    run_id: RunId,
    sequence: u64,
    kind: RuntimeEventKind,
    payload: serde_json::Value,
) -> Result<(), RuntimeError> {
    sink.emit(RuntimeEvent {
        run_id,
        sequence,
        kind,
        payload,
        created_at: Utc::now(),
    })
}

fn assert_invalid_event(result: Result<(), RuntimeError>) {
    let err = result.expect_err("expected InvalidEvent");
    assert!(
        matches!(err, RuntimeError::InvalidEvent { .. }),
        "got {err}"
    );
}

fn host_with(mode: MaliciousMode) -> Arc<RuntimeHost> {
    Arc::new(RuntimeHost::new(
        Arc::new(MaliciousAdapter { mode }),
        HostConfig::default(),
    ))
}

#[test]
fn host_rejects_wrong_run_id_from_adapter() {
    let host = host_with(MaliciousMode::WrongRunId);
    let session = host
        .start_session(StartSession {
            conversation_key: "m".into(),
            working_directory: None,
        })
        .unwrap();
    assert_invalid_event(host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "x".into(),
        },
        &mut CollectingSink::default(),
    ));
}

#[test]
fn host_rejects_skipped_and_duplicate_sequences() {
    for mode in [
        MaliciousMode::SkipSequence,
        MaliciousMode::DuplicateSequence,
    ] {
        let host = host_with(mode);
        let session = host
            .start_session(StartSession {
                conversation_key: "m".into(),
                working_directory: None,
            })
            .unwrap();
        assert_invalid_event(host.send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: RunId::new(),
                content: "x".into(),
            },
            &mut CollectingSink::default(),
        ));
    }
}

#[test]
fn host_rejects_missing_terminal_event() {
    let host = host_with(MaliciousMode::MissingTerminal);
    let session = host
        .start_session(StartSession {
            conversation_key: "m".into(),
            working_directory: None,
        })
        .unwrap();
    assert_invalid_event(host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "x".into(),
        },
        &mut CollectingSink::default(),
    ));
}

#[test]
fn host_rejects_forbidden_provider_and_credential_payload_fields() {
    let host = host_with(MaliciousMode::ForbiddenPayload);
    let session = host
        .start_session(StartSession {
            conversation_key: "m".into(),
            working_directory: None,
        })
        .unwrap();
    let mut sink = CollectingSink::default();
    assert_invalid_event(host.send_turn(
        SendTurn {
            session_id: session.session_id,
            run_id: RunId::new(),
            content: "x".into(),
        },
        &mut sink,
    ));
    assert!(
        sink.events.is_empty(),
        "forbidden payload must not cross the boundary"
    );
}

#[test]
fn protocol_guard_finish_requires_exactly_one_terminal() {
    let run = RunId::new();
    let mut collect = CollectingSink::default();
    let mut guard = ProtocolGuardSink::new(run, &mut collect);
    emit(
        &mut guard,
        run,
        1,
        RuntimeEventKind::RunStarted,
        serde_json::json!({}),
    )
    .unwrap();
    assert!(guard.finish().is_err());
    emit(
        &mut guard,
        run,
        2,
        RuntimeEventKind::TurnCompleted,
        serde_json::json!({}),
    )
    .unwrap();
    guard.finish().unwrap();
}
