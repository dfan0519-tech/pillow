//! Grok ACP server-request normalization for Pillow-owned read manifests.

use std::path::{Component, Path};

use chrono::Utc;
use keeper_agent_protocol::{
    RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, ToolCallCompletedPayload,
    ToolCallOutputPayload, ToolCallStartedPayload, ToolCallStatus, ToolExecutionState,
    ToolPublicErrorCode, canonical_arguments_digest,
};
use keeper_runtime_host::{GovernedReadDelivery, GovernedReadReceipt, ProviderToolProposal};
use serde_json::{Map, Value, json};

const READ_METHODS: &[&str] = &["fs/readTextFile", "fs/read_text_file"];
const TOOL_METHOD: &str = "pillow/read_tool";
const DEFAULT_READ_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderResponseKind {
    AcpReadText,
    PillowToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedProviderTool {
    pub proposal: ProviderToolProposal,
    pub response_kind: ProviderResponseKind,
}

impl NormalizedProviderTool {
    pub(crate) fn is_native_acp_read(&self) -> bool {
        self.response_kind == ProviderResponseKind::AcpReadText
    }

    pub(crate) fn canonical_request_identity(&self) -> Result<String, ProviderToolRequestError> {
        let digest = canonical_arguments_digest(&self.proposal.arguments)
            .map_err(|_| ProviderToolRequestError::InvalidParams)?;
        Ok(format!(
            "{}@{}:{}",
            self.proposal.tool_id,
            self.proposal.tool_version,
            digest.as_str()
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderToolRequestError {
    MethodNotFound,
    InvalidParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderToolResponseError {
    Failed,
    Cancelled,
    DeadlineExceeded,
    Internal,
}

impl ProviderToolResponseError {
    pub(crate) fn rpc_error(self) -> (i64, &'static str) {
        match self {
            Self::Failed => (-32020, "governed_read_failed"),
            Self::Cancelled => (-32021, "governed_read_cancelled"),
            Self::DeadlineExceeded => (-32022, "governed_read_deadline_exceeded"),
            Self::Internal => (-32000, "governed_read_unavailable"),
        }
    }
}

pub(crate) fn normalize_provider_tool(
    method: &str,
    params: &Value,
    active_session_id: &str,
    project_root: &Path,
) -> Result<NormalizedProviderTool, ProviderToolRequestError> {
    if READ_METHODS.contains(&method) {
        return normalize_acp_read(params, active_session_id, project_root);
    }
    if method == TOOL_METHOD {
        return normalize_pillow_tool(params, active_session_id);
    }
    Err(ProviderToolRequestError::MethodNotFound)
}

fn normalize_acp_read(
    params: &Value,
    active_session_id: &str,
    project_root: &Path,
) -> Result<NormalizedProviderTool, ProviderToolRequestError> {
    let object = strict_object(
        params,
        &[
            "sessionId",
            "path",
            "offsetBytes",
            "offset_bytes",
            "maxBytes",
            "max_bytes",
        ],
    )?;
    require_active_session(object, active_session_id)?;
    let raw_path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or(ProviderToolRequestError::InvalidParams)?;
    let path = project_relative_path(project_root, raw_path)?;
    let offset_bytes = one_u64(object, "offsetBytes", "offset_bytes")?.unwrap_or(0);
    let max_bytes = one_u64(object, "maxBytes", "max_bytes")?.unwrap_or(DEFAULT_READ_BYTES);

    Ok(NormalizedProviderTool {
        proposal: ProviderToolProposal {
            call_id: String::new(),
            tool_id: "fs.read".into(),
            tool_version: 1,
            arguments: json!({
                "path": path,
                "offset_bytes": offset_bytes,
                "max_bytes": max_bytes
            }),
        },
        response_kind: ProviderResponseKind::AcpReadText,
    })
}

pub(crate) fn provider_transport_id(id: &Value) -> Result<String, ProviderToolRequestError> {
    match id {
        Value::Number(value) if value.as_u64().is_some() => Ok(format!("n:{value}")),
        Value::String(value) => bounded_identity(Some(value)).map(|value| format!("s:{value}")),
        _ => Err(ProviderToolRequestError::InvalidParams),
    }
}

fn normalize_pillow_tool(
    params: &Value,
    active_session_id: &str,
) -> Result<NormalizedProviderTool, ProviderToolRequestError> {
    let object = strict_object(
        params,
        &["sessionId", "callId", "toolId", "toolVersion", "arguments"],
    )?;
    require_active_session(object, active_session_id)?;
    let call_id = bounded_identity(object.get("callId").and_then(Value::as_str))?;
    let tool_id = object
        .get("toolId")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "fs.list" | "fs.read" | "fs.search"))
        .ok_or(ProviderToolRequestError::InvalidParams)?;
    let tool_version = object
        .get("toolVersion")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value == 1)
        .ok_or(ProviderToolRequestError::InvalidParams)?;
    let arguments = object
        .get("arguments")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or(ProviderToolRequestError::InvalidParams)?;
    Ok(NormalizedProviderTool {
        proposal: ProviderToolProposal {
            call_id,
            tool_id: tool_id.into(),
            tool_version,
            arguments,
        },
        response_kind: ProviderResponseKind::PillowToolResult,
    })
}

pub(crate) fn provider_response(
    receipt: &GovernedReadReceipt,
    kind: ProviderResponseKind,
) -> Result<Value, ProviderToolResponseError> {
    match receipt.result.state() {
        ToolExecutionState::Succeeded => {}
        ToolExecutionState::Cancelled => return Err(ProviderToolResponseError::Cancelled),
        ToolExecutionState::Failed
            if receipt
                .result
                .error()
                .is_some_and(|error| error.code() == ToolPublicErrorCode::DeadlineExceeded) =>
        {
            return Err(ProviderToolResponseError::DeadlineExceeded);
        }
        ToolExecutionState::Failed
        | ToolExecutionState::Proposed
        | ToolExecutionState::Validated
        | ToolExecutionState::Executing => return Err(ProviderToolResponseError::Failed),
    }
    let result = serde_json::to_value(receipt.result.result())
        .map_err(|_| ProviderToolResponseError::Internal)?;
    match kind {
        ProviderResponseKind::PillowToolResult => Ok(result),
        ProviderResponseKind::AcpReadText => {
            let content = receipt
                .result
                .output()
                .and_then(|output| output.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(json!({
                "content": content,
                "_meta": { "pillowToolResult": result }
            }))
        }
    }
}

pub(crate) fn normalized_result_events(
    run_id: RunId,
    first_sequence: u64,
    receipt: &GovernedReadReceipt,
) -> Result<Vec<RuntimeEvent>, RuntimeError> {
    if receipt.delivery == GovernedReadDelivery::Replayed {
        return Ok(Vec::new());
    }
    let call_id = receipt.result.call_id().to_string();
    let name = format!(
        "{}@{}",
        receipt.result.tool_id(),
        receipt.result.tool_version()
    );
    let chunk =
        serde_json::to_string(receipt.result.result()).map_err(|_| RuntimeError::Internal {
            message: "governed_tool_result_serialize_failed".into(),
        })?;
    let status = match receipt.result.state() {
        ToolExecutionState::Succeeded => ToolCallStatus::Completed,
        ToolExecutionState::Cancelled => ToolCallStatus::Cancelled,
        ToolExecutionState::Failed
        | ToolExecutionState::Proposed
        | ToolExecutionState::Validated
        | ToolExecutionState::Executing => ToolCallStatus::Failed,
    };
    Ok(vec![
        RuntimeEvent::new(
            run_id,
            first_sequence,
            RuntimeEventKind::ToolCallStarted,
            ToolCallStartedPayload {
                tool_call_id: call_id.clone(),
                name,
                summary: Some("Governed project read".into()),
            },
            Utc::now(),
        )?,
        RuntimeEvent::new(
            run_id,
            first_sequence + 1,
            RuntimeEventKind::ToolCallOutput,
            ToolCallOutputPayload {
                tool_call_id: call_id.clone(),
                chunk,
            },
            Utc::now(),
        )?,
        RuntimeEvent::new(
            run_id,
            first_sequence + 2,
            RuntimeEventKind::ToolCallCompleted,
            ToolCallCompletedPayload {
                tool_call_id: call_id,
                status,
            },
            Utc::now(),
        )?,
    ])
}

fn strict_object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, ProviderToolRequestError> {
    let object = value
        .as_object()
        .ok_or(ProviderToolRequestError::InvalidParams)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ProviderToolRequestError::InvalidParams);
    }
    Ok(object)
}

fn require_active_session(
    object: &Map<String, Value>,
    active_session_id: &str,
) -> Result<(), ProviderToolRequestError> {
    if object.get("sessionId").and_then(Value::as_str) == Some(active_session_id) {
        Ok(())
    } else {
        Err(ProviderToolRequestError::InvalidParams)
    }
}

fn one_u64(
    object: &Map<String, Value>,
    camel: &str,
    snake: &str,
) -> Result<Option<u64>, ProviderToolRequestError> {
    if object.contains_key(camel) && object.contains_key(snake) {
        return Err(ProviderToolRequestError::InvalidParams);
    }
    let value = object.get(camel).or_else(|| object.get(snake));
    value
        .map(|value| {
            value
                .as_u64()
                .ok_or(ProviderToolRequestError::InvalidParams)
        })
        .transpose()
}

fn bounded_identity(value: Option<&str>) -> Result<String, ProviderToolRequestError> {
    value
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 120
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        })
        .map(str::to_owned)
        .ok_or(ProviderToolRequestError::InvalidParams)
}

