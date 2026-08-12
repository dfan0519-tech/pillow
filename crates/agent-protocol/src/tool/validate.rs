use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use sha2::{Digest, Sha256};

use super::digest::canonical_arguments_digest;
use super::manifest::{
    LIST_RESULT_COUNT_CAP, PROCESS_OUTPUT_BYTE_CAP, READ_OUTPUT_BYTE_CAP, SEARCH_RESULT_COUNT_CAP,
    manifest_for,
};
use super::protocol_error::{ToolProtocolError, ToolProtocolErrorCode};
use super::reservation::{ToolCallReservation, ToolCallReservationKey, ToolProposalFingerprint};
use super::types::{
    ArgumentsDigest, FsListArguments, FsListOutput, FsReadArguments, FsReadOutput,
    FsSearchArguments, FsSearchOutput, FsWriteArguments, FsWriteOutput, PROCESS_ARGV_BYTE_CAP,
    PROCESS_ARGV_COUNT_CAP, ProcessRunArguments, ProcessRunOutput, ToolActiveContext,
    ToolArguments, ToolContentKind, ToolExecutionState, ToolId, ToolProposal, ToolResult,
    ValidatedToolProposal, ValidatedToolResult, WRITE_CONTENT_BYTE_CAP,
};

impl ToolActiveContext {
    pub fn new(
        project_id: impl Into<String>,
        conversation_id: impl Into<String>,
        run_id: crate::RunId,
    ) -> Result<Self, ToolProtocolError> {
        let project_id = project_id.into();
        let conversation_id = conversation_id.into();
        validate_identity("project_id", &project_id)?;
        validate_identity("conversation_id", &conversation_id)?;
        Ok(Self {
            project_id,
            conversation_id,
            run_id,
        })
    }
}

