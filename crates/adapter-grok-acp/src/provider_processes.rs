//! Strict `pillow/process_tool` normalization and provider-safe responses.

use keeper_agent_protocol::{
    GovernedProcessDelivery, GovernedProcessReceipt, ProcessRunOutput, ProviderProcessProposal,
};
use serde_json::{Map, Value};

const PROCESS_METHOD: &str = "pillow/process_tool";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedProviderProcess {
    pub proposal: ProviderProcessProposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderProcessRequestError {
    MethodNotFound,
    InvalidParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderProcessResponseError {
    Rejected,
    Expired,
    Cancelled,
    TimedOut,
    Failed,
    Conflict,
    Busy,
    Interrupted,
    Internal,
}

impl ProviderProcessResponseError {
    pub(crate) fn rpc_error(self) -> (i64, &'static str) {
        match self {
            Self::Rejected => (-32050, "governed_process_rejected"),
            Self::Expired => (-32051, "governed_process_expired"),
            Self::Cancelled => (-32052, "governed_process_cancelled"),
            Self::TimedOut => (-32053, "governed_process_timed_out"),
            Self::Failed => (-32054, "governed_process_failed"),
            Self::Conflict => (-32055, "governed_process_conflict"),
            Self::Busy => (-32056, "governed_process_in_progress"),
            Self::Interrupted => (-32057, "governed_process_interrupted"),
            Self::Internal => (-32000, "governed_process_unavailable"),
        }
    }
}

/// Normalize only the declared process RPC shape. Filesystem authority stays in
/// runtime-host, which validates the normalized arguments against its run binding.
pub(crate) fn normalize_provider_process(
    method: &str,
    params: &Value,
    active_session_id: &str,
) -> Result<NormalizedProviderProcess, ProviderProcessRequestError> {
    if method != PROCESS_METHOD {
        return Err(ProviderProcessRequestError::MethodNotFound);
    }
    let object = strict_object(
        params,
        &["sessionId", "callId", "toolId", "toolVersion", "arguments"],
    )?;
    if object.get("sessionId").and_then(Value::as_str) != Some(active_session_id) {
        return Err(ProviderProcessRequestError::InvalidParams);
    }
    let call_id = bounded_identity(object.get("callId").and_then(Value::as_str))?;
    let tool_id = object
        .get("toolId")
        .and_then(Value::as_str)
        .filter(|value| *value == "process.run")
        .ok_or(ProviderProcessRequestError::InvalidParams)?;
    let tool_version = object
        .get("toolVersion")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value == 1)
        .ok_or(ProviderProcessRequestError::InvalidParams)?;
    let arguments = object
        .get("arguments")
        .ok_or(ProviderProcessRequestError::InvalidParams)?;
    validate_process_arguments(arguments)?;

    Ok(NormalizedProviderProcess {
        proposal: ProviderProcessProposal {
            call_id,
            tool_id: tool_id.into(),
            tool_version,
            arguments: arguments.clone(),
        },
    })
}

/// Return only bounded terminal metadata. Raw output, grants, paths, session
/// identifiers and host errors never cross this provider boundary.
pub(crate) fn provider_process_response(
    receipt: &GovernedProcessReceipt,
) -> Result<Value, ProviderProcessResponseError> {
    if receipt.timed_out {
        return Err(ProviderProcessResponseError::TimedOut);
    }
    if receipt.cancelled {
        return Err(ProviderProcessResponseError::Cancelled);
    }
    let state = receipt
        .terminal
        .as_ref()
        .and_then(|terminal| serde_json::to_value(terminal).ok())
        .and_then(|terminal| {
            terminal
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    match state.as_deref() {
        Some("rejected") => return Err(ProviderProcessResponseError::Rejected),
        Some("expired") => return Err(ProviderProcessResponseError::Expired),
        Some("interrupted") => return Err(ProviderProcessResponseError::Interrupted),
        Some("failed") => return Err(ProviderProcessResponseError::Failed),
        _ => {}
    }
    let output = receipt
        .provider_ok
        .as_ref()
        .ok_or(ProviderProcessResponseError::Failed)?;
    if output.exit_code.is_some_and(|exit_code| exit_code != 0) {
        return Err(ProviderProcessResponseError::Failed);
    }
    serialize_output(
        output,
        receipt.delivery == GovernedProcessDelivery::Replayed,
    )
}

fn serialize_output(
    output: &ProcessRunOutput,
    replayed: bool,
) -> Result<Value, ProviderProcessResponseError> {
    let mut value =
        serde_json::to_value(output).map_err(|_| ProviderProcessResponseError::Internal)?;
    if replayed {
        value
            .as_object_mut()
            .expect("ProcessRunOutput serializes as an object")
            .insert("replayed".into(), Value::Bool(true));
    }
    Ok(value)
}

fn strict_object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, ProviderProcessRequestError> {
    let object = value
        .as_object()
        .ok_or(ProviderProcessRequestError::InvalidParams)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ProviderProcessRequestError::InvalidParams);
    }
    Ok(object)
}

fn validate_process_arguments(value: &Value) -> Result<(), ProviderProcessRequestError> {
    let object = strict_object(value, &["program", "argv", "cwd"])?;
    if !object.contains_key("program") || !object.contains_key("argv") {
        return Err(ProviderProcessRequestError::InvalidParams);
    }
    if object.get("program").and_then(Value::as_str).is_none()
        || !object
            .get("argv")
            .and_then(Value::as_array)
            .is_some_and(|argv| argv.iter().all(Value::is_string))
        || matches!(object.get("cwd"), Some(value) if !value.is_null() && !value.is_string())
    {
        return Err(ProviderProcessRequestError::InvalidParams);
    }
    Ok(())
}

fn bounded_identity(value: Option<&str>) -> Result<String, ProviderProcessRequestError> {
    value
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 120
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        })
        .map(str::to_owned)
        .ok_or(ProviderProcessRequestError::InvalidParams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_agent_protocol::GovernedProcessDelivery;
    use keeper_agent_protocol::ProcessTerminalOutcomePayload;
    use keeper_policy::ExecProposalState;
    use serde_json::json;

    #[test]
    fn accepts_only_the_exact_portable_process_shape() {
        let normalized = normalize_provider_process(
            PROCESS_METHOD,
            &json!({
                "sessionId": "active",
                "callId": "call-1",
                "toolId": "process.run",
                "toolVersion": 1,
                "arguments": {"program": "node", "argv": ["--version"], "cwd": "."}
            }),
            "active",
        )
        .unwrap();
        assert_eq!(normalized.proposal.call_id, "call-1");
        assert!(matches!(
            normalize_provider_process(
                PROCESS_METHOD,
                &json!({
                    "sessionId": "other", "callId": "", "toolId": "process.run", "toolVersion": 1,
                    "arguments": {"program": "node", "argv": [], "extra": true}, "extra": true
                }),
                "active",
            ),
            Err(ProviderProcessRequestError::InvalidParams)
        ));
    }

    #[test]
    fn rejects_every_wrong_identity_field_and_argument_type() {
        let valid = json!({
            "sessionId": "active", "callId": "call-1", "toolId": "process.run", "toolVersion": 1,
            "arguments": {"program": "node", "argv": ["--version"], "cwd": "."}
        });
        for mutation in [
            json!({"sessionId":"other","callId":"call-1","toolId":"process.run","toolVersion":1,"arguments":{"program":"node","argv":[]}}),
            json!({"sessionId":"active","callId":"","toolId":"process.run","toolVersion":1,"arguments":{"program":"node","argv":[]}}),
            json!({"sessionId":"active","callId":"call-1","toolId":"process.other","toolVersion":1,"arguments":{"program":"node","argv":[]}}),
            json!({"sessionId":"active","callId":"call-1","toolId":"process.run","toolVersion":2,"arguments":{"program":"node","argv":[]}}),
            json!({"sessionId":"active","callId":"call-1","toolId":"process.run","toolVersion":1,"arguments":{"program":4,"argv":[]}}),
            json!({"sessionId":"active","callId":"call-1","toolId":"process.run","toolVersion":1,"arguments":{"program":"node","argv":[4]}}),
            json!({"sessionId":"active","callId":"call-1","toolId":"process.run","toolVersion":1,"arguments":{"program":"node","argv":[],"cwd":4}}),
        ] {
            assert!(matches!(
                normalize_provider_process(PROCESS_METHOD, &mutation, "active"),
                Err(ProviderProcessRequestError::InvalidParams)
            ));
        }
        assert!(matches!(
            normalize_provider_process("pillow/other", &valid, "active"),
            Err(ProviderProcessRequestError::MethodNotFound)
        ));
    }

    #[test]
    fn success_response_is_metadata_only_with_optional_replay_marker() {
        let output = ProcessRunOutput {
            exit_code: Some(0),
            duration_ms: 1,
            stdout_sha256: "sha256:out".into(),
            stderr_sha256: "sha256:err".into(),
            captured_bytes: 0,
            truncated: false,
            encoding_lossy: false,
            cancelled: false,
            timed_out: false,
        };
        let receipt = GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Replayed,
            proposal_id: "hidden".into(),
            state: keeper_policy::ExecProposalState::Succeeded,
            requested: None,
            decided: None,
            started: None,
            terminal: None,
            provider_ok: Some(output),
            cancelled: false,
            timed_out: false,
        };
        let response = provider_process_response(&receipt).unwrap();
        assert_eq!(response["replayed"], true);
        for forbidden in [
            "stdout",
            "stderr",
            "grant",
            "proposal_id",
            "session",
            "path",
        ] {
            assert!(response.get(forbidden).is_none());
        }
    }

    fn terminal_receipt(
        state: ExecProposalState,
        output: Option<ProcessRunOutput>,
        cancelled: bool,
        timed_out: bool,
    ) -> GovernedProcessReceipt {
        GovernedProcessReceipt {
            delivery: GovernedProcessDelivery::Executed,
            proposal_id: "internal-proposal-id".into(),
            state,
            requested: None,
            decided: None,
            started: None,
            terminal: Some(ProcessTerminalOutcomePayload {
                proposal_id: "internal-proposal-id".into(),
                program: "node".into(),
                state,
                exit_code: None,
                duration_ms: None,
                truncated: None,
                encoding_lossy: None,
                captured_bytes: None,
                cancelled: Some(cancelled),
                timed_out: Some(timed_out),
            }),
            provider_ok: output,
            cancelled,
            timed_out,
        }
    }

    #[test]
    fn terminal_errors_have_fixed_safe_public_pairs() {
        let output = || ProcessRunOutput {
            exit_code: Some(0),
            duration_ms: 1,
            stdout_sha256: "sha256:out".into(),
            stderr_sha256: "sha256:err".into(),
            captured_bytes: 0,
            truncated: false,
            encoding_lossy: false,
            cancelled: false,
            timed_out: false,
        };
        for (receipt, expected) in [
            (
                terminal_receipt(ExecProposalState::Rejected, None, false, false),
                ProviderProcessResponseError::Rejected,
            ),
            (
                terminal_receipt(ExecProposalState::Expired, None, false, false),
                ProviderProcessResponseError::Expired,
            ),
            (
                terminal_receipt(ExecProposalState::Cancelled, Some(output()), true, false),
                ProviderProcessResponseError::Cancelled,
            ),
            (
                terminal_receipt(ExecProposalState::Failed, Some(output()), false, true),
                ProviderProcessResponseError::TimedOut,
            ),
            (
                terminal_receipt(ExecProposalState::Failed, Some(output()), false, false),
                ProviderProcessResponseError::Failed,
            ),
        ] {
            let error = provider_process_response(&receipt).unwrap_err();
            assert_eq!(error.rpc_error(), expected.rpc_error());
        }
        assert_eq!(
            ProviderProcessResponseError::Busy.rpc_error(),
            (-32056, "governed_process_in_progress")
        );
        assert_eq!(
            ProviderProcessResponseError::Conflict.rpc_error(),
            (-32055, "governed_process_conflict")
        );
    }
}
