//! Kind-specific payload validation for the provider-neutral boundary.
//!
//! Arbitrary JSON is accepted only after it deserializes into the exact payload
//! struct for the event kind. Unknown fields are rejected (`deny_unknown_fields`
//! on payload types). This keeps provider, credential, and authorization
//! fields from crossing the adapter boundary as protocol payload keys.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::RuntimeError;
use crate::types::{
    ArtifactProducedPayload, AssistantDeltaPayload, HeartbeatPayload, PlanUpdatedPayload,
    ReasoningSummaryDeltaPayload, RunCancelledPayload, RunStartedPayload, RuntimeEventKind,
    SubagentCompletedPayload, SubagentProgressPayload, SubagentStartedPayload,
    ToolCallCompletedPayload, ToolCallOutputPayload, ToolCallStartedPayload, TurnCompletedPayload,
    TurnFailedPayload,
};

/// Forbidden payload object keys (case-insensitive) that must never appear at
/// the protocol boundary, even if a future payload type forgot to deny them.
const FORBIDDEN_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "auth_token",
    "access_token",
    "refresh_token",
    "password",
    "secret",
    "bearer",
    "provider",
    "provider_id",
    "provider_payload",
    "openai",
    "anthropic",
    "grok_api",
    "acp_session",
    "raw_provider",
];

/// Validate and canonicalize a payload for `kind`.
///
/// Returns a re-serialized value produced from the typed payload so only
/// known, provider-neutral fields remain.
pub fn validate_payload(kind: RuntimeEventKind, payload: &Value) -> Result<Value, RuntimeError> {
    reject_forbidden_keys(payload)?;
    match kind {
        RuntimeEventKind::RunStarted => decode::<RunStartedPayload>(payload),
        RuntimeEventKind::AssistantDelta => decode::<AssistantDeltaPayload>(payload),
        RuntimeEventKind::ReasoningSummaryDelta => decode::<ReasoningSummaryDeltaPayload>(payload),
        RuntimeEventKind::PlanUpdated => decode::<PlanUpdatedPayload>(payload),
        RuntimeEventKind::ToolCallStarted => decode::<ToolCallStartedPayload>(payload),
        RuntimeEventKind::ToolCallOutput => decode::<ToolCallOutputPayload>(payload),
        RuntimeEventKind::ToolCallCompleted => decode::<ToolCallCompletedPayload>(payload),
        RuntimeEventKind::SubagentStarted => decode::<SubagentStartedPayload>(payload),
        RuntimeEventKind::SubagentProgress => decode::<SubagentProgressPayload>(payload),
        RuntimeEventKind::SubagentCompleted => decode::<SubagentCompletedPayload>(payload),
        RuntimeEventKind::ArtifactProduced => decode::<ArtifactProducedPayload>(payload),
        RuntimeEventKind::TurnCompleted => decode::<TurnCompletedPayload>(payload),
        RuntimeEventKind::TurnFailed => decode::<TurnFailedPayload>(payload),
        RuntimeEventKind::RunCancelled => decode::<RunCancelledPayload>(payload),
        RuntimeEventKind::Heartbeat => decode::<HeartbeatPayload>(payload),
    }
}

fn decode<T>(payload: &Value) -> Result<Value, RuntimeError>
where
    T: DeserializeOwned + Serialize,
{
    let typed: T =
        serde_json::from_value(payload.clone()).map_err(|e| RuntimeError::InvalidEvent {
            message: format!("payload does not match event kind: {e}"),
        })?;
    serde_json::to_value(typed).map_err(|e| RuntimeError::Internal {
        message: format!("payload re-serialize failed: {e}"),
    })
}

fn reject_forbidden_keys(value: &Value) -> Result<(), RuntimeError> {
    match value {
        Value::Object(map) => {
            for key in map.keys() {
                let lower = key.to_ascii_lowercase();
                if FORBIDDEN_KEYS.iter().any(|f| *f == lower) {
                    return Err(RuntimeError::InvalidEvent {
                        message: format!(
                            "forbidden protocol payload field `{key}` rejected at boundary"
                        ),
                    });
                }
            }
            // Only top-level keys of the payload object are protocol fields.
            // Nested content strings may mention words like "password" legitimately.
            Ok(())
        }
        Value::Null => Ok(()),
        other => Err(RuntimeError::InvalidEvent {
            message: format!("payload must be a JSON object, got {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_unknown_fields_for_assistant_delta() {
        let err = validate_payload(
            RuntimeEventKind::AssistantDelta,
            &json!({"text": "hi", "provider": "grok"}),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");
    }

    #[test]
    fn rejects_forbidden_credential_keys() {
        let err = validate_payload(
            RuntimeEventKind::RunStarted,
            &json!({"prompt_preview": "x", "api_key": "sk-secret"}),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(msg.contains("api_key") || msg.contains("forbidden") || msg.contains("unknown"));
    }

    #[test]
    fn rejects_authorization_field() {
        let err = validate_payload(
            RuntimeEventKind::Heartbeat,
            &json!({"tick": 1, "authorization": "Bearer abc"}),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");
    }

    #[test]
    fn accepts_canonical_payloads() {
        let v =
            validate_payload(RuntimeEventKind::AssistantDelta, &json!({"text": "hello"})).unwrap();
        assert_eq!(v, json!({"text": "hello"}));
    }

    #[test]
    fn rejects_wrong_shape_for_kind() {
        let err = validate_payload(
            RuntimeEventKind::TurnFailed,
            &json!({"text": "not a failure payload"}),
        )
        .unwrap_err();
        assert!(matches!(err, RuntimeError::InvalidEvent { .. }), "{err}");
    }
}
