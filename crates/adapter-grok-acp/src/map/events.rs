//! `session/update` → provider-neutral `RuntimeEvent` construction.

use chrono::{DateTime, Utc};
use keeper_agent_protocol::{
    AssistantDeltaPayload, PlanStep, PlanUpdatedPayload, ReasoningSummaryDeltaPayload, RunId,
    RuntimeError, RuntimeEvent, RuntimeEventKind, ToolCallCompletedPayload, ToolCallOutputPayload,
    ToolCallStartedPayload, ToolCallStatus,
};
use serde_json::Value;

/// One mapped stream event (or a drop).
#[derive(Debug)]
pub enum MappedEvent {
    Emit(RuntimeEvent),
    /// Tool observation only (for permission registry); may also emit.
    ToolObserve {
        #[allow(dead_code)]
        update: Value,
        emit: Option<RuntimeEvent>,
    },
    Drop,
}

/// Map a standard `session/update` payload into zero or more neutral events.
///
/// Unknown update kinds are dropped. Raw provider fields never enter payloads.
pub fn map_session_update(
    run_id: RunId,
    sequence: u64,
    update: &Value,
    now: DateTime<Utc>,
) -> Result<MappedEvent, RuntimeError> {
    let kind = update
        .get("sessionUpdate")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match kind {
        "agent_message_chunk" => {
            let text = content_text(update.get("content"));
            if text.is_empty() {
                return Ok(MappedEvent::Drop);
            }
            let event = RuntimeEvent::new(
                run_id,
                sequence,
                RuntimeEventKind::AssistantDelta,
                AssistantDeltaPayload { text },
                now,
            )?;
            Ok(MappedEvent::Emit(event))
        }
        "agent_thought_chunk" => {
            let text = content_text(update.get("content"));
            if text.is_empty() {
                return Ok(MappedEvent::Drop);
            }
            let event = RuntimeEvent::new(
                run_id,
                sequence,
                RuntimeEventKind::ReasoningSummaryDelta,
                ReasoningSummaryDeltaPayload { text },
                now,
            )?;
            Ok(MappedEvent::Emit(event))
        }
        "plan" => map_plan(run_id, sequence, update, now),
        "tool_call" => map_tool_call(run_id, sequence, update, now),
        "tool_call_update" => map_tool_call_update(run_id, sequence, update, now),
        // Intentionally ignored in B2.
        "user_message_chunk"
        | "available_commands_update"
        | "current_mode_update"
        | "session_info_update" => Ok(MappedEvent::Drop),
        _ => Ok(MappedEvent::Drop),
    }
}

fn map_plan(
    run_id: RunId,
    sequence: u64,
    update: &Value,
    now: DateTime<Utc>,
) -> Result<MappedEvent, RuntimeError> {
    let entries = update
        .get("entries")
        .or_else(|| update.get("steps"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut steps = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step-{idx}"));
        let title = entry
            .get("title")
            .or_else(|| entry.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("step")
            .chars()
            .take(200)
            .collect();
        let status = entry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending")
            .to_string();
        steps.push(PlanStep { id, title, status });
    }
    if steps.is_empty() {
        return Ok(MappedEvent::Drop);
    }
    let event = RuntimeEvent::new(
        run_id,
        sequence,
        RuntimeEventKind::PlanUpdated,
        PlanUpdatedPayload { steps },
        now,
    )?;
    Ok(MappedEvent::Emit(event))
}

fn map_tool_call(
    run_id: RunId,
    sequence: u64,
    update: &Value,
    now: DateTime<Utc>,
) -> Result<MappedEvent, RuntimeError> {
    let tool_call_id = update
        .get("toolCallId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if tool_call_id.is_empty() || tool_call_id.len() > 200 {
        return Ok(MappedEvent::Drop);
    }
    let name = update
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| update.get("kind").and_then(|v| v.as_str()))
        .unwrap_or("tool")
        .chars()
        .take(120)
        .collect::<String>();
    let summary = update
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.chars().take(200).collect());
    let event = RuntimeEvent::new(
        run_id,
        sequence,
        RuntimeEventKind::ToolCallStarted,
        ToolCallStartedPayload {
            tool_call_id,
            name,
            summary,
        },
        now,
    )?;
    Ok(MappedEvent::ToolObserve {
        update: update.clone(),
        emit: Some(event),
    })
}