fn project_relative_path(
    project_root: &Path,
    raw: &str,
) -> Result<String, ProviderToolRequestError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ProviderToolRequestError::InvalidParams);
    }
    let path = Path::new(raw);
    let relative = if path.is_absolute() {
        path.strip_prefix(project_root)
            .map_err(|_| ProviderToolRequestError::InvalidParams)?
    } else {
        path
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(
                part.to_str()
                    .filter(|part| !part.is_empty())
                    .ok_or(ProviderToolRequestError::InvalidParams)?,
            ),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ProviderToolRequestError::InvalidParams);
            }
        }
    }
    if parts.is_empty() {
        Ok(".".into())
    } else {
        Ok(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_agent_protocol::{
        ToolActiveContext, ToolProposal, ToolPublicError, ToolPublicErrorCode, ToolResult,
        canonical_arguments_digest,
    };
    use uuid::Uuid;

    fn terminal_receipt(
        state: ToolExecutionState,
        error: Option<ToolPublicErrorCode>,
    ) -> GovernedReadReceipt {
        let run_id = RunId(Uuid::new_v4());
        let context = ToolActiveContext::new("project", "conversation", run_id).unwrap();
        let arguments = json!({"path":"missing.txt","offset_bytes":0,"max_bytes":64});
        let proposal = ToolProposal {
            project_id: context.project_id().into(),
            conversation_id: context.conversation_id().into(),
            run_id,
            call_id: "call-terminal".into(),
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
            run_id,
            call_id: proposal.call_id().into(),
            tool_id: proposal.tool_id().into(),
            tool_version: proposal.tool_version(),
            proposal_fingerprint: proposal.fingerprint().clone(),
            arguments_digest: proposal.normalized_arguments_digest().clone(),
            state,
            output: None,
            error: error.map(ToolPublicError::canonical),
        }
        .validate_for_proposal(&context, &proposal)
        .unwrap();
        GovernedReadReceipt {
            result,
            delivery: GovernedReadDelivery::Executed,
        }
    }

    #[test]
    fn custom_tool_surface_accepts_only_fixed_read_manifests() {
        let root = std::env::temp_dir();
        let accepted = normalize_provider_tool(
            TOOL_METHOD,
            &json!({
                "sessionId":"s",
                "callId":"call-1",
                "toolId":"fs.search",
                "toolVersion":1,
                "arguments":{"path":".","query":"needle","case_sensitive":false,"max_results":10}
            }),
            "s",
            &root,
        )
        .unwrap();
        assert_eq!(accepted.proposal.tool_id, "fs.search");
        assert!(matches!(
            normalize_provider_tool(
                TOOL_METHOD,
                &json!({
                    "sessionId":"s",
                    "callId":"call-2",
                    "toolId":"fs.write",
                    "toolVersion":1,
                    "arguments":{"path":"x"}
                }),
                "s",
                &root
            ),
            Err(ProviderToolRequestError::InvalidParams)
        ));
    }

    #[test]
    fn acp_read_rejects_foreign_session_and_parent_escape() {
        let root = std::env::temp_dir();
        assert!(matches!(
            normalize_provider_tool(
                "fs/readTextFile",
                &json!({"sessionId":"foreign","path":"note.txt"}),
                "active",
                &root
            ),
            Err(ProviderToolRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_tool(
                "fs/readTextFile",
                &json!({"sessionId":"active","path":"../note.txt"}),
                "active",
                &root
            ),
            Err(ProviderToolRequestError::InvalidParams)
        ));
    }

    #[test]
    fn native_acp_aliases_share_one_canonical_request_identity() {
        let root = std::env::temp_dir();
        let camel = normalize_provider_tool(
            "fs/readTextFile",
            &json!({
                "sessionId":"active",
                "path":"./note.txt",
                "offsetBytes":0,
                "maxBytes":64
            }),
            "active",
            &root,
        )
        .unwrap();
        let snake = normalize_provider_tool(
            "fs/read_text_file",
            &json!({
                "sessionId":"active",
                "path":"note.txt",
                "offset_bytes":0,
                "max_bytes":64
            }),
            "active",
            &root,
        )
        .unwrap();

        assert_eq!(
            camel.canonical_request_identity().unwrap(),
            snake.canonical_request_identity().unwrap()
        );
    }

    #[test]
    fn custom_tool_keeps_its_explicit_call_id() {
        let normalized = normalize_provider_tool(
            TOOL_METHOD,
            &json!({
                "sessionId":"s",
                "callId":"provider-call-7",
                "toolId":"fs.read",
                "toolVersion":1,
                "arguments":{"path":"note.txt","offset_bytes":0,"max_bytes":64}
            }),
            "s",
            &std::env::temp_dir(),
        )
        .unwrap();

        assert_eq!(normalized.proposal.call_id, "provider-call-7");
    }

    #[test]
    fn failed_cancelled_and_deadline_reads_are_not_successful_empty_content() {
        for (case, receipt, expected) in [
            (
                "failed",
                terminal_receipt(
                    ToolExecutionState::Failed,
                    Some(ToolPublicErrorCode::PathRejected),
                ),
                (-32020, "governed_read_failed"),
            ),
            (
                "cancelled",
                terminal_receipt(ToolExecutionState::Cancelled, None),
                (-32021, "governed_read_cancelled"),
            ),
            (
                "deadline",
                terminal_receipt(
                    ToolExecutionState::Failed,
                    Some(ToolPublicErrorCode::DeadlineExceeded),
                ),
                (-32022, "governed_read_deadline_exceeded"),
            ),
        ] {
            let error =
                provider_response(&receipt, ProviderResponseKind::AcpReadText).expect_err(case);
            assert_eq!(error.rpc_error(), expected, "{case}");
        }
    }
}
