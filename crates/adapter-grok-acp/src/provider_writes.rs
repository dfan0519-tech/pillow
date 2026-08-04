//! Strict `pillow/write_tool` normalization. Independent of the read normalizer.

use keeper_agent_protocol::{FsWriteOutput, WritePublicErrorCode};
use keeper_runtime_host::{GovernedWriteReceipt, ProviderWriteProposal};
use serde_json::{Map, Value};

const WRITE_METHOD: &str = "pillow/write_tool";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedProviderWrite {
    pub proposal: ProviderWriteProposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderWriteRequestError {
    MethodNotFound,
    InvalidParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderWriteResponseError {
    Rejected,
    Expired,
    Cancelled,
    Conflict,
    Failed,
    Interrupted,
    Internal,
}

impl ProviderWriteResponseError {
    pub(crate) fn rpc_error(self) -> (i64, &'static str) {
        match self {
            Self::Rejected => (-32040, "governed_write_rejected"),
            Self::Expired => (-32041, "governed_write_expired"),
            Self::Cancelled => (-32042, "governed_write_cancelled"),
            Self::Conflict => (-32043, "governed_write_conflict"),
            Self::Failed => (-32044, "governed_write_failed"),
            Self::Interrupted => (-32045, "governed_write_interrupted"),
            Self::Internal => (-32000, "governed_write_unavailable"),
        }
    }

    pub(crate) fn from_public(code: WritePublicErrorCode) -> Self {
        match code {
            WritePublicErrorCode::ApprovalRejected => Self::Rejected,
            WritePublicErrorCode::ApprovalExpired => Self::Expired,
            WritePublicErrorCode::Cancelled => Self::Cancelled,
            WritePublicErrorCode::WriteConflict => Self::Conflict,
            WritePublicErrorCode::Interrupted => Self::Interrupted,
            WritePublicErrorCode::ApprovalRequired
            | WritePublicErrorCode::BaseChanged
            | WritePublicErrorCode::PathRejected
            | WritePublicErrorCode::WriteFailed => Self::Failed,
        }
    }
}

/// Normalize a provider write method. Only `pillow/write_tool` is accepted.
pub(crate) fn normalize_provider_write(
    method: &str,
    params: &Value,
    active_session_id: &str,
) -> Result<NormalizedProviderWrite, ProviderWriteRequestError> {
    if method != WRITE_METHOD {
        return Err(ProviderWriteRequestError::MethodNotFound);
    }
    let object = strict_object(
        params,
        &["sessionId", "callId", "toolId", "toolVersion", "arguments"],
    )?;
    require_active_session(object, active_session_id)?;
    let call_id = bounded_identity(object.get("callId").and_then(Value::as_str))?;
    let tool_id = object
        .get("toolId")
        .and_then(Value::as_str)
        .filter(|value| *value == "fs.write")
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    let tool_version = object
        .get("toolVersion")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value == 1)
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    let arguments = object
        .get("arguments")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    // Exact argument keys only.
    let args_obj = arguments
        .as_object()
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    if args_obj
        .keys()
        .any(|key| !matches!(key.as_str(), "path" | "mode" | "content"))
    {
        return Err(ProviderWriteRequestError::InvalidParams);
    }
    if !args_obj.contains_key("path")
        || !args_obj.contains_key("mode")
        || !args_obj.contains_key("content")
    {
        return Err(ProviderWriteRequestError::InvalidParams);
    }
    Ok(NormalizedProviderWrite {
        proposal: ProviderWriteProposal {
            call_id,
            tool_id: tool_id.into(),
            tool_version,
            arguments,
        },
    })
}

pub(crate) fn provider_write_response(
    receipt: &GovernedWriteReceipt,
) -> Result<Value, ProviderWriteResponseError> {
    if let Some(output) = receipt.provider_ok.as_ref() {
        return serialize_output(output);
    }
    if let Some(code) = receipt.provider_error {
        return Err(ProviderWriteResponseError::from_public(code));
    }
    Err(ProviderWriteResponseError::Internal)
}

fn serialize_output(output: &FsWriteOutput) -> Result<Value, ProviderWriteResponseError> {
    serde_json::to_value(output).map_err(|_| ProviderWriteResponseError::Internal)
}

fn strict_object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, ProviderWriteRequestError> {
    let object = value
        .as_object()
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ProviderWriteRequestError::InvalidParams);
    }
    Ok(object)
}