impl ToolProposal {
    pub fn validate_for_context(
        &self,
        expected: &ToolActiveContext,
    ) -> Result<ValidatedToolProposal, ToolProtocolError> {
        validate_identity("project_id", &self.project_id)?;
        validate_identity("conversation_id", &self.conversation_id)?;
        validate_identity("call_id", &self.call_id)?;
        if self.project_id != expected.project_id
            || self.conversation_id != expected.conversation_id
            || self.run_id != expected.run_id
        {
            return Err(error(
                ToolProtocolErrorCode::ActiveContextMismatch,
                "tool proposal is not bound to the expected active context",
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
        let normalized_arguments_digest = canonical_arguments_digest(&canonical_arguments)?;
        if normalized_arguments_digest != self.arguments_digest {
            return Err(error(
                ToolProtocolErrorCode::ArgumentsDigestMismatch,
                "arguments_digest does not match the normalized typed canonical arguments",
            ));
        }
        let fingerprint = proposal_fingerprint(self, &normalized_arguments_digest)?;
        let reservation_key = call_reservation_key(self)?;

        Ok(ValidatedToolProposal {
            proposal: self.clone(),
            context: expected.clone(),
            manifest: manifest_for(tool_id),
            arguments,
            canonical_arguments,
            normalized_arguments_digest,
            reservation: ToolCallReservation::new(reservation_key, fingerprint),
        })
    }
}

impl ToolResult {
    pub fn validate_for_proposal(
        &self,
        expected: &ToolActiveContext,
        proposal: &ValidatedToolProposal,
    ) -> Result<ValidatedToolResult, ToolProtocolError> {
        if proposal.context != *expected {
            return Err(error(
                ToolProtocolErrorCode::ActiveContextMismatch,
                "validated proposal is not bound to the expected active context",
            ));
        }
        validate_identity("project_id", &self.project_id)?;
        validate_identity("conversation_id", &self.conversation_id)?;
        validate_identity("call_id", &self.call_id)?;
        if self.project_id != proposal.project_id()
            || self.conversation_id != proposal.conversation_id()
            || self.run_id != proposal.run_id()
            || self.call_id != proposal.call_id()
            || self.tool_id != proposal.tool_id()
            || self.tool_version != proposal.tool_version()
            || self.proposal_fingerprint != *proposal.fingerprint()
            || self.arguments_digest != *proposal.normalized_arguments_digest()
        {
            return Err(invalid_result(
                "tool result identity does not match the validated proposal",
            ));
        }

        let tool_id = ToolId::parse(&self.tool_id).ok_or_else(|| {
            invalid_result("validated tool_id is not in the fixed Keeper manifest catalog")
        })?;

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
                validated.output = Some(decode_output(tool_id, output, proposal.arguments())?);
            }
            ToolExecutionState::Failed => {
                if self.output.is_some() {
                    return Err(invalid_result("a failed tool result cannot carry output"));
                }
                self.error
                    .as_ref()
                    .ok_or_else(|| invalid_result("a failed tool result requires an error"))?;
            }
            ToolExecutionState::Cancelled => {
                if self.output.is_some() || self.error.is_some() {
                    return Err(invalid_result(
                        "a cancelled tool result cannot carry output or a public error",
                    ));
                }
            }
        }
        Ok(ValidatedToolResult {
            result: validated,
            context: expected.clone(),
        })
    }
}

fn call_reservation_key(
    proposal: &ToolProposal,
) -> Result<ToolCallReservationKey, ToolProtocolError> {
    let value = json!({
        "project_id": proposal.project_id,
        "conversation_id": proposal.conversation_id,
        "run_id": proposal.run_id,
        "call_id": proposal.call_id,
    });
    let digest = canonical_arguments_digest(&value)?;
    Ok(ToolCallReservationKey::from_digest(&digest))
}

fn proposal_fingerprint(
    proposal: &ToolProposal,
    normalized_arguments_digest: &super::types::ArgumentsDigest,
) -> Result<ToolProposalFingerprint, ToolProtocolError> {
    let value = json!({
        "project_id": proposal.project_id,
        "conversation_id": proposal.conversation_id,
        "run_id": proposal.run_id,
        "call_id": proposal.call_id,
        "tool_id": proposal.tool_id,
        "tool_version": proposal.tool_version,
        "arguments_digest": normalized_arguments_digest,
    });
    let digest = canonical_arguments_digest(&value)?;
    Ok(ToolProposalFingerprint::from_digest(&digest))
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
        ToolId::FsWrite => {
            let arguments =
                decode::<FsWriteArguments>(raw, ToolProtocolErrorCode::InvalidArguments)?;
            validate_write_path(&arguments.path)?;
            validate_write_content(&arguments.content)?;
            let canonical = encode(&arguments, ToolProtocolErrorCode::InvalidArguments)?;
            Ok((ToolArguments::FsWrite(arguments), canonical))
        }
        ToolId::ProcessRun => {
            let arguments =
                decode::<ProcessRunArguments>(raw, ToolProtocolErrorCode::InvalidArguments)?;
            validate_process_program(&arguments.program)?;
            validate_process_argv(&arguments.program, &arguments.argv)?;
            if let Some(cwd) = arguments.cwd.as_deref() {
                validate_project_path(cwd)?;
            }
            let canonical = encode(&arguments, ToolProtocolErrorCode::InvalidArguments)?;
            Ok((ToolArguments::ProcessRun(arguments), canonical))
        }
    }
}