fn map_tool_call_update(
    run_id: RunId,
    sequence: u64,
    update: &Value,
    now: DateTime<Utc>,
) -> Result<MappedEvent, RuntimeError> {
    let tool_call_id = update
        .get("toolCallId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if tool_call_id.is_empty() || tool_call_id.len() > 200 {
        return Ok(MappedEvent::Drop);
    }

    let status = update
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_ascii_lowercase());

    if let Some(status) = status.as_deref()
        && matches!(status, "completed" | "failed" | "cancelled")
    {
        let mapped = match status {
            "completed" => ToolCallStatus::Completed,
            "cancelled" => ToolCallStatus::Cancelled,
            _ => ToolCallStatus::Failed,
        };
        let event = RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::ToolCallCompleted,
            ToolCallCompletedPayload {
                tool_call_id,
                status: mapped,
            },
            now,
        )?;
        return Ok(MappedEvent::ToolObserve {
            update: update.clone(),
            emit: Some(event),
        });
    }

    if let Some(chunk) = sanitized_tool_content(update) {
        let event = RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::ToolCallOutput,
            ToolCallOutputPayload {
                tool_call_id,
                chunk,
            },
            now,
        )?;
        return Ok(MappedEvent::ToolObserve {
            update: update.clone(),
            emit: Some(event),
        });
    }

    Ok(MappedEvent::ToolObserve {
        update: update.clone(),
        emit: None,
    })
}

fn content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
        return text.to_string();
    }
    if content.get("type").and_then(|v| v.as_str()) == Some("text") {
        return content
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }
    String::new()
}

/// Extract a short sanitized tool output chunk; never include rawOutput blobs.
fn sanitized_tool_content(update: &Value) -> Option<String> {
    let content = update.get("content")?;
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if item.get("type").and_then(|v| v.as_str()) == Some("content") {
                let text = content_text(item.get("content"));
                if !text.is_empty() {
                    parts.push(text);
                }
            } else if item.get("type").and_then(|v| v.as_str()) == Some("diff") {
                let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                // Relative-looking basename only; no full absolute dump.
                let name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file");
                parts.push(format!("diff:{name}"));
            }
        }
        if parts.is_empty() {
            return None;
        }
        let joined = parts.join("\n");
        return Some(joined.chars().take(500).collect());
    }
    let text = content_text(Some(content));
    if text.is_empty() {
        None
    } else {
        Some(text.chars().take(500).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_agent_protocol::validate_payload;
    use serde_json::json;
    use uuid::Uuid;

    fn rid() -> RunId {
        RunId(Uuid::nil())
    }

    #[test]
    fn maps_message_and_thought() {
        let m = map_session_update(
            rid(),
            1,
            &json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}),
            Utc::now(),
        )
        .unwrap();
        match m {
            MappedEvent::Emit(e) => {
                assert_eq!(e.kind, RuntimeEventKind::AssistantDelta);
                validate_payload(e.kind, &e.payload).unwrap();
            }
            _ => panic!("expected emit"),
        }
        let t = map_session_update(
            rid(),
            2,
            &json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"think"}}),
            Utc::now(),
        )
        .unwrap();
        match t {
            MappedEvent::Emit(e) => {
                assert_eq!(e.kind, RuntimeEventKind::ReasoningSummaryDelta);
            }
            _ => panic!("expected emit"),
        }
    }

    #[test]
    fn drops_unknown_and_user_chunks() {
        let d = map_session_update(
            rid(),
            1,
            &json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"u"}}),
            Utc::now(),
        )
        .unwrap();
        assert!(matches!(d, MappedEvent::Drop));
        let d2 = map_session_update(
            rid(),
            1,
            &json!({"sessionUpdate":"totally_unknown"}),
            Utc::now(),
        )
        .unwrap();
        assert!(matches!(d2, MappedEvent::Drop));
    }

    #[test]
    fn tool_payloads_have_no_raw_provider_fields() {
        let m = map_session_update(
            rid(),
            1,
            &json!({
                "sessionUpdate":"tool_call",
                "toolCallId":"tc-1",
                "title":"read_file",
                "rawInput":{"target_file":"C:\\\\secret\\\\auth.json"},
                "_meta":{"x.ai/tool":{"kind":"read"}}
            }),
            Utc::now(),
        )
        .unwrap();
        match m {
            MappedEvent::ToolObserve { emit: Some(e), .. } => {
                let s = e.payload.to_string().to_ascii_lowercase();
                assert!(!s.contains("rawinput"));
                assert!(!s.contains("auth.json"));
                assert!(!s.contains("x.ai"));
                validate_payload(e.kind, &e.payload).unwrap();
            }
            _ => panic!("expected tool start"),
        }
    }
}
