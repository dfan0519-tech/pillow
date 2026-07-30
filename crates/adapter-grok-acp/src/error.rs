//! Map transport and RPC failures into protocol errors.

use keeper_agent_protocol::RuntimeError;

/// Build a sanitized internal error (never embed raw provider envelopes).
pub fn internal(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Internal {
        message: message.into(),
    }
}

/// Transport / framing failure.
pub fn transport(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Internal {
        message: format!("acp_transport: {}", message.into()),
    }
}

/// JSON-RPC request failed with a code.
pub fn rpc_failed(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Internal {
        message: format!("acp_request_failed: {}", message.into()),
    }
}

/// Request timed out.
pub fn timeout(method: &str) -> RuntimeError {
    RuntimeError::Internal {
        message: format!("acp_timeout: {method}"),
    }
}

/// Output line exceeded the configured bound.
pub fn output_too_large() -> RuntimeError {
    RuntimeError::Internal {
        message: "acp_output_too_large".into(),
    }
}

/// Malformed NDJSON from the agent.
pub fn invalid_json() -> RuntimeError {
    RuntimeError::Internal {
        message: "acp_invalid_json".into(),
    }
}

/// Child process exited unexpectedly.
pub fn process_exited() -> RuntimeError {
    RuntimeError::Internal {
        message: "acp_process_exited".into(),
    }
}

/// Response text exceeded the configured bound.
pub fn response_too_large() -> RuntimeError {
    RuntimeError::Internal {
        message: "acp_response_too_large".into(),
    }
}
