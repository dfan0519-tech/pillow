use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::digest::canonical_arguments_digest;
use super::manifest::{
    LIST_RESULT_COUNT_CAP, READ_OUTPUT_BYTE_CAP, SEARCH_RESULT_COUNT_CAP, manifest_for,
};
use super::types::{
    FsListArguments, FsListOutput, FsReadArguments, FsReadOutput, FsSearchArguments,
    FsSearchOutput, ToolArguments, ToolContentKind, ToolExecutionState, ToolId, ToolProposal,
    ToolProtocolError, ToolProtocolErrorCode, ToolPublicError, ToolResult, ValidatedToolProposal,
};

impl ToolProposal {
    pub fn validate_for_project(
        &self,
        expected_project_id: &str,
    ) -> Result<ValidatedToolProposal, ToolProtocolError> {
        validate_identity("project_id", &self.project_id)?;
        validate_identity("conversation_id", &self.conversation_id)?;
        validate_identity("call_id", &self.call_id)?;
        if self.project_id != expected_project_id {
            return Err(error(
                ToolProtocolErrorCode::ProjectMismatch,
                "tool proposal is not bound to the expected project",
            ));
        }

        let tool_id = ToolId::parse(&self.tool_id).ok_or_else(|| {
            error(
                ToolProtocolErrorCode::UnknownTool,
                "tool_id is not in the fixed Keeper manifest catalog",
            )
        })?;
        if self.tool_version != 1 {
            return Err(error(
                ToolProtocolErrorCode::UnsupportedToolVersion,
                "only tool_version 1 is accepted for the fixed read tools",
            ));
        }

        let (arguments, canonical_arguments) = decode_arguments(tool_id, &self.arguments)?;
        let actual_digest = canonical_arguments_digest(&self.arguments)?;
        if actual_digest != self.arguments_digest {
            return Err(error(
                ToolProtocolErrorCode::ArgumentsDigestMismatch,
                "arguments_digest does not match the canonical structured arguments",
            ));
        }

        Ok(ValidatedToolProposal {
            proposal: self.clone(),
            manifest: manifest_for(tool_id),
            arguments,
            canonical_arguments,
        })
    }
}

impl ToolResult {
    pub fn validate_for_project(
        &self,
        expected_project_id: &str,
    ) -> Result<Self, ToolProtocolError> {
        validate_identity("project_id", &self.project_id)?;
        validate_identity("conversation_id", &self.conversation_id)?;
        validate_identity("call_id", &self.call_id)?;
        if self.project_id != expected_project_id {
            return Err(error(
                ToolProtocolErrorCode::ProjectMismatch,
                "tool result is not bound to the expected project",
            ));
        }

        let tool_id = ToolId::parse(&self.tool_id).ok_or_else(|| {
            error(
                ToolProtocolErrorCode::UnknownTool,
                "tool_id is not in the fixed Keeper manifest catalog",
            )
        })?;
        if self.tool_version != 1 {
            return Err(error(
                ToolProtocolErrorCode::UnsupportedToolVersion,
                "only tool_version 1 is accepted for the fixed read tools",
            ));
        }

        let mut validated = self.clone();
        match self.state {
            ToolExecutionState::Proposed
            | ToolExecutionState::Validated
            | ToolExecutionState::Executing => {
                if self.output.is_some() || self.error.is_some() {
                    return Err(invalid_result(
                        "non-terminal tool states cannot carry output or public errors",
                    ));
                }
            }
            ToolExecutionState::Succeeded => {
                if self.error.is_some() {
                    return Err(invalid_result(
                        "a succeeded tool result cannot carry a public error",
                    ));
                }
                let output = self
                    .output
                    .as_ref()
                    .ok_or_else(|| invalid_result("a succeeded tool result requires output"))?;
                validated.output = Some(decode_output(tool_id, output)?);
            }
            ToolExecutionState::Failed => {
                if self.output.is_some() {
                    return Err(invalid_result("a failed tool result cannot carry output"));
                }
                validate_public_error(
                    self.error
                        .as_ref()
                        .ok_or_else(|| invalid_result("a failed tool result requires an error"))?,
                )?;
            }
            ToolExecutionState::Cancelled => {
                if self.output.is_some() || self.error.is_some() {
                    return Err(invalid_result(
                        "a cancelled tool result cannot carry output or a public error",
                    ));
                }
            }
        }
        Ok(validated)
    }
}

fn decode_arguments(
    tool_id: ToolId,
    raw: &Value,
) -> Result<(ToolArguments, Value), ToolProtocolError> {
    match tool_id {
        ToolId::FsList => {
            let arguments =
                decode::<FsListArguments>(raw, ToolProtocolErrorCode::InvalidArguments)?;
            validate_project_path(&arguments.path)?;
            if arguments.max_results == 0 || arguments.max_results > LIST_RESULT_COUNT_CAP {
                return Err(invalid_arguments(format!(
                    "fs.list max_results must be between 1 and {LIST_RESULT_COUNT_CAP}"
                )));
            }
            let canonical = encode(&arguments, ToolProtocolErrorCode::InvalidArguments)?;
            Ok((ToolArguments::FsList(arguments), canonical))
        }
        ToolId::FsRead => {
            let arguments =
                decode::<FsReadArguments>(raw, ToolProtocolErrorCode::InvalidArguments)?;
            validate_project_path(&arguments.path)?;
            if arguments.max_bytes == 0 || arguments.max_bytes > READ_OUTPUT_BYTE_CAP {
                return Err(invalid_arguments(format!(
                    "fs.read max_bytes must be between 1 and {READ_OUTPUT_BYTE_CAP}"
                )));
            }
            let canonical = encode(&arguments, ToolProtocolErrorCode::InvalidArguments)?;
            Ok((ToolArguments::FsRead(arguments), canonical))
        }
        ToolId::FsSearch => {
            let arguments =
                decode::<FsSearchArguments>(raw, ToolProtocolErrorCode::InvalidArguments)?;
            validate_project_path(&arguments.path)?;
            if arguments.query.trim().is_empty()
                || arguments.query.len() > 1_024
                || arguments.query.chars().any(char::is_control)
            {
                return Err(invalid_arguments(
                    "fs.search query must be 1..=1024 bytes and contain no control characters",
                ));
            }
            if arguments.max_results == 0 || arguments.max_results > SEARCH_RESULT_COUNT_CAP {
                return Err(invalid_arguments(format!(
                    "fs.search max_results must be between 1 and {SEARCH_RESULT_COUNT_CAP}"
                )));
            }
            let canonical = encode(&arguments, ToolProtocolErrorCode::InvalidArguments)?;
            Ok((ToolArguments::FsSearch(arguments), canonical))
        }
    }
}

