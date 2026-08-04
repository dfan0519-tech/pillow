use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId as ProtocolRunId, RunStartedPayload, RuntimeError,
    RuntimeEvent, RuntimeEventKind, RuntimeProbe, RuntimeSession, SendTurn, StartSession,
    ToolCallCompletedPayload, ToolCallOutputPayload, ToolCallStartedPayload, ToolCallStatus,
};
use keeper_runtime_host::{ProviderToolProposal, execute_governed_read};
use serde_json::json;

use crate::conversation_events::RecordingEmitter;

use super::super::service::ConversationService;
use super::support::{seed_project, temp_db};

struct GovernedReadAdapter;

impl AgentRuntimeAdapter for GovernedReadAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        Ok(RuntimeProbe {
            healthy: true,
            adapter_name: "governed-read-test".into(),
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

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        sink.emit(RuntimeEvent::new(
            request.run_id,
            1,
            RuntimeEventKind::RunStarted,
            RunStartedPayload::default(),
            Utc::now(),
        )?)?;
        let receipt = execute_governed_read(
            request.run_id,
            ProviderToolProposal {
                call_id: "service-call-1".into(),
                tool_id: "fs.read".into(),
                tool_version: 1,
                arguments: json!({
                    "path": "note.txt",
                    "offset_bytes": 0,
                    "max_bytes": 128
                }),
            },
            &AtomicBool::new(false),
        )
        .map_err(|_| RuntimeError::Internal {
            message: "governed_read_failed".into(),
        })?;
        sink.emit(RuntimeEvent::new(
            request.run_id,
            2,
            RuntimeEventKind::ToolCallStarted,
            ToolCallStartedPayload {
                tool_call_id: receipt.result.call_id().into(),
                name: "fs.read@1".into(),
                summary: Some("Governed project read".into()),
            },
            Utc::now(),
        )?)?;
        sink.emit(RuntimeEvent::new(
            request.run_id,
            3,
            RuntimeEventKind::ToolCallOutput,
            ToolCallOutputPayload {
                tool_call_id: receipt.result.call_id().into(),
                chunk: serde_json::to_string(receipt.result.result()).unwrap(),
            },
            Utc::now(),
        )?)?;
        sink.emit(RuntimeEvent::new(
            request.run_id,
            4,
            RuntimeEventKind::ToolCallCompleted,
            ToolCallCompletedPayload {
                tool_call_id: receipt.result.call_id().into(),
                status: ToolCallStatus::Completed,
            },
            Utc::now(),
        )?)?;
        sink.emit(RuntimeEvent::new(
            request.run_id,
            5,
            RuntimeEventKind::TurnCompleted,
            keeper_agent_protocol::TurnCompletedPayload::default(),
            Utc::now(),
        )?)
    }

    fn cancel_turn(&self, _: ProtocolRunId) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[test]
fn service_binds_governed_context_and_restores_the_normalized_receipt() {
    let (_temp, db) = temp_db();
    let project_id = seed_project(&db, "governed-project");
    let project_root = db.parent().unwrap().join("governed-project");
    fs::write(project_root.join("note.txt"), "timeline content").unwrap();
    let emitter = Arc::new(RecordingEmitter::new());
    let service = ConversationService::open_with_adapter(
        &db,
        emitter,
        Arc::new(GovernedReadAdapter),
        keeper_runtime_host::HostConfig::new(1, false, 16),
    )
    .unwrap();
    let conversation = service
        .create_conversation(project_id.0.to_string(), Some("Read".into()))
        .unwrap();

    service
        .send_turn(conversation.id.clone(), "read note".into())
        .unwrap();
    service.drain(Duration::from_secs(5)).unwrap();

    let timeline = service.load_timeline(conversation.id).unwrap();
    let outputs = timeline
        .events
        .iter()
        .filter(|event| event.kind == "tool_call_output")
        .collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    let chunk = outputs[0].payload["chunk"].as_str().unwrap();
    assert!(chunk.contains("timeline content"));
    assert!(chunk.contains("\"tool_id\":\"fs.read\""));
    service.shutdown().unwrap();
}