fn decode_output(
    tool_id: ToolId,
    raw: &Value,
    arguments: &ToolArguments,
) -> Result<Value, ToolProtocolError> {
    let (output, count) = match tool_id {
        ToolId::FsList => {
            let output = decode::<FsListOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            let ToolArguments::FsList(arguments) = arguments else {
                return Err(invalid_result(
                    "validated argument type does not match tool",
                ));
            };
            for entry in &output.entries {
                validate_project_path_for_result(&entry.path)?;
                validate_result_scope(&arguments.path, &entry.path)?;
            }
            let count = output.entries.len();
            if count > arguments.max_results as usize {
                return Err(invalid_result(
                    "fs.list output exceeds the proposal max_results",
                ));
            }
            (
                encode(&output, ToolProtocolErrorCode::InvalidResult)?,
                Some(count),
            )
        }
        ToolId::FsRead => {
            let output = decode::<FsReadOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            let ToolArguments::FsRead(arguments) = arguments else {
                return Err(invalid_result(
                    "validated argument type does not match tool",
                ));
            };
            validate_project_path_for_result(&output.path)?;
            if output.path != arguments.path {
                return Err(invalid_result(
                    "fs.read output path does not match the requested path",
                ));
            }
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
            if output.bytes_returned > arguments.max_bytes {
                return Err(invalid_result(
                    "fs.read output exceeds the proposal max_bytes",
                ));
            }
            (encode(&output, ToolProtocolErrorCode::InvalidResult)?, None)
        }
        ToolId::FsSearch => {
            let output = decode::<FsSearchOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            let ToolArguments::FsSearch(arguments) = arguments else {
                return Err(invalid_result(
                    "validated argument type does not match tool",
                ));
            };
            for item in &output.matches {
                validate_project_path_for_result(&item.path)?;
                validate_result_scope(&arguments.path, &item.path)?;
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
            if count > arguments.max_results as usize {
                return Err(invalid_result(
                    "fs.search output exceeds the proposal max_results",
                ));
            }
            (
                encode(&output, ToolProtocolErrorCode::InvalidResult)?,
                Some(count),
            )
        }
        ToolId::FsWrite => {
            let output = decode::<FsWriteOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            let ToolArguments::FsWrite(arguments) = arguments else {
                return Err(invalid_result(
                    "validated argument type does not match tool",
                ));
            };
            validate_write_path_for_result(&output.path)?;
            if output.path != arguments.path {
                return Err(invalid_result(
                    "fs.write output path does not match the requested path",
                ));
            }
            if output.mode != arguments.mode {
                return Err(invalid_result(
                    "fs.write output mode does not match the requested mode",
                ));
            }
            let expected_bytes = arguments.content.len() as u64;
            if output.bytes_written != expected_bytes {
                return Err(invalid_result(
                    "fs.write bytes_written does not match the sealed content length",
                ));
            }
            let expected_digest = content_digest(&arguments.content);
            if output.content_digest != expected_digest.as_str() {
                return Err(invalid_result(
                    "fs.write content_digest does not match the sealed content",
                ));
            }
            (encode(&output, ToolProtocolErrorCode::InvalidResult)?, None)
        }
        ToolId::ProcessRun => {
            let output = decode::<ProcessRunOutput>(raw, ToolProtocolErrorCode::InvalidResult)?;
            let ToolArguments::ProcessRun(_) = arguments else {
                return Err(invalid_result(
                    "validated argument type does not match tool",
                ));
            };
            validate_stream_digest(&output.stdout_sha256)?;
            validate_stream_digest(&output.stderr_sha256)?;
            if output.captured_bytes > PROCESS_OUTPUT_BYTE_CAP {
                return Err(invalid_result(
                    "process.run captured_bytes exceeds the manifest output byte cap",
                ));
            }
            (encode(&output, ToolProtocolErrorCode::InvalidResult)?, None)
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

fn validate_write_path(path: &str) -> Result<(), ToolProtocolError> {
    if path == "." {
        return Err(invalid_arguments(
            "fs.write path must name a project-relative file",
        ));
    }
    validate_project_path(path)
}

fn validate_write_path_for_result(path: &str) -> Result<(), ToolProtocolError> {
    if path == "." {
        return Err(invalid_result(
            "fs.write result path must name a project-relative file",
        ));
    }
    validate_project_path_for_result(path)
}

fn validate_write_content(content: &str) -> Result<(), ToolProtocolError> {
    let bytes = content.as_bytes();
    if bytes.is_empty() || bytes.len() as u64 > WRITE_CONTENT_BYTE_CAP {
        return Err(invalid_arguments(format!(
            "fs.write content must be 1..={WRITE_CONTENT_BYTE_CAP} UTF-8 bytes"
        )));
    }
    if bytes.contains(&0) {
        return Err(invalid_arguments(
            "fs.write content must not contain NUL bytes",
        ));
    }
    Ok(())
}

/// Program label grammar: `^[a-z][a-z0-9_-]{0,31}$` (1..=32 ASCII chars).
fn validate_process_program(program: &str) -> Result<(), ToolProtocolError> {
    let bytes = program.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return Err(invalid_arguments(
            "process.run program label must be 1..=32 characters matching ^[a-z][a-z0-9_-]{0,31}$",
        ));
    }
    let first = bytes[0];
    if !first.is_ascii_lowercase() {
        return Err(invalid_arguments(
            "process.run program label must start with a lowercase ASCII letter",
        ));
    }
    if !bytes[1..].iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
    }) {
        return Err(invalid_arguments(
            "process.run program label may only contain lowercase letters, digits, '_' and '-'",
        ));
    }
    Ok(())
}

fn validate_process_argv(program: &str, argv: &[String]) -> Result<(), ToolProtocolError> {
    if argv.len() > PROCESS_ARGV_COUNT_CAP {
        return Err(invalid_arguments(format!(
            "process.run argv must contain at most {PROCESS_ARGV_COUNT_CAP} entries"
        )));
    }
    let mut combined = program.len();
    for entry in argv {
        // Control (Cc) plus bidi Format controls — see is_bidi_formatting_char.
        if entry
            .chars()
            .any(|character| character.is_control() || is_bidi_formatting_char(character))
        {
            return Err(invalid_arguments(
                "process.run argv entries must not contain control or bidi formatting characters",
            ));
        }
        combined = combined.saturating_add(entry.len());
        if combined > PROCESS_ARGV_BYTE_CAP {
            return Err(invalid_arguments(format!(
                "process.run combined program and argv must be at most {PROCESS_ARGV_BYTE_CAP} UTF-8 bytes"
            )));
        }
    }
    Ok(())
}

/// Unicode bidi *formatting* controls (category Format). These are **not**
/// matched by `char::is_control` (Control/Cc only), so they previously passed
/// process argv and path validation while still reordering browser display.
///
/// Audit EA-06 list: U+202A, U+202B, U+202D, U+202E, U+2066..=U+2069.
/// RTL script letters (Arabic/Hebrew) are intentionally **not** included.
fn is_bidi_formatting_char(character: char) -> bool {
    matches!(
        character,
        '\u{202A}' // LEFT-TO-RIGHT EMBEDDING
            | '\u{202B}' // RIGHT-TO-LEFT EMBEDDING
            | '\u{202D}' // LEFT-TO-RIGHT OVERRIDE
            | '\u{202E}' // RIGHT-TO-LEFT OVERRIDE
            | '\u{2066}' // LEFT-TO-RIGHT ISOLATE
            | '\u{2067}' // RIGHT-TO-LEFT ISOLATE
            | '\u{2068}' // FIRST STRONG ISOLATE
            | '\u{2069}' // POP DIRECTIONAL ISOLATE
    )
}

fn validate_stream_digest(value: &str) -> Result<(), ToolProtocolError> {
    ArgumentsDigest::parse(value).map(|_| ()).map_err(|_| {
        invalid_result("process.run stream digests must be sha256:<64 lowercase hex> values")
    })
}

fn content_digest(content: &str) -> ArgumentsDigest {
    let digest = Sha256::digest(content.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    ArgumentsDigest::from_sha256_hex(hex)
}

fn validate_result_scope(requested: &str, candidate: &str) -> Result<(), ToolProtocolError> {
    if requested == "."
        || candidate == requested
        || candidate
            .strip_prefix(requested)
            .is_some_and(|remainder| remainder.starts_with('/'))
    {
        return Ok(());
    }
    Err(invalid_result(
        "result path is outside the requested project-relative scope",
    ))
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
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err("path contains an empty, dot, or parent segment");
        }
        if component.len() > 255 {
            return Err("path component exceeds the portable 255-byte cap");
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err("path component has a trailing dot or space alias");
        }
        if component.chars().any(|character| {
            character.is_control()
                || is_bidi_formatting_char(character)
                || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        }) {
            return Err("path component contains non-portable characters");
        }
        if is_windows_device_component(component) {
            return Err("path component is a reserved Windows device name");
        }
    }
    Ok(())
}

fn is_windows_device_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let bytes = stem.as_bytes();
    (bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && matches!(bytes[3], b'1'..=b'9'))
        || matches!(
            stem.as_str(),
            "COM\u{00b9}"
                | "COM\u{00b2}"
                | "COM\u{00b3}"
                | "LPT\u{00b9}"
                | "LPT\u{00b2}"
                | "LPT\u{00b3}"
        )
}

fn decode<T>(raw: &Value, code: ToolProtocolErrorCode) -> Result<T, ToolProtocolError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(raw.clone())
        .map_err(|_| error(code, "payload does not match the fixed schema"))
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

fn error(code: ToolProtocolErrorCode, _message: impl Into<String>) -> ToolProtocolError {
    ToolProtocolError::new(code)
}