fn decode_output(tool_id: ToolId, raw: &Value) -> Result<Value, ToolProtocolError> {
    let (output, count) = match tool_id {
        ToolId::FsList => {
            let output = decode::<FsListOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            for entry in &output.entries {
                validate_project_path_for_result(&entry.path)?;
            }
            let count = output.entries.len();
            (
                encode(&output, ToolProtocolErrorCode::InvalidResult)?,
                Some(count),
            )
        }
        ToolId::FsRead => {
            let output = decode::<FsReadOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            validate_project_path_for_result(&output.path)?;
            match (&output.content_kind, &output.content) {
                (ToolContentKind::Utf8, Some(content))
                    if content.len() as u64 == output.bytes_returned => {}
                (ToolContentKind::Binary, None) if output.bytes_returned == 0 => {}
                _ => {
                    return Err(invalid_result(
                        "fs.read content, classification, and bytes_returned are inconsistent",
                    ));
                }
            }
            (encode(&output, ToolProtocolErrorCode::InvalidResult)?, None)
        }
        ToolId::FsSearch => {
            let output = decode::<FsSearchOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            for item in &output.matches {
                validate_project_path_for_result(&item.path)?;
                if item.line == 0
                    || item.column == 0
                    || item.preview.len() > 2_048
                    || item
                        .preview
                        .chars()
                        .any(|character| character.is_control() && character != '\t')
                {
                    return Err(invalid_result(
                        "fs.search matches require 1-based positions and bounded text previews",
                    ));
                }
            }
            let count = output.matches.len();
            (
                encode(&output, ToolProtocolErrorCode::InvalidResult)?,
                Some(count),
            )
        }
    };

    let manifest = manifest_for(tool_id);
    if let (Some(count), Some(cap)) = (count, manifest.limits.result_count_cap)
        && count > cap as usize
    {
        return Err(invalid_result(
            "tool output exceeds the manifest result count cap",
        ));
    }
    let bytes = serde_json::to_vec(&output)
        .map_err(|_| invalid_result("tool output could not be measured"))?;
    if bytes.len() as u64 > manifest.limits.output_byte_cap {
        return Err(invalid_result(
            "tool output exceeds the manifest output byte cap",
        ));
    }
    Ok(output)
}

fn validate_identity(name: &str, value: &str) -> Result<(), ToolProtocolError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error(
            ToolProtocolErrorCode::InvalidEnvelope,
            format!("{name} must be 1..=128 portable identifier characters"),
        ));
    }
    Ok(())
}

fn validate_project_path(path: &str) -> Result<(), ToolProtocolError> {
    validate_project_path_inner(path)
        .map_err(|message| invalid_arguments(format!("invalid project-relative path: {message}")))
}

fn validate_project_path_for_result(path: &str) -> Result<(), ToolProtocolError> {
    validate_project_path_inner(path)
        .map_err(|message| invalid_result(format!("invalid result path: {message}")))
}

fn validate_project_path_inner(path: &str) -> Result<(), &'static str> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > 4_096 {
        return Err("path length is outside the protocol bounds");
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err("path must use portable relative slash-separated syntax");
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("path contains an empty, dot, or parent segment");
    }
    if path.chars().any(char::is_control) {
        return Err("path contains control characters");
    }
    Ok(())
}

fn validate_public_error(value: &ToolPublicError) -> Result<(), ToolProtocolError> {
    if value != &ToolPublicError::canonical(value.code) {
        return Err(invalid_result(
            "public tool errors must use the fixed secret-safe message for their code",
        ));
    }
    Ok(())
}

fn decode<T>(raw: &Value, code: ToolProtocolErrorCode) -> Result<T, ToolProtocolError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(raw.clone()).map_err(|failure| {
        error(
            code,
            format!("payload does not match the fixed schema: {failure}"),
        )
    })
}

fn encode<T>(value: &T, code: ToolProtocolErrorCode) -> Result<Value, ToolProtocolError>
where
    T: Serialize,
{
    serde_json::to_value(value).map_err(|_| error(code, "typed payload serialization failed"))
}

fn invalid_arguments(message: impl Into<String>) -> ToolProtocolError {
    error(ToolProtocolErrorCode::InvalidArguments, message)
}

fn invalid_result(message: impl Into<String>) -> ToolProtocolError {
    error(ToolProtocolErrorCode::InvalidResult, message)
}

fn error(code: ToolProtocolErrorCode, message: impl Into<String>) -> ToolProtocolError {
    ToolProtocolError::new(code, message)
}
