//! Strict provider-write normalization into Pillow's single governed write port.

use std::path::Path;

use keeper_agent_protocol::{
    FsWriteOutput, GovernedWriteReceipt, ProviderWriteProposal, WritePublicErrorCode,
    canonical_arguments_digest,
};
use serde_json::{Map, Value, json};

use crate::provider_tools::project_relative_path;

const WRITE_METHOD: &str = "pillow/write_tool";
const ACP_WRITE_METHODS: &[&str] = &["fs/writeTextFile", "fs/write_text_file"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderWriteResponseKind {
    AcpWriteText,
    PillowToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedProviderWrite {
    pub proposal: ProviderWriteProposal,
    pub response_kind: ProviderWriteResponseKind,
}

impl NormalizedProviderWrite {
    pub(crate) fn is_native_acp_write(&self) -> bool {
        self.response_kind == ProviderWriteResponseKind::AcpWriteText
    }

    pub(crate) fn canonical_request_identity(&self) -> Result<String, ProviderWriteRequestError> {
        let digest = canonical_arguments_digest(&self.proposal.arguments)
            .map_err(|_| ProviderWriteRequestError::InvalidParams)?;
        Ok(format!(
            "{}@{}:{}",
            self.proposal.tool_id,
            self.proposal.tool_version,
            digest.as_str()
        ))
    }
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

pub(crate) fn is_provider_write_method(method: &str) -> bool {
    method == WRITE_METHOD || ACP_WRITE_METHODS.contains(&method)
}

/// Normalize every supported provider write surface into `fs.write@1`.
pub(crate) fn normalize_provider_write(
    method: &str,
    params: &Value,
    active_session_id: &str,
    project_root: &Path,
) -> Result<NormalizedProviderWrite, ProviderWriteRequestError> {
    if ACP_WRITE_METHODS.contains(&method) {
        return normalize_acp_write(params, active_session_id, project_root);
    }
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
        response_kind: ProviderWriteResponseKind::PillowToolResult,
    })
}

fn normalize_acp_write(
    params: &Value,
    active_session_id: &str,
    project_root: &Path,
) -> Result<NormalizedProviderWrite, ProviderWriteRequestError> {
    let object = strict_object(params, &["sessionId", "path", "content", "_meta"])?;
    require_active_session(object, active_session_id)?;
    let raw_path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    let path = project_relative_path(project_root, raw_path)
        .map_err(|_| ProviderWriteRequestError::InvalidParams)?;
    if path == "." {
        return Err(ProviderWriteRequestError::InvalidParams);
    }
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .ok_or(ProviderWriteRequestError::InvalidParams)?;
    let mode = match project_root.join(&path).try_exists() {
        Ok(true) => "modify",
        Ok(false) => "create",
        Err(_) => return Err(ProviderWriteRequestError::InvalidParams),
    };

    Ok(NormalizedProviderWrite {
        proposal: ProviderWriteProposal {
            // ACP filesystem requests carry no provider call id. The prompt
            // router replaces this with a run-scoped Pillow identity derived
            // from the JSON-RPC transport id before governance sees it.
            call_id: String::new(),
            tool_id: "fs.write".into(),
            tool_version: 1,
            arguments: json!({
                "path": path,
                "mode": mode,
                "content": content
            }),
        },
        response_kind: ProviderWriteResponseKind::AcpWriteText,
    })
}

pub(crate) fn provider_write_response(
    receipt: &GovernedWriteReceipt,
    kind: ProviderWriteResponseKind,
) -> Result<Value, ProviderWriteResponseError> {
    if let Some(output) = receipt.provider_ok.as_ref() {
        return match kind {
            ProviderWriteResponseKind::AcpWriteText => Ok(json!({})),
            ProviderWriteResponseKind::PillowToolResult => serialize_output(output),
        };
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
    use std::path::Path;

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
            Path::new("."),
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
                "active",
                Path::new(".")
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
                "active",
                Path::new(".")
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
                "active",
                Path::new(".")
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
                "active",
                Path::new(".")
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
        assert!(matches!(
            normalize_provider_write("pillow/read_tool", &json!({}), "active", Path::new(".")),
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
                "active",
                Path::new(".")
            ),
            Err(ProviderWriteRequestError::InvalidParams)
        ));
    }

    #[test]
    fn acp_write_aliases_normalize_absolute_paths_and_infer_create_or_modify() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("existing.txt");
        std::fs::write(&existing, "old\n").unwrap();
        let create_path = root.path().join("created.txt");

        let snake = normalize_provider_write(
            "fs/write_text_file",
            &json!({
                "sessionId": "active",
                "path": create_path.to_string_lossy(),
                "content": "new\n",
                "_meta": {"trace": "ignored"}
            }),
            "active",
            root.path(),
        )
        .unwrap();
        let camel = normalize_provider_write(
            "fs/writeTextFile",
            &json!({
                "sessionId": "active",
                "path": existing.to_string_lossy(),
                "content": "replacement\n"
            }),
            "active",
            root.path(),
        )
        .unwrap();

        assert!(snake.is_native_acp_write());
        assert!(camel.is_native_acp_write());
        assert_eq!(
            snake.proposal.arguments,
            json!({"path":"created.txt","mode":"create","content":"new\n"})
        );
        assert_eq!(
            camel.proposal.arguments,
            json!({"path":"existing.txt","mode":"modify","content":"replacement\n"})
        );
        assert_eq!(snake.proposal.call_id, "");
        assert_eq!(camel.proposal.call_id, "");
    }

    #[test]
    fn acp_write_rejects_foreign_sessions_unknown_keys_and_project_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().parent().unwrap().join("outside.txt");
        for params in [
            json!({
                "sessionId": "foreign",
                "path": root.path().join("a.txt").to_string_lossy(),
                "content": "x"
            }),
            json!({
                "sessionId": "active",
                "path": root.path().join("a.txt").to_string_lossy(),
                "content": "x",
                "grant": "not-accepted"
            }),
            json!({
                "sessionId": "active",
                "path": outside.to_string_lossy(),
                "content": "x"
            }),
        ] {
            assert!(matches!(
                normalize_provider_write("fs/write_text_file", &params, "active", root.path()),
                Err(ProviderWriteRequestError::InvalidParams)
            ));
        }
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
            delivery: keeper_agent_protocol::GovernedWriteDelivery::Executed,
            requested: None,
            resolved: keeper_agent_protocol::WriteProposalResolvedPayload {
                proposal_id: "wp".into(),
                path: "src/a.ts".into(),
                mode: keeper_agent_protocol::WriteMode::Create,
                state: keeper_agent_protocol::WriteProposalState::Succeeded,
                error_code: None,
                auto_approved_by_rule: None,
            },
            provider_ok: Some(output.clone()),
            provider_error: None,
        };
        let value =
            provider_write_response(&receipt, ProviderWriteResponseKind::PillowToolResult).unwrap();
        assert_eq!(value["path"], "src/a.ts");
        assert!(value.get("approval_grant").is_none());
        assert!(value.get("content").is_none());
        assert_eq!(
            provider_write_response(&receipt, ProviderWriteResponseKind::AcpWriteText).unwrap(),
            json!({}),
            "ACP acknowledges the governed receipt without exposing Pillow receipt internals"
        );
        let _ = output;
    }
}
