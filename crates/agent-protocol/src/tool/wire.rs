// Process tool payloads (process.run@1) convert through typed
// ProcessRunArguments / ProcessRunOutput validation in validate.rs.
// This module only owns the shared proposal/result envelope wire parsers.

use serde::Deserialize;
use serde_json::Value;

use super::protocol_error::{ToolProtocolError, ToolProtocolErrorCode};
use super::public_error::ToolPublicError;
use super::reservation::ToolProposalFingerprint;
use super::types::{ArgumentsDigest, ToolExecutionState, ToolProposal, ToolResult};
use crate::RunId;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolProposalWire {
    project_id: String,
    conversation_id: String,
    run_id: RunId,
    call_id: String,
    tool_id: String,
    tool_version: u16,
    arguments: Value,
    arguments_digest: ArgumentsDigest,
}

impl From<ToolProposalWire> for ToolProposal {
    fn from(wire: ToolProposalWire) -> Self {
        Self {
            project_id: wire.project_id,
            conversation_id: wire.conversation_id,
            run_id: wire.run_id,
            call_id: wire.call_id,
            tool_id: wire.tool_id,
            tool_version: wire.tool_version,
            arguments: wire.arguments,
            arguments_digest: wire.arguments_digest,
        }
    }
}

impl ToolProposal {
    pub fn parse_str(input: &str) -> Result<Self, ToolProtocolError> {
        serde_json::from_str::<ToolProposalWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidEnvelope))
    }

    pub fn parse_bytes(input: &[u8]) -> Result<Self, ToolProtocolError> {
        serde_json::from_slice::<ToolProposalWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidEnvelope))
    }

    pub fn parse_value(input: Value) -> Result<Self, ToolProtocolError> {
        serde_json::from_value::<ToolProposalWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidEnvelope))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolResultWire {
    project_id: String,
    conversation_id: String,
    run_id: RunId,
    call_id: String,
    tool_id: String,
    tool_version: u16,
    proposal_fingerprint: ToolProposalFingerprint,
    arguments_digest: ArgumentsDigest,
    state: ToolExecutionState,
    output: Option<Value>,
    error: Option<ToolPublicError>,
}

impl From<ToolResultWire> for ToolResult {
    fn from(wire: ToolResultWire) -> Self {
        Self {
            project_id: wire.project_id,
            conversation_id: wire.conversation_id,
            run_id: wire.run_id,
            call_id: wire.call_id,
            tool_id: wire.tool_id,
            tool_version: wire.tool_version,
            proposal_fingerprint: wire.proposal_fingerprint,
            arguments_digest: wire.arguments_digest,
            state: wire.state,
            output: wire.output,
            error: wire.error,
        }
    }
}

impl ToolResult {
    pub fn parse_str(input: &str) -> Result<Self, ToolProtocolError> {
        serde_json::from_str::<ToolResultWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidResult))
    }

    pub fn parse_bytes(input: &[u8]) -> Result<Self, ToolProtocolError> {
        serde_json::from_slice::<ToolResultWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidResult))
    }

    pub fn parse_value(input: Value) -> Result<Self, ToolProtocolError> {
        serde_json::from_value::<ToolResultWire>(input)
            .map(Into::into)
            .map_err(|_| ToolProtocolError::new(ToolProtocolErrorCode::InvalidResult))
    }
}