fn require_active_session(
    object: &Map<String, Value>,
    active_session_id: &str,
) -> Result<(), ProviderWriteRequestError> {
    if object.get("sessionId").and_then(Value::as_str) == Some(active_session_id) {
        Ok(())
    } else {
        Err(ProviderWriteRequestError::InvalidParams)
    }
}

fn bounded_identity(value: Option<&str>) -> Result<String, ProviderWriteRequestError> {
    value
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 120
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        })
        .map(str::to_owned)
        .ok_or(ProviderWriteRequestError::InvalidParams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_exact_pillow_write_tool_shape() {
        let normalized = normalize_provider_write(
            WRITE_METHOD,
            &json!({
                "sessionId": "active",
                "callId": "call-1",
                "toolId": "fs.write",
                "toolVersion": 1,
                "arguments": {
                    "path": "src/a.ts",
                    "mode": "create",
                    "content": "export const n = 1;\n"
                }
            }),
            "active",
        )
        .unwrap();
        assert_eq!(normalized.proposal.call_id, "call-1");
        assert_eq!(normalized.proposal.tool_id, "fs.write");
        assert_eq!(normalized.proposal.tool_version, 1);
    }

    #[test]
    fn rejects_foreign_session_unknown_keys_and_tools() {
        assert!(matches!(
            normalize_provider_write(
                WRITE_METHOD,
                &json!({
                    "sessionId": "other",
                    "callId": "c",
                    "toolId": "fs.write",
                    "toolVersion": 1,
                    "arguments": {"path":"a","mode":"create","content":"x"}
                }),
                "active"
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_write(
                WRITE_METHOD,
                &json!({
                    "sessionId": "active",
                    "callId": "c",
                    "toolId": "fs.write",
                    "toolVersion": 1,
                    "arguments": {"path":"a","mode":"create","content":"x"},
                    "extra": true
                }),
                "active"
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_write(
                WRITE_METHOD,
                &json!({
                    "sessionId": "active",
                    "callId": "c",
                    "toolId": "fs.read",
                    "toolVersion": 1,
                    "arguments": {"path":"a","mode":"create","content":"x"}
                }),
                "active"
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_write(
                WRITE_METHOD,
                &json!({
                    "sessionId": "active",
                    "callId": "c",
                    "toolId": "fs.write",
                    "toolVersion": 2,
                    "arguments": {"path":"a","mode":"create","content":"x"}
                }),
                "active"
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_write("pillow/read_tool", &json!({}), "active"),
            Err(ProviderWriteRequestError::MethodNotFound)
        ));
    }

    #[test]
    fn rejects_unknown_argument_keys() {
        assert!(matches!(
            normalize_provider_write(
                WRITE_METHOD,
                &json!({
                    "sessionId": "active",
                    "callId": "c",
                    "toolId": "fs.write",
                    "toolVersion": 1,
                    "arguments": {
                        "path": "a",
                        "mode": "create",
                        "content": "x",
                        "base_digest": "sha256:dead"
                    }
                }),
                "active"
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
    }

    #[test]
    fn success_response_is_typed_output_only() {
        let output = FsWriteOutput {
            path: "src/a.ts".into(),
            mode: keeper_agent_protocol::WriteMode::Create,
            bytes_written: 3,
            content_digest:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        };
        let receipt = GovernedWriteReceipt {
            delivery: keeper_runtime_host::GovernedWriteDelivery::Executed,
            requested: None,
            resolved: keeper_agent_protocol::WriteProposalResolvedPayload {
                proposal_id: "wp".into(),
                path: "src/a.ts".into(),
                mode: keeper_agent_protocol::WriteMode::Create,
                state: keeper_agent_protocol::WriteProposalState::Succeeded,
                error_code: None,
            },
            provider_ok: Some(output.clone()),
            provider_error: None,
        };
        let value = provider_write_response(&receipt).unwrap();
        assert_eq!(value["path"], "src/a.ts");
        assert!(value.get("approval_grant").is_none());
        assert!(value.get("content").is_none());
        let _ = output;
    }
}
